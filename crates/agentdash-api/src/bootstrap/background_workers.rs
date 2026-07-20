use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;
use std::time::Duration;

use crate::app_state::AppState;

const WORKFLOW_RECOVERY_BATCH_LIMIT: usize = 64;
const WORKFLOW_RECOVERY_POLL_INTERVAL: Duration = Duration::from_secs(1);
const COMPANION_CONTINUATION_RECOVERY_BATCH_LIMIT: usize = 64;
const COMPANION_CONTINUATION_RECOVERY_POLL_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) async fn start_post_app_state_workers(state: &mut Arc<AppState>) {
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

    let companion_continuations = state.services.companion_continuations.clone();
    let companion_continuation_effects = state.services.companion_continuation_effects.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(COMPANION_CONTINUATION_RECOVERY_POLL_INTERVAL);
        loop {
            interval.tick().await;
            let request_ids = match companion_continuations
                .list_recoverable(COMPANION_CONTINUATION_RECOVERY_BATCH_LIMIT)
                .await
            {
                Ok(request_ids) => request_ids,
                Err(error) => {
                    let context = DiagnosticErrorContext::new(
                        "background_workers.companion_continuation",
                        "list_recoverable",
                    );
                    diag_error!(
                        Warn,
                        Subsystem::Api,
                        context = &context,
                        error = &error,
                        "扫描可恢复 Companion continuation 失败"
                    );
                    continue;
                }
            };
            let worker =
                agentdash_application_agentrun::agent_run::CompanionContinuationWorker::new(
                    companion_continuations.as_ref(),
                    companion_continuation_effects.as_ref(),
                );
            for request_id in request_ids {
                if let Err(error) = worker.advance(request_id).await {
                    diag!(
                        Warn,
                        Subsystem::Api,
                        operation = "background_workers.companion_continuation",
                        stage = "advance_saga",
                        request_id = %request_id,
                        error = %error,
                        "恢复 Companion continuation 失败"
                    );
                }
            }
        }
    });

    let workflow_recovery = state.services.workflow_recovery.clone();
    let orchestration_launcher = state.services.orchestration_executor_launcher.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(WORKFLOW_RECOVERY_POLL_INTERVAL);
        loop {
            interval.tick().await;
            let run_ids = match workflow_recovery
                .list_recoverable_run_ids(WORKFLOW_RECOVERY_BATCH_LIMIT)
                .await
            {
                Ok(run_ids) => run_ids,
                Err(error) => {
                    let context = DiagnosticErrorContext::new(
                        "background_workers.workflow_recovery",
                        "list_recoverable_run_ids",
                    );
                    diag_error!(
                        Warn,
                        Subsystem::Api,
                        context = &context,
                        error = &error,
                        "扫描可恢复 Workflow executor 失败"
                    );
                    continue;
                }
            };
            for run_id in run_ids {
                if let Err(error) = orchestration_launcher.drain_ready_nodes(run_id).await {
                    let context = DiagnosticErrorContext::new(
                        "background_workers.workflow_recovery",
                        "drain_ready_nodes",
                    );
                    diag_error!(
                        Warn,
                        Subsystem::Api,
                        context = &context,
                        error = &error,
                        run_id = %run_id,
                        "恢复 Workflow executor 失败"
                    );
                }
            }
        }
    });
}
