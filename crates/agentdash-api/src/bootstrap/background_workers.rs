use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;
use std::time::Duration;

use crate::app_state::AppState;
use agentdash_application::routine::{RoutineExecutor, RoutineMailboxRuntime};
use agentdash_application::runtime_session_agent_run_bridge::{
    agent_run_session_control, agent_run_session_core, agent_run_session_eventing,
    agent_run_session_launch,
};
use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectReplayPhase, AgentRunControlEffectReplayPort,
};

const CONTROL_EFFECT_REPLAY_BATCH_LIMIT: u32 = 100;
const CONTROL_EFFECT_REPLAY_MAX_DELIVERY_BATCHES: usize = 20;
const CONTROL_EFFECT_REPLAY_INTERVAL: Duration = Duration::from_secs(10);
const CONTROL_EFFECT_REPLAY_INITIAL_DELAY: Duration = Duration::from_secs(1);

pub(crate) async fn start_post_app_state_workers(state: &mut Arc<AppState>) {
    let control_effect_replay: Arc<dyn AgentRunControlEffectReplayPort> =
        Arc::new(state.services.agent_run_control_effects.clone());
    replay_delivery_convergence_to_quiescence(control_effect_replay.as_ref()).await;
    spawn_control_effect_replay_worker(control_effect_replay);

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

fn spawn_control_effect_replay_worker(replay: Arc<dyn AgentRunControlEffectReplayPort>) {
    tokio::spawn(async move {
        tokio::time::sleep(CONTROL_EFFECT_REPLAY_INITIAL_DELAY).await;
        loop {
            replay_delivery_convergence_to_quiescence(replay.as_ref()).await;
            replay_control_effect_phase(
                replay.as_ref(),
                AgentRunControlEffectReplayPhase::TerminalSideEffects,
                "terminal_side_effects",
            )
            .await;
            tokio::time::sleep(CONTROL_EFFECT_REPLAY_INTERVAL).await;
        }
    });
}

async fn replay_delivery_convergence_to_quiescence(
    replay: &dyn AgentRunControlEffectReplayPort,
) -> usize {
    let mut total = 0usize;
    for _ in 0..CONTROL_EFFECT_REPLAY_MAX_DELIVERY_BATCHES {
        let count = replay_control_effect_phase(
            replay,
            AgentRunControlEffectReplayPhase::DeliveryConvergence,
            "delivery_convergence",
        )
        .await;
        total = total.saturating_add(count);
        if count < CONTROL_EFFECT_REPLAY_BATCH_LIMIT as usize {
            break;
        }
    }
    total
}

async fn replay_control_effect_phase(
    replay: &dyn AgentRunControlEffectReplayPort,
    phase: AgentRunControlEffectReplayPhase,
    phase_name: &'static str,
) -> usize {
    match replay
        .replay_control_effect_outbox_phase(phase, CONTROL_EFFECT_REPLAY_BATCH_LIMIT)
        .await
    {
        Ok(count) if count > 0 => {
            diag!(
                Info,
                Subsystem::Api,
                count,
                phase = phase_name,
                "已调度 AgentRun control effect outbox 分相恢复执行"
            );
            count
        }
        Ok(_) => 0,
        Err(error) => {
            let context = DiagnosticErrorContext::new(
                "background_workers.start",
                "replay_agent_run_control_effects",
            );
            diag_error!(
                Warn,
                Subsystem::Api,
                context = &context,
                error = &std::io::Error::other(error),
                batch_limit = 100,
                phase = phase_name,
                "AgentRun control effect outbox 分相恢复执行失败"
            );
            0
        }
    }
}
