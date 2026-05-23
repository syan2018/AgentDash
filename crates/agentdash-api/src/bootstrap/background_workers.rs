use std::sync::Arc;
use std::time::Duration;

use crate::app_state::AppState;
use agentdash_application::routine::RoutineExecutor;

pub(crate) async fn start_post_app_state_workers(state: &mut Arc<AppState>) {
    match state
        .services
        .session_effects
        .replay_terminal_effect_outbox(100)
        .await
    {
        Ok(count) if count > 0 => {
            tracing::info!(count, "已调度 terminal effect outbox 恢复执行");
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(error = %error, "terminal effect outbox 恢复执行失败");
        }
    }

    agentdash_application::session::stall_detector::spawn_stall_detector(
        state.services.session_runtime.clone(),
        agentdash_application::session::stall_detector::DEFAULT_STALL_TIMEOUT_MS,
    );

    let routine_executor = Arc::new(
        RoutineExecutor::new(
            state.repos.clone(),
            state.services.session_core.clone(),
            state.services.session_launch.clone(),
            state.services.vfs_service.clone(),
            state.services.connector.clone(),
            state.config.platform_config.clone(),
            state.services.backend_registry.clone(),
        )
        .with_audit_bus(state.services.audit_bus.clone()),
    );
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
                Ok(count) if count > 0 => tracing::info!(deleted = count, "已清理过期认证会话"),
                Ok(_) => {}
                Err(err) => tracing::warn!(error = %err, "清理过期认证会话失败"),
            }
        }
    });
}
