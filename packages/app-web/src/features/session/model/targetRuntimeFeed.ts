export type RuntimeFeedItem = {
  itemId: string;
  turnId: string;
  kind: string;
  text: string;
};

export type RuntimeCompactionLifecycle =
  | { state: "idle" }
  | { state: "started"; operationId: string }
  | { state: "completed"; operationId: string }
  | { state: "failed"; operationId: string; reason: string }
  | { state: "lost"; operationId: string; reason: string };

export type RuntimeCommandAvailability = {
  submitInput: boolean;
  compact: boolean;
  reason: "compaction_in_progress" | "compaction_state_lost" | null;
};

export type TargetRuntimeFeedSnapshot = {
  sourceCoordinate: string;
  snapshotRevision: number;
  committedSequence: number;
  cursor: string | null;
  items: RuntimeFeedItem[];
  compaction: RuntimeCompactionLifecycle;
  availability: RuntimeCommandAvailability;
};

export type TargetRuntimeCommittedChangePayload =
  | { kind: "item_upserted"; item: RuntimeFeedItem }
  | { kind: "item_removed"; itemId: string }
  | { kind: "compaction_started"; operationId: string }
  | { kind: "compaction_completed"; operationId: string }
  | { kind: "compaction_failed"; operationId: string; reason: string }
  | { kind: "compaction_lost"; operationId: string; reason: string };

export type TargetRuntimeCommittedChange = {
  sequence: number;
  previousSnapshotRevision: number;
  snapshotRevision: number;
  payload: TargetRuntimeCommittedChangePayload;
};

export type TargetRuntimeChangePage = {
  sourceCoordinate: string;
  afterCursor: string | null;
  nextCursor: string | null;
  gap: boolean;
  changes: TargetRuntimeCommittedChange[];
};

export type TargetRuntimeFeedReduceOutcome =
  | { kind: "applied"; snapshot: TargetRuntimeFeedSnapshot }
  | { kind: "snapshot_reload_required" };

export class TargetRuntimeFeedProtocolError extends Error {}

export function availabilityForCompaction(
  compaction: RuntimeCompactionLifecycle,
): RuntimeCommandAvailability {
  if (compaction.state === "started") {
    return {
      submitInput: false,
      compact: false,
      reason: "compaction_in_progress",
    };
  }
  if (compaction.state === "lost") {
    return {
      submitInput: false,
      compact: false,
      reason: "compaction_state_lost",
    };
  }
  return { submitInput: true, compact: true, reason: null };
}

export function reduceTargetRuntimeChangePage(
  current: TargetRuntimeFeedSnapshot,
  page: TargetRuntimeChangePage,
): TargetRuntimeFeedReduceOutcome {
  if (current.sourceCoordinate !== page.sourceCoordinate) {
    throw new TargetRuntimeFeedProtocolError(
      "change page source does not match target Runtime snapshot",
    );
  }
  if (page.gap) {
    return { kind: "snapshot_reload_required" };
  }
  if (current.cursor !== page.afterCursor) {
    throw new TargetRuntimeFeedProtocolError(
      "change page cursor does not continue target Runtime snapshot",
    );
  }

  const next: TargetRuntimeFeedSnapshot = {
    ...current,
    items: [...current.items],
    compaction: { ...current.compaction },
  };
  for (const change of page.changes) {
    if (change.sequence !== next.committedSequence + 1) {
      throw new TargetRuntimeFeedProtocolError(
        "committed change sequence is not contiguous",
      );
    }
    if (
      change.previousSnapshotRevision !== next.snapshotRevision ||
      change.snapshotRevision !== next.snapshotRevision + 1
    ) {
      throw new TargetRuntimeFeedProtocolError(
        "committed change revision is not contiguous",
      );
    }
    applyCommittedChange(next, change.payload);
    next.committedSequence = change.sequence;
    next.snapshotRevision = change.snapshotRevision;
  }
  next.cursor = page.nextCursor;
  next.availability = availabilityForCompaction(next.compaction);
  return { kind: "applied", snapshot: next };
}

function applyCommittedChange(
  snapshot: TargetRuntimeFeedSnapshot,
  payload: TargetRuntimeCommittedChangePayload,
): void {
  switch (payload.kind) {
    case "item_upserted": {
      const index = snapshot.items.findIndex(
        (item) => item.itemId === payload.item.itemId,
      );
      if (index === -1) {
        snapshot.items.push(payload.item);
      } else {
        snapshot.items[index] = payload.item;
      }
      return;
    }
    case "item_removed":
      snapshot.items = snapshot.items.filter(
        (item) => item.itemId !== payload.itemId,
      );
      return;
    case "compaction_started":
      snapshot.compaction = {
        state: "started",
        operationId: payload.operationId,
      };
      return;
    case "compaction_completed":
      snapshot.compaction = {
        state: "completed",
        operationId: payload.operationId,
      };
      return;
    case "compaction_failed":
      snapshot.compaction = {
        state: "failed",
        operationId: payload.operationId,
        reason: payload.reason,
      };
      return;
    case "compaction_lost":
      snapshot.compaction = {
        state: "lost",
        operationId: payload.operationId,
        reason: payload.reason,
      };
  }
}
