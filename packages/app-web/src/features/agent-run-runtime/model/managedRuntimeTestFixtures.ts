import type { ManagedRuntimeOperationStatus } from "../../../generated/agent-runtime-contracts";
import type {
  ManagedRuntimeCommandAvailability,
  ManagedRuntimeSnapshot,
} from "../../../generated/agent-runtime-validators";

type FixtureStatus = "running" | "completed" | "failed" | "lost";

function operationStatus(status: FixtureStatus): ManagedRuntimeOperationStatus {
  if (status === "completed") return "succeeded";
  return status;
}

function availability(
  status: FixtureStatus,
): ManagedRuntimeCommandAvailability {
  const evidence = {
    blocking_operation_id:
      status === "running" ? "operation-compaction" : null,
    bound_surface_revision: null,
    applied_surface_revision: null,
  };
  if (status === "running") {
    return {
      status: "unavailable",
      reason: "operation_in_flight",
      evidence,
    };
  }
  if (status === "lost") {
    return {
      status: "unavailable",
      reason: "source_unavailable",
      evidence,
    };
  }
  return { status: "available", evidence };
}

function runtimeSnapshot(
  status: FixtureStatus,
  revision: bigint,
): ManagedRuntimeSnapshot {
  return {
    thread_id: "runtime-thread-child",
    revision,
    captured_at_ms: 1000n + revision,
    lifecycle: "active",
    conversation_history: [],
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
      submit_input: availability(status),
      request_compaction: availability(status),
      resolve_interaction: availability(status),
    },
  };
}

const started = runtimeSnapshot("running", 5n);
const completed = runtimeSnapshot("completed", 6n);
const failed = runtimeSnapshot("failed", 7n);
const lost = runtimeSnapshot("lost", 8n);

export const managedRuntimeTestFixtures = {
  snapshots: { started, completed, failed, lost },
};
