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
use agentdash_domain::agent::{AgentRepository, ProjectAgentLinkRepository};
use agentdash_domain::session_binding::{
    SessionBinding, SessionBindingRepository, SessionOwnerType,
};
use agentdash_spi::action_type as at;
use agentdash_spi::schema::schema_value;
use agentdash_spi::{
    AgentConfig, ExecutionContext, HookEvaluationQuery, HookPendingAction,
    HookPendingActionResolutionKind, HookPendingActionStatus, HookTraceEntry, HookTrigger,
    MountCapability, SessionHookRefreshQuery, Vfs,
};
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::vfs::tools::provider::SharedSessionHubHandle;

// ─── 公共枚举（保留不变，内部逻辑使用） ────────────────────────────────

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

// ─── companion_request ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompanionRequestTarget {
    Sub,
    Parent,
    Human,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompanionRequestParams {
    /// 发给谁：sub（子 agent）、parent（父 agent）、human（用户）
    pub target: CompanionRequestTarget,
    /// 是否期望等待对方回应（创建 follow_up_required pending action，是否阻塞由 workflow 决定）
    #[serde(default)]
    pub wait: bool,
    /// JSON 字符串格式的 payload，内容由 target 决定。示例：{"type":"task","prompt":"...","label":"reviewer"}
    pub payload: String,
}

#[derive(Clone)]
pub struct CompanionRequestTool {
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    agent_repo: Arc<dyn AgentRepository>,
    agent_link_repo: Arc<dyn ProjectAgentLinkRepository>,
    repos: crate::repository_set::RepositorySet,
    platform_config: crate::platform_config::SharedPlatformConfig,
    session_hub_handle: SharedSessionHubHandle,
    current_session_id: Option<String>,
    current_turn_id: String,
    current_executor_config: AgentConfig,
    working_dir: String,
    vfs: Option<Vfs>,
    mcp_servers: Vec<agent_client_protocol::McpServer>,
    hook_session: Option<agentdash_spi::hooks::SharedHookSessionRuntime>,
    system_context: Option<String>,
}

impl CompanionRequestTool {
    pub fn new(
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        agent_repo: Arc<dyn AgentRepository>,
        agent_link_repo: Arc<dyn ProjectAgentLinkRepository>,
        repos: crate::repository_set::RepositorySet,
        platform_config: crate::platform_config::SharedPlatformConfig,
        session_hub_handle: SharedSessionHubHandle,
        context: &ExecutionContext,
    ) -> Self {
        Self {
            session_binding_repo,
            agent_repo,
            agent_link_repo,
            repos,
            platform_config,
            session_hub_handle,
            current_session_id: context
                .hook_session
                .as_ref()
                .map(|session| session.session_id().to_string()),
            current_turn_id: context.turn_id.clone(),
            current_executor_config: context.executor_config.clone(),
            working_dir: relative_working_dir(context),
            vfs: context.vfs.clone(),
            mcp_servers: context.mcp_servers.clone(),
            hook_session: context.hook_session.clone(),
            system_context: context.system_context.clone(),
        }
    }
}

#[async_trait]
impl AgentTool for CompanionRequestTool {
    fn name(&self) -> &str {
        "companion_request"
    }

    fn description(&self) -> &str {
        "向 companion 信道发起请求。target 决定方向（sub/parent/human），wait 决定是否暂停等回应，payload 为自由结构。\n\n\
         payload 填写约定：\n\
         ▸ target=sub 派发子任务：{\"type\":\"task\", \"prompt\":\"...\", \"label\":\"companion\", \"context_mode\":\"compact\", \"agent_key\":\"<agent name>\"}\n\
         ▸ target=parent 向上提审：{\"type\":\"review\", \"prompt\":\"...\"}\n\
         ▸ target=human 问用户：{\"type\":\"approval\", \"prompt\":\"...\", \"options\":[\"A\",\"B\"]}\n\
         ▸ target=human 通知：{\"type\":\"notification\", \"message\":\"...\"}\n\n\
         agent_key（仅 target=sub）：可选，指定执行子任务的 agent 名称（如 \"code-reviewer\"），必须是当前项目已关联的 agent。\
         不指定则使用当前会话的执行器配置。可用 agent 列表见系统上下文中的 Companion Agents 章节。\n\n\
         workflow_key（仅 target=sub）：可选，为子 agent 分配一个 workflow。指定后子 session 会自动创建 lifecycle run 并获得 workflow 的 port 门禁、能力指令和上下文注入。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CompanionRequestParams>()
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let raw: CompanionRequestParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;

        let payload: serde_json::Value = serde_json::from_str(&raw.payload)
            .map_err(|e| AgentToolError::InvalidArguments(format!("payload 不是合法 JSON: {e}")))?;

        // payload type 校验
        let registry = super::payload_types::PayloadTypeRegistry::with_builtins();
        if let Some(error) = registry.validate_request(&payload) {
            return Err(AgentToolError::InvalidArguments(error));
        }

        match raw.target {
            CompanionRequestTarget::Sub => {
                self.execute_sub_request(
                    raw.target,
                    raw.wait,
                    &payload,
                    tool_call_id,
                    cancel,
                    on_update,
                )
                .await
            }
            CompanionRequestTarget::Parent => {
                self.execute_parent_request(raw.wait, &payload, cancel)
                    .await
            }
            CompanionRequestTarget::Human => {
                self.execute_human_request(raw.wait, &payload, cancel).await
            }
        }
    }
}

impl CompanionRequestTool {
    /// target=sub 的执行逻辑，复用现有 companion_dispatch 的全部内部逻辑
    async fn execute_sub_request(
        &self,
        _target: CompanionRequestTarget,
        wait: bool,
        payload: &serde_json::Value,
        _tool_call_id: &str,
        cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let prompt = payload
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if prompt.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "payload.prompt 不能为空".to_string(),
            ));
        }

        let current_session_id = self.current_session_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有可识别的 hook runtime，无法执行 companion request".to_string(),
            )
        })?;
        let hook_session = self.hook_session.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前缺少 hook runtime，无法生成 companion request 上下文".to_string(),
            )
        })?;

        let companion_label = payload
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("companion")
            .to_string();
        let slice_mode = parse_slice_mode(
            payload
                .get("context_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("compact"),
        );
        // target=sub 时 adoption_mode 由 payload 指定，默认 suggestion
        let adoption_mode = parse_adoption_mode(
            payload
                .get("adoption_mode")
                .and_then(|v| v.as_str())
                .unwrap_or(at::SUGGESTION),
        );
        let auto_create = payload
            .get("auto_create")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let title = payload
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let max_fragments = payload
            .get("max_fragments")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let max_constraints = payload
            .get("max_constraints")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let agent_key = payload.get("agent_key").and_then(|v| v.as_str());
        let workflow_key = payload.get("workflow_key").and_then(|v| v.as_str());

        let companion_executor_config = if let Some(key) = agent_key {
            self.resolve_companion_agent_config(hook_session.as_ref(), key)
                .await?
        } else {
            self.current_executor_config.clone()
        };

        let before_resolution = evaluate_subagent_hook(
            hook_session.as_ref(),
            HookTrigger::BeforeSubagentDispatch,
            Some(self.current_turn_id.clone()),
            &companion_label,
            Some(serde_json::json!({
                "prompt": prompt,
                "companion_label": companion_label,
                "auto_create": auto_create,
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
            })),
        )
        .await
        .map_err(AgentToolError::ExecutionFailed)?;

        let session_hub = self.session_hub_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "SessionHub 尚未完成初始化，无法执行 companion request".to_string(),
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
                max_fragments,
                max_constraints,
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

        let isolated_label = format!("companion:{}:{}", current_session_id, companion_label);
        let target_binding = self
            .resolve_or_create_companion_binding(
                hook_session.as_ref(),
                &isolated_label,
                auto_create,
                title,
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
            agent_name: agent_key.map(|s| s.to_string()),
        };
        let _ = session_hub
            .update_session_meta(&target_binding.session_id, |meta| {
                meta.companion_context = Some(companion_context.clone());
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        let final_prompt = build_companion_dispatch_prompt(&dispatch_plan, prompt);
        let execution_slice =
            build_companion_execution_slice(self.vfs.as_ref(), &self.mcp_servers, slice_mode);
        let base_req = PromptSessionRequest {
            user_input: UserPromptInput {
                prompt_blocks: None,
                working_dir: Some(self.working_dir.clone()),
                env: std::collections::HashMap::new(),
                executor_config: None,
            },
            mcp_servers: Vec::new(),
            relay_mcp_server_names: Default::default(),
            vfs: None,
            flow_capabilities: None,
            effective_capability_keys: None,
            system_context: self.system_context.clone(),
            bootstrap_action: crate::session::SessionBootstrapAction::None,
            identity: None,
            post_turn_handler: None,
        };

        let companion_spec = crate::session::CompanionSpec {
            parent_vfs: self.vfs.as_ref(),
            parent_mcp_servers: &self.mcp_servers,
            parent_system_context: self.system_context.as_deref(),
            slice_mode,
            companion_executor_config,
            dispatch_prompt: final_prompt,
        };

        let prepared = if let Some(wf_key) = workflow_key {
            self.setup_companion_workflow(
                hook_session.as_ref(),
                &target_binding,
                &companion_spec,
                wf_key,
            )
            .await?
        } else {
            crate::session::compose_companion(companion_spec)
        };

        let turn_id = session_hub
            .start_prompt_with_follow_up(
                &target_binding.session_id,
                None,
                crate::session::finalize_request(base_req, prepared),
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
                "agent_name": agent_key,
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "inherited_fragment_labels": dispatch_plan.slice.inherited_fragment_labels,
                "inherited_constraint_keys": dispatch_plan.slice.inherited_constraint_keys,
                "inherited_mount_ids": execution_slice.vfs.as_ref().map(|space| {
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

        if wait {
            // wait=true: register in companion_wait_registry then block until result
            let rx = session_hub
                .companion_wait_registry
                .register(
                    &current_session_id,
                    &dispatch_plan.dispatch_id,
                    &self.current_turn_id,
                    None,
                )
                .await;

            let wait_result = tokio::select! {
                result = rx => result.ok(),
                _ = cancel.cancelled() => {
                    session_hub.companion_wait_registry.remove(&dispatch_plan.dispatch_id).await;
                    return Err(AgentToolError::ExecutionFailed("companion wait 被取消".to_string()));
                }
            };

            if let Some(result_payload) = wait_result {
                let summary = result_payload
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(无摘要)");
                let status = result_payload
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let findings = result_payload
                    .get("findings")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join("\n- ")
                    })
                    .unwrap_or_default();

                let mut text = format!(
                    "Companion `{companion_label}` 已完成。\n- status: {status}\n- summary: {summary}",
                );
                if !findings.is_empty() {
                    text.push_str(&format!("\n- findings:\n- {findings}"));
                }

                return Ok(AgentToolResult {
                    content: vec![ContentPart::text(text)],
                    is_error: false,
                    details: Some(serde_json::json!({
                        "request_id": dispatch_plan.dispatch_id,
                        "wait": true,
                        "companion_label": companion_label,
                        "companion_session_id": target_binding.session_id,
                        "status": status,
                        "summary": summary,
                        "result": result_payload,
                    })),
                });
            }
            // oneshot sender dropped without sending — companion may have errored
            return Err(AgentToolError::ExecutionFailed(
                "companion session 未返回结果（可能已异常终止）".to_string(),
            ));
        }

        // wait=false: async dispatch
        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已派发到 companion session（异步）。\n- request_id: {}\n- label: {}\n- session_id: {}\n- turn_id: {}",
                dispatch_plan.dispatch_id, companion_label, target_binding.session_id, turn_id,
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "request_id": dispatch_plan.dispatch_id,
                "wait": false,
                "companion_label": companion_label,
                "companion_session_id": target_binding.session_id,
                "turn_id": turn_id,
                "dispatch_id": dispatch_plan.dispatch_id,
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "inherited_fragment_labels": dispatch_plan.slice.inherited_fragment_labels,
                "inherited_constraint_keys": dispatch_plan.slice.inherited_constraint_keys,
                "inherited_mount_ids": execution_slice.vfs.as_ref().map(|space| {
                    space.mounts.iter().map(|mount| mount.id.clone()).collect::<Vec<_>>()
                }).unwrap_or_default(),
                "mcp_server_count": execution_slice.mcp_servers.len(),
                "matched_rule_keys": after_resolution.matched_rule_keys,
            })),
        })
    }

    /// target=parent 的执行逻辑：在父 session 的 hook runtime 中创建 pending action
    async fn execute_parent_request(
        &self,
        wait: bool,
        payload: &serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<AgentToolResult, AgentToolError> {
        let prompt = payload
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if prompt.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "payload.prompt 不能为空".to_string(),
            ));
        }

        let current_session_id = self.current_session_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有可识别的上下文，无法向上提审".to_string(),
            )
        })?;
        let session_hub = self.session_hub_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed("SessionHub 尚未完成初始化，无法向上提审".to_string())
        })?;

        // 获取 companion context 以找到父 session
        let session_meta = session_hub
            .get_session_meta(&current_session_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            .ok_or_else(|| AgentToolError::ExecutionFailed("当前 session 不存在".to_string()))?;
        let companion_context = session_meta.companion_context.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 不是 companion session，无法使用 target=parent 向上提审".to_string(),
            )
        })?;

        let request_id = format!("review-{}", Uuid::new_v4().simple());

        // 走 SubagentResult hook trigger 统一路径，由 hook 规则决定是否创建 pending action
        // 与 companion_respond → try_complete_to_parent 的回流路径对称
        let review_payload = serde_json::json!({
            "dispatch_id": companion_context.dispatch_id,
            "companion_label": companion_context.companion_label,
            "companion_session_id": current_session_id,
            "parent_session_id": companion_context.parent_session_id,
            "parent_turn_id": companion_context.parent_turn_id,
            "request_id": request_id,
            "request_type": "review",
            "adoption_mode": at::FOLLOW_UP_REQUIRED,
            "status": "pending",
            "summary": prompt,
            "wait": wait,
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
                Some(review_payload.clone()),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
            if let Some(action) = build_subagent_pending_action(
                &companion_context.parent_turn_id,
                &companion_context.companion_label,
                &review_payload,
                &resolution,
            ) {
                parent_hook_session.enqueue_pending_action(action);
            }
            record_subagent_trace(
                parent_hook_session.as_ref(),
                Some(&session_hub),
                Some(companion_context.parent_turn_id.as_str()),
                HookTrigger::SubagentResult,
                "review_request",
                &companion_context.companion_label,
                &resolution,
            )
            .await;
        }

        // 通知父 session 事件流
        let notification = build_companion_event_notification(
            &companion_context.parent_session_id,
            &companion_context.parent_turn_id,
            "companion_review_request",
            format!(
                "Companion `{}` 请求审阅: {}",
                companion_context.companion_label, prompt
            ),
            review_payload,
        );
        let _ = session_hub
            .inject_notification(&companion_context.parent_session_id, notification)
            .await;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已向父 session 提审。\n- request_id: {request_id}\n- parent_session_id: {}",
                companion_context.parent_session_id
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "request_id": request_id,
                "wait": wait,
                "parent_session_id": companion_context.parent_session_id,
            })),
        })
    }

    /// target=human：只发通知事件到前端，不碰 hook 通道
    /// wait=true → agent 自然结束，人回应后 auto-resume
    /// wait=false → agent 继续，人回应作为后续事件注入
    async fn execute_human_request(
        &self,
        wait: bool,
        payload: &serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<AgentToolResult, AgentToolError> {
        let prompt = payload
            .get("prompt")
            .and_then(|v| v.as_str())
            .or_else(|| payload.get("message").and_then(|v| v.as_str()))
            .unwrap_or("")
            .trim();
        if prompt.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "payload.prompt 或 payload.message 不能为空".to_string(),
            ));
        }

        let current_session_id = self.current_session_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有可识别的上下文，无法向用户发起请求".to_string(),
            )
        })?;

        let request_id = format!("human-{}", Uuid::new_v4().simple());
        let payload_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let options: Vec<String> = payload
            .get("options")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        if wait {
            // 工具不返回 → agent loop 卡在这个 tool call → session 挂起
            // respond_companion_request 找到 sender 发回来 → 工具返回 → session 恢复
            let session_hub = self.session_hub_handle.get().await.ok_or_else(|| {
                AgentToolError::ExecutionFailed("SessionHub 尚未初始化".to_string())
            })?;
            let request_type = (!payload_type.is_empty()).then(|| payload_type.to_string());
            let rx = session_hub
                .companion_wait_registry
                .register(
                    &current_session_id,
                    &request_id,
                    &self.current_turn_id,
                    request_type,
                )
                .await;

            let notification = build_companion_event_notification(
                &current_session_id,
                &self.current_turn_id,
                "companion_human_request",
                prompt.to_string(),
                serde_json::json!({
                    "request_id": request_id,
                    "prompt": prompt,
                    "options": options,
                    "wait": true,
                    "payload_type": payload_type,
                }),
            );
            if let Err(error) = session_hub
                .inject_notification(&current_session_id, notification)
                .await
            {
                session_hub
                    .companion_wait_registry
                    .remove(&request_id)
                    .await;
                return Err(AgentToolError::ExecutionFailed(format!(
                    "发送用户协作请求失败: {error}"
                )));
            }

            let response_payload = tokio::select! {
                _ = cancel.cancelled() => {
                    session_hub.companion_wait_registry.remove(&request_id).await;
                    return Err(AgentToolError::ExecutionFailed(
                        "等待用户回应时被取消".to_string(),
                    ));
                }
                result = rx => {
                    result.unwrap_or_else(|_| serde_json::json!({
                        "status": "error",
                        "summary": "用户回应通道已断开"
                    }))
                }
            };

            Ok(AgentToolResult {
                content: vec![ContentPart::text(format!(
                    "用户已回应。\n- request_id: {request_id}\n- response: {}",
                    serde_json::to_string_pretty(&response_payload).unwrap_or_default()
                ))],
                is_error: false,
                details: Some(serde_json::json!({
                    "request_id": request_id,
                    "wait": true,
                    "response_payload": response_payload,
                })),
            })
        } else {
            if let Some(session_hub) = self.session_hub_handle.get().await {
                let notification = build_companion_event_notification(
                    &current_session_id,
                    &self.current_turn_id,
                    "companion_human_request",
                    prompt.to_string(),
                    serde_json::json!({
                        "request_id": request_id,
                        "prompt": prompt,
                        "options": options,
                        "wait": false,
                        "payload_type": payload_type,
                    }),
                );
                let _ = session_hub
                    .inject_notification(&current_session_id, notification)
                    .await;
            }

            Ok(AgentToolResult {
                content: vec![ContentPart::text(format!(
                    "已向用户发送请求。\n- request_id: {request_id}\n- 用户回应后会作为事件注入当前 session。"
                ))],
                is_error: false,
                details: Some(serde_json::json!({
                    "request_id": request_id,
                    "wait": false,
                    "prompt": prompt,
                    "options": options,
                })),
            })
        }
    }

    /// 为 companion session 设置 workflow overlay：
    /// 查找 workflow → 搜索包含该 workflow 的 lifecycle → 创建 LifecycleRun →
    /// 创建 lifecycle binding → 通过 builder 组合 companion + workflow。
    async fn setup_companion_workflow(
        &self,
        hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
        target_binding: &SessionBinding,
        companion_spec: &crate::session::CompanionSpec<'_>,
        workflow_key: &str,
    ) -> Result<crate::session::PreparedSessionInputs, AgentToolError> {
        let snapshot = hook_session.snapshot();
        let project_id = snapshot
            .owners
            .first()
            .and_then(|o| o.project_id.as_deref())
            .and_then(|id| id.parse::<Uuid>().ok())
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "无法从当前 session 确定 project_id，无法解析 workflow_key".to_string(),
                )
            })?;

        let workflow = self
            .repos
            .workflow_definition_repo
            .get_by_project_and_key(project_id, workflow_key)
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(format!("查询 workflow 失败: {e}")))?
            .ok_or_else(|| {
                AgentToolError::InvalidArguments(format!(
                    "当前项目中未找到 workflow_key=`{workflow_key}`"
                ))
            })?;

        // 搜索项目中包含该 workflow 的 lifecycle（选择 entry step 使用该 workflow 的第一个）
        let (lifecycle, entry_step) = self
            .find_lifecycle_for_workflow(project_id, workflow_key)
            .await?;

        let run_service = crate::workflow::LifecycleRunService::new(
            self.repos.lifecycle_definition_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
        );
        let run = run_service
            .start_run(crate::workflow::StartLifecycleRunCommand {
                project_id,
                lifecycle_id: Some(lifecycle.id),
                lifecycle_key: Some(lifecycle.key.clone()),
                session_id: target_binding.session_id.clone(),
            })
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(format!("创建 lifecycle run 失败: {e}")))?;

        let node_label = crate::workflow::build_lifecycle_node_label(&entry_step.key);
        let lifecycle_binding = SessionBinding::new(
            project_id,
            target_binding.session_id.clone(),
            agentdash_domain::session_binding::SessionOwnerType::Project,
            project_id,
            node_label,
        );
        self.session_binding_repo
            .create(&lifecycle_binding)
            .await
            .map_err(|e| {
                AgentToolError::ExecutionFailed(format!("创建 lifecycle session binding 失败: {e}"))
            })?;

        let output = crate::session::compose_companion_with_workflow(
            &self.repos,
            &self.platform_config,
            crate::session::CompanionWorkflowSpec {
                companion: crate::session::CompanionSpec {
                    parent_vfs: companion_spec.parent_vfs,
                    parent_mcp_servers: companion_spec.parent_mcp_servers,
                    parent_system_context: companion_spec.parent_system_context,
                    slice_mode: companion_spec.slice_mode,
                    companion_executor_config: companion_spec.companion_executor_config.clone(),
                    dispatch_prompt: companion_spec.dispatch_prompt.clone(),
                },
                run: &run,
                lifecycle: &lifecycle,
                step: &entry_step,
                workflow: Some(&workflow),
            },
        )
        .await
        .map_err(|e| AgentToolError::ExecutionFailed(format!("compose companion+workflow 失败: {e}")))?;

        Ok(output.prepared)
    }

    /// 在项目的 lifecycle 定义中搜索第一个 entry step 绑定到指定 workflow 的 lifecycle。
    async fn find_lifecycle_for_workflow(
        &self,
        project_id: Uuid,
        workflow_key: &str,
    ) -> Result<
        (
            agentdash_domain::workflow::LifecycleDefinition,
            agentdash_domain::workflow::LifecycleStepDefinition,
        ),
        AgentToolError,
    > {
        let lifecycles = self
            .repos
            .lifecycle_definition_repo
            .list_by_project(project_id)
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(format!("查询 lifecycles 失败: {e}")))?;

        for lifecycle in &lifecycles {
            let entry = lifecycle
                .steps
                .iter()
                .find(|s| s.key == lifecycle.entry_step_key);
            if let Some(step) = entry {
                if step.effective_workflow_key() == Some(workflow_key) {
                    return Ok((lifecycle.clone(), step.clone()));
                }
            }
        }

        // fallback: 搜索所有 step 中引用该 workflow 的第一个 lifecycle
        for lifecycle in &lifecycles {
            for step in &lifecycle.steps {
                if step.effective_workflow_key() == Some(workflow_key) {
                    return Ok((lifecycle.clone(), step.clone()));
                }
            }
        }

        Err(AgentToolError::ExecutionFailed(format!(
            "当前项目中没有任何 lifecycle 引用 workflow_key=`{workflow_key}`，\
             请先创建一个包含此 workflow 的 lifecycle"
        )))
    }

    async fn resolve_companion_agent_config(
        &self,
        hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
        agent_name: &str,
    ) -> Result<AgentConfig, AgentToolError> {
        let snapshot = hook_session.snapshot();
        let project_id = snapshot
            .owners
            .first()
            .and_then(|o| o.project_id.as_deref())
            .and_then(|id| id.parse::<Uuid>().ok())
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "无法从当前 session 确定 project_id，无法解析 agent_key".to_string(),
                )
            })?;

        let links = self
            .agent_link_repo
            .list_by_project(project_id)
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        for link in &links {
            if let Ok(Some(agent)) = self.agent_repo.get_by_id(link.agent_id).await {
                if agent.name.eq_ignore_ascii_case(agent_name) {
                    let merged = link.merged_config(&agent.base_config);
                    return Ok(build_agent_config_from_merged(&agent.agent_type, &merged));
                }
            }
        }

        let available: Vec<String> = {
            let mut names = Vec::new();
            for link in &links {
                if let Ok(Some(agent)) = self.agent_repo.get_by_id(link.agent_id).await {
                    names.push(agent.name.clone());
                }
            }
            names
        };
        Err(AgentToolError::InvalidArguments(format!(
            "当前项目中未找到名为 `{agent_name}` 的 agent。可用 agent: [{}]",
            available.join(", ")
        )))
    }

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
        session_hub
            .mark_owner_bootstrap_pending(&binding.session_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        Ok(binding)
    }
}

// ─── companion_respond ──────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompanionRespondParams {
    /// 回应的 request_id（companion_request 返回的 dispatch_id 或 hook pending action id）
    pub request_id: String,
    /// JSON 字符串格式的 payload。示例：{"type":"resolution","status":"approved","summary":"..."}
    pub payload: String,
}

#[derive(Clone)]
pub struct CompanionRespondTool {
    session_hub_handle: SharedSessionHubHandle,
    current_session_id: Option<String>,
    current_turn_id: String,
    hook_session: Option<agentdash_spi::hooks::SharedHookSessionRuntime>,
    vfs: Option<Vfs>,
    mcp_servers: Vec<McpServer>,
}

impl CompanionRespondTool {
    pub fn new(session_hub_handle: SharedSessionHubHandle, context: &ExecutionContext) -> Self {
        Self {
            session_hub_handle,
            current_session_id: context
                .hook_session
                .as_ref()
                .map(|session| session.session_id().to_string()),
            current_turn_id: context.turn_id.clone(),
            hook_session: context.hook_session.clone(),
            vfs: context.vfs.clone(),
            mcp_servers: context.mcp_servers.clone(),
        }
    }
}

#[async_trait]
impl AgentTool for CompanionRespondTool {
    fn name(&self) -> &str {
        "companion_respond"
    }

    fn description(&self) -> &str {
        "回应 companion 信道上的请求。request_id 指定回应哪个请求，payload 为自由结构。\n\n\
         payload 填写约定：\n\
         ▸ 审批通过：{\"type\":\"resolution\", \"status\":\"approved\", \"summary\":\"...\"}\n\
         ▸ 驳回：{\"type\":\"resolution\", \"status\":\"rejected\", \"summary\":\"...\"}\n\
         ▸ 任务完成：{\"type\":\"completion\", \"status\":\"completed\", \"summary\":\"...\", \"final\":true}\n\
         ▸ 需要修改：{\"type\":\"resolution\", \"status\":\"needs_revision\", \"summary\":\"...\", \"follow_ups\":[\"...\"]}"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CompanionRespondParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let raw: CompanionRespondParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;

        let payload: serde_json::Value = serde_json::from_str(&raw.payload)
            .map_err(|e| AgentToolError::InvalidArguments(format!("payload 不是合法 JSON: {e}")))?;

        // payload type 校验（request_type 暂时无法获取，传 None 跳过匹配校验）
        let registry = super::payload_types::PayloadTypeRegistry::with_builtins();
        if let Some(error) = registry.validate_response(&payload, None) {
            return Err(AgentToolError::InvalidArguments(error));
        }

        let request_id = raw.request_id.trim();
        if request_id.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "request_id 不能为空".to_string(),
            ));
        }

        let current_session_id = self.current_session_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有可识别的上下文，无法回应 companion 请求".to_string(),
            )
        })?;

        // 两个独立副作用，不互斥：
        // 1. resolve 当前 session 的 pending action（hook runtime → 解锁 before_stop gate）
        // 2. 回传结果给父 session（companion context → SubagentResult hook）
        // 哪些命中由上下文决定，可以同时命中多个。

        let resolved_action = self
            .try_resolve_pending_action(request_id, &current_session_id, &payload)
            .await?;

        let completed_to_parent = self
            .try_complete_to_parent(request_id, &current_session_id, &payload)
            .await?;

        // 根据命中情况构造返回值
        let mut modes = Vec::new();
        if resolved_action.is_some() {
            modes.push("resolve_pending_action");
        }
        if completed_to_parent.is_some() {
            modes.push("complete_to_parent");
        }

        if modes.is_empty() {
            return Err(AgentToolError::ExecutionFailed(format!(
                "request_id=`{request_id}` 不匹配任何 pending action 或 companion session"
            )));
        }

        // 优先使用 pending action 的返回值（包含结案详情），其次用 complete_to_parent 的
        let result = resolved_action
            .or(completed_to_parent)
            .unwrap_or_else(|| AgentToolResult {
                content: vec![ContentPart::text(format!(
                    "已回应 companion 请求。\n- request_id: {}\n- modes: {}",
                    request_id,
                    modes.join(", ")
                ))],
                is_error: false,
                details: Some(serde_json::json!({
                    "modes": modes,
                    "request_id": request_id,
                    "session_id": current_session_id,
                })),
            });

        Ok(result)
    }
}

impl CompanionRespondTool {
    /// 路径 1：resolve 当前 session 的 hook pending action（替代 resolve_hook_action）
    async fn try_resolve_pending_action(
        &self,
        request_id: &str,
        current_session_id: &str,
        payload: &serde_json::Value,
    ) -> Result<Option<AgentToolResult>, AgentToolError> {
        let hook_session = match &self.hook_session {
            Some(session) => session,
            None => return Ok(None),
        };

        // 检查是否存在匹配的 pending action
        if !hook_session
            .pending_actions()
            .iter()
            .any(|a| a.id == request_id)
        {
            return Ok(None);
        }

        let resolution_kind = match payload
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("approved")
        {
            "approved" | "completed" => HookPendingActionResolutionKind::Adopted,
            "rejected" | "dismissed" | "needs_revision" => {
                HookPendingActionResolutionKind::Dismissed
            }
            _ => HookPendingActionResolutionKind::Adopted,
        };
        let note = payload
            .get("summary")
            .and_then(|v| v.as_str())
            .or_else(|| payload.get("note").and_then(|v| v.as_str()))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let action = hook_session
            .resolve_pending_action(
                request_id,
                resolution_kind,
                note.clone(),
                Some(self.current_turn_id.clone()),
            )
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!(
                    "当前 session 中不存在 request_id=`{request_id}` 的 pending action"
                ))
            })?;

        if let Some(session_hub) = self.session_hub_handle.get().await {
            let notification = build_hook_action_resolved_notification(
                current_session_id,
                &self.current_turn_id,
                &action,
            );
            let _ = session_hub
                .inject_notification(current_session_id, notification)
                .await;
        }

        Ok(Some(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已回应 companion 请求（resolve pending action）。\n- request_id: {}\n- status: {}\n- resolution: {}",
                action.id,
                hook_action_status_key(action.status),
                action
                    .resolution_kind
                    .map(hook_action_resolution_key)
                    .unwrap_or("unknown")
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "mode": "resolve_pending_action",
                "session_id": current_session_id,
                "turn_id": self.current_turn_id,
                "action": action,
            })),
        }))
    }

    /// 路径 3：回传结果给父 session（替代 companion_complete）
    /// 如果当前 session 不是 companion session 则返回 None
    async fn try_complete_to_parent(
        &self,
        _request_id: &str,
        current_session_id: &str,
        payload: &serde_json::Value,
    ) -> Result<Option<AgentToolResult>, AgentToolError> {
        let session_hub = match self.session_hub_handle.get().await {
            Some(hub) => hub,
            None => return Ok(None),
        };
        let session_meta = match session_hub
            .get_session_meta(current_session_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
        {
            Some(meta) => meta,
            None => return Ok(None),
        };
        let companion_context = match session_meta.companion_context {
            Some(ctx) => ctx,
            None => return Ok(None),
        };

        let summary = payload
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        let status =
            normalize_companion_result_status(payload.get("status").and_then(|v| v.as_str()))?;
        let findings: Vec<String> = payload
            .get("findings")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let follow_ups: Vec<String> = payload
            .get("follow_ups")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let artifact_refs: Vec<String> = payload
            .get("artifact_refs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let hook_payload = serde_json::json!({
            "dispatch_id": companion_context.dispatch_id,
            "companion_label": companion_context.companion_label,
            "agent_name": companion_context.agent_name,
            "companion_session_id": current_session_id,
            "companion_turn_id": self.current_turn_id,
            "parent_session_id": companion_context.parent_session_id,
            "parent_turn_id": companion_context.parent_turn_id,
            "slice_mode": companion_context.slice_mode,
            "adoption_mode": companion_context.adoption_mode,
            "status": status,
            "summary": summary,
            "findings": findings,
            "follow_ups": follow_ups,
            "artifact_refs": artifact_refs,
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
                Some(hook_payload.clone()),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
            if let Some(action) = build_subagent_pending_action(
                &companion_context.parent_turn_id,
                &companion_context.companion_label,
                &hook_payload,
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

        let result_agent_display = companion_context
            .agent_name
            .as_deref()
            .unwrap_or(&companion_context.companion_label);
        let parent_notification = build_companion_event_notification(
            &companion_context.parent_session_id,
            &companion_context.parent_turn_id,
            "companion_result_available",
            format!("Companion `{result_agent_display}` 已回传结果，等待主 session 采纳",),
            hook_payload.clone(),
        );
        session_hub
            .inject_notification(&companion_context.parent_session_id, parent_notification)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        let child_notification = build_companion_event_notification(
            current_session_id,
            &self.current_turn_id,
            "companion_result_returned",
            "已将当前 companion 结果回传到主 session".to_string(),
            hook_payload.clone(),
        );
        let _ = session_hub
            .inject_notification(current_session_id, child_notification)
            .await;

        // Unblock wait=true callers or resume idle parent sessions
        let wait_resolved = session_hub
            .companion_wait_registry
            .resolve(
                &companion_context.parent_session_id,
                &companion_context.dispatch_id,
                hook_payload.clone(),
            )
            .await;

        if wait_resolved.is_none() {
            // wait=false path: parent is not blocking on this dispatch.
            // Resume the parent session with the companion result as a follow-up.
            let parent_running = session_hub
                .has_live_runtime(&companion_context.parent_session_id)
                .await;
            if !parent_running {
                let resume_prompt = format!(
                    "[Companion Result]\nAgent `{result_agent_display}` (dispatch_id: {}) has completed with status={status}.\n\nSummary: {summary}\n\nPlease process this companion result and continue.",
                    companion_context.dispatch_id,
                );

                // Read parent session meta for executor config
                let parent_meta = session_hub
                    .get_session_meta(&companion_context.parent_session_id)
                    .await
                    .ok()
                    .flatten();
                let resume_config = parent_meta
                    .as_ref()
                    .and_then(|m| m.executor_config.clone())
                    .unwrap_or_else(|| AgentConfig::new("PI_AGENT"));

                let parent_sid = companion_context.parent_session_id.clone();
                let hub_clone = session_hub.clone();
                let resume_vfs = self.vfs.clone();
                let resume_mcp_servers = self.mcp_servers.clone();
                tokio::spawn(async move {
                    let _ = hub_clone
                        .start_prompt(
                            &parent_sid,
                            PromptSessionRequest {
                                user_input: UserPromptInput {
                                    prompt_blocks: Some(vec![serde_json::json!({
                                        "type": "text",
                                        "text": resume_prompt,
                                    })]),
                                    working_dir: None,
                                    env: std::collections::HashMap::new(),
                                    executor_config: Some(resume_config),
                                },
                                mcp_servers: resume_mcp_servers,
                                relay_mcp_server_names: Default::default(),
                                vfs: resume_vfs,
                                flow_capabilities: None,
                                effective_capability_keys: None,
                                system_context: None,
                                bootstrap_action: crate::session::SessionBootstrapAction::None,
                                identity: None,
                                post_turn_handler: None,
                            },
                        )
                        .await;
                });
            }
        }

        Ok(Some(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已回应 companion 请求（回传结果到主 session）。\n- parent_session_id: {}\n- dispatch_id: {}\n- status: {}",
                companion_context.parent_session_id, companion_context.dispatch_id, status
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "mode": "complete_to_parent",
                "payload": hook_payload,
            })),
        }))
    }
}

// ─── Payload 解析辅助 ───────────────────────────────────────────────

fn parse_slice_mode(value: &str) -> CompanionSliceMode {
    match value {
        "full" => CompanionSliceMode::Full,
        "workflow_only" => CompanionSliceMode::WorkflowOnly,
        "constraints_only" => CompanionSliceMode::ConstraintsOnly,
        _ => CompanionSliceMode::Compact,
    }
}

fn parse_adoption_mode(value: &str) -> CompanionAdoptionMode {
    if value == at::FOLLOW_UP_REQUIRED {
        CompanionAdoptionMode::FollowUpRequired
    } else if value == at::BLOCKING_REVIEW {
        CompanionAdoptionMode::BlockingReview
    } else {
        CompanionAdoptionMode::Suggestion
    }
}

// ─── hook action 辅助（从 hook_action.rs 合并） ─────────────────────

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
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(
            merge_agentdash_meta(None, &agentdash).expect("构造 hook action ACP Meta 不应失败"),
        )),
    )
}

fn hook_action_status_key(status: HookPendingActionStatus) -> &'static str {
    match status {
        HookPendingActionStatus::Pending => "pending",
        HookPendingActionStatus::Resolved => "resolved",
    }
}

fn hook_action_resolution_key(kind: HookPendingActionResolutionKind) -> &'static str {
    match kind {
        HookPendingActionResolutionKind::Adopted => "adopted",
        HookPendingActionResolutionKind::Dismissed => "dismissed",
    }
}

// ─── 以下为原有内部逻辑函数，保持不变 ──────────────────────────────

pub fn relative_working_dir(context: &ExecutionContext) -> String {
    let Some(space) = context.vfs.as_ref() else {
        return ".".to_string();
    };
    let Some(mount) = space.default_mount() else {
        return ".".to_string();
    };

    // 这里刻意不做 Path 语义运算：只把两端都规范化成字符串路径后做前缀裁剪，
    // 避免在业务层引入“云端理解本机路径”的依赖链路。
    let root = mount
        .root_ref
        .trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    if root.is_empty() {
        return ".".to_string();
    }
    let wd = context
        .working_directory
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();

    if wd == root {
        return ".".to_string();
    }
    let prefix = format!("{root}/");
    wd.strip_prefix(&prefix)
        .filter(|s| !s.is_empty())
        .unwrap_or(".")
        .to_string()
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
            token_stats: None,
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
        injections: resolution.injections.clone(),
    };
    hook_session.append_trace(trace.clone());

    // Only inject notification when the session has NO active connector.
    // Active connectors already receive traces via trace_broadcast → hook_trace_rx,
    // so inject_notification would cause duplicate event cards.
    if let (Some(session_hub), Some(turn_id)) = (session_hub, turn_id) {
        let session_id = hook_session.session_id();
        let has_live = session_hub.has_live_runtime(session_id).await;
        if !has_live {
            if let Some(notification) = build_hook_trace_notification(
                session_id,
                Some(turn_id),
                hook_trace_source(),
                &trace,
            ) {
                let _ = session_hub
                    .inject_notification(session_id, notification)
                    .await;
            }
        }
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
        .unwrap_or(at::SUGGESTION)
        .trim()
        .to_string();
    if adoption_mode.is_empty() || adoption_mode == at::SUGGESTION {
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
        title: if adoption_mode == at::BLOCKING_REVIEW {
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
    pub vfs: Option<Vfs>,
    pub mcp_servers: Vec<McpServer>,
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
        "## 回流要求\n- 完成后请调用 `companion_respond`。\n- payload 中必填 summary。\n- 如有关键发现请写入 findings。\n- 如需要主 session 后续行动请写入 follow_ups。".to_string(),
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
    vfs: Option<&Vfs>,
    mcp_servers: &[McpServer],
    mode: CompanionSliceMode,
) -> CompanionExecutionSlice {
    match mode {
        CompanionSliceMode::Full => CompanionExecutionSlice {
            vfs: vfs.cloned(),
            mcp_servers: mcp_servers.to_vec(),
        },
        CompanionSliceMode::Compact => CompanionExecutionSlice {
            vfs: Some(filter_vfs_capabilities(
                vfs,
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
                vfs: Some(Vfs::default()),
                mcp_servers: Vec::new(),
            }
        }
    }
}

fn build_agent_config_from_merged(agent_type: &str, config: &serde_json::Value) -> AgentConfig {
    let mut ec = AgentConfig::new(agent_type.to_string());
    if let Some(v) = config.get("provider_id").and_then(|v| v.as_str()) {
        ec.provider_id = Some(v.to_string());
    }
    if let Some(v) = config.get("model_id").and_then(|v| v.as_str()) {
        ec.model_id = Some(v.to_string());
    }
    if let Some(v) = config.get("agent_id").and_then(|v| v.as_str()) {
        ec.agent_id = Some(v.to_string());
    }
    if let Some(v) = config.get("permission_policy").and_then(|v| v.as_str()) {
        ec.permission_policy = Some(v.to_string());
    }
    if let Some(v) = config
        .get("thinking_level")
        .and_then(|v| serde_json::from_value::<agentdash_spi::ThinkingLevel>(v.clone()).ok())
    {
        ec.thinking_level = Some(v);
    }
    if let Some(arr) = config.get("tool_clusters").and_then(|v| v.as_array()) {
        let clusters: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        if !clusters.is_empty() {
            ec.tool_clusters = Some(clusters);
        }
    }
    if let Some(v) = config.get("system_prompt").and_then(|v| v.as_str()) {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            ec.system_prompt = Some(trimmed.to_string());
        }
    }
    if let Some(v) = config
        .get("system_prompt_mode")
        .and_then(|v| serde_json::from_value::<agentdash_spi::SystemPromptMode>(v.clone()).ok())
    {
        ec.system_prompt_mode = Some(v);
    }
    ec
}

fn filter_vfs_capabilities(vfs: Option<&Vfs>, allowed: &[MountCapability]) -> Vfs {
    let Some(vfs) = vfs else {
        return Vfs::default();
    };

    let mounts = vfs
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

    let default_mount_id = vfs.default_mount_id.as_ref().and_then(|default_id| {
        mounts
            .iter()
            .any(|mount| mount.id == *default_id)
            .then(|| default_id.clone())
    });

    Vfs {
        mounts,
        default_mount_id,
        source_project_id: vfs.source_project_id.clone(),
        source_story_id: vfs.source_story_id.clone(),
        links: vfs.links.clone(),
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
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(
            merge_agentdash_meta(None, &agentdash).expect("构造 companion ACP Meta 不应失败"),
        )),
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
        CompanionAdoptionMode::Suggestion => at::SUGGESTION,
        CompanionAdoptionMode::FollowUpRequired => at::FOLLOW_UP_REQUIRED,
        CompanionAdoptionMode::BlockingReview => at::BLOCKING_REVIEW,
    }
}

pub fn companion_owner_candidates(
    snapshot: &agentdash_spi::SessionHookSnapshot,
) -> Result<Vec<(SessionOwnerType, Uuid, Option<String>)>, AgentToolError> {
    let mut owners = Vec::new();
    for owner in &snapshot.owners {
        if let Some(candidate) =
            parse_owner_candidate(owner.owner_type, &owner.owner_id, owner.label.clone())?
        {
            owners.push(candidate);
        }
        if owner.owner_type == SessionOwnerType::Task
            && let Some(story_id) = owner.story_id.as_deref()
            && let Some(candidate) =
                parse_owner_candidate(SessionOwnerType::Story, story_id, owner.label.clone())?
        {
            owners.push(candidate);
        }
    }
    owners.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    Ok(owners)
}

fn parse_owner_candidate(
    owner_type: SessionOwnerType,
    owner_id: &str,
    label: Option<String>,
) -> Result<Option<(SessionOwnerType, Uuid, Option<String>)>, AgentToolError> {
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
        .find(|owner| owner.owner_type == owner_type && owner.owner_id == owner_id_raw)
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
    use agentdash_spi::{MountCapability, Vfs};
    use uuid::Uuid;

    #[test]
    fn companion_owner_candidates_fallback_from_task_to_story() {
        let story_id = Uuid::new_v4();
        let snapshot = agentdash_spi::SessionHookSnapshot {
            session_id: "sess-test".to_string(),
            owners: vec![agentdash_spi::HookOwnerSummary {
                owner_type: SessionOwnerType::Task,
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
                owner_type: SessionOwnerType::Task,
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
        let vfs = Vfs {
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
            Some(&vfs),
            &[McpServer::Stdio(
                agent_client_protocol::McpServerStdio::new("test-mcp", "cmd"),
            )],
            CompanionSliceMode::Compact,
        );

        let sliced_space = slice.vfs.expect("compact should keep sliced vfs");
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
    fn workflow_only_execution_slice_uses_empty_vfs() {
        let vfs = Vfs {
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
            Some(&vfs),
            &[McpServer::Stdio(
                agent_client_protocol::McpServerStdio::new("test-mcp", "cmd"),
            )],
            CompanionSliceMode::WorkflowOnly,
        );

        let sliced_space = slice.vfs.expect("workflow_only should force empty vfs");
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

        assert!(prompt.contains("companion_respond"));
        assert!(prompt.contains("dispatch_id: dispatch-1"));
        assert!(prompt.contains("请帮我 review 当前实现"));
    }
}
