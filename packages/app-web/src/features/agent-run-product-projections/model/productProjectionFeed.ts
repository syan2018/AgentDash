import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import type { AgentRunProjectionTarget } from "../../../generated/agent-run-product-projection-contracts";

export type ProductProjectionFeedLifecycle =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closed";

export interface ProductProjectionSnapshot {
  target: AgentRunProjectionTarget;
  latest_change_sequence: number;
}

export interface ProductProjectionChange {
  target: AgentRunProjectionTarget;
  sequence: number;
}

export interface ProductProjectionChangePage<TChange extends ProductProjectionChange> {
  target: AgentRunProjectionTarget;
  changes: TChange[];
  next: number;
  gap?: unknown | null;
}

export interface ProductProjectionFeedObserver<
  TSnapshot extends ProductProjectionSnapshot,
  TChange extends ProductProjectionChange,
> {
  onSnapshot: (snapshot: TSnapshot, reason: "initial" | "gap_reload") => void;
  onChanges: (changes: readonly TChange[]) => void;
  onLifecycleChange?: (lifecycle: ProductProjectionFeedLifecycle) => void;
  onError?: (error: Error) => void;
}

export interface ProductProjectionFeedDependencies<
  TSnapshot extends ProductProjectionSnapshot,
  TChange extends ProductProjectionChange,
  TPage extends ProductProjectionChangePage<TChange>,
> {
  fetchSnapshot: (target: AgentRunRuntimeTarget) => Promise<TSnapshot>;
  fetchChanges: (target: AgentRunRuntimeTarget, after?: number) => Promise<TPage>;
  schedule: (callback: () => void) => unknown;
  cancel: (handle: unknown) => void;
}

export interface ProductProjectionFeedConnection {
  ready: Promise<void>;
  close: () => void;
}

function exactTarget(
  actual: AgentRunProjectionTarget,
  expected: AgentRunRuntimeTarget,
): boolean {
  return actual.run_id === expected.runId && actual.agent_id === expected.agentId;
}

function normalizeError(error: unknown): Error {
  return error instanceof Error ? error : new Error("Product projection feed 读取失败");
}

export function connectProductProjectionFeed<
  TSnapshot extends ProductProjectionSnapshot,
  TChange extends ProductProjectionChange,
  TPage extends ProductProjectionChangePage<TChange>,
>(
  target: AgentRunRuntimeTarget,
  observer: ProductProjectionFeedObserver<TSnapshot, TChange>,
  dependencies: ProductProjectionFeedDependencies<TSnapshot, TChange, TPage>,
): ProductProjectionFeedConnection {
  let closed = false;
  let scheduled: unknown;
  let cursor: number | null = null;
  let baselineLoaded = false;

  const notifyLifecycle = (lifecycle: ProductProjectionFeedLifecycle): void => {
    observer.onLifecycleChange?.(lifecycle);
  };

  const loadSnapshot = async (reason: "initial" | "gap_reload"): Promise<void> => {
    const snapshot = await dependencies.fetchSnapshot(target);
    if (closed) return;
    if (!exactTarget(snapshot.target, target)) {
      throw new Error("Product projection snapshot target fence mismatch");
    }
    cursor = snapshot.latest_change_sequence;
    baselineLoaded = true;
    observer.onSnapshot(snapshot, reason);
  };

  const poll = async (): Promise<void> => {
    if (closed || cursor === null) return;
    try {
      const page = await dependencies.fetchChanges(target, cursor);
      if (closed) return;
      if (
        !exactTarget(page.target, target)
        || page.changes.some((change) => !exactTarget(change.target, target))
      ) {
        throw new Error("Product projection change target fence mismatch");
      }
      if (page.gap) {
        notifyLifecycle("reconnecting");
        await loadSnapshot("gap_reload");
      } else {
        const changes = page.changes.filter((change) => change.sequence > cursor!);
        let expected = cursor + 1;
        for (const change of changes) {
          if (change.sequence !== expected) {
            throw new Error("Product projection change sequence is not contiguous");
          }
          expected += 1;
        }
        if (page.next < (changes.at(-1)?.sequence ?? cursor)) {
          throw new Error("Product projection cursor regressed");
        }
        cursor = page.next;
        if (changes.length > 0) observer.onChanges(changes);
        notifyLifecycle("connected");
      }
    } catch (error) {
      if (!closed) {
        notifyLifecycle("reconnecting");
        observer.onError?.(normalizeError(error));
      }
    }
    if (!closed) {
      scheduled = dependencies.schedule(() => {
        scheduled = undefined;
        void poll();
      });
    }
  };

  const connect = async (): Promise<void> => {
    notifyLifecycle("connecting");
    try {
      await loadSnapshot(baselineLoaded ? "gap_reload" : "initial");
      if (closed) return;
      notifyLifecycle("connected");
      await poll();
    } catch (error) {
      if (closed) return;
      observer.onError?.(normalizeError(error));
      notifyLifecycle("reconnecting");
      scheduled = dependencies.schedule(() => {
        scheduled = undefined;
        void connect();
      });
    }
  };

  const ready = connect();

  return {
    ready,
    close: () => {
      if (closed) return;
      closed = true;
      if (scheduled !== undefined) dependencies.cancel(scheduled);
      scheduled = undefined;
      if (baselineLoaded) notifyLifecycle("closed");
    },
  };
}
