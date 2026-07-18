import type { WorkspaceModulePresentation } from "../generated/workspace-module-contracts";

export interface AgentRunProjectionTarget {
  run_id: string;
  agent_id: string;
}

export interface WorkspaceModulePresentationCause {
  runtime_thread_id: string;
  runtime_operation_id?: string | null;
  runtime_turn_id: string;
  runtime_item_id: string;
}

export interface WorkspaceModulePresentationCurrentnessFence {
  binding_id: string;
  binding_generation: number;
  surface_revision: number;
  module_id: string;
  view_key: string;
  renderer_kind: string;
  presentation_uri: string;
}

export interface WorkspaceModulePresentationIntent {
  intent_id: string;
  effect_id: string;
  target: AgentRunProjectionTarget;
  actor: {
    kind: "agent_tool" | "user" | "system";
    actor_id: string;
  };
  cause: WorkspaceModulePresentationCause;
  currentness_fence: WorkspaceModulePresentationCurrentnessFence;
  presentation_digest: string;
  presentation: WorkspaceModulePresentation;
  committed_at_ms: number;
}

export interface WorkspaceModulePresentationChange {
  change_id: string;
  target: AgentRunProjectionTarget;
  sequence: number;
  revision: number;
  status: "pending" | "fulfilled";
  intent: WorkspaceModulePresentationIntent;
  acknowledgement?: {
    ack_id: string;
    target: AgentRunProjectionTarget;
    intent_id: string;
    effect_id: string;
    acknowledged_change_sequence: number;
    fulfilled_at_ms: number;
  } | null;
}

export interface WorkspaceModulePresentationSnapshot {
  target: AgentRunProjectionTarget;
  revision: number;
  latest_change_sequence: number;
  captured_at_ms: number;
  pending_intents: WorkspaceModulePresentationIntent[];
}

export interface WorkspaceModulePresentationChangeGap {
  requested_after?: number | null;
  earliest_available: number;
  latest_available: number;
  snapshot_revision: number;
}

export interface WorkspaceModulePresentationChangePage {
  target: AgentRunProjectionTarget;
  changes: WorkspaceModulePresentationChange[];
  next: number;
  gap?: WorkspaceModulePresentationChangeGap | null;
}

export type AgentRunTerminalLifecycleState =
  | "starting"
  | "running"
  | "exited"
  | "killed"
  | "lost";

export type AgentRunTerminalCapability = "interactive" | "read_only_output";
export type AgentRunTerminalOutputStream = "stdout" | "stderr" | "pty";

export interface AgentRunTerminalOwnerFence {
  terminal_owner_epoch_id: string;
  target: AgentRunProjectionTarget;
  runtime_thread_id: string;
  binding_id: string;
  binding_generation: number;
  backend_id: string;
}

export interface AgentRunTerminalOutputProjection {
  next_sequence: number;
  retained_output: string;
  truncated: boolean;
  omitted_bytes: number;
}

export interface AgentRunTerminalProjection {
  terminal_id: string;
  owner: AgentRunTerminalOwnerFence;
  mount_id?: string | null;
  cwd?: string | null;
  capability: AgentRunTerminalCapability;
  max_output_bytes: number;
  state: AgentRunTerminalLifecycleState;
  availability: "online" | "offline" | "reconciling";
  latest_source_sequence: number;
  exit_code?: number | null;
  process_id?: number | null;
  created_at_ms: number;
  exited_at_ms?: number | null;
  output: AgentRunTerminalOutputProjection;
}

export type AgentRunTerminalProjectionDelta =
  | { kind: "registered"; terminal: AgentRunTerminalProjection }
  | {
      kind: "output_appended";
      terminal_id: string;
      owner: AgentRunTerminalOwnerFence;
      output_sequence: number;
      stream: AgentRunTerminalOutputStream;
      data: string;
    }
  | {
      kind: "output_omitted";
      terminal_id: string;
      owner: AgentRunTerminalOwnerFence;
      output_sequence: number;
      omitted_bytes: number;
      retained_output: string;
    }
  | {
      kind: "state_changed";
      terminal_id: string;
      owner: AgentRunTerminalOwnerFence;
      state: AgentRunTerminalLifecycleState;
      exit_code?: number | null;
      changed_at_ms: number;
    }
  | {
      kind: "availability_changed";
      terminal_id: string;
      owner: AgentRunTerminalOwnerFence;
      availability: "online" | "offline" | "reconciling";
      changed_at_ms: number;
    }
  | {
      kind: "control_correlated";
      terminal_id: string;
      owner: AgentRunTerminalOwnerFence;
      correlation_id: string;
      control: "input" | "resize" | "terminate" | "read" | "status";
      status: "accepted" | "completed" | "failed";
      diagnostic?: string | null;
    }
  | {
      kind: "removed";
      terminal_id: string;
      owner: AgentRunTerminalOwnerFence;
    };

export interface AgentRunTerminalChange {
  change_id: string;
  target: AgentRunProjectionTarget;
  sequence: number;
  revision: number;
  source_sequence: number;
  payload_digest: string;
  delta: AgentRunTerminalProjectionDelta;
}

export interface AgentRunTerminalSnapshot {
  target: AgentRunProjectionTarget;
  revision: number;
  latest_change_sequence: number;
  captured_at_ms: number;
  terminals: AgentRunTerminalProjection[];
}

export interface AgentRunTerminalChangeGap {
  requested_after?: number | null;
  earliest_available: number;
  latest_available: number;
  snapshot_revision: number;
}

export interface AgentRunTerminalChangePage {
  target: AgentRunProjectionTarget;
  changes: AgentRunTerminalChange[];
  next: number;
  gap?: AgentRunTerminalChangeGap | null;
}
