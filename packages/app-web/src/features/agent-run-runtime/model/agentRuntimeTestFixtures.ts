import type { AgentRuntimeOperationStatus } from "../../../generated/agent-runtime-contracts";
import type {
  AgentRuntimeCommandAvailability,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-validators";

type FixtureStatus = "running" | "completed" | "failed" | "lost";

function operationStatus(status: FixtureStatus): AgentRuntimeOperationStatus {
  if (status === "completed") return "succeeded";
  return status;
}

function availability(
  status: FixtureStatus,
  available: boolean,
): AgentRuntimeCommandAvailability {
  const evidence = {
    blocking_operation_id:
      status === "running" ? "operation-compaction" : null,
    bound_surface_revision: null,
    applied_surface_revision: null,
  };
  if (status === "lost") {
    return {
      status: "unavailable",
      reason: "source_unavailable",
      evidence,
    };
  }
  return available
    ? { status: "available", evidence }
    : {
      status: "unavailable",
      reason: status === "running"
        ? "operation_in_flight"
        : "active_turn_required",
      evidence,
    };
}

function runtimeSnapshot(
  status: FixtureStatus,
  revision: bigint,
): AgentRuntimeView {
  const active = status === "running";
  return {
    thread_id: "runtime-thread-child",
    view_revision: revision,
    captured_at_ms: 1000n + revision,
    lifecycle: "active",
    execution: {
      status: active ? "active" : "idle",
      active_turn_id: active ? "turn-compaction" : null,
      latest_turn_id: "turn-compaction",
    },
    conversation: [],
    interactions: [],
    thread_name: null,
    thread_name_source: null,
    operations: [
      {
        id: "operation-compaction",
        turn_id: "turn-compaction",
        status: operationStatus(status),
        evidence: null,
      },
    ],
    source_binding: null,
    authority: "source_authoritative",
    fidelity: "exact",
    command_availability: {
      submit_input: availability(status, !active),
      steer: availability(status, active),
      interrupt: availability(status, active),
      request_compaction: availability(status, !active),
      resolve_interaction: availability(status, false),
    },
  };
}

const started = runtimeSnapshot("running", 5n);
const completed = runtimeSnapshot("completed", 6n);
const failed = runtimeSnapshot("failed", 7n);
const lost = runtimeSnapshot("lost", 8n);

export const agentRuntimeTestFixtures = {
  snapshots: { started, completed, failed, lost },
};
