import type {
  ManagedRuntimeChangePage,
  ManagedRuntimeItem,
  ManagedRuntimeSnapshot,
} from "../../../generated/agent-runtime-validators";

export class ManagedRuntimeFeedProtocolError extends Error {}

/**
 * Accept a committed Runtime change page without translating its identity,
 * ordering, delta, or availability vocabulary into a UI-owned protocol.
 */
export function consumeManagedRuntimeChangePage<
  TSnapshot extends {
    readonly thread_id: string;
    readonly revision: bigint;
    readonly latest_change_sequence: bigint;
  },
  TPage extends {
    readonly thread_id: string;
    readonly changes: readonly {
      readonly thread_id: string;
      readonly sequence: bigint;
      readonly revision: bigint;
      readonly delta: unknown;
    }[];
    readonly next: bigint;
    readonly gap: unknown | null;
  },
>(snapshot: TSnapshot, page: TPage) {
  if (snapshot.thread_id !== page.thread_id) {
    throw new ManagedRuntimeFeedProtocolError(
      "change page thread does not match the managed Runtime snapshot",
    );
  }
  if (page.gap !== null) {
    return { kind: "snapshot_reload_required" } as const;
  }

  let sequence = snapshot.latest_change_sequence;
  let revision = snapshot.revision;
  for (const change of page.changes) {
    if (change.thread_id !== snapshot.thread_id) {
      throw new ManagedRuntimeFeedProtocolError(
        "change thread does not match the managed Runtime snapshot",
      );
    }
    if (change.sequence <= snapshot.latest_change_sequence) {
      if (change.revision > snapshot.revision) {
        throw new ManagedRuntimeFeedProtocolError(
          "duplicate managed Runtime change has a future revision",
        );
      }
      continue;
    }
    if (change.sequence !== sequence + 1n) {
      throw new ManagedRuntimeFeedProtocolError(
        "managed Runtime changes are not contiguous",
      );
    }
    if (change.revision < revision) {
      throw new ManagedRuntimeFeedProtocolError(
        "managed Runtime change revision moved backwards",
      );
    }
    sequence = change.sequence;
    revision = change.revision;
  }
  if (
    sequence === snapshot.latest_change_sequence
    && page.next <= snapshot.latest_change_sequence
  ) {
    return { kind: "duplicate" } as const;
  }
  if (page.next !== sequence) {
    throw new ManagedRuntimeFeedProtocolError(
      "managed Runtime page cursor does not match its committed tail",
    );
  }

  return { kind: "apply", change_page: page } as const;
}

/**
 * Read the Runtime-owned decision verbatim. Command availability is not
 * inferred from item, operation, worker, or request timing in the UI.
 */
export function managedRuntimeCommandAvailability<
  TAvailability extends Readonly<Record<string, unknown>>,
  TCommand extends keyof TAvailability,
>(
  snapshot: { readonly command_availability: TAvailability },
  command: TCommand,
): TAvailability[TCommand] {
  return snapshot.command_availability[command];
}

function replaceById<T extends { readonly id: string }>(
  current: readonly T[],
  value: T,
): T[] {
  const index = current.findIndex((item) => item.id === value.id);
  if (index < 0) return [...current, value];
  const next = [...current];
  next[index] = value;
  return next;
}

type SemanticItemTransition = Extract<
  Extract<
    ManagedRuntimeChangePage["changes"][number]["delta"],
    { kind: "source_projection_changed" }
  >["delta"],
  { kind: "item_transitioned" }
>["transition"];

function applyItemTransition(
  items: readonly ManagedRuntimeItem[],
  itemId: string,
  transition: SemanticItemTransition,
): ManagedRuntimeItem[] {
  const current = items.find((item) => item.id === itemId);
  if (!current) {
    throw new ManagedRuntimeFeedProtocolError(
      `managed Runtime item transition references unknown item ${itemId}`,
    );
  }
  const status =
    transition.kind === "terminal"
      ? transition.presentation.terminal?.outcome
      : transition.kind === "started"
        ? "running"
        : current.status;
  if (!status) {
    throw new ManagedRuntimeFeedProtocolError(
      `terminal managed Runtime item ${itemId} has no terminal outcome`,
    );
  }
  return replaceById(items, {
    ...current,
    status,
    presentation: transition.presentation,
  });
}

/**
 * Fold only canonical Runtime deltas. Source observation metadata never
 * manufactures projection state; the matching typed source projection delta
 * carries the authoritative section value.
 */
export function applyManagedRuntimeChangePage(
  snapshot: ManagedRuntimeSnapshot,
  page: ManagedRuntimeChangePage,
): ManagedRuntimeSnapshot | null {
  const outcome = consumeManagedRuntimeChangePage(snapshot, page);
  if (outcome.kind === "snapshot_reload_required") return null;
  if (outcome.kind === "duplicate") return snapshot;

  let next = snapshot;
  for (const change of page.changes) {
    if (change.sequence <= snapshot.latest_change_sequence) continue;
    const delta = change.delta;
    switch (delta.kind) {
      case "source_observation_applied":
      case "surface_evidence_changed":
        break;
      case "thread_name_changed":
        next = {
          ...next,
          thread_name: delta.thread_name,
          thread_name_source: delta.thread_name === null ? null : delta.source,
        };
        break;
      case "runtime_lifecycle_changed":
        next = { ...next, lifecycle: delta.lifecycle };
        break;
      case "source_binding_changed":
        next = { ...next, source_binding: delta.binding };
        break;
      case "operation_upserted":
        next = {
          ...next,
          operations: replaceById(next.operations, delta.operation),
        };
        break;
      case "command_availability_changed":
        next = {
          ...next,
          command_availability: {
            ...next.command_availability,
            [delta.command]: delta.availability,
          },
        };
        break;
      case "source_projection_changed": {
        const sourceDelta = delta.delta;
        switch (sourceDelta.kind) {
          case "snapshot_replaced":
            next = {
              ...next,
              lifecycle: sourceDelta.lifecycle,
              active_turn_id: sourceDelta.active_turn_id,
              turns: sourceDelta.turns,
              items: sourceDelta.items,
              interactions: sourceDelta.interactions,
              authority: sourceDelta.authority,
              fidelity: sourceDelta.fidelity,
            };
            break;
          case "lifecycle_changed":
            next = { ...next, lifecycle: sourceDelta.lifecycle };
            break;
          case "active_turn_changed":
            next = { ...next, active_turn_id: sourceDelta.active_turn_id };
            break;
          case "turns_changed":
            next = { ...next, turns: sourceDelta.turns };
            break;
          case "items_changed":
            next = { ...next, items: sourceDelta.items };
            break;
          case "item_transitioned":
            next = {
              ...next,
              items: applyItemTransition(
                next.items,
                sourceDelta.item_id,
                sourceDelta.transition,
              ),
            };
            break;
          case "interactions_changed":
          case "surface_changed":
            next =
              sourceDelta.kind === "interactions_changed"
                ? { ...next, interactions: sourceDelta.interactions }
                : next;
            break;
        }
        break;
      }
    }
    next = {
      ...next,
      revision: change.revision,
      latest_change_sequence: change.sequence,
    };
  }
  return next;
}
