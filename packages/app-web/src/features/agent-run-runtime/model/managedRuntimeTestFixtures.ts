import type {
  ManagedRuntimeChangePage,
  ManagedRuntimeCommandAvailability,
  ManagedRuntimeEntityStatus,
  ManagedRuntimeOperationStatus,
  ManagedRuntimeSnapshot,
} from "../../../generated/agent-runtime-contracts";

function operationStatus(
  status: ManagedRuntimeEntityStatus,
): ManagedRuntimeOperationStatus {
  if (status === "completed") return "succeeded";
  return status;
}

function availability(
  status: ManagedRuntimeEntityStatus,
  revision: number,
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
  revision: number,
  latestChangeSequence: number,
): ManagedRuntimeSnapshot {
  return {
    thread_id: "runtime-thread-child",
    revision,
    latest_change_sequence: latestChangeSequence,
    captured_at_ms: 1000 + revision,
    lifecycle: "active",
    active_turn_id: status === "running" ? "turn-compaction" : null,
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
        content: { kind: "context_compaction" },
        content_digest: `sha256:compaction-${revision}`,
      },
    ],
    interactions: [],
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

const started = runtimeSnapshot("running", 5, 8);
const completed = runtimeSnapshot("completed", 6, 9);
const failed = runtimeSnapshot("failed", 7, 10);
const lost = runtimeSnapshot("lost", 8, 11);

const changePage: ManagedRuntimeChangePage = {
  thread_id: started.thread_id,
  changes: [
    {
      thread_id: started.thread_id,
      sequence: 9,
      revision: 6,
      delta: {
        kind: "source_projection_changed",
        source_change_sequence: 9,
        source_projection_revision: 6,
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
  next: 9,
  gap: null,
};

const gapPage: ManagedRuntimeChangePage = {
  thread_id: started.thread_id,
  changes: [],
  next: 12,
  gap: {
    requested_after: 4,
    earliest_available: 9,
    latest_available: 12,
    snapshot_revision: 8,
  },
};

export const managedRuntimeTestFixtures = {
  snapshots: { started, completed, failed, lost },
  changePage,
  gapPage,
};
