export class ManagedRuntimeFeedProtocolError extends Error {}

/**
 * Accept a committed Runtime change page without translating its identity,
 * ordering, delta, or availability vocabulary into a UI-owned protocol.
 */
export function consumeManagedRuntimeChangePage<
  TSnapshot extends {
    readonly thread_id: string;
    readonly revision: number;
    readonly latest_change_sequence: number;
  },
  TPage extends {
    readonly thread_id: string;
    readonly changes: readonly {
      readonly thread_id: string;
      readonly sequence: number;
      readonly revision: number;
      readonly delta: unknown;
    }[];
    readonly next: number;
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
    if (change.sequence !== sequence + 1) {
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
