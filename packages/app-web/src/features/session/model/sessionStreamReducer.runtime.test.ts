import { describe, expect, it } from "vitest";
import type { RuntimeEventEnvelope } from "../../../generated/agent-runtime-contracts";
import { projectRuntimeEnvelope } from "./runtimeSessionAdapter";
import { createInitialStreamState, reduceStreamState } from "./sessionStreamReducer";

function project(
  event: RuntimeEventEnvelope["event"],
  sequence: bigint | null,
  transientSequence = 1n,
  streamGeneration = 1n,
) {
  const envelope: RuntimeEventEnvelope = {
    thread_id: "thread-1", sequence, revision: 1n,
    occurred_at_ms: 100n,
    transient: sequence == null ? { binding_id: "binding-1", stream_generation: streamGeneration, sequence: transientSequence, event_id: `${streamGeneration}:${transientSequence}`, turn_id: "turn-1" } : null,
    event,
  };
  const projected = projectRuntimeEnvelope(envelope);
  if (!projected) throw new Error("expected presentation event");
  return projected;
}

describe("sessionStreamReducer canonical Runtime projection", () => {
  it("aggregates transient text and lets durable terminal content become authoritative", () => {
    const delta = project({ kind: "conversation_delta", turn_id: "turn-1", item_id: "item-1", delta: { kind: "agent_message", delta: "hel" } }, null);
    const completed = project({ kind: "item_terminal", turn_id: "turn-1", item_id: "item-1", terminal: { kind: "completed", final_content: { type: "agentMessage", id: "item-1", text: "hello", phase: null } } }, 8n);
    const state = reduceStreamState(createInitialStreamState([]), [delta, completed]);
    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]).toMatchObject({ accumulatedText: "hello", isStreaming: false });
    expect(state.lastAppliedSeq).toBe(8);
    expect(state.lastEphemeralSeq).toBe(1);
  });

  it("deduplicates durable and transient lanes independently", () => {
    const durable = project({ kind: "item_started", turn_id: "turn-1", item_id: "item-1", initial_content: { type: "contextCompaction", id: "item-1" } }, 4n);
    const transient = project({ kind: "conversation_delta", turn_id: "turn-1", item_id: "item-1", delta: { kind: "command_output", delta: "x" } }, null, 2n);
    const once = reduceStreamState(createInitialStreamState([]), [durable, transient]);
    const twice = reduceStreamState(once, [durable, transient]);
    expect(twice.lastAppliedSeq).toBe(4);
    expect(twice.lastEphemeralSeq).toBe(2);
    expect(twice.entries).toEqual(once.entries);
  });

  it("accepts a low transient sequence after stream generation changes", () => {
    const oldGeneration = project(
      { kind: "conversation_delta", turn_id: "turn-1", item_id: "item-1", delta: { kind: "agent_message", delta: "old" } },
      null,
      9n,
      1n,
    );
    const newGeneration = project(
      { kind: "conversation_delta", turn_id: "turn-1", item_id: "item-1", delta: { kind: "agent_message", delta: "new" } },
      null,
      1n,
      2n,
    );
    const state = reduceStreamState(createInitialStreamState([]), [oldGeneration, newGeneration]);
    expect(state.lastEphemeralGeneration).toBe(2);
    expect(state.lastEphemeralSeq).toBe(1);
    expect(state.entries[0]?.accumulatedText).toBe("new");
  });

  it("terminates a failed item card by item identity", () => {
    const started = project({ kind: "item_started", turn_id: "turn-1", item_id: "item-1", initial_content: { type: "commandExecution", id: "item-1", command: "pnpm test", cwd: "D:/Projects/AgentDash", processId: null, source: "agent", status: "inProgress", commandActions: [], aggregatedOutput: "", exitCode: null, durationMs: null } }, 3n);
    const failed = project({ kind: "item_terminal", turn_id: "turn-1", item_id: "item-1", terminal: { kind: "failed", message: "boom" } }, 4n);
    const state = reduceStreamState(createInitialStreamState([]), [started, failed]);
    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]).toMatchObject({ id: "item:item-1", itemFreshness: "completed", isStreaming: false, terminalFailure: "failed", terminalMessage: "boom" });
  });
});
