use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;
use std::time::Duration;

use crate::app_state::AppState;
use agentdash_application::routine::{RoutineExecutor, RoutineMailboxRuntime};
use agentdash_application::runtime_session_agent_run_bridge::{
    agent_run_session_control, agent_run_session_core, agent_run_session_eventing,
    agent_run_session_launch,
};

pub(crate) async fn start_post_app_state_workers(state: &mut Arc<AppState>) {
    match state
        .services
        .session_effects
        .replay_terminal_effect_outbox(100)
        .await
    {
        Ok(count) if count > 0 => {
            diag!(
                Info,
                Subsystem::Api,
                count,
                "已调度 terminal effect outbox 恢复执行"
            );
        }
        Ok(_) => {}
        Err(error) => {
            let context =
                DiagnosticErrorContext::new("background_workers.start", "replay_terminal_effects");
            diag_error!(
                Warn,
                Subsystem::Api,
                context = &context,
                error = &error,
                batch_limit = 100,
                "terminal effect outbox 恢复执行失败"
            );
        }
    }

    agentdash_application_runtime_session::session::stall_detector::spawn_stall_detector(
        state.services.session_runtime.clone(),
        agentdash_application_runtime_session::session::stall_detector::DEFAULT_STALL_TIMEOUT_MS,
    );

    let routine_executor = Arc::new(RoutineExecutor::new(
        state.repos.clone(),
        state.services.backend_registry.clone(),
        RoutineMailboxRuntime {
            session_core: agent_run_session_core(state.services.session_core.clone()),
            session_control: agent_run_session_control(state.services.session_control.clone()),
            session_eventing: agent_run_session_eventing(state.services.session_eventing.clone()),
            session_launch: agent_run_session_launch(state.services.session_launch.clone()),
        },
    ));
    if let Some(s) = Arc::get_mut(state) {
        s.services.routine_executor = Some(routine_executor.clone());
    }
    let cron_repos = state.repos.clone();
    let cron_handle = state.services.cron_scheduler.clone();
    tokio::spawn(async move {
        agentdash_application::scheduling::cron_scheduler::spawn_cron_scheduler(
            cron_repos,
            routine_executor,
            &cron_handle,
        )
        .await;
    });

    let auth_session_service = state.services.auth_session_service.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10 * 60));
        loop {
            interval.tick().await;
            match auth_session_service.cleanup_expired_sessions().await {
                Ok(count) if count > 0 => {
                    diag!(Info, Subsystem::Api, deleted = count, "已清理过期认证会话")
                }
                Ok(_) => {}
                Err(err) => {
                    let context = DiagnosticErrorContext::new(
                        "background_workers.auth_session_cleanup",
                        "cleanup_expired_sessions",
                    );
                    diag_error!(
                        Warn,
                        Subsystem::Api,
                        context = &context,
                        error = &err,
                        "清理过期认证会话失败"
                    );
                }
            }
        }
    });
}
