import type {
  WorkspaceModulePresentationChange,
  WorkspaceModulePresentationIntent,
  WorkspaceModulePresentationPendingIntent,
  WorkspaceModulePresentationSnapshot,
} from "../../../generated/agent-run-product-projection-contracts";

export interface WorkspacePresentationPendingConsumerDependencies {
  fulfill: (intent: WorkspaceModulePresentationIntent) => Promise<void>;
  acknowledge: (intentId: string, observedChangeSequence: bigint) => Promise<unknown>;
  scheduleRetry: (callback: () => void) => unknown;
  cancelRetry: (handle: unknown) => void;
  onError: (error: Error) => void;
}

interface PendingAttempt {
  pending: WorkspaceModulePresentationPendingIntent;
  localFulfilled: boolean;
  inFlight: boolean;
  retryHandle?: unknown;
}

function normalizeError(error: unknown): Error {
  return error instanceof Error
    ? error
    : new Error("Workspace presentation pending intent 履行失败");
}

export class WorkspacePresentationPendingConsumer {
  private readonly attempts = new Map<string, PendingAttempt>();
  private readonly dependencies: WorkspacePresentationPendingConsumerDependencies;
  private closed = false;

  constructor(dependencies: WorkspacePresentationPendingConsumerDependencies) {
    this.dependencies = dependencies;
  }

  consumeSnapshot(snapshot: WorkspaceModulePresentationSnapshot): void {
    const pendingIds = new Set(
      snapshot.pending_intents.map((pending) => pending.intent.intent_id),
    );
    for (const intentId of this.attempts.keys()) {
      if (!pendingIds.has(intentId)) this.remove(intentId);
    }
    for (const pending of snapshot.pending_intents) {
      this.enqueue(pending);
    }
  }

  consumeChanges(changes: readonly WorkspaceModulePresentationChange[]): void {
    for (const change of changes) {
      if (change.status === "fulfilled") {
        this.remove(change.intent.intent_id);
        continue;
      }
      this.enqueue({
        change_sequence: change.sequence,
        intent: change.intent,
      });
    }
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    for (const intentId of this.attempts.keys()) this.remove(intentId);
  }

  private enqueue(pending: WorkspaceModulePresentationPendingIntent): void {
    if (this.closed) return;
    const intentId = pending.intent.intent_id;
    const existing = this.attempts.get(intentId);
    if (existing) {
      if (pending.change_sequence < existing.pending.change_sequence) return;
      existing.pending = pending;
      if (existing.inFlight || existing.retryHandle !== undefined) return;
      void this.attempt(intentId, existing);
      return;
    }
    const attempt: PendingAttempt = {
      pending,
      localFulfilled: false,
      inFlight: false,
    };
    this.attempts.set(intentId, attempt);
    void this.attempt(intentId, attempt);
  }

  private async attempt(intentId: string, attempt: PendingAttempt): Promise<void> {
    if (this.closed || attempt.inFlight || this.attempts.get(intentId) !== attempt) return;
    attempt.inFlight = true;
    attempt.retryHandle = undefined;
    let retry = false;
    try {
      if (!attempt.localFulfilled) {
        await this.dependencies.fulfill(attempt.pending.intent);
        attempt.localFulfilled = true;
      }
      await this.dependencies.acknowledge(
        intentId,
        attempt.pending.change_sequence,
      );
      this.remove(intentId);
    } catch (error) {
      if (this.closed || this.attempts.get(intentId) !== attempt) return;
      this.dependencies.onError(normalizeError(error));
      retry = true;
    } finally {
      attempt.inFlight = false;
    }
    if (retry && !this.closed && this.attempts.get(intentId) === attempt) {
      attempt.retryHandle = this.dependencies.scheduleRetry(() => {
        attempt.retryHandle = undefined;
        void this.attempt(intentId, attempt);
      });
    }
  }

  private remove(intentId: string): void {
    const attempt = this.attempts.get(intentId);
    if (!attempt) return;
    if (attempt.retryHandle !== undefined) {
      this.dependencies.cancelRetry(attempt.retryHandle);
    }
    this.attempts.delete(intentId);
  }
}
