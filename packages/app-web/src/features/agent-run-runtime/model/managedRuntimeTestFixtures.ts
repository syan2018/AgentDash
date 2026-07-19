import type {
  ManagedRuntimeEntityStatus,
  ManagedRuntimeOperationStatus,
} from "../../../generated/agent-runtime-contracts";
import type {
  ManagedRuntimeChangePage,
  ManagedRuntimeCommandAvailability,
  ManagedRuntimeSnapshot,
} from "../../../generated/agent-runtime-validators";

function operationStatus(
  status: ManagedRuntimeEntityStatus,
): ManagedRuntimeOperationStatus {
  if (status === "completed") return "succeeded";
  return status;
}

function availability(
  status: ManagedRuntimeEntityStatus,
  revision: bigint,
): ManagedRuntimeCommandAvailability {
  const evidence = {
    decided_at_revision: revision,
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
  status: ManagedRuntimeEntityStatus,
  revision: bigint,
  latestChangeSequence: bigint,
): ManagedRuntimeSnapshot {
  const terminal =
    status === "completed"
      || status === "failed"
      || status === "interrupted"
      || status === "lost"
      ? {
          outcome: status,
          completed_at_ms: 1001n + revision,
          duration_ms: 1n,
          process_exit: null,
          error: null,
        }
      : null;
  return {
    thread_id: "runtime-thread-child",
    revision,
    latest_change_sequence: latestChangeSequence,
    captured_at_ms: 1000n + revision,
    lifecycle: "active",
    active_turn_id: status === "running" ? "turn-compaction" : null,
    conversation_history: [],
    turns: [
      {
        id: "turn-compaction",
        status,
        item_ids: ["item-compaction"],
      },
    ],
    items: [
      {
        id: "item-compaction",
        turn_id: "turn-compaction",
        status,
        presentation: {
          body: {
            kind: "context_compaction",
            summary: null,
            source_digest: `sha256:compaction-${revision}`,
          },
          started_at_ms: 1000n + revision,
          updated_at_ms: 1001n + revision,
          terminal,
          body_digest: `sha256:compaction-body-${revision}`,
          presentation_digest: `sha256:compaction-presentation-${revision}`,
        },
      },
    ],
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
      submit_input: availability(status, revision),
      request_compaction: availability(status, revision),
      resolve_interaction: availability(status, revision),
    },
  };
}

const started = runtimeSnapshot("running", 5n, 8n);
const completed = runtimeSnapshot("completed", 6n, 9n);
const failed = runtimeSnapshot("failed", 7n, 10n);
const lost = runtimeSnapshot("lost", 8n, 11n);

const changePage: ManagedRuntimeChangePage = {
  thread_id: started.thread_id,
  changes: [
    {
      thread_id: started.thread_id,
      sequence: 9n,
      revision: 6n,
      delta: {
        kind: "source_projection_changed",
        source_change_sequence: 9n,
        source_projection_revision: 6n,
        observation_digest: "sha256:observation-6",
        section: "snapshot",
        section_digest: "sha256:snapshot-6",
        delta: {
          kind: "snapshot_replaced",
          lifecycle: completed.lifecycle,
          active_turn_id: completed.active_turn_id,
          turns: completed.turns,
          items: completed.items,
          interactions: completed.interactions,
          authority: completed.authority,
          fidelity: completed.fidelity,
          applied_surface_revision: null,
        },
      },
    },
  ],
  next: 9n,
  gap: null,
};

const gapPage: ManagedRuntimeChangePage = {
  thread_id: started.thread_id,
  changes: [],
  next: 12n,
  gap: {
    requested_after: 4n,
    earliest_available: 9n,
    latest_available: 12n,
    snapshot_revision: 8n,
  },
};

export const managedRuntimeTestFixtures = {
  snapshots: { started, completed, failed, lost },
  changePage,
  gapPage,
};
