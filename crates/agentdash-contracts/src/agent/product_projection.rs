use agentdash_agent_runtime_contract::{
    ManagedRuntimeContentBlock, ManagedRuntimeInteractionResponse, ManagedRuntimeOperationReceipt,
    ManagedRuntimeSourceBindingEvidence, RuntimeInteractionId, RuntimeProjectionRevision,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::workspace_module::WorkspaceModulePresentation;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentRunProjectionTarget {
    pub run_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRunProductRuntimeCommand {
    SubmitInput {
        content: Vec<ManagedRuntimeContentBlock>,
    },
    Interrupt,
    RequestCompaction,
    ResolveInteraction {
        interaction_id: RuntimeInteractionId,
        response: ManagedRuntimeInteractionResponse,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AgentRunProductRuntimeCommandRequest {
    pub client_command_id: String,
    pub expected_revision: RuntimeProjectionRevision,
    pub command: AgentRunProductRuntimeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationCause {
    pub runtime_thread_id: String,
    pub runtime_operation_id: Option<String>,
    pub runtime_turn_id: String,
    pub runtime_item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationCurrentnessFence {
    pub runtime_thread_id: String,
    pub source_binding: ManagedRuntimeSourceBindingEvidence,
    pub surface_revision: u64,
    pub module_id: String,
    pub view_key: String,
    pub renderer_kind: String,
    pub presentation_uri: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModulePresentationActorKind {
    AgentTool,
    User,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationActor {
    pub kind: WorkspaceModulePresentationActorKind,
    pub actor_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationIntent {
    pub intent_id: String,
    pub effect_id: String,
    pub target: AgentRunProjectionTarget,
    pub actor: WorkspaceModulePresentationActor,
    pub cause: WorkspaceModulePresentationCause,
    pub currentness_fence: WorkspaceModulePresentationCurrentnessFence,
    pub presentation_digest: String,
    pub presentation: WorkspaceModulePresentation,
    pub committed_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModulePresentationIntentStatus {
    Pending,
    Fulfilled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationAcknowledgement {
    pub ack_id: String,
    pub target: AgentRunProjectionTarget,
    pub intent_id: String,
    pub effect_id: String,
    pub acknowledged_change_sequence: u64,
    pub fulfilled_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationChange {
    pub change_id: String,
    pub target: AgentRunProjectionTarget,
    pub sequence: u64,
    pub revision: u64,
    pub status: WorkspaceModulePresentationIntentStatus,
    pub intent: WorkspaceModulePresentationIntent,
    pub acknowledgement: Option<WorkspaceModulePresentationAcknowledgement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationSnapshot {
    pub target: AgentRunProjectionTarget,
    pub revision: u64,
    pub latest_change_sequence: u64,
    pub captured_at_ms: u64,
    pub pending_intents: Vec<WorkspaceModulePresentationPendingIntent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationPendingIntent {
    pub change_sequence: u64,
    pub intent: WorkspaceModulePresentationIntent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationChangeGap {
    pub requested_after: Option<u64>,
    pub earliest_available: u64,
    pub latest_available: u64,
    pub snapshot_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationChangePage {
    pub target: AgentRunProjectionTarget,
    pub changes: Vec<WorkspaceModulePresentationChange>,
    pub next: u64,
    pub gap: Option<WorkspaceModulePresentationChangeGap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceModulePresentationAcknowledgeRequest {
    pub observed_change_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalLifecycleState {
    Starting,
    Running,
    Exited,
    Killed,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalAvailability {
    Online,
    Offline,
    Reconciling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalCapability {
    Interactive,
    ReadOnlyOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalOutputStream {
    Stdout,
    Stderr,
    Pty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentRunTerminalOwnerFence {
    pub terminal_owner_epoch_id: String,
    pub target: AgentRunProjectionTarget,
    pub runtime_thread_id: String,
    pub source_binding: ManagedRuntimeSourceBindingEvidence,
    pub backend_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentRunTerminalOutputProjection {
    pub next_sequence: u64,
    pub retained_output: String,
    pub truncated: bool,
    pub omitted_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentRunTerminalProjection {
    pub terminal_id: String,
    pub owner: AgentRunTerminalOwnerFence,
    pub mount_id: Option<String>,
    pub cwd: Option<String>,
    pub capability: AgentRunTerminalCapability,
    pub max_output_bytes: u64,
    pub state: AgentRunTerminalLifecycleState,
    pub availability: AgentRunTerminalAvailability,
    pub latest_source_sequence: u64,
    pub exit_code: Option<i32>,
    pub process_id: Option<u32>,
    pub created_at_ms: u64,
    pub exited_at_ms: Option<u64>,
    pub output: AgentRunTerminalOutputProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalControlKind {
    Input,
    Resize,
    Terminate,
    Read,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalControlStatus {
    Accepted,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRunTerminalProjectionDelta {
    Registered {
        terminal: AgentRunTerminalProjection,
    },
    OutputAppended {
        terminal_id: String,
        owner: AgentRunTerminalOwnerFence,
        output_sequence: u64,
        stream: AgentRunTerminalOutputStream,
        data: String,
    },
    OutputOmitted {
        terminal_id: String,
        owner: AgentRunTerminalOwnerFence,
        output_sequence: u64,
        omitted_bytes: u64,
        retained_output: String,
    },
    StateChanged {
        terminal_id: String,
        owner: AgentRunTerminalOwnerFence,
        state: AgentRunTerminalLifecycleState,
        exit_code: Option<i32>,
        changed_at_ms: u64,
    },
    AvailabilityChanged {
        terminal_id: String,
        owner: AgentRunTerminalOwnerFence,
        availability: AgentRunTerminalAvailability,
        changed_at_ms: u64,
    },
    ControlCorrelated {
        terminal_id: String,
        owner: AgentRunTerminalOwnerFence,
        correlation_id: String,
        control: AgentRunTerminalControlKind,
        status: AgentRunTerminalControlStatus,
        diagnostic: Option<String>,
    },
    Removed {
        terminal_id: String,
        owner: AgentRunTerminalOwnerFence,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalProductChangeKind {
    BackendAvailability,
    ControlCorrelation,
    ReconcileLost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRunTerminalChangeOrigin {
    SourceFact {
        terminal_owner_epoch_id: String,
        source_sequence: u64,
    },
    ProductFact {
        change_kind: AgentRunTerminalProductChangeKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentRunTerminalChange {
    pub change_id: String,
    pub target: AgentRunProjectionTarget,
    pub sequence: u64,
    pub revision: u64,
    pub origin: AgentRunTerminalChangeOrigin,
    pub payload_digest: String,
    pub delta: AgentRunTerminalProjectionDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentRunTerminalSnapshot {
    pub target: AgentRunProjectionTarget,
    pub revision: u64,
    pub latest_change_sequence: u64,
    pub captured_at_ms: u64,
    pub terminals: Vec<AgentRunTerminalProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentRunTerminalChangeGap {
    pub requested_after: Option<u64>,
    pub earliest_available: u64,
    pub latest_available: u64,
    pub snapshot_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentRunTerminalChangePage {
    pub target: AgentRunProjectionTarget,
    pub changes: Vec<AgentRunTerminalChange>,
    pub next: u64,
    pub gap: Option<AgentRunTerminalChangeGap>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AgentRunProductProjectionContractSchema {
    pub runtime_command: AgentRunProductRuntimeCommandRequest,
    pub runtime_command_receipt: ManagedRuntimeOperationReceipt,
    pub workspace_snapshot: WorkspaceModulePresentationSnapshot,
    pub workspace_change_page: WorkspaceModulePresentationChangePage,
    pub terminal_snapshot: AgentRunTerminalSnapshot,
    pub terminal_change_page: AgentRunTerminalChangePage,
}
