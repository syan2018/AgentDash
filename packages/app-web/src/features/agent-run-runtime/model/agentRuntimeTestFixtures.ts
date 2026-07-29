import type { AgentRuntimeUnavailabilityReason } from "../../../generated/agent-runtime-contracts";
import type {
  AgentRuntimeCommandAvailability,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-validators";

type FixtureStatus = "running" | "completed" | "failed" | "lost";

function availability(
  status: FixtureStatus,
  available: boolean,
  unavailableReason?: AgentRuntimeUnavailabilityReason,
): AgentRuntimeCommandAvailability {
  const evidence = {
    expected_view_revision: null,
    expected_turn_id: status === "running" ? "turn-compaction" : null,
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
        ? (unavailableReason ?? "operation_in_flight")
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
      active_turn: active
        ? {
            turn_id: "turn-compaction",
            kind: "context_compaction",
            phase: "running",
            started_at_ms: 1000n,
            cancellable: false,
          }
        : null,
      queued_compaction: null,
      last_compaction_outcome: active
        ? null
        : {
            turn_id: "turn-compaction",
            status: status === "completed" ? "succeeded" : status,
            completed_at_ms: 1000n + revision,
            error: status === "failed" || status === "lost" ? status : null,
          },
      latest_turn_id: "turn-compaction",
    },
    context: {
      snapshot_revision: revision,
      context_revision: `context-${revision}`,
      recipe_digest: `sha256:context-${revision}`,
      authority: "source_authoritative",
      fidelity: "exact",
    },
    conversation: [],
    interactions: [],
    thread_name: null,
    thread_name_source: null,
    source_binding: null,
    authority: "source_authoritative",
    fidelity: "exact",
    command_availability: {
      submit_input: availability(status, true),
      steer: availability(status, false, "active_turn_not_steerable"),
      interrupt: availability(status, false, "turn_not_cancellable"),
      request_compaction: availability(status, !active, "compaction_in_progress"),
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
