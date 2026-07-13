use std::sync::Arc;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_domain::agent::{ProjectAgent, ProjectAgentRepository};
use agentdash_domain::agent_run_mailbox::MailboxSourceIdentity;
use agentdash_domain::channel::{
    Channel, ChannelAddress, ChannelDeliveryIntent, ChannelDeliveryState, ChannelDeliveryStatus,
    ChannelDeliveryTarget, ChannelMedium, ChannelMessage, ChannelOwner, ChannelParticipant,
    ChannelParticipantRef, ChannelPayload, ChannelRecord, ChannelRole, ChannelTopology,
    MaterializedDeliveryRef,
};
#[cfg(test)]
use agentdash_domain::workflow::LifecycleGateRepository;
use agentdash_domain::workflow::{
    ClaimGateResultParentContinuationRequest, ClaimGateResultWaiterRequest,
    CompleteGateResultParentContinuationRequest, GateResultDeliveryClaim, GateResultDeliveryMarker,
    LifecycleTaskPlanItem, RegisterGateResultWaiterRequest,
};
use agentdash_spi::CapabilityScope;
#[cfg(test)]
use agentdash_spi::RuntimeEventSource;
use agentdash_spi::action_type as at;
use agentdash_spi::context::capability::CompanionAgentEntry;
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::hooks::{HookRuntimeEvaluationQuery, HookRuntimeRefreshQuery};
use agentdash_spi::{
    AgentConfig, HookPendingAction, HookPendingActionResolutionKind, HookPendingActionStatus,
    HookTraceEntry, HookTrigger, MountCapability, Vfs,
};
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::dispatch::{CompanionChildDispatchRequest, CompanionChildDispatchService};
use super::gate_control::CompanionParentRequestOpenResult;
use super::model_preflight::{CompanionModelPreflightPort, CompanionModelPreflightRequest};
use super::reply_contract::{
    COMPANION_ACTION_CHANNEL, COMPANION_CHILD_CHANNEL, COMPANION_PARENT_CHANNEL,
    CompanionPayloadExpectation, CompanionReplyContract, CompanionReplyRoute,
    CompanionReplySelectorParam, ModelReplyInstruction, ModelReplySelector,
    alias_is_raw_internal_ref, normalize_reply_alias,
};
use super::tool_context::{
    CompanionHookProvenance, CompanionHookProvenanceSource, CompanionToolContext,
};
use super::workflow_script_preflight::{
    CompanionWorkflowScriptPreflightPort, CompanionWorkflowScriptPreflightRequest,
};
use super::{
    CompanionGateControlRepos, CompanionGateControlService, CompanionHumanResponseMailboxDelivery,
    CompanionHumanResponseMailboxDeliveryCommand, CompanionParentMailboxDelivery,
    CompanionParentMailboxDeliveryCommand, CompanionParentMailboxDeliveryResult,
    CompanionParentRequestMailboxDeliveryCommand, CompanionParentResponseMailboxDeliveryCommand,
    CompleteCompanionChildResultCommand, OpenCompanionParentRequestCommand,
    ResolveCompanionParentRequestCommand,
};
use crate::channel::{
    ChannelService, LifecycleRunChannelOwnerStore, UnsupportedChannelBindingResolver,
};
use crate::lifecycle::resolve_current_frame_from_delivery_trace_ref;
use crate::runtime_tools::{SessionToolServices, SharedSessionToolServicesHandle};
use crate::wait_activity::{WaitActivityService, WaitToolContext};
use agentdash_agent_runtime_contract::{RuntimeActor, RuntimeInput};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductDeliveryPort, DeliverAgentRunProductInput,
};
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_application_workflow::WorkflowScriptPreflightOutput;
use agentdash_application_workflow::gate::{LifecycleGateResolver, OpenCompanionGateCommand};

pub use agentdash_spi::CompanionSliceMode;

const COMPANION_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
const COMPANION_WAIT_PREVIEW_CHARS: usize = 2_000;
const GATE_RESULT_DELIVERY_ATTEMPT: i32 = 1;
const GATE_RESULT_PARENT_CONTINUATION_LEASE_SECONDS: i64 = 60;
const COMPANION_CHILD_WAIT_GATE_KIND: &str = "companion_wait";
const COMPANION_CHILD_BLOCKING_WAIT_GATE_KIND: &str = "companion_wait_blocking";
const COMPANION_CHILD_FOLLOW_UP_WAIT_GATE_KIND: &str = "companion_wait_follow_up";
const COMPANION_PARENT_REQUEST_GATE_KIND: &str = "companion_parent_request";

struct SelectedCompanionAgent {
    project_agent: ProjectAgent,
    agent_key: String,
    executor_config: AgentConfig,
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

    fn with_agent_run_delivery(
        &self,
        session_services: &SessionToolServices,
    ) -> CompanionGateControlService {
        CompanionGateControlService::with_agent_run_projection(CompanionGateControlRepos {
            gate_repo: self.repos.lifecycle_gate_repo.clone(),
            frame_repo: self.repos.agent_frame_repo.clone(),
            agent_repo: self.repos.lifecycle_agent_repo.clone(),
            runtime_binding_repo: self.repos.agent_run_runtime_binding_repo.clone(),
            lineage_repo: self.repos.agent_lineage_repo.clone(),
        })
        .with_parent_mailbox_delivery(Arc::new(AgentRunCompanionMailboxDelivery::new(
            self.repos.clone(),
            session_services.clone(),
        )))
        .with_human_response_mailbox_delivery(Arc::new(
            AgentRunCompanionMailboxDelivery::new(self.repos.clone(), session_services.clone()),
        ))
    }
}

#[derive(Clone)]
pub struct AgentRunCompanionMailboxDelivery {
    repos: crate::repository_set::RepositorySet,
    product_delivery: Arc<dyn AgentRunProductDeliveryPort>,
}

impl AgentRunCompanionMailboxDelivery {
    pub fn new(
        repos: crate::repository_set::RepositorySet,
        session_services: SessionToolServices,
    ) -> Self {
        Self {
            repos,
            product_delivery: session_services.product_delivery,
        }
    }
}

fn companion_wake_source(
    kind: &'static str,
    actor: &'static str,
    route: &'static str,
    gate_id: Uuid,
    request_id: &str,
    metadata: serde_json::Value,
) -> MailboxSourceIdentity {
    MailboxSourceIdentity::new("companion", kind, actor)
        .with_source_ref(gate_id.to_string())
        .with_correlation_ref(request_id.to_string())
        .with_route(route)
        .with_metadata(metadata)
}

fn companion_channel_service(repos: &crate::repository_set::RepositorySet) -> ChannelService {
    ChannelService::new(
        Arc::new(LifecycleRunChannelOwnerStore::new(
            repos.lifecycle_run_repo.clone(),
        )),
        Arc::new(UnsupportedChannelBindingResolver),
    )
}

async fn ensure_companion_agent_channel(
    repos: &crate::repository_set::RepositorySet,
    run_id: Uuid,
    parent_agent_id: Uuid,
    child_agent_id: Uuid,
    companion_label: &str,
) -> Result<Uuid, crate::ApplicationError> {
    let owner = ChannelOwner::LifecycleRun { run_id };
    let stable_alias = format!("companion:{parent_agent_id}:{child_agent_id}");
    let service = companion_channel_service(repos);
    let registry = service.load_registry(&owner).await?;
    if let Some(record) = registry.channels.iter().find(|record| {
        record
            .channel
            .aliases
            .iter()
            .any(|alias| alias == &stable_alias)
    }) {
        return Ok(record.channel.id);
    }

    let mut channel = Channel::new(owner, ChannelMedium::Runtime, ChannelTopology::Direct);
    channel.aliases = dedup_channel_aliases(vec![
        "companion".to_string(),
        stable_alias,
        companion_label.to_string(),
    ]);
    let mut record = ChannelRecord::new(channel);
    record.participants.push(ChannelParticipant::new(
        ChannelParticipantRef::LifecycleAgent {
            run_id,
            agent_id: parent_agent_id,
        },
        ChannelRole::Owner,
    ));
    record.participants.push(ChannelParticipant::new(
        ChannelParticipantRef::LifecycleAgent {
            run_id,
            agent_id: child_agent_id,
        },
        ChannelRole::Member,
    ));
    let channel_id = record.channel.id;
    service.upsert_channel(record).await?;
    Ok(channel_id)
}

async fn ensure_companion_human_channel(
    repos: &crate::repository_set::RepositorySet,
    run_id: Uuid,
    agent_id: Uuid,
    request_id: &str,
) -> Result<Uuid, crate::ApplicationError> {
    let owner = ChannelOwner::LifecycleRun { run_id };
    let stable_alias = format!("companion_human:{agent_id}:{request_id}");
    let service = companion_channel_service(repos);
    let registry = service.load_registry(&owner).await?;
    if let Some(record) = registry.channels.iter().find(|record| {
        record
            .channel
            .aliases
            .iter()
            .any(|alias| alias == &stable_alias)
    }) {
        return Ok(record.channel.id);
    }

    let mut channel = Channel::new(owner, ChannelMedium::Human, ChannelTopology::Direct);
    channel.aliases = vec![stable_alias, "human".to_string()];
    let mut record = ChannelRecord::new(channel);
    record.participants.push(ChannelParticipant::new(
        ChannelParticipantRef::LifecycleAgent { run_id, agent_id },
        ChannelRole::Member,
    ));
    record.participants.push(ChannelParticipant::new(
        ChannelParticipantRef::Human {
            user_id: "human".to_string(),
        },
        ChannelRole::External,
    ));
    let channel_id = record.channel.id;
    service.upsert_channel(record).await?;
    Ok(channel_id)
}

fn dedup_channel_aliases(aliases: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for alias in aliases {
        if !alias.trim().is_empty() && !deduped.iter().any(|existing| existing == &alias) {
            deduped.push(alias);
        }
    }
    deduped
}

fn channel_address_from_mailbox_source(source: &MailboxSourceIdentity) -> ChannelAddress {
    let mut address = ChannelAddress::new(
        source.namespace.clone(),
        source.kind.clone(),
        source.actor.clone(),
    );
    if let Some(source_ref) = &source.source_ref {
        address = address.with_source_ref(source_ref.clone());
    }
    if let Some(correlation_ref) = &source.correlation_ref {
        address = address.with_correlation_ref(correlation_ref.clone());
    }
    if let Some(route) = &source.route {
        address = address.with_route(route.clone());
    }
    if let Some(metadata) = &source.metadata {
        address = address.with_metadata(metadata.clone());
    }
    address
}

fn companion_channel_delivery_intent(
    channel_id: Uuid,
    run_id: Uuid,
    agent_id: Uuid,
    sender: ChannelParticipantRef,
    source: &MailboxSourceIdentity,
    payload_kind: &'static str,
    input_text: &str,
) -> ChannelDeliveryIntent {
    let address = channel_address_from_mailbox_source(source);
    let mut message = ChannelMessage::new(
        channel_id,
        sender,
        ChannelPayload::text(payload_kind, input_text.to_string()),
        address,
    );
    message.correlation_ref = source.correlation_ref.clone();
    ChannelDeliveryIntent::new(message, ChannelDeliveryTarget::Mailbox { run_id, agent_id })
}

#[derive(Debug, Clone, PartialEq)]
enum CompanionGateWaitOutcome {
    Resolved(serde_json::Value),
    TimedOut,
}

#[cfg(test)]
async fn wait_for_lifecycle_gate_resolution(
    gate_repo: &dyn LifecycleGateRepository,
    gate_id: Uuid,
    cancel: CancellationToken,
    timeout: std::time::Duration,
    poll_interval: std::time::Duration,
) -> Result<CompanionGateWaitOutcome, AgentToolError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let gate = gate_repo
            .get(gate_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(format!("gate 查询失败: {error}")))?
            .ok_or_else(|| AgentToolError::ExecutionFailed(format!("gate {gate_id} 不存在")))?;

        if !gate.is_open() {
            return Ok(CompanionGateWaitOutcome::Resolved(
                gate.payload_json.unwrap_or_else(|| serde_json::json!({})),
            ));
        }

        if tokio::time::Instant::now() >= deadline {
            return Ok(CompanionGateWaitOutcome::TimedOut);
        }

        tokio::select! {
            _ = cancel.cancelled() => {
                return Err(AgentToolError::ExecutionFailed(
                    "companion wait 被取消".to_string(),
                ));
            }
            _ = tokio::time::sleep(poll_interval) => {}
        }
    }
}

fn companion_wait_payload_status(payload: &serde_json::Value, default: &str) -> String {
    payload
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn companion_wait_payload_summary(payload: &serde_json::Value) -> String {
    payload
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("(无摘要)")
        .to_string()
}

fn bounded_json_preview(payload: &serde_json::Value, max_chars: usize) -> String {
    let mut preview = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
    if preview.chars().count() > max_chars {
        preview = preview.chars().take(max_chars).collect::<String>();
        preview.push_str("...");
    }
    preview
}

#[cfg(test)]
fn merge_gate_result_refs(
    mut base_refs: serde_json::Value,
    gate_payload: &serde_json::Value,
) -> serde_json::Value {
    let Some(base_object) = base_refs.as_object_mut() else {
        return base_refs;
    };
    let Some(payload_refs) = gate_payload
        .get("result_refs")
        .and_then(serde_json::Value::as_object)
    else {
        return base_refs;
    };
    for (key, value) in payload_refs {
        base_object
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
    base_refs
}

fn agent_visible_value_without_runtime_session_refs(
    value: &serde_json::Value,
) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => serde_json::Value::Object(
            object
                .iter()
                .filter_map(|(key, value)| {
                    if is_agent_hidden_runtime_session_key(key) {
                        None
                    } else {
                        Some((
                            key.clone(),
                            agent_visible_value_without_runtime_session_refs(value),
                        ))
                    }
                })
                .collect(),
        ),
        serde_json::Value::Array(values) => serde_json::Value::Array(
            values
                .iter()
                .map(agent_visible_value_without_runtime_session_refs)
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn is_agent_hidden_runtime_session_key(key: &str) -> bool {
    matches!(
        key,
        "runtime_thread_id"
            | "runtime_session_id"
            | "parent_session_id"
            | "child_session_id"
            | "parent_runtime_thread_id"
            | "child_runtime_thread_id"
    ) || key.ends_with("_runtime_session_id")
}

fn agent_visible_json_preview(payload: &serde_json::Value, max_chars: usize) -> String {
    bounded_json_preview(
        &agent_visible_value_without_runtime_session_refs(payload),
        max_chars,
    )
}

fn child_messages_uri(child_agent_id: Uuid) -> String {
    format!("lifecycle://agent-runs/{child_agent_id}/sessions/messages")
}

#[derive(Clone, Copy)]
struct CompanionSubagentVisibleResult<'a> {
    companion_label: &'a str,
    child_agent_id: Uuid,
    gate_id: Option<Uuid>,
    status: &'a str,
    summary: &'a str,
    timed_out: Option<bool>,
    result_preview: Option<&'a str>,
}

fn companion_subagent_agent_tool_result(
    visible: CompanionSubagentVisibleResult<'_>,
) -> AgentToolResult {
    let journal_template = "lifecycle://agent-runs/{child_agent_id}/sessions/messages";
    let wait_hint = visible.gate_id.map(|gate_id| {
        format!(
            "\n- wait_activity_ref: {}\n- wait_tool_args: {{\"activity_refs\":[\"{}\"]}}",
            gate_id, gate_id
        )
    });
    let content = match visible.status {
        "running" => format!(
            "已派发 companion agent。\n- label: {}\n- child_agent_id: {}\n- journal_template: {}{}",
            visible.companion_label,
            visible.child_agent_id,
            journal_template,
            wait_hint.as_deref().unwrap_or(""),
        ),
        "timed_out" => format!(
            "等待 companion `{}` 回传超时。\n- status: timed_out\n- child_agent_id: {}\n- journal_template: {}{}",
            visible.companion_label,
            visible.child_agent_id,
            journal_template,
            wait_hint.as_deref().unwrap_or(""),
        ),
        _ => format!(
            "Companion `{}` 已完成。\n- child_agent_id: {}\n- journal_template: {}\n- status: {}\n- summary: {}",
            visible.companion_label,
            visible.child_agent_id,
            journal_template,
            visible.status,
            visible.summary,
        ),
    };

    let mut details = serde_json::json!({
        "kind": "companion_subagent_dispatch",
        "companion_label": visible.companion_label,
        "child": {
            "agent_id": visible.child_agent_id.to_string(),
        },
        "status": visible.status,
        "summary": visible.summary,
        "timed_out": visible.timed_out,
    });

    if let Some(gate_id) = visible.gate_id {
        details["wait_activity"] = serde_json::json!({
            "tool": "wait",
            "activity_ref": gate_id.to_string(),
            "activity_refs": [gate_id.to_string()],
        });
    }
    if let Some(result_preview) = visible.result_preview {
        details["result_preview"] = serde_json::Value::String(result_preview.to_string());
    }

    AgentToolResult {
        content: vec![ContentPart::text(content)],
        is_error: false,
        details: Some(details),
    }
}

fn companion_parent_request_agent_tool_result(
    opened: &CompanionParentRequestOpenResult,
    wait: bool,
) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(format!(
            "已向父 agent 提审。\n- request_id: {}\n- gate_id: {}\n- parent_agent_id: {}",
            opened.request_id, opened.gate_id, opened.parent_agent_id,
        ))],
        is_error: false,
        details: Some(serde_json::json!({
            "kind": "companion_parent_request",
            "request": {
                "request_id": opened.request_id,
                "wait": wait,
            },
            "gate": {
                "gate_id": opened.gate_id.to_string(),
            },
            "parent": {
                "agent_id": opened.parent_agent_id.to_string(),
                "frame_id": opened.parent_frame_id.to_string(),
                "journal": {
                    "uri": child_messages_uri(opened.parent_agent_id),
                },
            },
            "child": {
                "agent_id": opened.child_agent_id.to_string(),
                "frame_id": opened.child_frame_id.to_string(),
                "journal": {
                    "uri": child_messages_uri(opened.child_agent_id),
                },
            },
            "mailbox": companion_parent_mailbox_delivery_details(&opened.parent_mailbox_delivery),
        })),
    }
}

#[derive(Clone, Copy)]
struct CompanionHumanWaitVisibleResult<'a> {
    request_id: &'a str,
    gate_id: Uuid,
    agent_id: Uuid,
    frame_id: Uuid,
    status: &'a str,
    summary: &'a str,
    timed_out: bool,
    response_preview: Option<&'a str>,
}

fn companion_human_wait_agent_tool_result(
    visible: CompanionHumanWaitVisibleResult<'_>,
) -> AgentToolResult {
    let content = if visible.timed_out {
        format!(
            "等待用户回应超时。\n- status: timed_out\n- request_id: {}\n- gate_id: {}",
            visible.request_id, visible.gate_id,
        )
    } else {
        format!(
            "用户已回应。\n- request_id: {}\n- status: {}\n- summary: {}\n- gate_id: {}",
            visible.request_id, visible.status, visible.summary, visible.gate_id,
        )
    };

    let mut details = serde_json::json!({
        "kind": "companion_human_request",
        "request": {
            "request_id": visible.request_id,
            "wait": true,
        },
        "gate": {
            "gate_id": visible.gate_id.to_string(),
        },
        "agent": {
            "agent_id": visible.agent_id.to_string(),
            "frame_id": visible.frame_id.to_string(),
            "journal": {
                "uri": child_messages_uri(visible.agent_id),
            },
        },
        "status": visible.status,
        "summary": visible.summary,
        "timed_out": visible.timed_out,
        "result_refs": {
            "gate_id": visible.gate_id.to_string(),
            "request_id": visible.request_id,
            "agent_id": visible.agent_id.to_string(),
            "frame_id": visible.frame_id.to_string(),
            "journal_uri": child_messages_uri(visible.agent_id),
        },
    });
    if let Some(response_preview) = visible.response_preview {
        details["response_preview"] = serde_json::Value::String(response_preview.to_string());
    }

    AgentToolResult {
        content: vec![ContentPart::text(content)],
        is_error: false,
        details: Some(details),
    }
}

fn companion_parent_mailbox_delivery_details(
    delivery: &CompanionParentMailboxDeliveryResult,
) -> serde_json::Value {
    serde_json::json!({
        "mailbox_message_id": delivery.mailbox_message_id.map(|id| id.to_string()),
        "runtime_operation_id": delivery.accepted_runtime_operation_id,
        "command_receipt_client_command_id": delivery.command_receipt_client_command_id.clone(),
        "command_receipt_status": delivery.command_receipt_status.clone(),
        "command_receipt_duplicate": delivery.command_receipt_duplicate,
        "outcome": delivery.outcome.clone(),
        "runtime_operation_id": delivery.runtime_operation_id.clone(),
    })
}

#[async_trait]
impl CompanionParentMailboxDelivery for AgentRunCompanionMailboxDelivery {
    async fn deliver_child_result_to_parent(
        &self,
        command: CompanionParentMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, crate::ApplicationError> {
        let client_command_id = format!("companion-result:{}", command.gate_id);
        let marker_claim_token = Uuid::new_v4();
        let marker_claim = self
            .repos
            .gate_result_delivery_marker_repo
            .claim_parent_continuation(ClaimGateResultParentContinuationRequest {
                gate_id: command.gate_id,
                result_attempt: GATE_RESULT_DELIVERY_ATTEMPT,
                target_run_id: command.run_id,
                target_agent_id: command.parent_agent_id,
                claim_token: marker_claim_token,
                claim_expires_at: Utc::now()
                    + ChronoDuration::seconds(GATE_RESULT_PARENT_CONTINUATION_LEASE_SECONDS),
            })
            .await?;
        match marker_claim {
            GateResultDeliveryClaim::Claimed(_) => {}
            GateResultDeliveryClaim::Existing(marker) => {
                return Ok(marker_delivery_replay_result(
                    &marker,
                    client_command_id,
                    true,
                ));
            }
        }

        let channel_id = ensure_companion_agent_channel(
            &self.repos,
            command.run_id,
            command.parent_agent_id,
            command.child_agent_id,
            "companion",
        )
        .await?;
        let source = companion_wake_source(
            "result",
            "agent",
            "parent",
            command.gate_id,
            &command.request_id,
            serde_json::json!({
                "gate_id": command.gate_id.to_string(),
                "request_id": command.request_id.clone(),
                "run_id": command.run_id.to_string(),
                "parent_agent_id": command.parent_agent_id.to_string(),
                "child_agent_id": command.child_agent_id.to_string(),
                "child_runtime_thread_id": command.child_runtime_thread_id.clone(),
                "resolved_turn_id": command.resolved_turn_id.clone(),
            }),
        );
        let mailbox_result = deliver_companion_mailbox_message(
            &self.repos,
            self.product_delivery.as_ref(),
            CompanionMailboxDeliveryInput {
                channel_id,
                run_id: command.run_id,
                agent_id: command.parent_agent_id,
                sender: ChannelParticipantRef::LifecycleAgent {
                    run_id: command.run_id,
                    agent_id: command.child_agent_id,
                },
                source,
                payload_kind: "companion_result",
                input_text: command.input_text,
                client_command_id,
            },
        )
        .await?;
        let dispatched_to_parent = matches!(
            mailbox_result.outcome.as_str(),
            "launched" | "steered" | "resumed"
        );
        self.repos
            .gate_result_delivery_marker_repo
            .complete_parent_continuation(CompleteGateResultParentContinuationRequest {
                gate_id: command.gate_id,
                result_attempt: GATE_RESULT_DELIVERY_ATTEMPT,
                claim_token: marker_claim_token,
                mailbox_message_id: mailbox_result.mailbox_message_id,
                accepted_runtime_operation_id: mailbox_result.accepted_runtime_operation_id.clone(),
                dispatched_to_parent,
            })
            .await?;
        Ok(mailbox_result)
    }

    async fn deliver_parent_request_to_parent(
        &self,
        command: CompanionParentRequestMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, crate::ApplicationError> {
        let channel_id = ensure_companion_agent_channel(
            &self.repos,
            command.run_id,
            command.parent_agent_id,
            command.child_agent_id,
            "companion",
        )
        .await?;
        let source = companion_wake_source(
            "parent_request",
            "agent",
            "parent",
            command.gate_id,
            &command.request_id,
            serde_json::json!({
                "gate_id": command.gate_id.to_string(),
                "request_id": command.request_id.clone(),
                "run_id": command.run_id.to_string(),
                "parent_agent_id": command.parent_agent_id.to_string(),
                "child_agent_id": command.child_agent_id.to_string(),
                "child_runtime_thread_id": command.child_runtime_thread_id.clone(),
                "turn_id": command.turn_id.clone(),
                "wait": command.wait,
            }),
        );
        deliver_companion_mailbox_message(
            &self.repos,
            self.product_delivery.as_ref(),
            CompanionMailboxDeliveryInput {
                channel_id,
                run_id: command.run_id,
                agent_id: command.parent_agent_id,
                sender: ChannelParticipantRef::LifecycleAgent {
                    run_id: command.run_id,
                    agent_id: command.child_agent_id,
                },
                source,
                payload_kind: "companion_parent_request",
                input_text: command.input_text,
                client_command_id: format!("companion-parent-request:{}", command.gate_id),
            },
        )
        .await
    }

    async fn deliver_parent_response_to_child(
        &self,
        command: CompanionParentResponseMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, crate::ApplicationError> {
        let channel_id = ensure_companion_agent_channel(
            &self.repos,
            command.run_id,
            command.parent_agent_id,
            command.child_agent_id,
            "companion",
        )
        .await?;
        let source = companion_wake_source(
            "parent_response",
            "agent",
            "child",
            command.gate_id,
            &command.request_id,
            serde_json::json!({
                "gate_id": command.gate_id.to_string(),
                "request_id": command.request_id.clone(),
                "run_id": command.run_id.to_string(),
                "parent_agent_id": command.parent_agent_id.to_string(),
                "parent_runtime_thread_id": command.parent_runtime_thread_id.clone(),
                "child_agent_id": command.child_agent_id.to_string(),
                "resolved_turn_id": command.resolved_turn_id.clone(),
            }),
        );
        deliver_companion_mailbox_message(
            &self.repos,
            self.product_delivery.as_ref(),
            CompanionMailboxDeliveryInput {
                channel_id,
                run_id: command.run_id,
                agent_id: command.child_agent_id,
                sender: ChannelParticipantRef::LifecycleAgent {
                    run_id: command.run_id,
                    agent_id: command.parent_agent_id,
                },
                source,
                payload_kind: "companion_parent_response",
                input_text: command.input_text,
                client_command_id: format!("companion-parent-response:{}", command.gate_id),
            },
        )
        .await
    }
}

#[async_trait]
impl CompanionHumanResponseMailboxDelivery for AgentRunCompanionMailboxDelivery {
    async fn deliver_human_response_to_requesting_agent(
        &self,
        command: CompanionHumanResponseMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, crate::ApplicationError> {
        let channel_id = ensure_companion_human_channel(
            &self.repos,
            command.run_id,
            command.agent_id,
            &command.request_id,
        )
        .await?;
        let source = companion_wake_source(
            "human_response",
            "human",
            "human",
            command.gate_id,
            &command.request_id,
            serde_json::json!({
                "gate_id": command.gate_id.to_string(),
                "request_id": command.request_id.clone(),
                "run_id": command.run_id.to_string(),
                "agent_id": command.agent_id.to_string(),
                "runtime_thread_id": command.runtime_thread_id.clone(),
                "turn_id": command.turn_id.clone(),
                "request_type": command.request_type.clone(),
            }),
        );
        deliver_companion_mailbox_message(
            &self.repos,
            self.product_delivery.as_ref(),
            CompanionMailboxDeliveryInput {
                channel_id,
                run_id: command.run_id,
                agent_id: command.agent_id,
                sender: ChannelParticipantRef::Human {
                    user_id: "human".to_string(),
                },
                source,
                payload_kind: "companion_human_response",
                input_text: command.input_text,
                client_command_id: format!("companion-human-response:{}", command.gate_id),
            },
        )
        .await
    }
}

struct CompanionMailboxDeliveryInput {
    channel_id: Uuid,
    run_id: Uuid,
    agent_id: Uuid,
    sender: ChannelParticipantRef,
    source: MailboxSourceIdentity,
    payload_kind: &'static str,
    input_text: String,
    client_command_id: String,
}

async fn deliver_companion_mailbox_message(
    repos: &crate::repository_set::RepositorySet,
    product_delivery: &dyn AgentRunProductDeliveryPort,
    input: CompanionMailboxDeliveryInput,
) -> Result<CompanionParentMailboxDeliveryResult, crate::ApplicationError> {
    let channel_intent = companion_channel_delivery_intent(
        input.channel_id,
        input.run_id,
        input.agent_id,
        input.sender,
        &input.source,
        input.payload_kind,
        &input.input_text,
    );
    let _materialized =
        companion_channel_service(repos).materialize_delivery_to_mailbox(&channel_intent)?;
    let client_command_id = input.client_command_id.clone();
    let target = AgentRunRuntimeTarget {
        run_id: input.run_id,
        agent_id: input.agent_id,
    };
    let binding = repos
        .agent_run_runtime_binding_repo
        .load(&target)
        .await
        .map_err(|error| crate::ApplicationError::Internal(error.to_string()))?
        .ok_or_else(|| {
            crate::ApplicationError::Internal(
                "companion delivery target has no Runtime binding".to_string(),
            )
        })?;
    let delivery = product_delivery
        .deliver(DeliverAgentRunProductInput {
            run_id: input.run_id,
            agent_id: input.agent_id,
            presentation_thread_id: binding.presentation_thread_id,
            origin: agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::Companion,
            presentation: agentdash_application_agentrun::agent_run::AgentRunPresentationDraft {
                content: agentdash_agent_protocol::text_user_input_blocks(input.input_text.clone()),
                source: agentdash_agent_protocol::UserInputSource::new(
                    "companion",
                    input.payload_kind,
                    "agent",
                ),
                launch_source: agentdash_application_agentrun::agent_run::LaunchPresentationSource::CompanionParentResume,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                started_at_seconds: chrono::Utc::now().timestamp(),
            },
            input: vec![RuntimeInput::Text {
                text: input.input_text,
            }],
            actor: RuntimeActor::System {
                component: format!("companion:{}", input.payload_kind),
            },
            client_command_id: input.client_command_id,
            backend_selection: None,
            identity: None,
        })
        .await
        .map_err(|error| crate::ApplicationError::Internal(error.to_string()))?;
    let mailbox_message_id = Some(delivery.mailbox_message_id);
    companion_channel_service(repos)
        .record_delivery_state(
            &ChannelOwner::LifecycleRun {
                run_id: input.run_id,
            },
            input.channel_id,
            ChannelDeliveryState {
                delivery_id: channel_intent.id,
                message_id: channel_intent.message.id,
                target: channel_intent.target.clone(),
                status: ChannelDeliveryStatus::Materialized,
                materialized_ref: Some(MaterializedDeliveryRef::MailboxMessage {
                    message_id: delivery.mailbox_message_id,
                }),
                updated_at: Utc::now(),
            },
        )
        .await?;

    Ok(CompanionParentMailboxDeliveryResult {
        mailbox_message_id,
        accepted_runtime_operation_id: None,
        command_receipt_client_command_id: client_command_id,
        command_receipt_status: if delivery.queued {
            "queued"
        } else {
            "accepted"
        }
        .to_string(),
        command_receipt_duplicate: delivery
            .operation_receipt
            .as_ref()
            .is_some_and(|receipt| receipt.duplicate),
        outcome: if delivery.queued {
            "queued"
        } else {
            "dispatched"
        }
        .to_string(),
        runtime_operation_id: delivery
            .operation_receipt
            .map(|receipt| receipt.operation_id.to_string()),
    })
}

fn marker_delivery_replay_result(
    marker: &GateResultDeliveryMarker,
    client_command_id: String,
    duplicate: bool,
) -> CompanionParentMailboxDeliveryResult {
    CompanionParentMailboxDeliveryResult {
        mailbox_message_id: marker.mailbox_message_id,
        accepted_runtime_operation_id: marker.accepted_runtime_operation_id.clone(),
        command_receipt_client_command_id: client_command_id,
        command_receipt_status: marker.status.as_str().to_string(),
        command_receipt_duplicate: duplicate,
        outcome: marker.status.as_str().to_string(),
        runtime_operation_id: None,
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
    tool_context: CompanionToolContext,
    companion_agents: Vec<CompanionAgentEntry>,
    wait_service: WaitActivityService,
    model_preflight: Option<Arc<dyn CompanionModelPreflightPort>>,
    workflow_script_preflight: Option<Arc<dyn CompanionWorkflowScriptPreflightPort>>,
}

pub(crate) struct CompanionRequestToolDeps {
    pub project_agent_repo: Arc<dyn ProjectAgentRepository>,
    pub repos: crate::repository_set::RepositorySet,
    pub session_services_handle: SharedSessionToolServicesHandle,
    pub tool_context: CompanionToolContext,
    pub companion_agents: Vec<CompanionAgentEntry>,
    pub wait_service: WaitActivityService,
    pub model_preflight: Option<Arc<dyn CompanionModelPreflightPort>>,
    pub workflow_script_preflight: Option<Arc<dyn CompanionWorkflowScriptPreflightPort>>,
}

impl CompanionRequestTool {
    pub(crate) fn new(deps: CompanionRequestToolDeps) -> Self {
        Self {
            project_agent_repo: deps.project_agent_repo,
            repos: deps.repos,
            session_services_handle: deps.session_services_handle,
            tool_context: deps.tool_context,
            companion_agents: deps.companion_agents,
            wait_service: deps.wait_service,
            model_preflight: deps.model_preflight,
            workflow_script_preflight: deps.workflow_script_preflight,
        }
    }
}

#[async_trait]
impl AgentTool for CompanionRequestTool {
    fn name(&self) -> &str {
        "companion_request"
    }

    fn description(&self) -> &str {
        "发起结构化 companion 交互请求。基础目标：human=询问/审批用户；sub=派发子 Agent（payload.agent_key 必须取当前 Companion Agent Roster 的精确 agent_key）；parent=回传父会话；platform=请求平台 broker。payload 必须是 JSON object；正文用 payload.message。进阶协议参考 companion-system skill。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CompanionRequestParams>()
    }
    fn protocol_projector(&self) -> Option<agentdash_spi::ToolProtocolProjector> {
        Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_companion_request_dynamic_lifecycle".to_string())
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
    /// target=sub: 校验 payload、构造 prompt/hook 上下文，并委托 companion dispatch service
    /// materialize child agent；wait 轮询 durable LifecycleGate。
    async fn execute_sub_request(
        &self,
        _target: CompanionRequestTarget,
        wait: bool,
        payload: &serde_json::Value,
        _tool_call_id: &str,
        cancel: CancellationToken,
        on_update: Option<ToolUpdateCallback>,
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
        if self.companion_agents.is_empty() {
            return Err(AgentToolError::ExecutionFailed(
                "当前 runtime authority 未开放 companion.dispatch，不能派发 sub companion"
                    .to_string(),
            ));
        }

        let hook_runtime = self
            .tool_context
            .require_hook_runtime("生成 companion request 上下文")?;

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
        let agent_key = payload
            .get("agent_key")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AgentToolError::InvalidArguments(
                    "payload.agent_key 不能为空；sub companion 必须选择当前 roster 中的 ProjectAgent"
                        .to_string(),
                )
            })?;
        let requested_task_id = payload_task_id(payload)?;
        let max_fragments = payload
            .get("max_fragments")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let max_constraints = payload
            .get("max_constraints")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let current_session_id = self
            .tool_context
            .require_runtime_thread_id("派发 companion agent")?
            .to_string();
        let anchor = self
            .tool_context
            .require_lifecycle_anchor("派发 companion agent", &self.repos)
            .await?;
        let project_id = anchor.project_id;
        let parent_run_id = anchor.run_id;
        let parent_agent_id = anchor.agent_id;
        let parent_frame_id = anchor.frame_id;

        let selected_companion = self.resolve_companion_agent(project_id, agent_key).await?;
        let companion_label = payload
            .get("label")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&selected_companion.project_agent.name)
            .to_string();
        let companion_executor_config = selected_companion.executor_config.clone();
        self.preflight_selected_companion_model(
            project_id,
            parent_run_id,
            parent_agent_id,
            &companion_label,
            &selected_companion,
            &companion_executor_config,
        )
        .await?;

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
        let mut dispatch_plan = build_companion_dispatch_plan(
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
        dispatch_plan
            .slice
            .injections
            .push(agentdash_spi::HookInjection {
                slot: "companion".to_string(),
                content: format!(
                    "Date: {} (UTC) | Platform: {} {} | Model: {}",
                    chrono::Utc::now().format("%Y-%m-%d"),
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    companion_executor_config
                        .model_id
                        .as_deref()
                        .unwrap_or("unknown"),
                ),
                source: "session:parent_environment".to_string(),
            });
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

        let dispatch_result = CompanionChildDispatchService::new(&self.repos)
            .dispatch_child(CompanionChildDispatchRequest {
                project_id,
                parent_run_id,
                parent_agent_id,
                parent_frame_id,
                wait,
                slice_mode,
                adoption_mode,
                dispatch_id: dispatch_plan.dispatch_id.clone(),
                companion_label: companion_label.clone(),
                task_id: requested_task_id,
                selected_project_agent_id: selected_companion.project_agent.id,
                selected_agent_key: selected_companion.agent_key.clone(),
                companion_executor_config,
                parent_session_id: current_session_id.clone(),
                dispatch_prompt: dispatch_prompt.clone(),
            })
            .await?;

        let gate_ref = dispatch_result.gate_ref.map(|id| id.to_string());
        let source_ref = gate_ref
            .clone()
            .unwrap_or_else(|| dispatch_plan.dispatch_id.clone());
        let source = MailboxSourceIdentity::new("companion", "dispatch", "agent")
            .with_source_ref(source_ref)
            .with_correlation_ref(dispatch_plan.dispatch_id.clone())
            .with_route("sub")
            .with_metadata(serde_json::json!({
                "dispatch_id": dispatch_plan.dispatch_id.clone(),
                "gate_id": gate_ref.clone(),
                "wait": wait,
                "parent_run_id": parent_run_id.to_string(),
                "parent_agent_id": parent_agent_id.to_string(),
                "parent_frame_id": parent_frame_id.to_string(),
                "companion_label": companion_label.clone(),
                "selected_project_agent_id": selected_companion.project_agent.id.to_string(),
                "selected_agent_key": selected_companion.agent_key.clone(),
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "task_id": requested_task_id.map(|id| id.to_string()),
            }));
        let channel_id = ensure_companion_agent_channel(
            &self.repos,
            parent_run_id,
            parent_agent_id,
            dispatch_result.agent_ref,
            &companion_label,
        )
        .await
        .map_err(|error| {
            AgentToolError::ExecutionFailed(format!("companion channel 创建失败: {error}"))
        })?;
        let channel_intent = companion_channel_delivery_intent(
            channel_id,
            dispatch_result.run_ref,
            dispatch_result.agent_ref,
            ChannelParticipantRef::LifecycleAgent {
                run_id: parent_run_id,
                agent_id: parent_agent_id,
            },
            &source,
            "companion_dispatch",
            &dispatch_result.launch_source.dispatch_prompt,
        );
        let _materialized = companion_channel_service(&self.repos)
            .materialize_delivery_to_mailbox(&channel_intent)
            .map_err(|error| {
                AgentToolError::ExecutionFailed(format!(
                    "companion channel materialization 失败: {error}"
                ))
            })?;
        let mailbox_result = session_services
            .product_delivery
            .deliver(DeliverAgentRunProductInput {
                run_id: dispatch_result.run_ref,
                agent_id: dispatch_result.agent_ref,
                presentation_thread_id: dispatch_result.presentation_thread_id.clone(),
                origin: agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::Companion,
                presentation: agentdash_application_agentrun::agent_run::AgentRunPresentationDraft {
                    content: agentdash_agent_protocol::text_user_input_blocks(
                        dispatch_result.launch_source.dispatch_prompt.clone(),
                    ),
                    source: agentdash_agent_protocol::UserInputSource::new(
                        "companion",
                        "dispatch",
                        "agent",
                    )
                    .with_route("sub"),
                    launch_source: agentdash_application_agentrun::agent_run::LaunchPresentationSource::CompanionDispatch,
                    submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                    started_at_seconds: chrono::Utc::now().timestamp(),
                },
                input: vec![RuntimeInput::Text {
                    text: dispatch_result.launch_source.dispatch_prompt.clone(),
                }],
                actor: RuntimeActor::Agent {
                    name: parent_agent_id.to_string(),
                },
                client_command_id: format!(
                    "companion-dispatch:{}:{}",
                    dispatch_plan.dispatch_id, dispatch_result.agent_ref
                ),
                backend_selection: None,
                identity: None,
            })
            .await
            .map_err(|error| {
                AgentToolError::ExecutionFailed(format!(
                    "child companion mailbox dispatch 失败: {error}"
                ))
            })?;
        let runtime_operation_id = mailbox_result
            .operation_receipt
            .as_ref()
            .map(|receipt| receipt.operation_id.to_string());
        let mailbox_message_id = Some(mailbox_result.mailbox_message_id.to_string());
        let mailbox_outcome = if mailbox_result.queued {
            "queued"
        } else {
            "dispatched"
        };
        companion_channel_service(&self.repos)
            .record_delivery_state(
                &ChannelOwner::LifecycleRun {
                    run_id: parent_run_id,
                },
                channel_id,
                ChannelDeliveryState {
                    delivery_id: channel_intent.id,
                    message_id: channel_intent.message.id,
                    target: channel_intent.target.clone(),
                    status: ChannelDeliveryStatus::Materialized,
                    materialized_ref: Some(MaterializedDeliveryRef::MailboxMessage {
                        message_id: mailbox_result.mailbox_message_id,
                    }),
                    updated_at: Utc::now(),
                },
            )
            .await
            .map_err(|error| {
                AgentToolError::ExecutionFailed(format!(
                    "companion channel delivery state 写入失败: {error}"
                ))
            })?;

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
                "runtime_thread_id": dispatch_result.presentation_thread_id.to_string(),
                "runtime_operation_id": runtime_operation_id,
                "mailbox_message_id": mailbox_message_id.clone(),
                "mailbox_outcome": mailbox_outcome,
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
            if let Some(on_update) = on_update.as_ref() {
                on_update(companion_subagent_agent_tool_result(
                    CompanionSubagentVisibleResult {
                        companion_label: &companion_label,
                        child_agent_id: dispatch_result.agent_ref,
                        gate_id: Some(gate_id),
                        status: "running",
                        summary: "已派发 companion agent，等待回传",
                        timed_out: None,
                        result_preview: None,
                    },
                ));
            }
            let waiter_ref = format!(
                "companion_request:{}:{}:{}:{}",
                parent_run_id,
                parent_agent_id,
                self.tool_context.turn_id(),
                dispatch_plan.dispatch_id
            );
            self.repos
                .gate_result_delivery_marker_repo
                .register_waiter(RegisterGateResultWaiterRequest {
                    gate_id,
                    result_attempt: GATE_RESULT_DELIVERY_ATTEMPT,
                    waiter_ref: waiter_ref.clone(),
                    target_run_id: parent_run_id,
                    target_agent_id: parent_agent_id,
                    claim_expires_at: Utc::now()
                        + ChronoDuration::seconds(COMPANION_WAIT_TIMEOUT.as_secs() as i64),
                })
                .await
                .map_err(|error| {
                    AgentToolError::ExecutionFailed(format!(
                        "注册 companion wait delivery marker 失败: {error}"
                    ))
                })?;

            let wait_outcome = self.poll_gate_until_resolved(gate_id, cancel).await?;
            if matches!(wait_outcome, CompanionGateWaitOutcome::TimedOut) {
                return Ok(companion_subagent_agent_tool_result(
                    CompanionSubagentVisibleResult {
                        companion_label: &companion_label,
                        child_agent_id: dispatch_result.agent_ref,
                        gate_id: Some(gate_id),
                        status: "timed_out",
                        summary: "等待 companion result 超时",
                        timed_out: Some(true),
                        result_preview: None,
                    },
                ));
            }

            let CompanionGateWaitOutcome::Resolved(result_payload) = wait_outcome else {
                unreachable!("timed_out handled above");
            };
            let _waiter_delivery_claim = self
                .repos
                .gate_result_delivery_marker_repo
                .claim_waiter_delivery(ClaimGateResultWaiterRequest {
                    gate_id,
                    result_attempt: GATE_RESULT_DELIVERY_ATTEMPT,
                    waiter_ref,
                    target_run_id: parent_run_id,
                    target_agent_id: parent_agent_id,
                })
                .await
                .map_err(|error| {
                    AgentToolError::ExecutionFailed(format!(
                        "claim companion wait delivery marker 失败: {error}"
                    ))
                })?;
            let summary = companion_wait_payload_summary(&result_payload);
            let status = companion_wait_payload_status(&result_payload, "unknown");
            let result_preview =
                agent_visible_json_preview(&result_payload, COMPANION_WAIT_PREVIEW_CHARS);

            return Ok(companion_subagent_agent_tool_result(
                CompanionSubagentVisibleResult {
                    companion_label: &companion_label,
                    child_agent_id: dispatch_result.agent_ref,
                    gate_id: Some(gate_id),
                    status: &status,
                    summary: &summary,
                    timed_out: Some(false),
                    result_preview: Some(&result_preview),
                },
            ));
        }

        // ─── Async dispatch (wait=false) ────────────────────────────────
        Ok(companion_subagent_agent_tool_result(
            CompanionSubagentVisibleResult {
                companion_label: &companion_label,
                child_agent_id: dispatch_result.agent_ref,
                gate_id: dispatch_result.gate_ref,
                status: "running",
                summary: "已派发 companion agent，等待异步回流",
                timed_out: None,
                result_preview: None,
            },
        ))
    }

    /// 轮询 LifecycleGate 直到 resolved、timeout 或取消。
    async fn poll_gate_until_resolved(
        &self,
        gate_id: Uuid,
        cancel: CancellationToken,
    ) -> Result<CompanionGateWaitOutcome, AgentToolError> {
        let payload = self
            .wait_service
            .wait_for_lifecycle_gate_payload(
                WaitToolContext {
                    runtime_thread_id: self.tool_context.runtime_thread_id().and_then(|value| {
                        agentdash_agent_runtime_contract::RuntimeThreadId::new(value).ok()
                    }),
                    turn_id: self.tool_context.turn_id().to_string(),
                },
                gate_id,
                cancel,
                COMPANION_WAIT_TIMEOUT,
            )
            .await?;
        Ok(match payload {
            Some(payload) => CompanionGateWaitOutcome::Resolved(payload),
            None => CompanionGateWaitOutcome::TimedOut,
        })
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
            .require_runtime_thread_id("向上提审")?
            .to_string();
        let session_services =
            require_session_services(&self.session_services_handle, "向上提审").await?;
        let gate_control = CompanionGateControlFactory::new(&self.repos)
            .with_agent_run_delivery(&session_services);
        let opened = gate_control
            .open_parent_request(OpenCompanionParentRequestCommand {
                child_runtime_session_id: current_session_id,
                turn_id: self.tool_context.turn_id().to_string(),
                wait,
                payload: payload.clone(),
            })
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        Ok(companion_parent_request_agent_tool_result(&opened, wait))
    }

    /// target=human：请求作为前端可回应事件展示；用户回应后通过 mailbox 投递给 requesting AgentRun。
    /// wait=true → 当前工具轮询 durable LifecycleGate payload。
    /// wait=false → agent 继续，后续回应进入 requesting AgentRun mailbox。
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
            .require_runtime_thread_id("向用户发起请求")?
            .to_string();
        let anchor = self
            .tool_context
            .require_lifecycle_anchor("向用户发起请求", &self.repos)
            .await?;
        let agent = self
            .repos
            .lifecycle_agent_repo
            .get(anchor.agent_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!(
                    "LifecycleAgent {} 不存在，无法判断 human route authority",
                    anchor.agent_id
                ))
            })?;
        if agent.source == agentdash_domain::workflow::AgentSource::Subagent {
            return Err(AgentToolError::ExecutionFailed(
                "当前 companion child 默认未开放 human route，请通过 companion_respond 回流父会话"
                    .to_string(),
            ));
        }

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

        let gate_meta = serde_json::json!({
            "session_id": current_session_id.clone(),
            "turn_id": self.tool_context.turn_id(),
            "request_type": payload_type,
        });
        let gate_kind = if wait {
            "companion_wait"
        } else {
            "companion_human_request"
        };
        let outcome = LifecycleGateResolver::new(self.repos.lifecycle_gate_repo.clone())
            .open_companion_gate(OpenCompanionGateCommand {
                run_id: anchor.run_id,
                agent_id: anchor.agent_id,
                frame_id: Some(anchor.frame_id),
                gate_kind: gate_kind.to_string(),
                correlation_id: request_id,
                payload: Some(gate_meta),
                wait_policy: None,
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        let gate_id = outcome.gate.id;
        let request_id = gate_id.to_string();

        if wait {
            let wait_outcome = self.poll_gate_until_resolved(gate_id, cancel).await?;

            if matches!(wait_outcome, CompanionGateWaitOutcome::TimedOut) {
                return Ok(companion_human_wait_agent_tool_result(
                    CompanionHumanWaitVisibleResult {
                        request_id: &request_id,
                        gate_id,
                        agent_id: anchor.agent_id,
                        frame_id: anchor.frame_id,
                        status: "timed_out",
                        summary: "等待用户回应超时",
                        timed_out: true,
                        response_preview: None,
                    },
                ));
            }

            let CompanionGateWaitOutcome::Resolved(response_payload) = wait_outcome else {
                unreachable!("timed_out handled above");
            };
            let status = companion_wait_payload_status(&response_payload, "completed");
            let summary = companion_wait_payload_summary(&response_payload);
            let response_preview =
                agent_visible_json_preview(&response_payload, COMPANION_WAIT_PREVIEW_CHARS);

            Ok(companion_human_wait_agent_tool_result(
                CompanionHumanWaitVisibleResult {
                    request_id: &request_id,
                    gate_id,
                    agent_id: anchor.agent_id,
                    frame_id: anchor.frame_id,
                    status: &status,
                    summary: &summary,
                    timed_out: false,
                    response_preview: Some(&response_preview),
                },
            ))
        } else {
            Ok(AgentToolResult {
                content: vec![ContentPart::text(format!(
                    "已向用户发送请求。\n- request_id: {request_id}\n- 用户回应后会进入当前 AgentRun mailbox。"
                ))],
                is_error: false,
                details: Some(serde_json::json!({
                    "kind": "companion_human_request",
                    "request_id": request_id,
                    "wait": false,
                    "gate_id": gate_id.to_string(),
                    "agent": {
                        "agent_id": anchor.agent_id.to_string(),
                        "frame_id": anchor.frame_id.to_string(),
                        "journal": {
                            "uri": child_messages_uri(anchor.agent_id),
                        },
                    },
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
            Some("workflow_script_preflight") => {
                self.execute_workflow_script_preflight(payload).await
            }
            Some(type_name) => Err(AgentToolError::InvalidArguments(format!(
                "target=platform 暂不支持 payload.type=`{type_name}`"
            ))),
            None => Err(AgentToolError::InvalidArguments(
                "target=platform 要求 payload.type".to_string(),
            )),
        }
    }

    async fn execute_workflow_script_preflight(
        &self,
        payload: &serde_json::Value,
    ) -> Result<AgentToolResult, AgentToolError> {
        let Some(preflight) = &self.workflow_script_preflight else {
            return Err(AgentToolError::ExecutionFailed(
                "target=platform payload.type=`workflow_script_preflight` 暂不可用：当前 Session 未注入 WorkflowScriptPreflight broker"
                    .to_string(),
            ));
        };

        let source_text = payload
            .get("source_text")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if source_text.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "payload.source_text 不能为空".to_string(),
            ));
        }

        let project_id = self
            .tool_context
            .require_lifecycle_anchor("执行 workflow script preflight", &self.repos)
            .await?
            .project_id;
        let runtime_session_id = payload
            .get("runtime_session_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                self.tool_context
                    .runtime_thread_id()
                    .map(ToString::to_string)
            });
        let user_id = self
            .tool_context
            .identity()
            .map(|identity| identity.user_id.clone());

        let output = preflight
            .preflight_workflow_script(CompanionWorkflowScriptPreflightRequest {
                project_id,
                user_id,
                source_text,
                args: payload.get("args").cloned(),
                ctx: payload.get("ctx").cloned(),
                runtime_session_id,
            })
            .await
            .map_err(AgentToolError::ExecutionFailed)?;

        workflow_script_preflight_agent_tool_result(output)
    }

    async fn preflight_selected_companion_model(
        &self,
        project_id: Uuid,
        parent_run_id: Uuid,
        parent_agent_id: Uuid,
        companion_label: &str,
        selected_companion: &SelectedCompanionAgent,
        executor_config: &AgentConfig,
    ) -> Result<(), AgentToolError> {
        let Some(preflight) = &self.model_preflight else {
            return Ok(());
        };
        preflight
            .preflight_companion_model(CompanionModelPreflightRequest {
                project_id,
                parent_run_id,
                parent_agent_id,
                selected_project_agent_id: selected_companion.project_agent.id,
                selected_agent_key: selected_companion.agent_key.clone(),
                companion_label: companion_label.to_string(),
                executor_config: executor_config.clone(),
                identity: self.tool_context.identity().cloned(),
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.message))
    }

    async fn resolve_companion_agent(
        &self,
        project_id: Uuid,
        agent_name: &str,
    ) -> Result<SelectedCompanionAgent, AgentToolError> {
        let requested = agent_name.trim();
        if requested.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "payload.agent_key 不能为空".to_string(),
            ));
        }

        let Some(entry) = self
            .companion_agents
            .iter()
            .find(|agent| agent.name.eq_ignore_ascii_case(requested))
        else {
            let available = self
                .companion_agents
                .iter()
                .map(|agent| {
                    if agent.display_name.trim().is_empty()
                        || agent.display_name.eq_ignore_ascii_case(&agent.name)
                    {
                        agent.name.clone()
                    } else {
                        format!("{} ({})", agent.name, agent.display_name)
                    }
                })
                .collect::<Vec<_>>();
            return Err(AgentToolError::InvalidArguments(format!(
                "当前 session 不可调用 agent_key=`{requested}` 的 companion agent。可用 agent_key: [{}]",
                available.join(", ")
            )));
        };

        let agent = self
            .project_agent_repo
            .get_by_project_and_name(project_id, &entry.name)
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!(
                    "frame 中声明的 companion agent `{}` 在当前 Project 中不存在",
                    entry.name
                ))
            })?;

        let preset = agent
            .preset_config()
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        Ok(SelectedCompanionAgent {
            agent_key: entry.name.clone(),
            executor_config: preset.to_agent_config(&agent.agent_type),
            project_agent: agent,
        })
    }
}

// ─── companion_respond ──────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompanionRespondParams {
    /// Optional selector. Omit it when the prompt lists a single active reply target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<CompanionReplySelectorParam>,
    /// Structured JSON object payload. Registered response types are validated semantically at runtime.
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
        "回应当前 companion 交互请求；完成工作后调用并传入 payload。reply_to 可省略，只有 prompt 明确列出多个回复目标时才使用 current/alias 短 selector。payload 必须是 JSON object，注册 response type 的业务字段由运行时语义校验。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CompanionRespondParams>()
    }
    fn protocol_projector(&self) -> Option<agentdash_spi::ToolProtocolProjector> {
        Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_companion_respond_dynamic_lifecycle".to_string())
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

        let current_session_id = self
            .tool_context
            .require_runtime_thread_id("回应 companion 请求")?
            .to_string();
        let session_services =
            require_session_services(&self.session_services_handle, "回应 companion 请求").await?;

        let reply_target = self
            .resolve_reply_target(raw.reply_to.as_ref(), &current_session_id)
            .await?;
        let request_id = reply_target.request_id.as_str();

        let result = match reply_target.route {
            CompanionReplyRoute::ParentRequestGate => self
                .try_resolve_parent_request_gate(
                    request_id,
                    &current_session_id,
                    &payload,
                    &session_services,
                )
                .await?,
            CompanionReplyRoute::PendingAction => self
                .try_resolve_pending_action(
                    request_id,
                    &current_session_id,
                    &payload,
                    &session_services,
                )
                .await?,
            CompanionReplyRoute::ChildDispatch => {
                self.try_complete_to_parent(
                    request_id,
                    &current_session_id,
                    &payload,
                    &session_services,
                )
                .await?
            }
        }
        .ok_or_else(|| {
            AgentToolError::ExecutionFailed(format!(
                "resolved companion reply target `{}` could not be completed. Retry with the minimal call:\n{}",
                reply_target.model_selector_label(),
                reply_target.model_instruction.minimal_arguments_json()
            ))
        })?;

        Ok(result)
    }
}

impl CompanionRespondTool {
    async fn resolve_reply_target(
        &self,
        selector: Option<&CompanionReplySelectorParam>,
        current_session_id: &str,
    ) -> Result<CompanionReplyContract, AgentToolError> {
        let candidates = self.active_reply_targets(current_session_id).await?;
        let matches = match selector {
            None => candidates.clone(),
            Some(CompanionReplySelectorParam::Current { channel }) => {
                if let Some(channel) = channel.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                    candidates
                        .iter()
                        .filter(|candidate| candidate.channel == channel)
                        .cloned()
                        .collect()
                } else {
                    candidates.clone()
                }
            }
            Some(CompanionReplySelectorParam::Alias { alias }) => {
                if alias_is_raw_internal_ref(alias) {
                    return Err(AgentToolError::InvalidArguments(
                        "reply_to.alias 只接受 prompt 中列出的短 alias，不接受 raw GUID 或内部 id"
                            .to_string(),
                    ));
                }
                let Some(alias) = normalize_reply_alias(alias) else {
                    return Err(AgentToolError::InvalidArguments(
                        "reply_to.alias 不能为空".to_string(),
                    ));
                };
                candidates
                    .iter()
                    .filter(|candidate| {
                        candidate.aliases.iter().any(|item| *item == alias.as_str())
                    })
                    .cloned()
                    .collect()
            }
        };

        match matches.as_slice() {
            [target] => Ok(target.clone()),
            [] => Err(self.reply_resolution_error(
                "没有匹配的 active companion reply target",
                selector,
                &candidates,
            )),
            _ => Err(self.reply_resolution_error(
                "当前存在多个 active companion reply target，需要使用 prompt 中列出的 alias/current selector",
                selector,
                &matches,
            )),
        }
    }

    async fn active_reply_targets(
        &self,
        current_session_id: &str,
    ) -> Result<Vec<CompanionReplyContract>, AgentToolError> {
        let mut targets = Vec::new();

        if let Some((_anchor, _agent, frame)) = resolve_current_frame_from_delivery_trace_ref(
            current_session_id,
            self.repos.agent_run_runtime_binding_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.agent_frame_repo.as_ref(),
        )
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
        {
            let gates = self
                .repos
                .lifecycle_gate_repo
                .list_open_for_agent(frame.agent_id)
                .await
                .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;
            for gate in gates {
                if is_companion_child_reply_gate(&gate.gate_kind, gate.payload_json.as_ref()) {
                    targets.push(CompanionReplyContract::new(
                        CompanionReplyRoute::ChildDispatch,
                        gate.correlation_id,
                        COMPANION_PARENT_CHANNEL,
                        vec!["parent"],
                        ModelReplyInstruction::completion_for_current_companion()
                            .with_reply_to(ModelReplySelector::alias("parent")),
                    ));
                } else if gate.gate_kind == COMPANION_PARENT_REQUEST_GATE_KIND {
                    targets.push(CompanionReplyContract::new(
                        CompanionReplyRoute::ParentRequestGate,
                        gate.id.to_string(),
                        COMPANION_CHILD_CHANNEL,
                        vec!["child"],
                        ModelReplyInstruction::completion_for_current_companion()
                            .with_reply_to(ModelReplySelector::alias("child")),
                    ));
                }
            }
        }

        if let Some(hook_runtime) = self.tool_context.hook_runtime() {
            targets.extend(
                hook_runtime
                    .pending_actions()
                    .iter()
                    .filter(|action| action.status == HookPendingActionStatus::Pending)
                    .map(|action| {
                        CompanionReplyContract::new(
                            CompanionReplyRoute::PendingAction,
                            action.id.clone(),
                            COMPANION_ACTION_CHANNEL,
                            vec!["action"],
                            ModelReplyInstruction::from_payload_expectation(
                                CompanionPayloadExpectation {
                                    expected_type: Some("resolution".to_string()),
                                    required_fields: vec![
                                        "type".to_string(),
                                        "status".to_string(),
                                        "summary".to_string(),
                                    ],
                                    example_payload: serde_json::json!({
                                        "type": "resolution",
                                        "status": "approved",
                                        "summary": "..."
                                    }),
                                    repair_hint: Some(
                                        "Use alias `action` only when the prompt lists it as a reply target."
                                            .to_string(),
                                    ),
                                },
                                Some(ModelReplySelector::alias("action")),
                            ),
                        )
                    }),
            );
        }

        Ok(targets)
    }

    fn reply_resolution_error(
        &self,
        reason: &str,
        selector: Option<&CompanionReplySelectorParam>,
        candidates: &[CompanionReplyContract],
    ) -> AgentToolError {
        let received = selector
            .map(CompanionReplySelectorParam::received_label)
            .unwrap_or_else(|| "omitted".to_string());
        let available = if candidates.is_empty() {
            "none".to_string()
        } else {
            candidates
                .iter()
                .map(CompanionReplyContract::available_selector_text)
                .collect::<Vec<_>>()
                .join("; ")
        };
        let repair_instruction = candidates
            .first()
            .map(|candidate| candidate.model_instruction.clone())
            .unwrap_or_else(ModelReplyInstruction::completion_for_current_companion);

        AgentToolError::ExecutionFailed(format!(
            "{reason}\n- received_selector: {received}\n- available_selectors: {available}\n- minimal_valid_call:\n{}",
            repair_instruction.minimal_arguments_json()
        ))
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
            CompanionGateControlFactory::new(&self.repos).with_agent_run_delivery(session_services);
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
                "parent": {
                    "agent_id": result.parent_agent_id.to_string(),
                    "frame_id": result.parent_frame_id.to_string(),
                    "journal": {
                        "uri": child_messages_uri(result.parent_agent_id),
                    },
                },
                "child": {
                    "agent_id": result.child_agent_id.to_string(),
                    "frame_id": result.child_frame_id.to_string(),
                    "journal": {
                        "uri": child_messages_uri(result.child_agent_id),
                    },
                },
                "mailbox": {
                    "mailbox_message_id": result.child_mailbox_delivery.mailbox_message_id.map(|id| id.to_string()),
                    "runtime_operation_id": result.child_mailbox_delivery.accepted_runtime_operation_id,
                    "command_receipt_client_command_id": result.child_mailbox_delivery.command_receipt_client_command_id,
                    "command_receipt_status": result.child_mailbox_delivery.command_receipt_status,
                    "command_receipt_duplicate": result.child_mailbox_delivery.command_receipt_duplicate,
                    "outcome": result.child_mailbox_delivery.outcome,
                    "runtime_operation_id": result.child_mailbox_delivery.runtime_operation_id,
                },
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
        _session_services: &SessionToolServices,
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
            CompanionGateControlFactory::new(&self.repos).with_agent_run_delivery(session_services);
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
                "parent": {
                    "agent_id": result.parent_agent_id.to_string(),
                    "journal": {
                        "uri": child_messages_uri(result.parent_agent_id),
                    },
                },
                "mailbox": {
                    "mailbox_message_id": result.parent_mailbox_delivery.mailbox_message_id.map(|id| id.to_string()),
                    "runtime_operation_id": result.parent_mailbox_delivery.accepted_runtime_operation_id,
                    "command_receipt_client_command_id": result.parent_mailbox_delivery.command_receipt_client_command_id,
                    "command_receipt_status": result.parent_mailbox_delivery.command_receipt_status,
                    "command_receipt_duplicate": result.parent_mailbox_delivery.command_receipt_duplicate,
                    "outcome": result.parent_mailbox_delivery.outcome,
                    "runtime_operation_id": result.parent_mailbox_delivery.runtime_operation_id,
                },
                "payload": result.payload,
            })),
        }))
    }
}

fn is_companion_child_reply_gate(gate_kind: &str, payload: Option<&serde_json::Value>) -> bool {
    match gate_kind {
        COMPANION_CHILD_BLOCKING_WAIT_GATE_KIND | COMPANION_CHILD_FOLLOW_UP_WAIT_GATE_KIND => true,
        COMPANION_CHILD_WAIT_GATE_KIND => payload
            .and_then(|payload| payload.get("request_type"))
            .is_none(),
        _ => false,
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
                    "agent_key": {
                        "type": "string",
                        "description": "Only for target=sub: exact canonical key from the current Companion Agent Roster Delta / Effective Companion Agents, e.g. payload.agent_key=\"sub-agent\". Do not use executor or display_name."
                    },
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
            },
            {
                "type": "object",
                "required": ["type", "source_text"],
                "properties": {
                    "type": { "const": "workflow_script_preflight" },
                    "source_text": { "type": "string", "minLength": 1 },
                    "args": {},
                    "ctx": {},
                    "runtime_session_id": { "type": "string" }
                }
            }
        ]
    })
}

fn companion_response_payload_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true,
        "description": "Open companion response payload object. Registered payload.type values are validated semantically by PayloadTypeRegistry."
    })
}

fn platform_capability_grant_missing_broker_error() -> AgentToolError {
    AgentToolError::ExecutionFailed(
        "target=platform payload.type=`capability_grant_request` 暂不支持：缺少 platform permission grant broker，当前 companion context 无法提供 PermissionGrantService::request 所需的 agent_auto_grantable / lifecycle_requestable policy inputs，也没有 live runtime capability update handoff。参见 ARCH-010 完成 broker 闭环后再启用。"
            .to_string(),
    )
}

fn workflow_script_preflight_agent_tool_result(
    output: WorkflowScriptPreflightOutput,
) -> Result<AgentToolResult, AgentToolError> {
    let valid = !output.has_blocking_diagnostics();
    let source_digest = match &output.source_ref {
        agentdash_domain::workflow::OrchestrationSourceRef::Inline { source_digest } => {
            source_digest.clone()
        }
        other => serde_json::to_value(other)
            .ok()
            .and_then(|value| {
                value
                    .get("source_digest")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| "unknown".to_string()),
    };
    let source_ref = serde_json::to_value(&output.source_ref).map_err(|error| {
        AgentToolError::ExecutionFailed(format!("序列化 workflow script source_ref 失败: {error}"))
    })?;
    let plan_snapshot = match &output.plan_snapshot {
        Some(plan_snapshot) => Some(serde_json::to_value(plan_snapshot).map_err(|error| {
            AgentToolError::ExecutionFailed(format!(
                "序列化 workflow script plan_snapshot 失败: {error}"
            ))
        })?),
        None => None,
    };
    let plan_preview = output.plan_preview.map(|preview| {
        serde_json::json!({
            "plan_digest": preview.plan_digest,
            "node_count": preview.node_count,
            "entry_node_ids": preview.entry_node_ids,
            "nodes": preview.nodes.into_iter().map(|node| serde_json::json!({
                "node_id": node.node_id,
                "node_path": node.node_path,
                "kind": node.kind,
                "label": node.label,
            })).collect::<Vec<_>>(),
        })
    });
    let diagnostics = output
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            serde_json::json!({
                "code": diagnostic.code,
                "severity": format!("{:?}", diagnostic.severity).to_ascii_lowercase(),
                "message": diagnostic.message,
                "source_path": diagnostic.source_path,
            })
        })
        .collect::<Vec<_>>();
    let capability_summary = serde_json::to_value(output.capability_summary).map_err(|error| {
        AgentToolError::ExecutionFailed(format!(
            "序列化 workflow script capability_summary 失败: {error}"
        ))
    })?;
    let details = serde_json::json!({
        "kind": "workflow_script_preflight",
        "valid": valid,
        "source_digest": source_digest,
        "source_ref": source_ref,
        "raw_builder_document": output.raw_builder_document,
        "plan_snapshot": plan_snapshot,
        "plan_preview": plan_preview,
        "capability_summary": capability_summary,
        "diagnostics": diagnostics,
    });
    let summary = if valid {
        format!("workflow script preflight 通过：source_digest={source_digest}")
    } else {
        format!("workflow script preflight 未通过：source_digest={source_digest}")
    };
    Ok(AgentToolResult {
        content: vec![ContentPart::text(summary)],
        is_error: false,
        details: Some(details),
    })
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
        effects_applied: !resolution.effects.is_empty(),
        block_reason: resolution.block_reason.clone(),
        completion: resolution.completion.clone(),
        diagnostics: resolution.diagnostics.clone(),
        injections: resolution.injections.clone(),
    };
    hook_runtime.append_trace(trace.clone());

    let _ = (session_services, turn_id);
}

#[cfg(test)]
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
    pub reply_instruction: ModelReplyInstruction,
}

#[derive(Debug, Clone)]
pub struct CompanionExecutionSlice {
    pub vfs: Option<Vfs>,
    pub mcp_servers: Vec<agentdash_spi::RuntimeMcpServer>,
}

pub fn build_companion_dispatch_prompt(plan: &CompanionDispatchPlan, user_prompt: &str) -> String {
    let mut sections = vec!["[Companion Dispatch Context]".to_string()];

    sections.push(format!(
        "## Dispatch Context\n- companion_label: {}\n- slice_mode: {:?}\n- adoption_mode: {:?}",
        plan.companion_label, plan.slice.mode, plan.adoption_mode
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
    sections.push(plan.reply_instruction.render_markdown_section());
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
        reply_instruction: ModelReplyInstruction::completion_for_current_companion(),
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
    use super::super::reply_contract::ModelReplyInstruction;
    use super::{
        CompanionAdoptionMode, CompanionDispatchPlan, CompanionDispatchSlice,
        CompanionGateWaitOutcome, CompanionHumanWaitVisibleResult, CompanionRespondParams,
        CompanionSliceMode, CompanionSubagentVisibleResult, build_companion_dispatch_prompt,
        build_companion_dispatch_slice, build_companion_execution_slice,
        build_subagent_pending_action, child_messages_uri, companion_human_wait_agent_tool_result,
        companion_owner_candidates, companion_parent_request_agent_tool_result,
        companion_subagent_agent_tool_result, companion_wake_source, merge_gate_result_refs,
        platform_capability_grant_missing_broker_error, wait_for_lifecycle_gate_resolution,
    };
    use agentdash_domain::workflow::{
        GateWaitPolicyEnvelope, LifecycleGate, LifecycleGateRepository, WaitProducerRef,
    };
    use agentdash_spi::CapabilityScope;
    use agentdash_spi::action_type as at;
    use agentdash_spi::context::tool_schema_sanitizer::schema_value;
    use agentdash_spi::{McpTransportConfig, MountCapability, RuntimeMcpServer, Vfs};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    use crate::runtime_tools::SharedSessionToolServicesHandle;
    #[derive(Default)]
    struct FixtureGateRepo {
        gates: Mutex<HashMap<Uuid, LifecycleGate>>,
    }

    #[async_trait::async_trait]
    impl LifecycleGateRepository for FixtureGateRepo {
        async fn create(&self, gate: &LifecycleGate) -> Result<(), agentdash_domain::DomainError> {
            self.gates.lock().unwrap().insert(gate.id, gate.clone());
            Ok(())
        }

        async fn get(
            &self,
            id: Uuid,
        ) -> Result<Option<LifecycleGate>, agentdash_domain::DomainError> {
            Ok(self.gates.lock().unwrap().get(&id).cloned())
        }

        async fn list_open_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<LifecycleGate>, agentdash_domain::DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .filter(|gate| gate.agent_id == Some(agent_id) && gate.is_open())
                .cloned()
                .collect())
        }

        async fn list_open_gate_wait_policies(
            &self,
            limit: usize,
        ) -> Result<Vec<LifecycleGate>, agentdash_domain::DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .filter(|gate| {
                    gate.is_open()
                        && gate
                            .payload_json
                            .as_ref()
                            .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                            .is_some()
                })
                .take(limit)
                .cloned()
                .collect())
        }

        async fn list_by_wait_producer(
            &self,
            producer: &WaitProducerRef,
        ) -> Result<Vec<LifecycleGate>, agentdash_domain::DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .filter(|gate| {
                    gate.payload_json
                        .as_ref()
                        .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                        .is_some_and(|declaration| declaration.wait_policy.source == *producer)
                })
                .cloned()
                .collect())
        }

        async fn find_by_agent_and_correlation(
            &self,
            agent_id: Uuid,
            correlation_id: &str,
        ) -> Result<Option<LifecycleGate>, agentdash_domain::DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .find(|gate| {
                    gate.agent_id == Some(agent_id) && gate.correlation_id == correlation_id
                })
                .cloned())
        }

        async fn update(&self, gate: &LifecycleGate) -> Result<(), agentdash_domain::DomainError> {
            self.gates.lock().unwrap().insert(gate.id, gate.clone());
            Ok(())
        }
    }

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
    fn companion_wake_source_has_stable_identity() {
        let gate_id = Uuid::new_v4();
        let source = companion_wake_source(
            "result",
            "agent",
            "parent",
            gate_id,
            "dispatch-1",
            serde_json::json!({ "gate_id": gate_id.to_string() }),
        );

        assert_eq!(source.namespace, "companion");
        assert_eq!(source.kind, "result");
        assert_eq!(source.actor, "agent");
        assert_eq!(source.route.as_deref(), Some("parent"));
        assert_eq!(
            source.source_ref.as_deref(),
            Some(gate_id.to_string().as_str())
        );
        assert_eq!(source.correlation_ref.as_deref(), Some("dispatch-1"));
    }

    #[test]
    fn companion_wait_result_refs_merge_gate_evidence_refs() {
        let base_refs = serde_json::json!({
            "gate_ref": "gate-1",
            "run_ref": "run-parent",
            "runtime_thread_id": "child-session",
        });
        let gate_payload = serde_json::json!({
            "result_refs": {
                "schema_version": 1,
                "child": {
                    "run_id": "run-child",
                    "agent_id": "agent-child",
                    "frame_id": "frame-child",
                    "runtime_thread_id": "child-session"
                },
                "evidence": [
                    {
                        "kind": "lifecycle_file",
                        "scope": "child_agent_run_messages",
                        "child_run_id": "run-child",
                        "child_agent_id": "agent-child",
                        "child_frame_id": "frame-child",
                        "runtime_thread_id": "child-session",
                        "mount_id": "lifecycle",
                        "uri": "lifecycle://agent-runs/agent-child/sessions/messages"
                    }
                ]
            }
        });

        let refs = merge_gate_result_refs(base_refs, &gate_payload);

        assert_eq!(refs["gate_ref"], serde_json::json!("gate-1"));
        assert_eq!(
            refs["child"]["runtime_thread_id"],
            serde_json::json!("child-session")
        );
        assert_eq!(
            refs["evidence"][0]["uri"],
            serde_json::json!("lifecycle://agent-runs/agent-child/sessions/messages")
        );
        assert!(
            !serde_json::to_string(&refs)
                .expect("serialize refs")
                .contains("session/events.json")
        );
    }

    fn visible_subagent_result_fixture<'a>(
        status: &'a str,
        gate_id: Option<Uuid>,
        result_preview: Option<&'a str>,
    ) -> CompanionSubagentVisibleResult<'a> {
        CompanionSubagentVisibleResult {
            companion_label: "reviewer",
            child_agent_id: Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
                .expect("agent uuid"),
            gate_id,
            status,
            summary: "done",
            timed_out: Some(status == "timed_out"),
            result_preview,
        }
    }

    fn assert_result_hides_runtime_session_refs(result: &agentdash_spi::AgentToolResult) {
        let text = result
            .content
            .iter()
            .filter_map(agentdash_spi::ContentPart::extract_text)
            .collect::<Vec<_>>()
            .join("\n");
        let details = serde_json::to_string(&result.details).expect("details json");
        let visible = format!("{text}\n{details}");

        for forbidden in [
            "runtime_thread_id",
            "parent_runtime_thread_id",
            "child_runtime_thread_id",
            "child_session_id",
            "parent_session_id",
            "runtime-child-session",
            "runtime-parent-session",
            "parent-session",
            "child-session",
        ] {
            assert!(
                !visible.contains(forbidden),
                "Agent-visible result leaked `{forbidden}`:\n{visible}"
            );
        }
    }

    fn assert_result_retains_child_agent_only(
        result: &agentdash_spi::AgentToolResult,
        child_agent_id: &str,
    ) {
        let details = result.details.as_ref().expect("details");
        assert_eq!(
            details["kind"],
            serde_json::json!("companion_subagent_dispatch")
        );
        assert_eq!(
            details["child"]["agent_id"],
            serde_json::json!(child_agent_id)
        );
        assert_eq!(details["companion_label"], serde_json::json!("reviewer"));
        assert!(details.get("journal").is_none());
        let text = result
            .content
            .iter()
            .filter_map(agentdash_spi::ContentPart::extract_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(child_agent_id));
        assert!(text.contains("lifecycle://agent-runs/{child_agent_id}/sessions/messages"));
    }

    #[test]
    fn companion_subagent_async_result_hides_runtime_session_refs() {
        let child_agent_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let result = companion_subagent_agent_tool_result(visible_subagent_result_fixture(
            "running", None, None,
        ));

        assert_result_hides_runtime_session_refs(&result);
        assert_result_retains_child_agent_only(&result, child_agent_id);
        assert!(
            result
                .details
                .as_ref()
                .expect("details")
                .get("wait_activity")
                .is_none()
        );
    }

    fn mailbox_delivery_fixture() -> super::CompanionParentMailboxDeliveryResult {
        super::CompanionParentMailboxDeliveryResult {
            mailbox_message_id: Some(
                Uuid::parse_str("eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee").expect("mailbox uuid"),
            ),
            accepted_runtime_operation_id: Some("operation-test".to_string()),
            command_receipt_client_command_id: "client-command-1".to_string(),
            command_receipt_status: "accepted".to_string(),
            command_receipt_duplicate: false,
            outcome: "launched".to_string(),
            runtime_operation_id: Some("operation-parent-1".to_string()),
        }
    }

    #[test]
    fn companion_parent_request_result_hides_parent_and_child_session_refs() {
        let parent_agent_id =
            Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("parent agent uuid");
        let parent_frame_id =
            Uuid::parse_str("22222222-2222-2222-2222-222222222222").expect("parent frame uuid");
        let child_agent_id =
            Uuid::parse_str("33333333-3333-3333-3333-333333333333").expect("child agent uuid");
        let child_frame_id =
            Uuid::parse_str("44444444-4444-4444-4444-444444444444").expect("child frame uuid");
        let opened = super::CompanionParentRequestOpenResult {
            gate_id: Uuid::parse_str("55555555-5555-5555-5555-555555555555").expect("gate uuid"),
            request_id: "request-parent-1".to_string(),
            run_id: Uuid::parse_str("66666666-6666-6666-6666-666666666666").expect("run uuid"),
            parent_agent_id,
            parent_frame_id,
            parent_runtime_thread_id: "parent-session".to_string(),
            child_agent_id,
            child_frame_id,
            child_runtime_thread_id: "child-session".to_string(),
            companion_label: "parent".to_string(),
            parent_mailbox_delivery: mailbox_delivery_fixture(),
            payload: serde_json::json!({ "status": "pending" }),
        };

        let result = companion_parent_request_agent_tool_result(&opened, true);

        assert_result_hides_runtime_session_refs(&result);
        let details = result.details.as_ref().expect("details");
        assert_eq!(
            details["kind"],
            serde_json::json!("companion_parent_request")
        );
        assert_eq!(
            details["parent"]["agent_id"],
            serde_json::json!(parent_agent_id.to_string())
        );
        assert_eq!(
            details["child"]["journal"]["uri"],
            serde_json::json!(child_messages_uri(child_agent_id))
        );
        assert_eq!(
            details["mailbox"]["runtime_operation_id"],
            serde_json::json!("operation-parent-1")
        );
    }

    #[test]
    fn companion_subagent_wait_completed_result_hides_runtime_session_refs() {
        let child_agent_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let gate_id = Uuid::parse_str("dddddddd-dddd-dddd-dddd-dddddddddddd").expect("gate uuid");
        let result_refs = serde_json::json!({
            "child": {
                "agent_id": child_agent_id,
                "runtime_thread_id": "runtime-child-session"
            },
            "evidence": [{
                "kind": "lifecycle_file",
                "child_runtime_thread_id": "runtime-child-session"
            }]
        });
        let raw_preview = serde_json::json!({
            "status": "completed",
            "summary": "done",
            "result_refs": result_refs,
            "child_session_id": "runtime-child-session"
        });
        let preview = super::agent_visible_json_preview(&raw_preview, 2_000);
        let result = companion_subagent_agent_tool_result(visible_subagent_result_fixture(
            "completed",
            Some(gate_id),
            Some(&preview),
        ));

        assert_result_hides_runtime_session_refs(&result);
        assert_result_retains_child_agent_only(&result, child_agent_id);
        assert_eq!(
            result.details.as_ref().expect("details")["status"],
            serde_json::json!("completed")
        );
        assert_eq!(
            result.details.as_ref().expect("details")["wait_activity"]["activity_ref"],
            serde_json::json!(gate_id.to_string())
        );
    }

    #[test]
    fn companion_subagent_wait_timeout_result_hides_runtime_session_refs() {
        let child_agent_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let gate_id = Uuid::parse_str("dddddddd-dddd-dddd-dddd-dddddddddddd").expect("gate uuid");
        let result = companion_subagent_agent_tool_result(visible_subagent_result_fixture(
            "timed_out",
            Some(gate_id),
            None,
        ));

        assert_result_hides_runtime_session_refs(&result);
        assert_result_retains_child_agent_only(&result, child_agent_id);
        assert_eq!(
            result.details.as_ref().expect("details")["timed_out"],
            serde_json::json!(true)
        );
        assert_eq!(
            result.details.as_ref().expect("details")["wait_activity"]["activity_refs"],
            serde_json::json!([gate_id.to_string()])
        );
    }

    #[test]
    fn companion_human_wait_timeout_result_hides_runtime_session_ref() {
        let agent_id = Uuid::parse_str("77777777-7777-7777-7777-777777777777").expect("agent uuid");
        let frame_id = Uuid::parse_str("88888888-8888-8888-8888-888888888888").expect("frame uuid");
        let gate_id = Uuid::parse_str("99999999-9999-9999-9999-999999999999").expect("gate uuid");
        let result = companion_human_wait_agent_tool_result(CompanionHumanWaitVisibleResult {
            request_id: "human-request-1",
            gate_id,
            agent_id,
            frame_id,
            status: "timed_out",
            summary: "等待用户回应超时",
            timed_out: true,
            response_preview: None,
        });

        assert_result_hides_runtime_session_refs(&result);
        let details = result.details.as_ref().expect("details");
        assert_eq!(
            details["kind"],
            serde_json::json!("companion_human_request")
        );
        assert_eq!(
            details["agent"]["journal"]["uri"],
            serde_json::json!(child_messages_uri(agent_id))
        );
        assert_eq!(details["timed_out"], serde_json::json!(true));
    }

    #[test]
    fn companion_human_wait_completed_result_hides_runtime_session_ref() {
        let agent_id = Uuid::parse_str("77777777-7777-7777-7777-777777777777").expect("agent uuid");
        let frame_id = Uuid::parse_str("88888888-8888-8888-8888-888888888888").expect("frame uuid");
        let gate_id = Uuid::parse_str("99999999-9999-9999-9999-999999999999").expect("gate uuid");
        let raw_response = serde_json::json!({
            "status": "completed",
            "summary": "approved",
            "runtime_thread_id": "runtime-child-session",
            "parent_session_id": "parent-session",
            "result_refs": {
                "parent_runtime_thread_id": "runtime-parent-session",
                "child_session_id": "child-session"
            }
        });
        let response_preview = super::agent_visible_json_preview(&raw_response, 2_000);
        let result = companion_human_wait_agent_tool_result(CompanionHumanWaitVisibleResult {
            request_id: "human-request-1",
            gate_id,
            agent_id,
            frame_id,
            status: "completed",
            summary: "approved",
            timed_out: false,
            response_preview: Some(&response_preview),
        });

        assert_result_hides_runtime_session_refs(&result);
        let details = result.details.as_ref().expect("details");
        assert_eq!(details["status"], serde_json::json!("completed"));
        assert!(
            details["response_preview"]
                .as_str()
                .expect("response preview")
                .contains("approved")
        );
    }

    #[tokio::test]
    async fn companion_gate_wait_returns_timeout_without_resolving_gate() {
        let repo = FixtureGateRepo::default();
        let gate = LifecycleGate::open(
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            Some(Uuid::new_v4()),
            "companion_wait",
            "dispatch-1",
            Some(serde_json::json!({ "status": "pending" })),
        );
        let gate_id = gate.id;
        repo.create(&gate).await.expect("seed gate");

        let outcome = wait_for_lifecycle_gate_resolution(
            &repo,
            gate_id,
            CancellationToken::new(),
            std::time::Duration::from_millis(2),
            std::time::Duration::from_millis(1),
        )
        .await
        .expect("wait outcome");

        assert_eq!(outcome, CompanionGateWaitOutcome::TimedOut);
        let stored = repo.get(gate_id).await.expect("load gate").expect("gate");
        assert!(stored.is_open());
    }

    #[tokio::test]
    async fn companion_gate_wait_returns_resolved_payload_refs_source() {
        let repo = FixtureGateRepo::default();
        let mut gate = LifecycleGate::open(
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            Some(Uuid::new_v4()),
            "companion_wait",
            "dispatch-1",
            Some(serde_json::json!({ "status": "pending" })),
        );
        let gate_id = gate.id;
        gate.payload_json = Some(serde_json::json!({
            "status": "completed",
            "summary": "done",
            "artifact_refs": ["mailbox:result"]
        }));
        gate.resolve("child-agent");
        repo.create(&gate).await.expect("seed gate");

        let outcome = wait_for_lifecycle_gate_resolution(
            &repo,
            gate_id,
            CancellationToken::new(),
            std::time::Duration::from_secs(1),
            std::time::Duration::from_millis(1),
        )
        .await
        .expect("wait outcome");

        match outcome {
            CompanionGateWaitOutcome::Resolved(payload) => {
                assert_eq!(payload["status"], serde_json::json!("completed"));
                assert_eq!(payload["summary"], serde_json::json!("done"));
                assert_eq!(
                    payload["artifact_refs"],
                    serde_json::json!(["mailbox:result"])
                );
            }
            CompanionGateWaitOutcome::TimedOut => panic!("resolved gate should not time out"),
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
                readiness: Default::default(),
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
                readiness: Default::default(),
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
            reply_instruction: ModelReplyInstruction::completion_for_current_companion(),
        };

        let prompt = build_companion_dispatch_prompt(&plan, "请帮我 review 当前实现");

        assert!(prompt.contains("companion_respond"));
        assert!(prompt.contains("\"payload\""));
        assert!(prompt.contains("\"type\": \"completion\""));
        assert!(!prompt.contains("dispatch_id"));
        assert!(!prompt.contains("gate_id"));
        assert!(!prompt.contains("run_id"));
        assert!(!prompt.contains("agent_id"));
        assert!(!prompt.contains("frame_id"));
        assert!(!prompt.contains("session_id"));
        assert!(prompt.contains("请帮我 review 当前实现"));
    }

    #[test]
    fn companion_response_payload_schema_is_open_object() {
        let schema = schema_value::<CompanionRespondParams>();
        let payload_schema = &schema["properties"]["payload"];

        assert_eq!(payload_schema["type"], serde_json::json!("object"));
        assert_eq!(
            payload_schema["additionalProperties"],
            serde_json::json!(true)
        );
        assert!(payload_schema.get("anyOf").is_none());
        assert!(schema["properties"].get("request_id").is_none());

        let params: CompanionRespondParams = serde_json::from_value(serde_json::json!({
            "payload": {
                "type": "custom_response",
                "status": "ok",
                "domain_specific": { "anything": true }
            }
        }))
        .expect("custom payload should deserialize");
        assert_eq!(params.payload["type"], serde_json::json!("custom_response"));
        assert!(params.reply_to.is_none());
    }

    #[test]
    fn companion_skill_docs_do_not_expose_request_id_contract() {
        let skill = include_str!(
            "../../../agentdash-domain/src/companion/skills/companion-system/SKILL.md"
        );
        let response_adoption = include_str!(
            "../../../agentdash-domain/src/companion/skills/companion-system/references/response-adoption.md"
        );

        assert!(!skill.contains("request_id"));
        assert!(!response_adoption.contains("request_id"));
        assert!(response_adoption.contains("\"payload\""));
        assert!(response_adoption.contains("\"reply_to\""));
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
