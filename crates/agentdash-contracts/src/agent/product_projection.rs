use agentdash_agent_runtime_contract::{
    ManagedRuntimeContentBlock, ManagedRuntimeInteractionResponse, ManagedRuntimeOperationReceipt,
    ManagedRuntimeSourceBindingEvidence, RuntimeInteractionId,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentRunProjectionTarget {
    pub run_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRunProductRuntimeCommand {
    Resume,
    SubmitInput {
        content: Vec<ManagedRuntimeContentBlock>,
    },
    Interrupt,
    RequestCompaction,
    Rebind,
    ResolveInteraction {
        interaction_id: RuntimeInteractionId,
        response: ManagedRuntimeInteractionResponse,
    },
    Close,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AgentRunProductRuntimeCommandRequest {
    pub client_command_id: String,
    pub command: AgentRunProductRuntimeCommand,
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
    pub terminal_snapshot: AgentRunTerminalSnapshot,
    pub terminal_change_page: AgentRunTerminalChangePage,
}
