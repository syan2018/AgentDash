import { describe, expect, it, vi } from "vitest";

import type {
  WorkspaceModulePresentationIntent,
  WorkspaceModulePresentationSnapshot,
} from "../../../generated/agent-run-product-projection-contracts";
import type { RuntimeU64 } from "../../../generated/agent-runtime-contracts";
import { WorkspacePresentationPendingConsumer } from "./workspacePresentationPendingConsumer";

const target = { run_id: "run-1", agent_id: "agent-1" };
const runtimeU64 = (value: number): RuntimeU64 => String(value) as RuntimeU64;
const intent: WorkspaceModulePresentationIntent = {
  intent_id: "intent-1",
  effect_id: "effect-1",
  target,
  actor: { kind: "agent_tool", actor_id: "agent-1" },
  cause: {
    runtime_thread_id: "thread-1",
    runtime_operation_id: null,
    runtime_turn_id: "turn-1",
    runtime_item_id: "item-1",
  },
  currentness_fence: {
    runtime_thread_id: "thread-1",
    source_binding: {
      source_ref: "source-1",
      committed_at_revision: runtimeU64(1),
      applied_surface_revision: runtimeU64(3),
      activated_at_revision: runtimeU64(2),
    },
    surface_revision: 3n,
    module_id: "canvas:one",
    view_key: "preview",
    renderer_kind: "canvas",
    presentation_uri: "canvas://one",
  },
  presentation_digest: "sha256:presentation",
  presentation: {
    module_id: "canvas:one",
    view_key: "preview",
    renderer_kind: "canvas",
    presentation_uri: "canvas://one",
    title: "Canvas",
    payload: null,
    diagnostics: null,
  },
  committed_at_ms: 1n,
};

function snapshot(): WorkspaceModulePresentationSnapshot {
  return {
    target,
    revision: 4n,
    latest_change_sequence: 4n,
    captured_at_ms: 10n,
    pending_intents: [{ change_sequence: 4n, intent }],
  };
}

describe("WorkspacePresentationPendingConsumer", () => {
  it("deduplicates the same pending intent across initial and gap snapshots", async () => {
    let resolveAck!: () => void;
    const acknowledge = vi.fn(() => new Promise<void>((resolve) => {
      resolveAck = resolve;
    }));
    const fulfill = vi.fn(async () => {});
    const consumer = new WorkspacePresentationPendingConsumer({
      fulfill,
      acknowledge,
      scheduleRetry: vi.fn(),
      cancelRetry: vi.fn(),
      onError: vi.fn(),
    });

    consumer.consumeSnapshot(snapshot());
    consumer.consumeSnapshot(snapshot());
    await vi.waitFor(() => expect(acknowledge).toHaveBeenCalledTimes(1));
    expect(fulfill).toHaveBeenCalledTimes(1);
    resolveAck();
    await Promise.resolve();
    consumer.close();
  });

  it("retries an idempotent ack without reopening an already fulfilled panel", async () => {
    const scheduled: Array<() => void> = [];
    const fulfill = vi.fn(async () => {});
    const acknowledge = vi.fn()
      .mockRejectedValueOnce(new Error("ack response lost"))
      .mockResolvedValueOnce({});
    const onError = vi.fn();
    const consumer = new WorkspacePresentationPendingConsumer({
      fulfill,
      acknowledge,
      scheduleRetry: (callback) => {
        scheduled.push(callback);
        return callback;
      },
      cancelRetry: vi.fn(),
      onError,
    });

    consumer.consumeSnapshot(snapshot());
    await vi.waitFor(() => expect(scheduled).toHaveLength(1));
    expect(onError).toHaveBeenCalledWith(
      expect.objectContaining({ message: "ack response lost" }),
    );
    scheduled.shift()?.();
    await vi.waitFor(() => expect(acknowledge).toHaveBeenCalledTimes(2));
    expect(fulfill).toHaveBeenCalledTimes(1);
    consumer.close();
  });
});
