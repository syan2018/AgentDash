use std::sync::Arc;

use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};
use agentdash_agent::tools::schema_value;
use agentdash_connector_contract::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use agentdash_executor::{
    ExecutionContext, HookPendingAction, HookPendingActionResolutionKind,
    HookPendingActionStatus,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::address_space::tools::provider::SharedExecutorHubHandle;

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HookActionResolutionMode {
    Adopted,
    Rejected,
    Completed,
    Superseded,
    UserDismissed,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResolveHookActionParams {
    pub action_id: String,
    pub resolution_kind: HookActionResolutionMode,
    pub note: Option<String>,
}

#[derive(Clone)]
pub struct ResolveHookActionTool {
    current_session_id: Option<String>,
    current_turn_id: String,
    hook_session: Option<Arc<agentdash_executor::HookSessionRuntime>>,
    executor_hub_handle: SharedExecutorHubHandle,
}

impl ResolveHookActionTool {
    pub fn new(executor_hub_handle: SharedExecutorHubHandle, context: &ExecutionContext) -> Self {
        Self {
            current_session_id: context
                .hook_session
                .as_ref()
                .map(|session| session.session_id().to_string()),
            current_turn_id: context.turn_id.clone(),
            hook_session: context.hook_session.clone(),
            executor_hub_handle,
        }
    }
}

#[async_trait]
impl AgentTool for ResolveHookActionTool {
    fn name(&self) -> &str {
        "resolve_hook_action"
    }

    fn description(&self) -> &str {
        "把当前 session 中的 hook pending action 显式标记为 adopted/rejected/completed/superseded 等已结案状态"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<ResolveHookActionParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: ResolveHookActionParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let action_id = params.action_id.trim();
        if action_id.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "action_id 不能为空".to_string(),
            ));
        }

        let current_session_id = self.current_session_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有可识别的 hook runtime，无法结案 hook action".to_string(),
            )
        })?;
        let hook_session = self.hook_session.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有 hook runtime，无法结案 hook action".to_string(),
            )
        })?;
        let resolution_kind = map_hook_action_resolution_kind(params.resolution_kind);
        let note = params
            .note
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let action = hook_session
            .resolve_pending_action(
                action_id,
                resolution_kind,
                note.clone(),
                Some(self.current_turn_id.clone()),
            )
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!(
                    "当前 session 中不存在 action_id=`{action_id}` 的 hook action"
                ))
            })?;

        if let Some(executor_hub) = self.executor_hub_handle.get().await {
            let notification = build_hook_action_resolved_notification(
                &current_session_id,
                &self.current_turn_id,
                &action,
            );
            let _ = executor_hub
                .inject_notification(&current_session_id, notification)
                .await;
        }

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已更新 hook action 结案状态。\n- action_id: {}\n- status: {}\n- resolution_kind: {}",
                action.id,
                hook_action_status_key(action.status),
                action
                    .resolution_kind
                    .map(hook_action_resolution_key)
                    .unwrap_or("unknown")
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "session_id": current_session_id,
                "turn_id": self.current_turn_id,
                "action": action,
            })),
        })
    }
}

pub fn build_hook_action_resolved_notification(
    session_id: &str,
    turn_id: &str,
    action: &HookPendingAction,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let mut event = AgentDashEventV1::new("hook_action_resolved");
    event.severity = Some("info".to_string());
    event.message = Some(format!("Hook action `{}` 已显式结案", action.title));
    event.data = Some(serde_json::json!({
        "action_id": action.id,
        "action_type": action.action_type,
        "status": hook_action_status_key(action.status),
        "resolution_kind": action.resolution_kind.map(hook_action_resolution_key),
        "resolution_note": action.resolution_note,
        "resolution_turn_id": action.resolution_turn_id,
        "resolved_at_ms": action.resolved_at_ms,
        "summary": action.summary,
        "title": action.title,
    }));

    let source = AgentDashSourceV1::new("agentdash-hook-runtime", "runtime_tool");
    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace))
        .event(Some(event));

    SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(
            SessionInfoUpdate::new()
                .meta(merge_agentdash_meta(None, &agentdash).unwrap_or_default()),
        ),
    )
}

pub fn map_hook_action_resolution_kind(
    mode: HookActionResolutionMode,
) -> HookPendingActionResolutionKind {
    match mode {
        HookActionResolutionMode::Adopted => HookPendingActionResolutionKind::Adopted,
        HookActionResolutionMode::Rejected => HookPendingActionResolutionKind::Rejected,
        HookActionResolutionMode::Completed => HookPendingActionResolutionKind::Completed,
        HookActionResolutionMode::Superseded => HookPendingActionResolutionKind::Superseded,
        HookActionResolutionMode::UserDismissed => HookPendingActionResolutionKind::UserDismissed,
    }
}

pub fn hook_action_status_key(status: HookPendingActionStatus) -> &'static str {
    match status {
        HookPendingActionStatus::Pending => "pending",
        HookPendingActionStatus::Injected => "injected",
        HookPendingActionStatus::Resolved => "resolved",
        HookPendingActionStatus::Dismissed => "dismissed",
    }
}

pub fn hook_action_resolution_key(kind: HookPendingActionResolutionKind) -> &'static str {
    match kind {
        HookPendingActionResolutionKind::Adopted => "adopted",
        HookPendingActionResolutionKind::Rejected => "rejected",
        HookPendingActionResolutionKind::Completed => "completed",
        HookPendingActionResolutionKind::Superseded => "superseded",
        HookPendingActionResolutionKind::UserDismissed => "user_dismissed",
    }
}
