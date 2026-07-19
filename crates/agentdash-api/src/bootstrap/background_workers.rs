use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;
use std::time::Duration;

use crate::app_state::AppState;

const RUNTIME_PRODUCT_CHANGE_BATCH_LIMIT: usize = 64;
const RUNTIME_PRODUCT_CHANGE_POLL_INTERVAL: Duration = Duration::from_secs(1);
const WORKFLOW_RECOVERY_BATCH_LIMIT: usize = 64;
const WORKFLOW_RECOVERY_POLL_INTERVAL: Duration = Duration::from_secs(1);
const AGENT_RUN_PRODUCT_PROTOCOL_RECOVERY_BATCH_LIMIT: usize = 64;
const AGENT_RUN_PRODUCT_PROTOCOL_RECOVERY_POLL_INTERVAL: Duration = Duration::from_secs(1);

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

    let product_projection = state
        .services
        .agent_run_product_projection_composition
        .clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(RUNTIME_PRODUCT_CHANGE_POLL_INTERVAL);
        loop {
            interval.tick().await;
            loop {
                match product_projection
                    .drain_runtime_change_outbox(RUNTIME_PRODUCT_CHANGE_BATCH_LIMIT)
                    .await
                {
                    Ok(count) if count == RUNTIME_PRODUCT_CHANGE_BATCH_LIMIT => {
                        tokio::task::yield_now().await;
                    }
                    Ok(_) => break,
                    Err(err) => {
                        let context = DiagnosticErrorContext::new(
                            "background_workers.runtime_product_change",
                            "drain_runtime_change_outbox",
                        );
                        diag_error!(
                            Warn,
                            Subsystem::Api,
                            context = &context,
                            error = &err,
                            "投递 Managed Runtime Product change 失败"
                        );
                        break;
                    }
                }
            }
        }
    });

    let agent_run_product_protocol = state.services.agent_run_product_protocol.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(AGENT_RUN_PRODUCT_PROTOCOL_RECOVERY_POLL_INTERVAL);
        loop {
            interval.tick().await;
            let worker =
                agentdash_application_agentrun::agent_run::AgentRunProductProtocolRecoveryWorker::new(
                    agent_run_product_protocol.as_ref(),
                );
            match worker
                .advance_batch(AGENT_RUN_PRODUCT_PROTOCOL_RECOVERY_BATCH_LIMIT)
                .await
            {
                Ok(report) => {
                    for failure in report.failures {
                        diag!(
                            Warn,
                            Subsystem::Api,
                            operation = "background_workers.agent_run_product_protocol_recovery",
                            stage = "advance_saga",
                            protocol = failure.protocol,
                            request_id = %failure.request_id,
                            reason = %failure.reason,
                            "恢复 AgentRun Product protocol saga 失败"
                        );
                    }
                }
                Err(error) => {
                    let context = DiagnosticErrorContext::new(
                        "background_workers.agent_run_product_protocol_recovery",
                        "scan_recoverable_sagas",
                    );
                    diag_error!(
                        Warn,
                        Subsystem::Api,
                        context = &context,
                        error = &error,
                        "扫描 AgentRun Product protocol saga 失败"
                    );
                }
            }
        }
    });
}
