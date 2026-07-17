use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;
use std::time::Duration;

use crate::app_state::AppState;

pub(crate) async fn start_post_app_state_workers(state: &mut Arc<AppState>) {
    let terminal_effects = state.services.terminal_application_effect_worker.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            match terminal_effects.drain_once().await {
                Ok(count) if count > 0 => {
                    diag!(
                        Info,
                        Subsystem::AgentRun,
                        count,
                        "已收敛 Runtime terminal application effects"
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    let context = DiagnosticErrorContext::new(
                        "background_workers.terminal_application_effects",
                        "drain_once",
                    );
                    diag_error!(
                        Warn,
                        Subsystem::AgentRun,
                        context = &context,
                        error = &error,
                        "Runtime terminal application effect recovery failed"
                    );
                }
            }
        }
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
