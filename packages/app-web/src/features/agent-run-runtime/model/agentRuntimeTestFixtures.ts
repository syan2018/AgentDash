import type {
  AgentControlUnavailabilityReason,
} from "../../../generated/agent-runtime-contracts";
import type { AgentRuntimeView } from "../../../generated/agent-runtime-codecs";

type FixtureStatus = "running" | "completed" | "failed" | "lost";
type AgentControlAvailability = NonNullable<
  AgentRuntimeView["observation"]["command_availability"]["submit_input"]
>;

function availability(
  status: FixtureStatus,
  revision: bigint,
  available: boolean,
  unavailableReason?: AgentControlUnavailabilityReason,
): AgentControlAvailability {
  const evidence = {
    expected_snapshot_revision: revision,
    expected_turn_id: status === "running" ? "turn-compaction" : null,
  };
  if (status === "lost") {
    return {
      status: "unavailable",
      reason: "source_lost",
      evidence,
    };
  }
  return available
    ? { status: "available", evidence }
    : {
      status: "unavailable",
      reason: status === "running"
        ? (unavailableReason ?? "pending_interaction_required")
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
    observation: {
      revision,
      lifecycle: status === "lost" ? "lost" : "active",
      execution: {
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
      },
      context: {
        snapshot_revision: revision,
        context_revision: `context-${revision}`,
        recipe_digest: `sha256:context-${revision}`,
        authority: "agent_owned",
        fidelity: "exact",
      },
      conversation: [],
      interactions: [],
      thread_name: null,
      source_info: {
        authority: "agent_authoritative",
        source_revision: `source-${revision}`,
        fidelity: "exact",
        observed_at_ms: 1000n + revision,
      },
      command_availability: {
        submit_input: availability(status, revision, true),
        steer: availability(status, revision, false, "active_turn_not_steerable"),
        interrupt: availability(status, revision, false, "turn_not_cancellable"),
        request_compaction: availability(status, revision, !active, "compaction_in_progress"),
        resolve_interaction: availability(status, revision, false),
      },
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
