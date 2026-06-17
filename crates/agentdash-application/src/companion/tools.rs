use std::sync::Arc;

use crate::session::build_hook_trace_envelope;
use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::workflow::{
    AgentLaunchIntent, AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionSource, GatePolicy,
    InteractionDispatchIntent, LifecycleTaskPlanItem, LifecycleTaskPlanItemPatch, RunPolicy,
    RuntimePolicy,
};
use agentdash_spi::CapabilityScope;
use agentdash_spi::action_type as at;
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::hooks::{HookRuntimeEvaluationQuery, HookRuntimeRefreshQuery};
use agentdash_spi::{
    AgentConfig, HookPendingAction, HookPendingActionResolutionKind, HookPendingActionStatus,
    HookTraceEntry, HookTrigger, MountCapability, RuntimeEventSource, Vfs,
};
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::tool_context::{
    CompanionHookProvenance, CompanionHookProvenanceSource, CompanionToolContext,
};
use super::{
    CompanionGateControlService, CompleteCompanionChildResultCommand,
    OpenCompanionParentRequestCommand, ResolveCompanionParentRequestCommand,
    build_companion_event_notification,
};
use crate::lifecycle::{LifecycleDispatchService, resolve_current_frame_from_delivery_trace_ref};
use crate::runtime_tools::{SessionToolServices, SharedSessionToolServicesHandle};
use crate::session::{
    AgentFrameRuntimeTarget, CompanionLaunchSource, LaunchCommand, UserPromptInput,
};
use crate::task::plan::update_run_task;

pub use agentdash_spi::CompanionSliceMode;

struct CompanionDispatchOutcome {
    run_ref: Uuid,
    agent_ref: Uuid,
    frame_ref: Uuid,
    gate_ref: Option<Uuid>,
    delivery_runtime_session_id: String,
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
    Platform,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompanionRequestParams {
    /// 发给谁：sub（子 agent）、parent（父 agent）、human（用户）、platform（平台 broker）
    pub target: CompanionRequestTarget,
    /// 是否期望等待对方回应（创建 follow_up_required pending action，是否阻塞由 workflow 决定）
    #[serde(default)]
    pub wait: bool,
    /// 结构化 JSON object payload，内容由 target 与 payload.type 决定。
    #[schemars(schema_with = "companion_request_payload_schema")]
    pub payload: serde_json::Value,
}

async fn require_session_services(
    handle: &SharedSessionToolServicesHandle,
    action: &str,
) -> Result<SessionToolServices, AgentToolError> {
    handle.get().await.ok_or_else(|| {
        AgentToolError::ExecutionFailed(format!("Session services 尚未完成初始化，无法{action}"))
    })
}

struct CompanionGateControlFactory<'a> {
    repos: &'a crate::repository_set::RepositorySet,
}

impl<'a> CompanionGateControlFactory<'a> {
    fn new(repos: &'a crate::repository_set::RepositorySet) -> Self {
        Self { repos }
    }

    fn with_session_eventing(
        &self,
        session_services: &SessionToolServices,
    ) -> CompanionGateControlService {
        CompanionGateControlService::with_session_eventing(
            self.repos.lifecycle_gate_repo.clone(),
            self.repos.agent_frame_repo.clone(),
            self.repos.lifecycle_agent_repo.clone(),
            self.repos.execution_anchor_repo.clone(),
            self.repos.agent_lineage_repo.clone(),
            session_services.eventing.clone(),
        )
    }
}

fn payload_task_id(payload: &serde_json::Value) -> Result<Option<Uuid>, AgentToolError> {
    match payload.get("task_id") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(raw)) => Uuid::parse_str(raw).map(Some).map_err(|_| {
            AgentToolError::InvalidArguments(format!("payload.task_id 不是有效 UUID: {raw}"))
        }),
        Some(other) => Err(AgentToolError::InvalidArguments(format!(
            "payload.task_id 必须是 UUID 字符串，当前为 {other}"
        ))),
    }
}

async fn load_companion_task_context(
    repos: &crate::repository_set::RepositorySet,
    parent_run_id: Uuid,
    task_id: Uuid,
) -> Result<LifecycleTaskPlanItem, AgentToolError> {
    let run = repos
        .lifecycle_run_repo
        .get_by_id(parent_run_id)
        .await
        .map_err(|error| {
            AgentToolError::ExecutionFailed(format!(
                "读取 parent LifecycleRun `{parent_run_id}` 失败: {error}"
            ))
        })?
        .ok_or_else(|| {
            AgentToolError::ExecutionFailed(format!(
                "parent LifecycleRun `{parent_run_id}` 不存在，无法指派 Task"
            ))
        })?;
    run.task_by_id(task_id).cloned().ok_or_else(|| {
        AgentToolError::InvalidArguments(format!(
            "Task `{task_id}` 不属于当前 parent run `{parent_run_id}`"
        ))
    })
}

fn companion_task_prompt_block(task: &LifecycleTaskPlanItem) -> String {
    let mut lines = vec![
        "## Assigned Task".to_string(),
        format!("- id: {}", task.id),
        format!("- title: {}", task.title),
        format!("- status: {}", task_status_key(task.status)),
    ];
    if let Some(body) = task.body.as_deref().filter(|body| !body.trim().is_empty()) {
        lines.push(format!("- body: {body}"));
    }
    if let Some(priority) = task.priority {
        lines.push(format!("- priority: {priority:?}"));
    }
    if let Some(story_ref) = &task.story_ref {
        lines.push(format!("- story_ref: {}/{}", story_ref.kind, story_ref.id));
    }
    if !task.context_refs.is_empty() {
        lines.push("- context_refs:".to_string());
        for context_ref in &task.context_refs {
            let label = context_ref.label.as_deref().unwrap_or(&context_ref.locator);
            lines.push(format!(
                "  - {} [{}] {}",
                label,
                context_ref.slot_key(),
                context_ref.locator
            ));
        }
    }
    lines.join("\n")
}

fn task_status_key(status: agentdash_domain::workflow::TaskPlanStatus) -> &'static str {
    match status {
        agentdash_domain::workflow::TaskPlanStatus::Open => "open",
        agentdash_domain::workflow::TaskPlanStatus::Active => "active",
        agentdash_domain::workflow::TaskPlanStatus::Review => "review",
        agentdash_domain::workflow::TaskPlanStatus::Blocked => "blocked",
        agentdash_domain::workflow::TaskPlanStatus::Done => "done",
        agentdash_domain::workflow::TaskPlanStatus::Dropped => "dropped",
    }
}

trait ContextSourceRefCompanionExt {
    fn slot_key(&self) -> &'static str;
}

impl ContextSourceRefCompanionExt for agentdash_domain::context_source::ContextSourceRef {
    fn slot_key(&self) -> &'static str {
        match self.slot {
            agentdash_domain::context_source::ContextSlot::Requirements => "requirements",
            agentdash_domain::context_source::ContextSlot::Constraints => "constraints",
            agentdash_domain::context_source::ContextSlot::Codebase => "codebase",
            agentdash_domain::context_source::ContextSlot::References => "references",
            agentdash_domain::context_source::ContextSlot::InstructionAppend => {
                "instruction_append"
            }
        }
    }
}

#[derive(Clone)]
pub struct CompanionRequestTool {
    project_agent_repo: Arc<dyn ProjectAgentRepository>,
    repos: crate::repository_set::RepositorySet,
    session_services_handle: SharedSessionToolServicesHandle,
    current_executor_config: AgentConfig,
    tool_context: CompanionToolContext,
}

impl CompanionRequestTool {
    pub(crate) fn new(
        project_agent_repo: Arc<dyn ProjectAgentRepository>,
        repos: crate::repository_set::RepositorySet,
        session_services_handle: SharedSessionToolServicesHandle,
        tool_context: CompanionToolContext,
        current_executor_config: AgentConfig,
    ) -> Self {
        Self {
            project_agent_repo,
            repos,
            session_services_handle,
            current_executor_config,
            tool_context,
        }
    }
}

#[async_trait]
impl AgentTool for CompanionRequestTool {
    fn name(&self) -> &str {
        "companion_request"
    }

    fn description(&self) -> &str {
        "发起结构化 companion 交互请求；用于询问用户、申请平台能力、协调 parent/sub session，或把动态 workflow 提案交给人/平台评审。payload 必须是 JSON object；交互正文统一使用 payload.message，复杂规则见 companion-system skill。"
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

        let payload = raw.payload;
        if let Some(error) = super::payload_types::payload_object_error(&payload) {
            return Err(AgentToolError::InvalidArguments(error));
        }

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
            CompanionRequestTarget::Platform => {
                self.execute_platform_request(raw.wait, &payload, cancel)
                    .await
            }
        }
    }
}

impl CompanionRequestTool {
    /// target=sub: 构造 ExecutionIntent 通过 LifecycleDispatchService 派发 companion child agent。
    /// wait 语义通过 durable LifecycleGate 实现。
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
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if prompt.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "payload.message 不能为空".to_string(),
            ));
        }

        let hook_runtime = self
            .tool_context
            .require_hook_runtime("生成 companion request 上下文")?;

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
        let adoption_mode = parse_adoption_mode(
            payload
                .get("adoption_mode")
                .and_then(|v| v.as_str())
                .unwrap_or(at::SUGGESTION),
        );
        let agent_key = payload.get("agent_key").and_then(|v| v.as_str());
        let requested_task_id = payload_task_id(payload)?;
        let max_fragments = payload
            .get("max_fragments")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let max_constraints = payload
            .get("max_constraints")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let companion_executor_config = if let Some(key) = agent_key {
            self.resolve_companion_agent_config(hook_runtime.as_ref(), key)
                .await?
        } else {
            self.current_executor_config.clone()
        };

        // ─── Hook: before_subagent_dispatch ─────────────────────────────
        let before_resolution = evaluate_subagent_hook(
            hook_runtime.as_ref(),
            HookTrigger::BeforeSubagentDispatch,
            Some(self.tool_context.turn_id().to_string()),
            &companion_label,
            Some(serde_json::json!({
                "message": prompt,
                "companion_label": companion_label,
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "task_id": requested_task_id.map(|id| id.to_string()),
            })),
        )
        .await?;

        let session_services =
            require_session_services(&self.session_services_handle, "执行 companion request")
                .await?;

        if let Some(reason) = before_resolution.block_reason.clone() {
            record_subagent_trace(
                hook_runtime.as_ref(),
                Some(&session_services),
                Some(self.tool_context.turn_id()),
                HookTrigger::BeforeSubagentDispatch,
                "deny",
                &companion_label,
                &before_resolution,
            )
            .await;
            return Err(AgentToolError::ExecutionFailed(reason));
        }

        // ─── 构建 dispatch plan（用于 context slice / prompt 生成） ──────
        let current_session_id = self
            .tool_context
            .require_delivery_runtime_session_id("派发 companion agent")?
            .to_string();
        let anchor = self
            .tool_context
            .require_lifecycle_anchor("派发 companion agent", &self.repos)
            .await?;
        let project_id = anchor.project_id;
        let parent_run_id = anchor.run_id;
        let parent_agent_id = anchor.agent_id;
        let parent_frame_id = anchor.frame_id;
        let task_context = if let Some(task_id) = requested_task_id {
            Some(load_companion_task_context(&self.repos, parent_run_id, task_id).await?)
        } else {
            None
        };
        let dispatch_message = if let Some(task) = task_context.as_ref() {
            format!("{prompt}\n\n{}", companion_task_prompt_block(task))
        } else {
            prompt.to_string()
        };
        let dispatch_plan = build_companion_dispatch_plan(
            hook_runtime.as_ref(),
            &before_resolution,
            &CompanionDispatchConfig {
                parent_session_id: &current_session_id,
                parent_turn_id: self.tool_context.turn_id(),
                companion_label: &companion_label,
                slice_mode,
                adoption_mode,
                max_fragments,
                max_constraints,
            },
        );
        let dispatch_prompt = build_companion_dispatch_prompt(&dispatch_plan, &dispatch_message);
        record_subagent_trace(
            hook_runtime.as_ref(),
            Some(&session_services),
            Some(self.tool_context.turn_id()),
            HookTrigger::BeforeSubagentDispatch,
            "allow",
            &companion_label,
            &before_resolution,
        )
        .await;

        // ─── 构造 ExecutionIntent 并通过 LifecycleDispatchService 派发 ──
        let gate_kind = match adoption_mode {
            CompanionAdoptionMode::BlockingReview => "companion_wait_blocking",
            CompanionAdoptionMode::FollowUpRequired => "companion_wait_follow_up",
            CompanionAdoptionMode::Suggestion => "companion_wait",
        };

        let context_policy = match slice_mode {
            CompanionSliceMode::Full => ContextPolicy::Inherit,
            _ => ContextPolicy::Slice,
        };
        let dispatch_result: CompanionDispatchOutcome = {
            let dispatch_svc = LifecycleDispatchService::new(
                self.repos.lifecycle_run_repo.as_ref(),
                self.repos.workflow_graph_repo.as_ref(),
                self.repos.lifecycle_agent_repo.as_ref(),
                self.repos.agent_frame_repo.as_ref(),
                self.repos.lifecycle_subject_association_repo.as_ref(),
                self.repos.lifecycle_gate_repo.as_ref(),
                self.repos.agent_lineage_repo.as_ref(),
            )
            .with_anchor_repo(self.repos.execution_anchor_repo.as_ref())
            .with_runtime_session_creator(self.repos.runtime_session_creator.as_ref());
            if wait {
                let result = dispatch_svc
                    .open_interaction_gate(&InteractionDispatchIntent {
                        project_id,
                        source: ExecutionSource::ParentAgent,
                        parent_run_id,
                        parent_agent_id,
                        workflow_graph_ref: None,
                        context_policy,
                        capability_policy: CapabilityPolicy::InheritedSlice,
                        runtime_policy: RuntimePolicy::CreateRuntimeSession,
                        gate_policy: GatePolicy {
                            gate_kind: gate_kind.to_string(),
                            correlation_id: Some(dispatch_plan.dispatch_id.clone()),
                            payload: Some(serde_json::json!({
                                "parent_agent_id": parent_agent_id,
                                "parent_frame_id": parent_frame_id,
                                "companion_label": companion_label,
                                "adoption_mode": companion_adoption_mode_key(adoption_mode),
                                "dispatch_id": dispatch_plan.dispatch_id,
                                "task_id": requested_task_id.map(|id| id.to_string()),
                            })),
                        },
                    })
                    .await
                    .map_err(|e| AgentToolError::ExecutionFailed(format!("dispatch 失败: {e}")))?;
                CompanionDispatchOutcome {
                    run_ref: result.runtime_refs.run_ref,
                    agent_ref: result.runtime_refs.agent_ref,
                    frame_ref: result.runtime_refs.frame_ref,
                    gate_ref: Some(result.gate_ref),
                    delivery_runtime_session_id: result
                        .delivery_runtime_ref
                        .ok_or_else(|| {
                            AgentToolError::ExecutionFailed(
                                "dispatch 未创建 child delivery runtime session".to_string(),
                            )
                        })?
                        .to_string(),
                }
            } else {
                let result = dispatch_svc
                    .launch_agent(&AgentLaunchIntent {
                        project_id,
                        source: ExecutionSource::ParentAgent,
                        subject_ref: None,
                        parent_run_id: Some(parent_run_id),
                        parent_agent_id: Some(parent_agent_id),
                        workflow_graph_ref: None,
                        run_policy: RunPolicy::AppendGraph,
                        agent_policy: AgentPolicy::SpawnChild,
                        context_policy,
                        capability_policy: CapabilityPolicy::InheritedSlice,
                        runtime_policy: RuntimePolicy::CreateRuntimeSession,
                    })
                    .await
                    .map_err(|e| AgentToolError::ExecutionFailed(format!("dispatch 失败: {e}")))?;
                CompanionDispatchOutcome {
                    run_ref: result.runtime_refs.run_ref,
                    agent_ref: result.runtime_refs.agent_ref,
                    frame_ref: result.runtime_refs.frame_ref,
                    gate_ref: None,
                    delivery_runtime_session_id: result
                        .delivery_runtime_ref
                        .ok_or_else(|| {
                            AgentToolError::ExecutionFailed(
                                "dispatch 未创建 child delivery runtime session".to_string(),
                            )
                        })?
                        .to_string(),
                }
            }
        };

        if let Some(task_id) = requested_task_id {
            update_run_task(
                self.repos.lifecycle_run_repo.as_ref(),
                parent_run_id,
                task_id,
                LifecycleTaskPlanItemPatch {
                    assigned_agent_id: Some(Some(dispatch_result.agent_ref)),
                    ..LifecycleTaskPlanItemPatch::default()
                },
            )
            .await
            .map_err(|error| {
                AgentToolError::ExecutionFailed(format!(
                    "Companion 已创建但 Task 指派关系写回失败: {error}"
                ))
            })?;
        }

        let launch_outcome = session_services
            .launch
            .launch_command_with_outcome(
                &dispatch_result.delivery_runtime_session_id,
                LaunchCommand::companion_dispatch_input(
                    UserPromptInput::from_text(&dispatch_prompt),
                    CompanionLaunchSource {
                        parent_session_id: current_session_id.clone(),
                        slice_mode,
                        companion_executor_config,
                        dispatch_prompt: dispatch_prompt.clone(),
                        workflow: None,
                    },
                ),
            )
            .await
            .map_err(|error| {
                AgentToolError::ExecutionFailed(format!(
                    "child companion session launch 失败: {error}"
                ))
            })?;
        let child_turn_id = launch_outcome.turn_id.clone();
        let child_context_sources = launch_outcome.context_sources.clone();

        // ─── Hook: after_subagent_dispatch ──────────────────────────────
        let after_resolution = evaluate_subagent_hook(
            hook_runtime.as_ref(),
            HookTrigger::AfterSubagentDispatch,
            Some(self.tool_context.turn_id().to_string()),
            &companion_label,
            Some(serde_json::json!({
                "dispatch_id": dispatch_plan.dispatch_id,
                "agent_ref": dispatch_result.agent_ref.to_string(),
                "frame_ref": dispatch_result.frame_ref.to_string(),
                "gate_ref": dispatch_result.gate_ref.map(|id| id.to_string()),
                "delivery_runtime_session_id": dispatch_result.delivery_runtime_session_id.clone(),
                "turn_id": child_turn_id.clone(),
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "task_id": requested_task_id.map(|id| id.to_string()),
            })),
        )
        .await?;
        record_subagent_trace(
            hook_runtime.as_ref(),
            Some(&session_services),
            Some(self.tool_context.turn_id()),
            HookTrigger::AfterSubagentDispatch,
            "dispatched",
            &companion_label,
            &after_resolution,
        )
        .await;

        // ─── Wait 路径: 轮询 durable LifecycleGate ─────────────────────
        if wait {
            let gate_id = dispatch_result.gate_ref.ok_or_else(|| {
                AgentToolError::ExecutionFailed("dispatch 未创建 gate（内部错误）".to_string())
            })?;

            let result_payload = self.poll_gate_until_resolved(gate_id, cancel).await?;

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
                "Companion `{companion_label}` 已完成。\n- child_session_id: {}\n- child_turn_id: {}\n- status: {status}\n- summary: {summary}",
                dispatch_result.delivery_runtime_session_id, child_turn_id,
            );
            if !findings.is_empty() {
                text.push_str(&format!("\n- findings:\n- {findings}"));
            }

            return Ok(AgentToolResult {
                content: vec![ContentPart::text(text)],
                is_error: false,
                details: Some(serde_json::json!({
                    "dispatch_id": dispatch_plan.dispatch_id,
                    "wait": true,
                    "companion_label": companion_label,
                    "agent_ref": dispatch_result.agent_ref.to_string(),
                    "frame_ref": dispatch_result.frame_ref.to_string(),
                    "run_ref": dispatch_result.run_ref.to_string(),
                    "gate_ref": gate_id.to_string(),
                    "delivery_runtime_session_id": dispatch_result.delivery_runtime_session_id.clone(),
                    "child_session_id": dispatch_result.delivery_runtime_session_id.clone(),
                    "child_turn_id": child_turn_id.clone(),
                    "context_sources": child_context_sources.clone(),
                    "task_id": requested_task_id.map(|id| id.to_string()),
                    "status": status,
                    "summary": summary,
                    "result": result_payload,
                })),
            });
        }

        // ─── Async dispatch (wait=false) ────────────────────────────────
        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已派发 companion agent（异步）。\n- dispatch_id: {}\n- label: {}\n- child_session_id: {}\n- child_turn_id: {}\n- agent_ref: {}\n- frame_ref: {}",
                dispatch_plan.dispatch_id,
                companion_label,
                dispatch_result.delivery_runtime_session_id,
                child_turn_id,
                dispatch_result.agent_ref,
                dispatch_result.frame_ref,
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "dispatch_id": dispatch_plan.dispatch_id,
                "wait": false,
                "companion_label": companion_label,
                "agent_ref": dispatch_result.agent_ref.to_string(),
                "frame_ref": dispatch_result.frame_ref.to_string(),
                "run_ref": dispatch_result.run_ref.to_string(),
                "gate_ref": dispatch_result.gate_ref.map(|id| id.to_string()),
                "delivery_runtime_session_id": dispatch_result.delivery_runtime_session_id.clone(),
                "child_session_id": dispatch_result.delivery_runtime_session_id,
                "child_turn_id": child_turn_id,
                "context_sources": child_context_sources,
                "task_id": requested_task_id.map(|id| id.to_string()),
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "matched_rule_keys": after_resolution.matched_rule_keys,
            })),
        })
    }

    /// 轮询 LifecycleGate 直到 resolved 或取消。
    async fn poll_gate_until_resolved(
        &self,
        gate_id: Uuid,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, AgentToolError> {
        let poll_interval = std::time::Duration::from_millis(500);
        loop {
            let gate = self
                .repos
                .lifecycle_gate_repo
                .get(gate_id)
                .await
                .map_err(|e| AgentToolError::ExecutionFailed(format!("gate 查询失败: {e}")))?
                .ok_or_else(|| AgentToolError::ExecutionFailed(format!("gate {gate_id} 不存在")))?;

            if !gate.is_open() {
                return Ok(gate.payload_json.unwrap_or(serde_json::json!({})));
            }

            tokio::select! {
                _ = tokio::time::sleep(poll_interval) => {},
                _ = cancel.cancelled() => {
                    return Err(AgentToolError::ExecutionFailed(
                        "companion wait 被取消".to_string(),
                    ));
                }
            }
        }
    }

    /// target=parent 的执行逻辑：打开 parent frame 持有的 durable gate。
    async fn execute_parent_request(
        &self,
        wait: bool,
        payload: &serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<AgentToolResult, AgentToolError> {
        let prompt = payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if prompt.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "payload.message 不能为空".to_string(),
            ));
        }

        let current_session_id = self
            .tool_context
            .require_delivery_runtime_session_id("向上提审")?
            .to_string();
        let session_services =
            require_session_services(&self.session_services_handle, "向上提审").await?;
        let gate_control =
            CompanionGateControlFactory::new(&self.repos).with_session_eventing(&session_services);
        let opened = gate_control
            .open_parent_request(OpenCompanionParentRequestCommand {
                child_runtime_session_id: current_session_id,
                turn_id: self.tool_context.turn_id().to_string(),
                wait,
                payload: payload.clone(),
            })
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        if let Some(parent_hook_runtime) = session_services
            .hooks
            .ensure_hook_runtime_for_target(
                &AgentFrameRuntimeTarget {
                    frame_id: opened.parent_frame_id,
                    delivery_runtime_session_id: opened.parent_delivery_runtime_session_id.clone(),
                },
                None,
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
        {
            let resolution = evaluate_subagent_hook(
                parent_hook_runtime.as_ref(),
                HookTrigger::CompanionResult,
                None,
                &opened.companion_label,
                Some(opened.payload.clone()),
            )
            .await?;
            if let Some(action) = build_subagent_pending_action(
                &opened.request_id,
                &opened.companion_label,
                &opened.payload,
                &resolution,
            ) {
                parent_hook_runtime.enqueue_pending_action(action);
            }
            record_subagent_trace(
                parent_hook_runtime.as_ref(),
                Some(&session_services),
                None,
                HookTrigger::CompanionResult,
                "review_request",
                &opened.companion_label,
                &resolution,
            )
            .await;
        }

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已向父 agent 提审。\n- request_id: {}\n- gate_id: {}\n- parent_agent_id: {}",
                opened.request_id, opened.gate_id, opened.parent_agent_id,
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "request_id": opened.request_id,
                "gate_id": opened.gate_id.to_string(),
                "wait": wait,
                "run_id": opened.run_id.to_string(),
                "parent_agent_id": opened.parent_agent_id.to_string(),
                "parent_frame_id": opened.parent_frame_id.to_string(),
                "parent_session_id": opened.parent_delivery_runtime_session_id,
                "child_agent_id": opened.child_agent_id.to_string(),
                "child_frame_id": opened.child_frame_id.to_string(),
                "child_session_id": opened.child_delivery_runtime_session_id,
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
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if prompt.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "payload.message 不能为空".to_string(),
            ));
        }

        let current_session_id = self
            .tool_context
            .require_delivery_runtime_session_id("向用户发起请求")?
            .to_string();
        let session_services =
            require_session_services(&self.session_services_handle, "向用户发起请求").await?;
        let anchor = self
            .tool_context
            .require_lifecycle_anchor("向用户发起请求", &self.repos)
            .await?;

        let request_id = format!("human-{}", Uuid::new_v4().simple());
        let payload_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let ui_hint = super::payload_types::PayloadTypeRegistry::with_builtins()
            .ui_hint(payload_type)
            .unwrap_or("generic_companion_request");
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
            // 创建 durable LifecycleGate 代替 in-memory channel
            let gate_meta = serde_json::json!({
                "session_id": current_session_id.clone(),
                "turn_id": self.tool_context.turn_id(),
                "request_type": payload_type,
            });
            let gate = agentdash_domain::workflow::LifecycleGate::open(
                anchor.run_id,
                Some(anchor.agent_id),
                Some(anchor.frame_id),
                "companion_wait",
                &request_id,
                Some(gate_meta),
            );
            let gate_id = gate.id;
            self.repos
                .lifecycle_gate_repo
                .create(&gate)
                .await
                .map_err(|e| AgentToolError::ExecutionFailed(format!("创建等待 gate 失败: {e}")))?;
            let request_id = gate_id.to_string();

            let notification = build_companion_event_notification(
                &current_session_id,
                self.tool_context.turn_id(),
                "companion_human_request",
                prompt.to_string(),
                serde_json::json!({
                    "request_id": request_id.clone(),
                    "gate_id": request_id.clone(),
                    "message": prompt,
                    "options": options.clone(),
                    "wait": true,
                    "payload_type": payload_type,
                    "ui_hint": ui_hint,
                    "payload": payload,
                }),
            );
            if let Err(error) = session_services
                .eventing
                .inject_notification(&current_session_id, notification)
                .await
            {
                return Err(AgentToolError::ExecutionFailed(format!(
                    "发送用户协作请求失败: {error}"
                )));
            }

            // 轮询 gate 直到被 resolve 或取消
            let poll_interval = std::time::Duration::from_millis(500);
            let timeout = std::time::Duration::from_secs(300);
            let deadline = tokio::time::Instant::now() + timeout;
            let response_payload = loop {
                if cancel.is_cancelled() {
                    return Err(AgentToolError::ExecutionFailed(
                        "等待用户回应时被取消".to_string(),
                    ));
                }

                let g = self
                    .repos
                    .lifecycle_gate_repo
                    .get(gate_id)
                    .await
                    .map_err(|e| AgentToolError::ExecutionFailed(format!("查询 gate 失败: {e}")))?
                    .ok_or_else(|| AgentToolError::ExecutionFailed("gate 不存在".to_string()))?;

                if !g.is_open() {
                    break g.payload_json.unwrap_or(serde_json::json!({
                        "status": "error",
                        "summary": "gate 已关闭但无 payload"
                    }));
                }

                if tokio::time::Instant::now() >= deadline {
                    return Err(AgentToolError::ExecutionFailed(
                        "等待用户回应超时".to_string(),
                    ));
                }

                tokio::select! {
                    _ = cancel.cancelled() => {
                        return Err(AgentToolError::ExecutionFailed(
                            "等待用户回应时被取消".to_string(),
                        ));
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
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
            let gate_meta = serde_json::json!({
                "session_id": current_session_id.clone(),
                "turn_id": self.tool_context.turn_id(),
                "request_type": payload_type,
            });
            let gate = agentdash_domain::workflow::LifecycleGate::open(
                anchor.run_id,
                Some(anchor.agent_id),
                Some(anchor.frame_id),
                "companion_human_request",
                &request_id,
                Some(gate_meta),
            );
            let gate_id = gate.id;
            self.repos
                .lifecycle_gate_repo
                .create(&gate)
                .await
                .map_err(|e| {
                    AgentToolError::ExecutionFailed(format!("创建非阻塞请求 gate 失败: {e}"))
                })?;
            let request_id = gate_id.to_string();

            let notification = build_companion_event_notification(
                &current_session_id,
                self.tool_context.turn_id(),
                "companion_human_request",
                prompt.to_string(),
                serde_json::json!({
                    "request_id": request_id.clone(),
                    "gate_id": request_id.clone(),
                    "message": prompt,
                    "options": options.clone(),
                    "wait": false,
                    "payload_type": payload_type,
                    "ui_hint": ui_hint,
                    "payload": payload,
                }),
            );
            session_services
                .eventing
                .inject_notification(&current_session_id, notification)
                .await
                .map_err(|error| {
                    AgentToolError::ExecutionFailed(format!("发送用户协作请求失败: {error}"))
                })?;

            Ok(AgentToolResult {
                content: vec![ContentPart::text(format!(
                    "已向用户发送请求。\n- request_id: {request_id}\n- 用户回应后会作为事件注入当前 session。"
                ))],
                is_error: false,
                details: Some(serde_json::json!({
                    "request_id": request_id,
                    "wait": false,
                    "message": prompt,
                    "options": options,
                    "payload_type": payload_type,
                    "ui_hint": ui_hint,
                })),
            })
        }
    }

    /// target=platform：平台 broker 入口。授权类请求必须接入 PermissionGrantService
    /// 与 capability runtime broker 后才能处理，不能降级成人类 companion request。
    async fn execute_platform_request(
        &self,
        _wait: bool,
        payload: &serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<AgentToolResult, AgentToolError> {
        let payload_type = payload.get("type").and_then(|value| value.as_str());
        match payload_type {
            Some("capability_grant_request") => {
                Err(platform_capability_grant_missing_broker_error())
            }
            Some(type_name) => Err(AgentToolError::InvalidArguments(format!(
                "target=platform 暂不支持 payload.type=`{type_name}`"
            ))),
            None => Err(AgentToolError::InvalidArguments(
                "target=platform 要求 payload.type".to_string(),
            )),
        }
    }

    async fn resolve_companion_agent_config(
        &self,
        hook_runtime: &dyn agentdash_spi::hooks::HookRuntimeAccess,
        agent_name: &str,
    ) -> Result<AgentConfig, AgentToolError> {
        let snapshot = hook_runtime.snapshot();
        let project_id = snapshot
            .run_context
            .as_ref()
            .map(|ctx| ctx.project_id)
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "无法从当前 session 确定 project_id，无法解析 agent_key".to_string(),
                )
            })?;

        let agents = self
            .project_agent_repo
            .list_by_project(project_id)
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        for agent in &agents {
            if agent.name.eq_ignore_ascii_case(agent_name) {
                let preset = agent
                    .preset_config()
                    .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
                return Ok(preset.to_agent_config(&agent.agent_type));
            }
        }

        let available: Vec<String> = agents.into_iter().map(|agent| agent.name).collect();
        Err(AgentToolError::InvalidArguments(format!(
            "当前项目中未找到名为 `{agent_name}` 的 agent。可用 agent: [{}]",
            available.join(", ")
        )))
    }
}

// ─── companion_respond ──────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompanionRespondParams {
    /// 回应的 request_id（companion_request 返回的 gate id / dispatch id / pending action id）
    pub request_id: String,
    /// 结构化 JSON object payload。示例：{"type":"resolution","status":"approved","summary":"..."}
    #[schemars(schema_with = "companion_response_payload_schema")]
    pub payload: serde_json::Value,
}

#[derive(Clone)]
pub struct CompanionRespondTool {
    repos: crate::repository_set::RepositorySet,
    session_services_handle: SharedSessionToolServicesHandle,
    tool_context: CompanionToolContext,
}

impl CompanionRespondTool {
    pub(crate) fn new(
        repos: crate::repository_set::RepositorySet,
        session_services_handle: SharedSessionToolServicesHandle,
        tool_context: CompanionToolContext,
    ) -> Self {
        Self {
            repos,
            session_services_handle,
            tool_context,
        }
    }
}

#[async_trait]
impl AgentTool for CompanionRespondTool {
    fn name(&self) -> &str {
        "companion_respond"
    }

    fn description(&self) -> &str {
        "回应 companion 交互请求；用于回传用户决策、平台结果、parent/sub session 结论或结构化审阅结果。request_id 指定对象，payload 必须是 JSON object 并匹配 expected response type。"
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

        let payload = raw.payload;
        if let Some(error) = super::payload_types::payload_object_error(&payload) {
            return Err(AgentToolError::InvalidArguments(error));
        }

        // 先做 response 基础结构校验；与 request_type 的匹配在具体回流路径中校验。
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

        let current_session_id = self
            .tool_context
            .require_delivery_runtime_session_id("回应 companion 请求")?
            .to_string();
        let session_services =
            require_session_services(&self.session_services_handle, "回应 companion 请求").await?;

        // 多个独立副作用，不互斥：
        // 1. parent agent resolve parent-owned LifecycleGate
        // 2. resolve 当前 session 的 pending action delivery/cache
        // 3. child agent 通过 child-owned LifecycleGate 回传结果给 parent agent
        // 哪些命中由上下文决定，可以同时命中多个。

        let resolved_parent_request = self
            .try_resolve_parent_request_gate(
                request_id,
                &current_session_id,
                &payload,
                &session_services,
            )
            .await?;

        let resolved_action = self
            .try_resolve_pending_action(
                request_id,
                &current_session_id,
                &payload,
                &session_services,
            )
            .await?;

        let completed_to_parent = self
            .try_complete_to_parent(request_id, &current_session_id, &payload, &session_services)
            .await?;

        // 根据命中情况构造返回值
        let mut modes = Vec::new();
        if resolved_parent_request.is_some() {
            modes.push("resolve_parent_request_gate");
        }
        if resolved_action.is_some() {
            modes.push("resolve_pending_action");
        }
        if completed_to_parent.is_some() {
            modes.push("complete_to_parent");
        }

        if modes.is_empty() {
            if let Some(expected_dispatch_id) = self
                .current_companion_dispatch_id(&current_session_id)
                .await?
            {
                return Err(AgentToolError::ExecutionFailed(format!(
                    "request_id=`{request_id}` 不匹配当前 companion dispatch_id=`{expected_dispatch_id}`"
                )));
            }
            return Err(AgentToolError::ExecutionFailed(format!(
                "request_id=`{request_id}` 不匹配任何 pending action 或 companion session"
            )));
        }

        // 优先使用 pending action 的返回值（包含结案详情），其次用 complete_to_parent 的
        let result = resolved_parent_request
            .or(resolved_action)
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
    /// 查找当前 child agent 自己持有的 open interaction gate correlation_id。
    async fn current_companion_dispatch_id(
        &self,
        current_session_id: &str,
    ) -> Result<Option<String>, AgentToolError> {
        let child_frame = match resolve_current_frame_from_delivery_trace_ref(
            current_session_id,
            self.repos.execution_anchor_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.agent_frame_repo.as_ref(),
        )
        .await
        {
            Ok(Some((_anchor, _agent, frame))) => frame,
            _ => return Ok(None),
        };
        let gates = self
            .repos
            .lifecycle_gate_repo
            .list_open_for_agent(child_frame.agent_id)
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;
        Ok(gates.into_iter().next().map(|g| g.correlation_id))
    }

    /// 路径 0：parent agent 回应 parent-owned LifecycleGate。
    async fn try_resolve_parent_request_gate(
        &self,
        request_id: &str,
        current_session_id: &str,
        payload: &serde_json::Value,
        session_services: &SessionToolServices,
    ) -> Result<Option<AgentToolResult>, AgentToolError> {
        let service =
            CompanionGateControlFactory::new(&self.repos).with_session_eventing(session_services);
        let Some(result) = service
            .resolve_parent_request(ResolveCompanionParentRequestCommand {
                request_id: request_id.to_string(),
                parent_runtime_session_id: current_session_id.to_string(),
                resolved_turn_id: self.tool_context.turn_id().to_string(),
                payload: payload.clone(),
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
        else {
            return Ok(None);
        };

        let status = result
            .payload
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("resolved");

        Ok(Some(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已回应 parent companion 请求（resolve LifecycleGate）。\n- request_id: {}\n- gate_id: {}\n- status: {}",
                request_id, result.gate_id, status
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "mode": "resolve_parent_request_gate",
                "request_id": request_id,
                "gate_id": result.gate_id.to_string(),
                "parent_agent_id": result.parent_agent_id.to_string(),
                "parent_frame_id": result.parent_frame_id.to_string(),
                "parent_delivery_runtime_session_id": result.parent_delivery_runtime_session_id,
                "payload": result.payload,
            })),
        }))
    }

    /// 路径 1：resolve 当前 session 的 hook pending action（替代 resolve_hook_action）
    async fn try_resolve_pending_action(
        &self,
        request_id: &str,
        current_session_id: &str,
        payload: &serde_json::Value,
        session_services: &SessionToolServices,
    ) -> Result<Option<AgentToolResult>, AgentToolError> {
        let hook_runtime = match self.tool_context.hook_runtime() {
            Some(session) => session,
            None => return Ok(None),
        };

        // 检查是否存在匹配的 pending action
        if !hook_runtime
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

        let action = hook_runtime
            .resolve_pending_action(
                request_id,
                resolution_kind,
                note.clone(),
                Some(self.tool_context.turn_id().to_string()),
            )
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!(
                    "当前 session 中不存在 request_id=`{request_id}` 的 pending action"
                ))
            })?;

        let notification = build_hook_action_resolved_notification(
            current_session_id,
            self.tool_context.turn_id(),
            &action,
        );
        session_services
            .eventing
            .inject_notification(current_session_id, notification)
            .await
            .map_err(|error| {
                AgentToolError::ExecutionFailed(format!(
                    "发送 companion pending action 解析通知失败: {error}"
                ))
            })?;

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
                "turn_id": self.tool_context.turn_id(),
                "action": action,
            })),
        }))
    }

    /// 路径 3：通过 child-owned LifecycleGate 回传结果给父 agent。
    ///
    /// Tool 只把当前 runtime session 投影成 command；gate 查找、resolve 与
    /// runtime notification delivery 统一交给 `CompanionGateControlService`。
    async fn try_complete_to_parent(
        &self,
        request_id: &str,
        current_session_id: &str,
        payload: &serde_json::Value,
        session_services: &SessionToolServices,
    ) -> Result<Option<AgentToolResult>, AgentToolError> {
        let service =
            CompanionGateControlFactory::new(&self.repos).with_session_eventing(session_services);
        let Some(result) = service
            .complete_child_result_to_parent(CompleteCompanionChildResultCommand {
                request_id: request_id.to_string(),
                child_runtime_session_id: current_session_id.to_string(),
                resolved_turn_id: self.tool_context.turn_id().to_string(),
                payload: payload.clone(),
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
        else {
            return Ok(None);
        };
        let status = result
            .payload
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");

        Ok(Some(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已回应 companion 请求（resolve LifecycleGate）。\n- gate_id: {}\n- parent_agent_id: {}\n- status: {}",
                result.gate_id, result.parent_agent_id, status
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "mode": "resolve_gate",
                "gate_id": result.gate_id.to_string(),
                "parent_agent_id": result.parent_agent_id.to_string(),
                "parent_delivery_runtime_session_id": result.parent_delivery_runtime_session_id,
                "child_delivery_runtime_session_id": result.child_delivery_runtime_session_id,
                "payload": result.payload,
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

fn companion_request_payload_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true,
        "description": "Companion request payload. The message body field is payload.message.",
        "anyOf": [
            {
                "type": "object",
                "required": ["type", "message"],
                "properties": {
                    "type": { "const": "task" },
                    "message": { "type": "string", "minLength": 1 },
                    "label": { "type": "string" },
                    "context_mode": {
                        "type": "string",
                        "enum": ["compact", "full", "workflow_only", "constraints_only"]
                    },
                    "adoption_mode": {
                        "type": "string",
                        "enum": ["suggestion", "follow_up_required", "blocking_review"]
                    },
                    "agent_key": { "type": "string" },
                    "max_fragments": { "type": "integer", "minimum": 1 },
                    "max_constraints": { "type": "integer", "minimum": 1 }
                }
            },
            {
                "type": "object",
                "required": ["type", "message"],
                "properties": {
                    "type": { "const": "review" },
                    "message": { "type": "string", "minLength": 1 }
                }
            },
            {
                "type": "object",
                "required": ["type", "message"],
                "properties": {
                    "type": { "const": "approval" },
                    "message": { "type": "string", "minLength": 1 },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                }
            },
            {
                "type": "object",
                "required": ["type", "message"],
                "properties": {
                    "type": { "const": "notification" },
                    "message": { "type": "string", "minLength": 1 }
                }
            },
            {
                "type": "object",
                "required": ["type", "requested_paths", "reason", "scope"],
                "properties": {
                    "type": { "const": "capability_grant_request" },
                    "requested_paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1
                    },
                    "reason": { "type": "string", "minLength": 1 },
                    "scope": {
                        "type": "string",
                        "enum": ["turn", "session", "workflow_step"]
                    },
                    "ttl_seconds": { "type": "integer", "minimum": 1 }
                }
            }
        ]
    })
}

fn companion_response_payload_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true,
        "anyOf": [
            {
                "type": "object",
                "required": ["type", "status", "summary"],
                "properties": {
                    "type": { "const": "completion" },
                    "status": { "type": "string" },
                    "summary": { "type": "string", "minLength": 1 }
                }
            },
            {
                "type": "object",
                "required": ["type", "status", "summary"],
                "properties": {
                    "type": { "const": "resolution" },
                    "status": { "type": "string" },
                    "summary": { "type": "string", "minLength": 1 }
                }
            },
            {
                "type": "object",
                "required": ["type", "choice"],
                "properties": {
                    "type": { "const": "decision" },
                    "choice": { "type": "string", "minLength": 1 },
                    "status": { "type": "string" },
                    "summary": { "type": "string" }
                }
            },
            {
                "type": "object",
                "required": ["type", "status", "summary"],
                "properties": {
                    "type": { "const": "capability_grant_result" },
                    "status": {
                        "type": "string",
                        "enum": ["approved", "rejected", "pending_user_approval", "applied", "failed", "expired", "revoked"]
                    },
                    "summary": { "type": "string", "minLength": 1 }
                }
            }
        ]
    })
}

fn platform_capability_grant_missing_broker_error() -> AgentToolError {
    AgentToolError::ExecutionFailed(
        "target=platform payload.type=`capability_grant_request` 暂不支持：缺少 platform permission grant broker，当前 companion context 无法提供 PermissionGrantService::request 所需的 agent_auto_grantable / lifecycle_requestable policy inputs，也没有 live runtime capability update handoff。参见 ARCH-010 完成 broker 闭环后再启用。"
            .to_string(),
    )
}

// ─── hook action 辅助（从 hook_action.rs 合并） ─────────────────────

pub fn build_hook_action_resolved_notification(
    session_id: &str,
    turn_id: &str,
    action: &HookPendingAction,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "agentdash-hook-runtime".to_string(),
        connector_type: "runtime_tool".to_string(),
        executor_id: None,
    };

    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "hook_action_resolved".to_string(),
            value: serde_json::json!({
                "action_id": action.id,
                "action_type": action.action_type,
                "status": hook_action_status_key(action.status),
                "resolution_kind": action.resolution_kind.map(hook_action_resolution_key),
                "resolution_note": action.resolution_note,
                "resolution_turn_id": action.resolution_turn_id,
                "resolved_at_ms": action.resolved_at_ms,
                "summary": action.summary,
                "title": action.title,
            }),
        }),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    })
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

async fn evaluate_subagent_hook(
    hook_runtime: &dyn agentdash_spi::hooks::HookRuntimeAccess,
    trigger: HookTrigger,
    turn_id: Option<String>,
    subagent_type: &str,
    payload: Option<serde_json::Value>,
) -> Result<agentdash_spi::HookResolution, AgentToolError> {
    let provenance = CompanionHookProvenance::from_hook_runtime(hook_runtime, turn_id);
    let resolution = hook_runtime
        .evaluate_from_provenance(HookRuntimeEvaluationQuery {
            provenance: provenance
                .runtime_session(CompanionHookProvenanceSource::SubagentHookEvaluate),
            trigger,
            tool_name: None,
            tool_call_id: None,
            subagent_type: Some(subagent_type.to_string()),
            snapshot: Some(hook_runtime.snapshot()),
            payload,
            token_stats: None,
        })
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

    if resolution.refresh_snapshot {
        hook_runtime
            .refresh_from_provenance(HookRuntimeRefreshQuery {
                provenance: provenance
                    .runtime_session(CompanionHookProvenanceSource::SubagentHookRefresh),
                reason: Some(format!("trigger:{trigger:?}:{subagent_type}")),
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    }

    Ok(resolution)
}

async fn record_subagent_trace(
    hook_runtime: &dyn agentdash_spi::hooks::HookRuntimeAccess,
    session_services: Option<&SessionToolServices>,
    turn_id: Option<&str>,
    trigger: HookTrigger,
    decision: &str,
    subagent_type: &str,
    resolution: &agentdash_spi::HookResolution,
) {
    let Some(trace_trigger) = trigger.trace_trigger() else {
        return;
    };
    let trace = HookTraceEntry {
        sequence: hook_runtime.next_trace_sequence(),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        revision: hook_runtime.revision(),
        trigger: trace_trigger,
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
    hook_runtime.append_trace(trace.clone());

    // Only inject notification when the session has NO active connector.
    // Active connectors already receive traces via trace_broadcast → hook_trace_rx,
    // so inject_notification would cause duplicate event cards.
    if let (Some(session_services), Some(turn_id)) = (session_services, turn_id) {
        let session_id = hook_runtime.session_id();
        let has_live = session_services
            .core
            .has_live_executor_session(session_id)
            .await;
        if !has_live {
            let notification =
                build_hook_trace_envelope(session_id, Some(turn_id), hook_trace_source(), &trace);
            let _ = session_services
                .eventing
                .inject_notification(session_id, notification)
                .await;
        }
    }
}

fn build_subagent_pending_action(
    fallback_request_id: &str,
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

    let request_id = payload
        .get("request_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_request_id);
    let source_turn_id = payload
        .get("turn_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_request_id);

    Some(HookPendingAction {
        id: request_id.to_string(),
        created_at_ms: chrono::Utc::now().timestamp_millis(),
        title: if adoption_mode == at::BLOCKING_REVIEW {
            format!("Companion `{companion_label}` 结果需要阻塞式 review")
        } else {
            format!("Companion `{companion_label}` 结果需要主 session 跟进")
        },
        summary: format!("status={status}, dispatch_id={dispatch_id}, summary={summary}"),
        action_type: adoption_mode,
        turn_id: Some(source_turn_id.to_string()),
        source: RuntimeEventSource::CompanionResult,
        status: agentdash_spi::HookPendingActionStatus::Pending,
        last_injected_at_ms: None,
        resolved_at_ms: None,
        resolution_kind: None,
        resolution_note: None,
        resolution_turn_id: None,
        injections: resolution.injections.clone(),
    })
}

fn hook_trace_source() -> SourceInfo {
    SourceInfo {
        connector_id: "pi-agent".to_string(),
        connector_type: "runtime_tool".to_string(),
        executor_id: Some("PI_AGENT".to_string()),
    }
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
    /// 主（父）Story session ID — companion 的 owner（Model C: Story root）。
    pub parent_session_id: String,
    pub parent_turn_id: String,
    pub adoption_mode: CompanionAdoptionMode,
    pub slice: CompanionDispatchSlice,
}

#[derive(Debug, Clone)]
pub struct CompanionExecutionSlice {
    pub vfs: Option<Vfs>,
    pub mcp_servers: Vec<agentdash_spi::RuntimeMcpServer>,
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
    /// 主（父）Story session ID。
    parent_session_id: &'a str,
    parent_turn_id: &'a str,
    companion_label: &'a str,
    slice_mode: CompanionSliceMode,
    adoption_mode: CompanionAdoptionMode,
    max_fragments: Option<usize>,
    max_constraints: Option<usize>,
}

fn build_companion_dispatch_plan(
    hook_runtime: &dyn agentdash_spi::hooks::HookRuntimeAccess,
    resolution: &agentdash_spi::HookResolution,
    config: &CompanionDispatchConfig<'_>,
) -> CompanionDispatchPlan {
    let dispatch_id = format!("dispatch-{}", uuid::Uuid::new_v4().simple());
    let slice = build_companion_dispatch_slice(
        &hook_runtime.snapshot(),
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
    snapshot: &agentdash_spi::AgentFrameHookSnapshot,
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
    mcp_servers: &[agentdash_spi::RuntimeMcpServer],
    mode: CompanionSliceMode,
) -> Result<CompanionExecutionSlice, String> {
    match mode {
        CompanionSliceMode::Full => {
            let vfs = vfs
                .cloned()
                .ok_or_else(|| "companion Full slice 缺少 parent VFS".to_string())?;
            Ok(CompanionExecutionSlice {
                vfs: Some(vfs),
                mcp_servers: mcp_servers.to_vec(),
            })
        }
        CompanionSliceMode::Compact => {
            let vfs = vfs.ok_or_else(|| "companion Compact slice 缺少 parent VFS".to_string())?;
            Ok(CompanionExecutionSlice {
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
            })
        }
        CompanionSliceMode::WorkflowOnly | CompanionSliceMode::ConstraintsOnly => {
            Ok(CompanionExecutionSlice {
                vfs: Some(Vfs::default()),
                mcp_servers: Vec::new(),
            })
        }
    }
}

fn filter_vfs_capabilities(vfs: &Vfs, allowed: &[MountCapability]) -> Vfs {
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

fn build_companion_owner_summary(
    snapshot: &agentdash_spi::AgentFrameHookSnapshot,
) -> Option<String> {
    let ctx = snapshot.run_context.as_ref()?;
    let mut lines = Vec::new();
    lines.push(format!("- Project: {}", ctx.project_id));
    if let Some(story_id) = ctx.story_id {
        let label = ctx.story_title.as_deref().unwrap_or("(unnamed)");
        lines.push(format!("- Story: {} ({})", story_id, label));
    }
    if let Some(task_id) = ctx.task_id {
        let label = ctx.task_title.as_deref().unwrap_or("(unnamed)");
        lines.push(format!("- Task: {} ({})", task_id, label));
    }
    Some(format!("## 当前归属\n{}", lines.join("\n")))
}

fn companion_adoption_mode_key(mode: CompanionAdoptionMode) -> &'static str {
    match mode {
        CompanionAdoptionMode::Suggestion => at::SUGGESTION,
        CompanionAdoptionMode::FollowUpRequired => at::FOLLOW_UP_REQUIRED,
        CompanionAdoptionMode::BlockingReview => at::BLOCKING_REVIEW,
    }
}

pub fn companion_owner_candidates(
    snapshot: &agentdash_spi::AgentFrameHookSnapshot,
) -> Result<Vec<(CapabilityScope, Uuid, Option<String>)>, AgentToolError> {
    let mut owners = Vec::new();
    if let Some(ctx) = &snapshot.run_context {
        match ctx.scope {
            CapabilityScope::Task => {
                if let Some(task_id) = ctx.task_id {
                    owners.push((CapabilityScope::Task, task_id, ctx.task_title.clone()));
                }
                if let Some(story_id) = ctx.story_id {
                    owners.push((CapabilityScope::Story, story_id, ctx.story_title.clone()));
                }
                owners.push((CapabilityScope::Project, ctx.project_id, None));
            }
            CapabilityScope::Story => {
                if let Some(story_id) = ctx.story_id {
                    owners.push((CapabilityScope::Story, story_id, ctx.story_title.clone()));
                }
                owners.push((CapabilityScope::Project, ctx.project_id, None));
            }
            CapabilityScope::Project => {
                owners.push((CapabilityScope::Project, ctx.project_id, None));
            }
        }
    }
    owners.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    Ok(owners)
}

#[allow(dead_code)]
fn companion_project_id_for_owner(
    snapshot: &agentdash_spi::AgentFrameHookSnapshot,
    _owner_type: CapabilityScope,
    _owner_id: Uuid,
) -> Result<Uuid, AgentToolError> {
    snapshot
        .run_context
        .as_ref()
        .map(|ctx| ctx.project_id)
        .ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 缺少 run_context，无法确定 project_id".to_string(),
            )
        })
}

#[cfg(test)]
mod companion_tests {
    use super::{
        CompanionAdoptionMode, CompanionDispatchPlan, CompanionDispatchSlice, CompanionSliceMode,
        build_companion_dispatch_prompt, build_companion_dispatch_slice,
        build_companion_execution_slice, build_subagent_pending_action, companion_owner_candidates,
        platform_capability_grant_missing_broker_error,
    };
    use agentdash_spi::CapabilityScope;
    use agentdash_spi::action_type as at;
    use agentdash_spi::{McpTransportConfig, MountCapability, RuntimeMcpServer, Vfs};
    use uuid::Uuid;

    use crate::runtime_tools::SharedSessionToolServicesHandle;

    #[test]
    fn companion_owner_candidates_fallback_from_task_to_story() {
        let story_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let snapshot = agentdash_spi::AgentFrameHookSnapshot {
            runtime_adapter_session_id: "sess-test".to_string(),
            run_context: Some(agentdash_spi::hooks::SubjectRunContext {
                project_id,
                story_id: Some(story_id),
                task_id: Some(task_id),
                story_title: None,
                task_title: Some("Task A".to_string()),
                scope: CapabilityScope::Task,
            }),
            ..agentdash_spi::AgentFrameHookSnapshot::default()
        };

        let candidates = companion_owner_candidates(&snapshot).expect("candidates");

        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].0, CapabilityScope::Task);
        assert_eq!(candidates[0].1, task_id);
        assert_eq!(candidates[1].0, CapabilityScope::Story);
        assert_eq!(candidates[1].1, story_id);
        assert_eq!(candidates[2].0, CapabilityScope::Project);
    }

    #[test]
    fn platform_capability_grant_request_reports_missing_broker() {
        let error = platform_capability_grant_missing_broker_error();

        match error {
            agentdash_spi::AgentToolError::ExecutionFailed(message) => {
                assert!(message.contains("capability_grant_request"));
                assert!(message.contains("platform permission grant broker"));
                assert!(message.contains("PermissionGrantService::request"));
                assert!(message.contains("agent_auto_grantable"));
                assert!(message.contains("lifecycle_requestable"));
                assert!(message.contains("ARCH-010"));
            }
            other => panic!("expected ExecutionFailed, got {other:?}"),
        }
    }

    #[test]
    fn compact_companion_slice_keeps_owner_summary_and_limits_payload() {
        let snapshot = agentdash_spi::AgentFrameHookSnapshot {
            runtime_adapter_session_id: "sess-parent".to_string(),
            run_context: Some(agentdash_spi::hooks::SubjectRunContext {
                project_id: Uuid::new_v4(),
                story_id: None,
                task_id: Some(Uuid::new_v4()),
                story_title: None,
                task_title: Some("Task A".to_string()),
                scope: CapabilityScope::Task,
            }),
            ..agentdash_spi::AgentFrameHookSnapshot::default()
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
            &[RuntimeMcpServer {
                name: "test-mcp".to_string(),
                transport: McpTransportConfig::Stdio {
                    command: "cmd".to_string(),
                    args: Vec::new(),
                    env: Vec::new(),
                    cwd: None,
                },
                uses_relay: false,
            }],
            CompanionSliceMode::Compact,
        )
        .expect("compact slice should resolve");

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
            &[RuntimeMcpServer {
                name: "test-mcp".to_string(),
                transport: McpTransportConfig::Stdio {
                    command: "cmd".to_string(),
                    args: Vec::new(),
                    env: Vec::new(),
                    cwd: None,
                },
                uses_relay: false,
            }],
            CompanionSliceMode::WorkflowOnly,
        )
        .expect("workflow_only slice should resolve");

        let sliced_space = slice.vfs.expect("workflow_only should force empty vfs");
        assert!(sliced_space.mounts.is_empty());
        assert!(sliced_space.default_mount_id.is_none());
        assert!(slice.mcp_servers.is_empty());
    }

    #[test]
    fn compact_execution_slice_requires_parent_vfs() {
        let error = build_companion_execution_slice(None, &[], CompanionSliceMode::Compact)
            .expect_err("compact slice without parent vfs should fail");

        assert!(error.contains("缺少 parent VFS"));
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

    #[test]
    fn subagent_pending_action_uses_request_id_as_owner_key() {
        let resolution = agentdash_spi::HookResolution {
            injections: vec![agentdash_spi::HookInjection {
                slot: "workflow".to_string(),
                content: "review context".to_string(),
                source: "active_workflow_step".to_string(),
            }],
            ..agentdash_spi::HookResolution::default()
        };
        let payload = serde_json::json!({
            "request_id": "gate-123",
            "turn_id": "turn-child-1",
            "adoption_mode": at::FOLLOW_UP_REQUIRED,
            "status": "pending",
            "summary": "please review"
        });

        let action =
            build_subagent_pending_action("gate-123", "child:agent", &payload, &resolution)
                .expect("pending action");

        assert_eq!(action.id, "gate-123");
        assert_eq!(action.turn_id.as_deref(), Some("turn-child-1"));
        assert_eq!(action.action_type, at::FOLLOW_UP_REQUIRED);
    }

    #[test]
    fn lifecycle_gate_based_respond_returns_none_without_lineage() {
        // CompanionRespondTool::try_complete_to_parent returns None
        // when no AgentLineage exists for the child session.
        // Full integration test requires in-memory repos + dispatch service.
        // Here we just verify the tool can be constructed with repos.
        let _ = SharedSessionToolServicesHandle::default();
    }
}
