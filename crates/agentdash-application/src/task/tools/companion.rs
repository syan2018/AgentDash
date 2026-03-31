use std::sync::Arc;

use crate::session::{
    CompanionSessionContext, PromptSessionRequest, SessionHub, UserPromptInput,
    build_hook_trace_notification,
};
use agent_client_protocol::{
    McpServer, SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate,
};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};
use agentdash_domain::session_binding::{
    SessionBinding, SessionBindingRepository, SessionOwnerType,
};
use agentdash_spi::schema::schema_value;
use agentdash_spi::{
    AddressSpace, AgentConfig, ExecutionContext, HookEvaluationQuery, HookPendingAction,
    HookTraceEntry, HookTrigger, MountCapability, SessionHookRefreshQuery,
};
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::address_space::tools::provider::SharedSessionHubHandle;

#[derive(Clone)]
pub struct CompanionDispatchTool {
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    session_hub_handle: SharedSessionHubHandle,
    current_session_id: Option<String>,
    current_turn_id: String,
    current_executor_config: AgentConfig,
    workspace_root: std::path::PathBuf,
    working_dir: String,
    address_space: Option<AddressSpace>,
    mcp_servers: Vec<agent_client_protocol::McpServer>,
    hook_session: Option<agentdash_spi::hooks::SharedHookSessionRuntime>,
    system_context: Option<String>,
}

impl CompanionDispatchTool {
    pub fn new(
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        session_hub_handle: SharedSessionHubHandle,
        context: &ExecutionContext,
    ) -> Self {
        Self {
            session_binding_repo,
            session_hub_handle,
            current_session_id: context
                .hook_session
                .as_ref()
                .map(|session| session.session_id().to_string()),
            current_turn_id: context.turn_id.clone(),
            current_executor_config: context.executor_config.clone(),
            workspace_root: context.workspace_root.clone(),
            working_dir: relative_working_dir(context),
            address_space: context.address_space.clone(),
            mcp_servers: context.mcp_servers.clone(),
            hook_session: context.hook_session.clone(),
            system_context: context.system_context.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompanionSliceMode {
    #[default]
    Compact,
    Full,
    WorkflowOnly,
    ConstraintsOnly,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompanionAdoptionMode {
    #[default]
    Suggestion,
    FollowUpRequired,
    BlockingReview,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompanionDispatchParams {
    pub prompt: String,
    pub companion_label: Option<String>,
    pub title: Option<String>,
    pub auto_create: Option<bool>,
    pub wait_for_completion: Option<bool>,
    pub slice_mode: Option<CompanionSliceMode>,
    pub adoption_mode: Option<CompanionAdoptionMode>,
    pub max_fragments: Option<usize>,
    pub max_constraints: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CompanionDispatchSlice {
    pub mode: CompanionSliceMode,
    pub injections: Vec<agentdash_spi::HookInjection>,
    pub inherited_fragment_labels: Vec<String>,
    pub inherited_constraint_keys: Vec<String>,
    pub omitted_fragment_count: usize,
    pub omitted_constraint_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CompanionDispatchPlan {
    pub dispatch_id: String,
    pub companion_label: String,
    pub parent_session_id: String,
    pub parent_turn_id: String,
    pub adoption_mode: CompanionAdoptionMode,
    pub slice: CompanionDispatchSlice,
}

#[derive(Debug, Clone)]
pub struct CompanionExecutionSlice {
    pub address_space: Option<AddressSpace>,
    pub mcp_servers: Vec<McpServer>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompanionCompleteParams {
    pub summary: String,
    pub status: Option<String>,
    #[schemars(default)]
    pub findings: Vec<String>,
    #[schemars(default)]
    pub follow_ups: Vec<String>,
    #[schemars(default)]
    pub artifact_refs: Vec<String>,
}

#[derive(Clone)]
pub struct CompanionCompleteTool {
    session_hub_handle: SharedSessionHubHandle,
    current_session_id: Option<String>,
    current_turn_id: String,
}

impl CompanionCompleteTool {
    pub fn new(session_hub_handle: SharedSessionHubHandle, context: &ExecutionContext) -> Self {
        Self {
            session_hub_handle,
            current_session_id: context
                .hook_session
                .as_ref()
                .map(|session| session.session_id().to_string()),
            current_turn_id: context.turn_id.clone(),
        }
    }
}

#[async_trait]
impl AgentTool for CompanionDispatchTool {
    fn name(&self) -> &str {
        "companion_dispatch"
    }

    fn description(&self) -> &str {
        "把一个子任务派发到当前 owner 关联的 companion/subagent session，并返回派发结果"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CompanionDispatchParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let mut params: CompanionDispatchParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        if params.prompt.trim().is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "prompt 不能为空".to_string(),
            ));
        }
        if params.wait_for_completion.unwrap_or(false) {
            return Err(AgentToolError::InvalidArguments(
                "当前 companion_dispatch 仅支持异步派发，不支持 wait_for_completion=true"
                    .to_string(),
            ));
        }

        let current_session_id = self.current_session_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有可识别的 hook runtime，无法执行 companion dispatch".to_string(),
            )
        })?;
        let hook_session = self.hook_session.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前缺少 hook runtime，无法生成 companion dispatch 上下文".to_string(),
            )
        })?;
        let companion_label = params
            .companion_label
            .clone()
            .unwrap_or_else(|| "companion".to_string());
        let slice_mode = params.slice_mode.unwrap_or_default();
        let adoption_mode = params.adoption_mode.unwrap_or_default();

        let before_resolution = evaluate_subagent_hook(
            hook_session.as_ref(),
            HookTrigger::BeforeSubagentDispatch,
            Some(self.current_turn_id.clone()),
            &companion_label,
            Some(serde_json::json!({
                "prompt": params.prompt,
                "companion_label": companion_label,
                "auto_create": params.auto_create.unwrap_or(true),
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
            })),
        )
        .await
        .map_err(AgentToolError::ExecutionFailed)?;

        let session_hub = self.session_hub_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "SessionHub 尚未完成初始化，无法执行 companion dispatch".to_string(),
            )
        })?;

        if let Some(reason) = before_resolution.block_reason.clone() {
            record_subagent_trace(
                hook_session.as_ref(),
                Some(&session_hub),
                Some(self.current_turn_id.as_str()),
                HookTrigger::BeforeSubagentDispatch,
                "deny",
                &companion_label,
                &before_resolution,
            )
            .await;
            return Err(AgentToolError::ExecutionFailed(reason));
        }

        let dispatch_plan = build_companion_dispatch_plan(
            hook_session.as_ref(),
            &before_resolution,
            &CompanionDispatchConfig {
                parent_session_id: &current_session_id,
                parent_turn_id: &self.current_turn_id,
                companion_label: &companion_label,
                slice_mode,
                adoption_mode,
                max_fragments: params.max_fragments,
                max_constraints: params.max_constraints,
            },
        );
        record_subagent_trace(
            hook_session.as_ref(),
            Some(&session_hub),
            Some(self.current_turn_id.as_str()),
            HookTrigger::BeforeSubagentDispatch,
            "allow",
            &companion_label,
            &before_resolution,
        )
        .await;

        let target_binding = self
            .resolve_or_create_companion_binding(
                hook_session.as_ref(),
                &companion_label,
                params.auto_create.unwrap_or(true),
                params.title.take(),
            )
            .await?;
        if target_binding.session_id == current_session_id {
            return Err(AgentToolError::ExecutionFailed(
                "当前会话已经是目标 companion session，暂不允许向自身再次派发 companion"
                    .to_string(),
            ));
        }

        let companion_context = CompanionSessionContext {
            dispatch_id: dispatch_plan.dispatch_id.clone(),
            parent_session_id: current_session_id.clone(),
            parent_turn_id: self.current_turn_id.clone(),
            companion_label: companion_label.clone(),
            slice_mode: companion_slice_mode_key(slice_mode).to_string(),
            adoption_mode: companion_adoption_mode_key(adoption_mode).to_string(),
            inherited_fragment_labels: dispatch_plan.slice.inherited_fragment_labels.clone(),
            inherited_constraint_keys: dispatch_plan.slice.inherited_constraint_keys.clone(),
        };
        let _ = session_hub
            .update_session_meta(&target_binding.session_id, |meta| {
                meta.companion_context = Some(companion_context.clone());
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        let final_prompt = build_companion_dispatch_prompt(&dispatch_plan, &params.prompt);
        let execution_slice = build_companion_execution_slice(
            self.address_space.as_ref(),
            &self.mcp_servers,
            slice_mode,
        );
        let turn_id = session_hub
            .start_prompt_with_follow_up(
                &target_binding.session_id,
                None,
                PromptSessionRequest {
                    user_input: UserPromptInput {
                        prompt: Some(final_prompt),
                        prompt_blocks: None,
                        working_dir: Some(self.working_dir.clone()),
                        env: std::collections::HashMap::new(),
                        executor_config: Some(self.current_executor_config.clone()),
                    },
                    mcp_servers: execution_slice.mcp_servers.clone(),
                    workspace_root: Some(self.workspace_root.clone()),
                    address_space: execution_slice.address_space.clone(),
                    flow_capabilities: None,
                    system_context: self.system_context.clone(),
                },
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        let child_notification = build_companion_event_notification(
            &target_binding.session_id,
            &turn_id,
            "companion_dispatch_registered",
            format!("收到来自主 session 的 `{companion_label}` 派发任务"),
            serde_json::json!({
                "dispatch_id": dispatch_plan.dispatch_id,
                "parent_session_id": current_session_id,
                "parent_turn_id": self.current_turn_id,
                "companion_label": companion_label,
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "inherited_fragment_labels": dispatch_plan.slice.inherited_fragment_labels,
                "inherited_constraint_keys": dispatch_plan.slice.inherited_constraint_keys,
                "inherited_mount_ids": execution_slice.address_space.as_ref().map(|space| {
                    space.mounts.iter().map(|mount| mount.id.clone()).collect::<Vec<_>>()
                }).unwrap_or_default(),
                "mcp_server_count": execution_slice.mcp_servers.len(),
            }),
        );
        let _ = session_hub
            .inject_notification(&target_binding.session_id, child_notification)
            .await;

        let after_resolution = evaluate_subagent_hook(
            hook_session.as_ref(),
            HookTrigger::AfterSubagentDispatch,
            Some(self.current_turn_id.clone()),
            &companion_label,
            Some(serde_json::json!({
                "dispatch_id": dispatch_plan.dispatch_id,
                "companion_session_id": target_binding.session_id,
                "turn_id": turn_id,
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "fragment_count": dispatch_plan.slice.injections.iter().filter(|i| i.slot != "constraint").count(),
                "constraint_count": dispatch_plan.slice.injections.iter().filter(|i| i.slot == "constraint").count(),
            })),
        )
        .await
        .map_err(AgentToolError::ExecutionFailed)?;
        record_subagent_trace(
            hook_session.as_ref(),
            Some(&session_hub),
            Some(self.current_turn_id.as_str()),
            HookTrigger::AfterSubagentDispatch,
            "dispatched",
            &companion_label,
            &after_resolution,
        )
        .await;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已派发到 companion session。\n- label: {}\n- session_id: {}\n- turn_id: {}\n- slice_mode: {:?}\n- adoption_mode: {:?}\n- 当前为异步执行，可在对应会话中继续观察结果，并要求其通过 companion_complete 回传结果。",
                companion_label, target_binding.session_id, turn_id, slice_mode, adoption_mode
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "companion_label": companion_label,
                "companion_session_id": target_binding.session_id,
                "turn_id": turn_id,
                "dispatch_id": dispatch_plan.dispatch_id,
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "inherited_fragment_labels": dispatch_plan.slice.inherited_fragment_labels,
                "inherited_constraint_keys": dispatch_plan.slice.inherited_constraint_keys,
                "inherited_mount_ids": execution_slice.address_space.as_ref().map(|space| {
                    space.mounts.iter().map(|mount| mount.id.clone()).collect::<Vec<_>>()
                }).unwrap_or_default(),
                "mcp_server_count": execution_slice.mcp_servers.len(),
                "matched_rule_keys": after_resolution.matched_rule_keys,
            })),
        })
    }
}

impl CompanionDispatchTool {
    async fn resolve_or_create_companion_binding(
        &self,
        hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
        label: &str,
        auto_create: bool,
        title: Option<String>,
    ) -> Result<SessionBinding, AgentToolError> {
        let snapshot = hook_session.snapshot();
        let candidates = companion_owner_candidates(&snapshot)?;
        for (owner_type, owner_id, _) in &candidates {
            if let Some(binding) = self
                .session_binding_repo
                .find_by_owner_and_label(*owner_type, *owner_id, label)
                .await
                .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            {
                return Ok(binding);
            }
        }

        if !auto_create {
            return Err(AgentToolError::ExecutionFailed(format!(
                "当前 owner 还没有 label=`{label}` 的 companion session，且 auto_create=false"
            )));
        }

        let (owner_type, owner_id, owner_title) = candidates.first().cloned().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有关联 owner，无法创建 companion session".to_string(),
            )
        })?;
        let project_id = companion_project_id_for_owner(&snapshot, owner_type, owner_id)?;
        let session_hub = self.session_hub_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "SessionHub 尚未完成初始化，无法创建 companion session".to_string(),
            )
        })?;
        let meta = session_hub
            .create_session(
                title
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| owner_title.as_deref().unwrap_or("Companion Session")),
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        let binding =
            SessionBinding::new(project_id, meta.id, owner_type, owner_id, label.to_string());
        self.session_binding_repo
            .create(&binding)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        Ok(binding)
    }
}

#[async_trait]
impl AgentTool for CompanionCompleteTool {
    fn name(&self) -> &str {
        "companion_complete"
    }

    fn description(&self) -> &str {
        "把当前 companion session 的结构化结果回传给主 session，供主 Agent 采纳或继续推进"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CompanionCompleteParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: CompanionCompleteParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        if params.summary.trim().is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "summary 不能为空".to_string(),
            ));
        }

        let current_session_id = self.current_session_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有可识别的上下文，无法回传 companion 结果".to_string(),
            )
        })?;
        let session_hub = self.session_hub_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "SessionHub 尚未完成初始化，无法回传 companion 结果".to_string(),
            )
        })?;
        let session_meta = session_hub
            .get_session_meta(&current_session_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "当前 session 不存在，无法回传 companion 结果".to_string(),
                )
            })?;
        let companion_context = session_meta.companion_context.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 不是通过 companion_dispatch 建立的上下文，无法使用 companion_complete".to_string(),
            )
        })?;

        let status = normalize_companion_result_status(params.status.as_deref())?;
        let payload = serde_json::json!({
            "dispatch_id": companion_context.dispatch_id,
            "companion_label": companion_context.companion_label,
            "companion_session_id": current_session_id,
            "companion_turn_id": self.current_turn_id,
            "parent_session_id": companion_context.parent_session_id,
            "parent_turn_id": companion_context.parent_turn_id,
            "slice_mode": companion_context.slice_mode,
            "adoption_mode": companion_context.adoption_mode,
            "status": status,
            "summary": params.summary.trim(),
            "findings": params.findings,
            "follow_ups": params.follow_ups,
            "artifact_refs": params.artifact_refs,
        });

        if let Some(parent_hook_session) = session_hub
            .ensure_hook_session_runtime(
                &companion_context.parent_session_id,
                Some(companion_context.parent_turn_id.as_str()),
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
        {
            let resolution = evaluate_subagent_hook(
                parent_hook_session.as_ref(),
                HookTrigger::SubagentResult,
                Some(companion_context.parent_turn_id.clone()),
                &companion_context.companion_label,
                Some(payload.clone()),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
            if let Some(action) = build_subagent_pending_action(
                &companion_context.parent_turn_id,
                &companion_context.companion_label,
                &payload,
                &resolution,
            ) {
                parent_hook_session.enqueue_pending_action(action);
            }
            record_subagent_trace(
                parent_hook_session.as_ref(),
                Some(&session_hub),
                Some(companion_context.parent_turn_id.as_str()),
                HookTrigger::SubagentResult,
                "result_returned",
                &companion_context.companion_label,
                &resolution,
            )
            .await;
        }

        let parent_notification = build_companion_event_notification(
            &companion_context.parent_session_id,
            &companion_context.parent_turn_id,
            "companion_result_available",
            format!(
                "Companion `{}` 已回传结果，等待主 session 采纳",
                companion_context.companion_label
            ),
            payload.clone(),
        );
        session_hub
            .inject_notification(&companion_context.parent_session_id, parent_notification)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        let child_notification = build_companion_event_notification(
            &current_session_id,
            &self.current_turn_id,
            "companion_result_returned",
            "已将当前 companion 结果回传到主 session".to_string(),
            payload.clone(),
        );
        let _ = session_hub
            .inject_notification(&current_session_id, child_notification)
            .await;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已把 companion 结果回传到主 session。\n- parent_session_id: {}\n- dispatch_id: {}\n- status: {}",
                companion_context.parent_session_id, companion_context.dispatch_id, status
            ))],
            is_error: false,
            details: Some(payload),
        })
    }
}

pub fn relative_working_dir(context: &ExecutionContext) -> String {
    context
        .working_directory
        .strip_prefix(&context.workspace_root)
        .ok()
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                ".".to_string()
            } else {
                relative.to_string_lossy().replace('\\', "/")
            }
        })
        .unwrap_or_else(|| ".".to_string())
}

async fn evaluate_subagent_hook(
    hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
    trigger: HookTrigger,
    turn_id: Option<String>,
    subagent_type: &str,
    payload: Option<serde_json::Value>,
) -> Result<agentdash_spi::HookResolution, String> {
    let resolution = hook_session
        .evaluate(HookEvaluationQuery {
            session_id: hook_session.session_id().to_string(),
            trigger: trigger.clone(),
            turn_id: turn_id.clone(),
            tool_name: None,
            tool_call_id: None,
            subagent_type: Some(subagent_type.to_string()),
            snapshot: Some(hook_session.snapshot()),
            payload,
        })
        .await
        .map_err(|error| error.to_string())?;

    if resolution.refresh_snapshot {
        hook_session
            .refresh(SessionHookRefreshQuery {
                session_id: hook_session.session_id().to_string(),
                turn_id,
                reason: Some(format!("trigger:{trigger:?}:{subagent_type}")),
            })
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(resolution)
}

async fn record_subagent_trace(
    hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
    session_hub: Option<&SessionHub>,
    turn_id: Option<&str>,
    trigger: HookTrigger,
    decision: &str,
    subagent_type: &str,
    resolution: &agentdash_spi::HookResolution,
) {
    let trace = HookTraceEntry {
        sequence: hook_session.next_trace_sequence(),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        revision: hook_session.revision(),
        trigger,
        decision: decision.to_string(),
        tool_name: None,
        tool_call_id: None,
        subagent_type: Some(subagent_type.to_string()),
        matched_rule_keys: resolution.matched_rule_keys.clone(),
        refresh_snapshot: resolution.refresh_snapshot,
        block_reason: resolution.block_reason.clone(),
        completion: resolution.completion.clone(),
        diagnostics: resolution.diagnostics.clone(),
    };
    hook_session.append_trace(trace.clone());

    if let (Some(session_hub), Some(turn_id)) = (session_hub, turn_id)
        && let Some(notification) = build_hook_trace_notification(
            hook_session.session_id(),
            Some(turn_id),
            hook_trace_source(),
            &trace,
        )
    {
        let _ = session_hub
            .inject_notification(hook_session.session_id(), notification)
            .await;
    }
}

fn build_subagent_pending_action(
    parent_turn_id: &str,
    companion_label: &str,
    payload: &serde_json::Value,
    resolution: &agentdash_spi::HookResolution,
) -> Option<HookPendingAction> {
    if resolution.injections.is_empty() {
        return None;
    }

    let adoption_mode = payload
        .get("adoption_mode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("suggestion")
        .trim()
        .to_string();
    if adoption_mode.is_empty() || adoption_mode == "suggestion" {
        return None;
    }

    let summary = payload
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Companion 已回流结果")
        .trim()
        .to_string();
    let status = payload
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("completed")
        .trim()
        .to_string();
    let dispatch_id = payload
        .get("dispatch_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");

    Some(HookPendingAction {
        id: format!("{adoption_mode}:{dispatch_id}:{parent_turn_id}"),
        created_at_ms: chrono::Utc::now().timestamp_millis(),
        title: if adoption_mode == "blocking_review" {
            format!("Companion `{companion_label}` 结果需要阻塞式 review")
        } else {
            format!("Companion `{companion_label}` 结果需要主 session 跟进")
        },
        summary: format!("status={status}, dispatch_id={dispatch_id}, summary={summary}"),
        action_type: adoption_mode,
        turn_id: Some(parent_turn_id.to_string()),
        source_trigger: HookTrigger::SubagentResult,
        status: agentdash_spi::HookPendingActionStatus::Pending,
        last_injected_at_ms: None,
        resolved_at_ms: None,
        resolution_kind: None,
        resolution_note: None,
        resolution_turn_id: None,
        injections: resolution.injections.clone(),
    })
}

fn hook_trace_source() -> AgentDashSourceV1 {
    let mut source = AgentDashSourceV1::new("pi-agent", "runtime_tool");
    source.executor_id = Some("PI_AGENT".to_string());
    source
}

pub fn build_companion_dispatch_prompt(plan: &CompanionDispatchPlan, user_prompt: &str) -> String {
    let mut sections = vec!["[Companion Dispatch Context]".to_string()];

    sections.push(format!(
        "## Dispatch Metadata\n- dispatch_id: {}\n- companion_label: {}\n- slice_mode: {:?}\n- adoption_mode: {:?}",
        plan.dispatch_id, plan.companion_label, plan.slice.mode, plan.adoption_mode
    ));

    let context_injections: Vec<_> = plan
        .slice
        .injections
        .iter()
        .filter(|i| i.slot != "constraint")
        .collect();
    let constraint_injections: Vec<_> = plan
        .slice
        .injections
        .iter()
        .filter(|i| i.slot == "constraint")
        .collect();

    if !context_injections.is_empty() {
        sections.push(format!(
            "## 继承上下文\n{}",
            context_injections
                .iter()
                .map(|injection| format!("### {}\n{}", injection.source, injection.content.trim()))
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    }

    if !constraint_injections.is_empty() {
        sections.push(format!(
            "## 继承约束\n{}",
            constraint_injections
                .iter()
                .map(|injection| format!("- {}", injection.content))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    if plan.slice.omitted_fragment_count > 0 || plan.slice.omitted_constraint_count > 0 {
        sections.push(format!(
            "## 切片说明\n- omitted_fragments: {}\n- omitted_constraints: {}",
            plan.slice.omitted_fragment_count, plan.slice.omitted_constraint_count
        ));
    }

    sections.push(format!("## 派发任务\n{}", user_prompt.trim()));
    sections.push(
        "## 回流要求\n- 完成后请调用 `companion_complete`。\n- 必填 summary。\n- 如有关键发现请写入 findings。\n- 如需要主 session 后续行动请写入 follow_ups。".to_string(),
    );
    sections.join("\n\n")
}

struct CompanionDispatchConfig<'a> {
    parent_session_id: &'a str,
    parent_turn_id: &'a str,
    companion_label: &'a str,
    slice_mode: CompanionSliceMode,
    adoption_mode: CompanionAdoptionMode,
    max_fragments: Option<usize>,
    max_constraints: Option<usize>,
}

fn build_companion_dispatch_plan(
    hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
    resolution: &agentdash_spi::HookResolution,
    config: &CompanionDispatchConfig<'_>,
) -> CompanionDispatchPlan {
    let dispatch_id = format!("dispatch-{}", uuid::Uuid::new_v4().simple());
    let slice = build_companion_dispatch_slice(
        &hook_session.snapshot(),
        resolution,
        config.slice_mode,
        config.max_fragments.unwrap_or(3),
        config.max_constraints.unwrap_or(4),
    );
    CompanionDispatchPlan {
        dispatch_id,
        companion_label: config.companion_label.to_string(),
        parent_session_id: config.parent_session_id.to_string(),
        parent_turn_id: config.parent_turn_id.to_string(),
        adoption_mode: config.adoption_mode,
        slice,
    }
}

pub fn build_companion_dispatch_slice(
    snapshot: &agentdash_spi::SessionHookSnapshot,
    resolution: &agentdash_spi::HookResolution,
    mode: CompanionSliceMode,
    max_fragments: usize,
    max_constraints: usize,
) -> CompanionDispatchSlice {
    let context_injections: Vec<_> = resolution
        .injections
        .iter()
        .filter(|i| i.slot != "constraint")
        .cloned()
        .collect();
    let constraint_injections: Vec<_> = resolution
        .injections
        .iter()
        .filter(|i| i.slot == "constraint")
        .cloned()
        .collect();

    let all_context = match mode {
        CompanionSliceMode::Full => context_injections.clone(),
        CompanionSliceMode::WorkflowOnly => context_injections
            .iter()
            .filter(|injection| {
                injection.slot == "workflow"
                    || injection.source.contains("workflow")
                    || injection.source.contains("workflow:")
            })
            .cloned()
            .collect(),
        CompanionSliceMode::ConstraintsOnly => Vec::new(),
        CompanionSliceMode::Compact => {
            let mut compact = Vec::new();
            if let Some(owner_summary) = build_companion_owner_summary(snapshot) {
                compact.push(agentdash_spi::HookInjection {
                    slot: "companion".to_string(),
                    content: owner_summary,
                    source: "session:owner_summary".to_string(),
                });
            }
            compact.extend(
                context_injections
                    .iter()
                    .filter(|injection| {
                        injection.slot == "workflow" || injection.source.contains("workflow")
                    })
                    .take(1)
                    .cloned(),
            );
            compact.extend(
                context_injections
                    .iter()
                    .filter(|injection| injection.slot == "instruction_append")
                    .take(1)
                    .cloned(),
            );
            compact
        }
    };

    let all_constraints = match mode {
        CompanionSliceMode::ConstraintsOnly
        | CompanionSliceMode::Full
        | CompanionSliceMode::Compact => constraint_injections.clone(),
        CompanionSliceMode::WorkflowOnly => constraint_injections
            .iter()
            .filter(|injection| injection.source.contains("workflow:"))
            .cloned()
            .collect(),
    };

    let kept_context: Vec<_> = all_context
        .iter()
        .take(max_fragments.max(1))
        .cloned()
        .collect();
    let kept_constraints: Vec<_> = all_constraints
        .iter()
        .take(max_constraints.max(1))
        .cloned()
        .collect();

    let inherited_fragment_labels: Vec<String> = kept_context
        .iter()
        .map(|injection| injection.source.clone())
        .collect();
    let inherited_constraint_keys: Vec<String> = kept_constraints
        .iter()
        .map(|injection| injection.source.clone())
        .collect();
    let omitted_fragment_count = all_context.len().saturating_sub(kept_context.len());
    let omitted_constraint_count = all_constraints.len().saturating_sub(kept_constraints.len());

    let mut injections = kept_context;
    injections.extend(kept_constraints);

    CompanionDispatchSlice {
        mode,
        inherited_fragment_labels,
        inherited_constraint_keys,
        omitted_fragment_count,
        omitted_constraint_count,
        injections,
    }
}

pub fn build_companion_execution_slice(
    address_space: Option<&AddressSpace>,
    mcp_servers: &[McpServer],
    mode: CompanionSliceMode,
) -> CompanionExecutionSlice {
    match mode {
        CompanionSliceMode::Full => CompanionExecutionSlice {
            address_space: address_space.cloned(),
            mcp_servers: mcp_servers.to_vec(),
        },
        CompanionSliceMode::Compact => CompanionExecutionSlice {
            address_space: Some(filter_address_space_capabilities(
                address_space,
                &[
                    MountCapability::Read,
                    MountCapability::List,
                    MountCapability::Search,
                    MountCapability::Exec,
                ],
            )),
            mcp_servers: Vec::new(),
        },
        CompanionSliceMode::WorkflowOnly | CompanionSliceMode::ConstraintsOnly => {
            CompanionExecutionSlice {
                address_space: Some(AddressSpace::default()),
                mcp_servers: Vec::new(),
            }
        }
    }
}

fn filter_address_space_capabilities(
    address_space: Option<&AddressSpace>,
    allowed: &[MountCapability],
) -> AddressSpace {
    let Some(address_space) = address_space else {
        return AddressSpace::default();
    };

    let mounts = address_space
        .mounts
        .iter()
        .filter_map(|mount| {
            let capabilities = mount
                .capabilities
                .iter()
                .filter(|capability| allowed.contains(capability))
                .cloned()
                .collect::<Vec<_>>();
            if capabilities.is_empty() {
                return None;
            }

            let mut next_mount = mount.clone();
            next_mount.capabilities = capabilities;
            next_mount.default_write = next_mount.capabilities.contains(&MountCapability::Write);
            Some(next_mount)
        })
        .collect::<Vec<_>>();

    let default_mount_id = address_space
        .default_mount_id
        .as_ref()
        .and_then(|default_id| {
            mounts
                .iter()
                .any(|mount| mount.id == *default_id)
                .then(|| default_id.clone())
        });

    AddressSpace {
        mounts,
        default_mount_id,
        source_project_id: address_space.source_project_id.clone(),
        source_story_id: address_space.source_story_id.clone(),
    }
}

fn build_companion_owner_summary(snapshot: &agentdash_spi::SessionHookSnapshot) -> Option<String> {
    if snapshot.owners.is_empty() {
        return None;
    }
    Some(format!(
        "## 当前归属\n{}",
        snapshot
            .owners
            .iter()
            .map(|owner| format!(
                "- {}: {}",
                owner.owner_type,
                owner.label.as_deref().unwrap_or(owner.owner_id.as_str())
            ))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

fn build_companion_event_notification(
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    message: String,
    data: serde_json::Value,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let mut event = AgentDashEventV1::new(event_type);
    event.severity = Some("info".to_string());
    event.message = Some(message);
    event.data = Some(data);

    let source = AgentDashSourceV1::new("agentdash-companion", "runtime_tool");
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

fn normalize_companion_result_status(status: Option<&str>) -> Result<&str, AgentToolError> {
    match status.unwrap_or("completed").trim() {
        "" => Ok("completed"),
        "completed" => Ok("completed"),
        "blocked" => Ok("blocked"),
        "needs_follow_up" => Ok("needs_follow_up"),
        other => Err(AgentToolError::InvalidArguments(format!(
            "status 仅支持 completed / blocked / needs_follow_up，收到 `{other}`"
        ))),
    }
}

fn companion_slice_mode_key(mode: CompanionSliceMode) -> &'static str {
    match mode {
        CompanionSliceMode::Compact => "compact",
        CompanionSliceMode::Full => "full",
        CompanionSliceMode::WorkflowOnly => "workflow_only",
        CompanionSliceMode::ConstraintsOnly => "constraints_only",
    }
}

fn companion_adoption_mode_key(mode: CompanionAdoptionMode) -> &'static str {
    match mode {
        CompanionAdoptionMode::Suggestion => "suggestion",
        CompanionAdoptionMode::FollowUpRequired => "follow_up_required",
        CompanionAdoptionMode::BlockingReview => "blocking_review",
    }
}

pub fn companion_owner_candidates(
    snapshot: &agentdash_spi::SessionHookSnapshot,
) -> Result<Vec<(SessionOwnerType, Uuid, Option<String>)>, AgentToolError> {
    let mut owners = Vec::new();
    for owner in &snapshot.owners {
        if let Some(candidate) = parse_owner_candidate(
            owner.owner_type.as_str(),
            &owner.owner_id,
            owner.label.clone(),
        )? {
            owners.push(candidate);
        }
        if owner.owner_type == "task"
            && let Some(story_id) = owner.story_id.as_deref()
            && let Some(candidate) = parse_owner_candidate("story", story_id, owner.label.clone())?
        {
            owners.push(candidate);
        }
    }
    owners.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    Ok(owners)
}

fn parse_owner_candidate(
    owner_type: &str,
    owner_id: &str,
    label: Option<String>,
) -> Result<Option<(SessionOwnerType, Uuid, Option<String>)>, AgentToolError> {
    let owner_type = match owner_type {
        "project" => SessionOwnerType::Project,
        "story" => SessionOwnerType::Story,
        "task" => SessionOwnerType::Task,
        _ => return Ok(None),
    };
    let owner_id = Uuid::parse_str(owner_id).map_err(|error| {
        AgentToolError::ExecutionFailed(format!("owner_id 不是有效 UUID: {error}"))
    })?;
    Ok(Some((owner_type, owner_id, label)))
}

fn companion_project_id_for_owner(
    snapshot: &agentdash_spi::SessionHookSnapshot,
    owner_type: SessionOwnerType,
    owner_id: Uuid,
) -> Result<Uuid, AgentToolError> {
    let owner_id_raw = owner_id.to_string();
    let matching_owner = snapshot
        .owners
        .iter()
        .find(|owner| owner.owner_type == owner_type.to_string() && owner.owner_id == owner_id_raw)
        .ok_or_else(|| {
            AgentToolError::ExecutionFailed("当前 session owner 缺少 project 范围信息".to_string())
        })?;

    match owner_type {
        SessionOwnerType::Project => Ok(owner_id),
        SessionOwnerType::Story | SessionOwnerType::Task => matching_owner
            .project_id
            .as_deref()
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "当前 session owner 缺少 project_id，无法创建 companion session".to_string(),
                )
            })
            .and_then(|project_id| {
                Uuid::parse_str(project_id).map_err(|error| {
                    AgentToolError::ExecutionFailed(format!(
                        "owner.project_id 不是有效 UUID: {error}"
                    ))
                })
            }),
    }
}

#[cfg(test)]
mod companion_tests {
    use super::{
        CompanionAdoptionMode, CompanionDispatchPlan, CompanionDispatchSlice, CompanionSliceMode,
        build_companion_dispatch_prompt, build_companion_dispatch_slice,
        build_companion_execution_slice, companion_owner_candidates,
    };
    use agent_client_protocol::McpServer;
    use agentdash_domain::session_binding::SessionOwnerType;
    use agentdash_spi::{AddressSpace, MountCapability};
    use uuid::Uuid;

    #[test]
    fn companion_owner_candidates_fallback_from_task_to_story() {
        let story_id = Uuid::new_v4();
        let snapshot = agentdash_spi::SessionHookSnapshot {
            session_id: "sess-test".to_string(),
            owners: vec![agentdash_spi::HookOwnerSummary {
                owner_type: "task".to_string(),
                owner_id: Uuid::new_v4().to_string(),
                label: Some("Task A".to_string()),
                project_id: None,
                story_id: Some(story_id.to_string()),
                task_id: None,
            }],
            ..agentdash_spi::SessionHookSnapshot::default()
        };

        let candidates = companion_owner_candidates(&snapshot).expect("candidates");

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].0, SessionOwnerType::Task);
        assert_eq!(candidates[1].0, SessionOwnerType::Story);
        assert_eq!(candidates[1].1, story_id);
    }

    #[test]
    fn compact_companion_slice_keeps_owner_summary_and_limits_payload() {
        let snapshot = agentdash_spi::SessionHookSnapshot {
            session_id: "sess-parent".to_string(),
            owners: vec![agentdash_spi::HookOwnerSummary {
                owner_type: "task".to_string(),
                owner_id: Uuid::new_v4().to_string(),
                label: Some("Task A".to_string()),
                project_id: None,
                story_id: None,
                task_id: None,
            }],
            ..agentdash_spi::SessionHookSnapshot::default()
        };
        let resolution = agentdash_spi::HookResolution {
            injections: vec![
                agentdash_spi::HookInjection {
                    slot: "workflow".to_string(),
                    content: "step info".to_string(),
                    source: "active_workflow_step".to_string(),
                },
                agentdash_spi::HookInjection {
                    slot: "instruction_append".to_string(),
                    content: "follow rules".to_string(),
                    source: "workflow_step_constraints".to_string(),
                },
                agentdash_spi::HookInjection {
                    slot: "workflow".to_string(),
                    content: "should be omitted".to_string(),
                    source: "overflow".to_string(),
                },
                agentdash_spi::HookInjection {
                    slot: "constraint".to_string(),
                    content: "first".to_string(),
                    source: "constraint:1".to_string(),
                },
                agentdash_spi::HookInjection {
                    slot: "constraint".to_string(),
                    content: "second".to_string(),
                    source: "constraint:2".to_string(),
                },
            ],
            ..agentdash_spi::HookResolution::default()
        };

        let slice = build_companion_dispatch_slice(
            &snapshot,
            &resolution,
            CompanionSliceMode::Compact,
            2,
            1,
        );

        let context_count = slice
            .injections
            .iter()
            .filter(|i| i.slot != "constraint")
            .count();
        let constraint_count = slice
            .injections
            .iter()
            .filter(|i| i.slot == "constraint")
            .count();
        assert_eq!(context_count, 2);
        assert_eq!(constraint_count, 1);
        assert_eq!(slice.omitted_fragment_count, 1);
        assert_eq!(slice.omitted_constraint_count, 1);
        assert_eq!(slice.inherited_fragment_labels[0], "session:owner_summary");
    }

    #[test]
    fn compact_execution_slice_drops_write_and_mcp_servers() {
        let address_space = AddressSpace {
            mounts: vec![agentdash_spi::Mount {
                id: "main".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-1".to_string(),
                root_ref: "/workspace".to_string(),
                capabilities: vec![
                    MountCapability::Read,
                    MountCapability::Write,
                    MountCapability::List,
                    MountCapability::Search,
                    MountCapability::Exec,
                ],
                default_write: true,
                display_name: "main".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            ..Default::default()
        };

        let slice = build_companion_execution_slice(
            Some(&address_space),
            &[McpServer::Stdio(
                agent_client_protocol::McpServerStdio::new("test-mcp", "cmd"),
            )],
            CompanionSliceMode::Compact,
        );

        let sliced_space = slice
            .address_space
            .expect("compact should keep sliced address_space");
        assert_eq!(slice.mcp_servers.len(), 0);
        assert_eq!(sliced_space.mounts.len(), 1);
        assert!(
            !sliced_space.mounts[0]
                .capabilities
                .contains(&MountCapability::Write)
        );
        assert!(
            sliced_space.mounts[0]
                .capabilities
                .contains(&MountCapability::Exec)
        );
        assert!(!sliced_space.mounts[0].default_write);
    }

    #[test]
    fn workflow_only_execution_slice_uses_empty_address_space() {
        let address_space = AddressSpace {
            mounts: vec![agentdash_spi::Mount {
                id: "main".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-1".to_string(),
                root_ref: "/workspace".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::Write],
                default_write: true,
                display_name: "main".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            ..Default::default()
        };

        let slice = build_companion_execution_slice(
            Some(&address_space),
            &[McpServer::Stdio(
                agent_client_protocol::McpServerStdio::new("test-mcp", "cmd"),
            )],
            CompanionSliceMode::WorkflowOnly,
        );

        let sliced_space = slice
            .address_space
            .expect("workflow_only should force empty address_space");
        assert!(sliced_space.mounts.is_empty());
        assert!(sliced_space.default_mount_id.is_none());
        assert!(slice.mcp_servers.is_empty());
    }

    #[test]
    fn companion_dispatch_prompt_includes_return_instruction() {
        let plan = CompanionDispatchPlan {
            dispatch_id: "dispatch-1".to_string(),
            companion_label: "companion".to_string(),
            parent_session_id: "sess-parent".to_string(),
            parent_turn_id: "turn-parent-1".to_string(),
            adoption_mode: CompanionAdoptionMode::BlockingReview,
            slice: CompanionDispatchSlice {
                mode: CompanionSliceMode::Compact,
                injections: vec![
                    agentdash_spi::HookInjection {
                        slot: "workflow".to_string(),
                        content: "step info".to_string(),
                        source: "active_workflow_step".to_string(),
                    },
                    agentdash_spi::HookInjection {
                        slot: "constraint".to_string(),
                        content: "first".to_string(),
                        source: "constraint:1".to_string(),
                    },
                ],
                inherited_fragment_labels: vec!["active_workflow_step".to_string()],
                inherited_constraint_keys: vec!["constraint:1".to_string()],
                omitted_fragment_count: 0,
                omitted_constraint_count: 0,
            },
        };

        let prompt = build_companion_dispatch_prompt(&plan, "请帮我 review 当前实现");

        assert!(prompt.contains("companion_complete"));
        assert!(prompt.contains("dispatch_id: dispatch-1"));
        assert!(prompt.contains("请帮我 review 当前实现"));
    }
}
