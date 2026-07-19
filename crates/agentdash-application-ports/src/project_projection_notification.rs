use agentdash_agent_runtime_contract::RuntimeThreadId;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ControlPlaneProjectionChanged {
    pub projection: ControlPlaneProjection,
    pub reason: ControlPlaneProjectionChangeReason,
    pub run_id: String,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox_message_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlPlaneProjection {
    Workspace,
    AgentRunList,
    Mailbox,
    Waiting,
    Delivery,
    HookRuntime,
    ResourceSurface,
    Title,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlPlaneProjectionChangeReason {
    AgentRunLineageChanged,
    AgentRunShellChanged,
    AgentRunActivityChanged,
    MailboxStateChanged,
    WaitResolved,
    DeliveryTerminal,
    CompanionResult,
    HookEffectApplied,
    HookAutoResumeQueued,
    CapabilityStateChanged,
    ContextFrameChanged,
    TitleChanged,
}

#[derive(Debug, Clone)]
pub struct ProjectProjectionInvalidation {
    pub project_id: Uuid,
    pub projection: ControlPlaneProjection,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Option<Uuid>,
    pub gate_id: Option<Uuid>,
    pub mailbox_message_id: Option<Uuid>,
    pub reason: ControlPlaneProjectionChangeReason,
    pub runtime_thread_id: Option<RuntimeThreadId>,
}

impl ProjectProjectionInvalidation {
    pub fn agent_run_list(
        project_id: Uuid,
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Option<Uuid>,
        reason: ControlPlaneProjectionChangeReason,
        runtime_thread_id: Option<RuntimeThreadId>,
    ) -> Self {
        Self {
            project_id,
            projection: ControlPlaneProjection::AgentRunList,
            run_id,
            agent_id,
            frame_id,
            gate_id: None,
            mailbox_message_id: None,
            reason,
            runtime_thread_id,
        }
    }
}

#[async_trait]
pub trait ProjectProjectionNotificationPort: Send + Sync {
    async fn publish_project_projection_invalidated(
        &self,
        invalidation: ProjectProjectionInvalidation,
    ) -> Result<(), String>;
}
