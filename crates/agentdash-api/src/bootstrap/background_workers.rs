use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;
use std::time::Duration;

use crate::app_state::AppState;

const RUNTIME_PRODUCT_CHANGE_BATCH_LIMIT: usize = 64;
const RUNTIME_PRODUCT_CHANGE_POLL_INTERVAL: Duration = Duration::from_secs(1);

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
}
