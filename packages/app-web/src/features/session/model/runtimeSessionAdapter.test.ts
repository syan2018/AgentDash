import { describe, expect, it } from "vitest";
import type { RuntimeEventEnvelope, RuntimeSnapshot } from "../../../generated/agent-runtime-contracts";
import { projectRuntimeEnvelope, projectRuntimeSnapshot, runtimeSnapshotCursor } from "./runtimeSessionAdapter";

function envelope(event: RuntimeEventEnvelope["event"], sequence: bigint | null = 7n): RuntimeEventEnvelope {
  return { thread_id: "thread-1", occurred_at_ms: 100n, sequence, transient: sequence == null ? { binding_id: "binding-1", stream_generation: 2n, sequence: 3n, event_id: "2:3", turn_id: "turn-1" } : null, revision: 9n, event };
}

describe("runtimeSessionAdapter", () => {
  it("projects typed item and delta payloads without text fallback", () => {
    const item = { type: "commandExecution" as const, id: "item-1", command: "pnpm test", cwd: "D:/workspace", processId: null, source: "agent" as const, status: "inProgress" as const, commandActions: [], aggregatedOutput: "", exitCode: null, durationMs: null };
    expect(projectRuntimeEnvelope(envelope({ kind: "item_started", turn_id: "turn-1", item_id: "item-1", initial_content: item }))?.event).toMatchObject({ type: "item_started", payload: { item } });
    expect(projectRuntimeEnvelope(envelope({ kind: "conversation_delta", turn_id: "turn-1", item_id: "item-1", delta: { kind: "command_output", delta: "ok" } }, null))).toMatchObject({ event_seq: 3, ephemeral: true, event: { type: "command_output_delta", payload: { delta: "ok" } } });
  });

  it("uses snapshot transcript only as a presentation baseline", () => {
    const snapshot = { thread_id: "thread-1", latest_event_sequence: 12n, captured_at_ms: 100n, transcript: [{ turn_id: "turn-1", item_id: "item-1", final_content: { type: "agentMessage", id: "item-1", text: "done", phase: null } }] } as unknown as RuntimeSnapshot;
    const projected = projectRuntimeSnapshot(snapshot);
    expect(projected).toHaveLength(1);
    expect(projected[0]).toMatchObject({ ephemeral: false, event: { type: "item_completed", payload: { item: { type: "agentMessage", text: "done" } } } });
    expect(runtimeSnapshotCursor(snapshot)).toBe(12);
    expect(runtimeSnapshotCursor(null)).toBe(0);
  });

  it("projects runtime turn lifecycle as local presentation metadata", () => {
    expect(projectRuntimeEnvelope(envelope({ kind: "turn_terminal", turn_id: "turn-1", terminal: "completed", message: null }))?.event).toMatchObject({ type: "platform", payload: { kind: "session_meta_update", data: { key: "runtime_turn_terminal" } } });
  });
});
