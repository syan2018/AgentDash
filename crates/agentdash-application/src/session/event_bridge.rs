use agent_client_protocol::SessionNotification;
use agentdash_acp_meta::AgentDashSourceV1;
use agentdash_spi::hooks::{
    HookEvaluationQuery, HookSessionRuntimeAccess, HookTraceEntry, HookTrigger,
    SessionHookRefreshQuery,
};
use tokio::sync::broadcast;

use super::hook_events::build_hook_trace_notification;
use super::hub::SessionHub;
use super::hub_support::session_hook_trace_decision;

pub(super) struct HookTriggerInput<'a> {
    pub session_id: &'a str,
    pub turn_id: Option<&'a str>,
    pub trigger: HookTrigger,
    pub payload: Option<serde_json::Value>,
    pub refresh_reason: &'static str,
    pub source: AgentDashSourceV1,
}

impl SessionHub {
    pub(super) async fn emit_session_hook_trigger(
        &self,
        hook_session: &dyn HookSessionRuntimeAccess,
        input: &HookTriggerInput<'_>,
        tx: &broadcast::Sender<SessionNotification>,
    ) {
        let HookTriggerInput {
            session_id,
            turn_id,
            ref trigger,
            ref payload,
            refresh_reason,
            ref source,
        } = *input;
        match hook_session
            .evaluate(HookEvaluationQuery {
                session_id: session_id.to_string(),
                trigger: trigger.clone(),
                turn_id: turn_id.map(ToString::to_string),
                tool_name: None,
                tool_call_id: None,
                subagent_type: None,
                snapshot: Some(hook_session.snapshot()),
                payload: payload.clone(),
            })
            .await
        {
            Ok(resolution) => {
                if resolution.refresh_snapshot {
                    let _ = hook_session
                        .refresh(SessionHookRefreshQuery {
                            session_id: session_id.to_string(),
                            turn_id: turn_id.map(ToString::to_string),
                            reason: Some(refresh_reason.to_string()),
                        })
                        .await;
                }
                let trace = HookTraceEntry {
                    sequence: hook_session.next_trace_sequence(),
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    revision: hook_session.revision(),
                    trigger: trigger.clone(),
                    decision: session_hook_trace_decision(trigger, &resolution).to_string(),
                    tool_name: None,
                    tool_call_id: None,
                    subagent_type: None,
                    matched_rule_keys: resolution.matched_rule_keys,
                    refresh_snapshot: resolution.refresh_snapshot,
                    block_reason: resolution.block_reason,
                    completion: resolution.completion,
                    diagnostics: resolution.diagnostics,
                };
                hook_session.append_trace(trace.clone());
                if let Some(notification) =
                    build_hook_trace_notification(session_id, turn_id, source.clone(), &trace)
                {
                    let _ = self.store.append(session_id, &notification).await;
                    let _ = tx.send(notification);
                }
            }
            Err(error) => {
                tracing::warn!(
                    session_id = %session_id,
                    trigger = ?trigger,
                    error = %error,
                    "session hook 评估失败"
                );
            }
        }
    }
}
