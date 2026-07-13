import { beforeEach, describe, expect, it, vi } from "vitest";

import { advanceRuntimeStreamCursor, createRuntimeEventStream, parseRuntimeEventStreamItem, runtimeStreamSearchParams, type RuntimeStreamCursorState } from "./runtimeEventStream";

const mocks = vi.hoisted(() => ({
  authenticatedFetch: vi.fn(),
  registerStreamConnection: vi.fn(() => vi.fn()),
  buildApiPath: vi.fn((path: string) => `http://api.test/api${path}`),
}));

vi.mock("../../../api/client", () => ({ authenticatedFetch: mocks.authenticatedFetch }));
vi.mock("../../../api/origin", () => ({ buildApiPath: mocks.buildApiPath }));
vi.mock("../../../api/streamRegistry", () => ({
  registerStreamConnection: mocks.registerStreamConnection,
}));

beforeEach(() => {
  mocks.authenticatedFetch.mockReset();
  mocks.registerStreamConnection.mockReset();
  mocks.registerStreamConnection.mockReturnValue(vi.fn());
  mocks.buildApiPath.mockClear();
});

describe("Agent Runtime event stream", () => {
  const transientItem = (generation: number, sequence: number) => parseRuntimeEventStreamItem({
    kind: "event", durable_cursor: null,
    transient_cursor: { binding_id: "binding-1", stream_generation: generation, sequence, event_id: `${generation}:${sequence}`, turn_id: "turn-1" },
    envelope: { thread_id: "thread-1", occurred_at_ms: 100, sequence: null, transient: { binding_id: "binding-1", stream_generation: generation, sequence, event_id: `${generation}:${sequence}`, turn_id: "turn-1" }, revision: 1, event: { kind: "conversation_delta", turn_id: "turn-1", item_id: "item-1", delta: { kind: "agent_message", delta: "x" } } },
  })!;

  it("tracks generation, rejects duplicate transient sequences, clears on terminal and isolates targets", () => {
    let state: RuntimeStreamCursorState = { targetKey: "run-a:agent-a", durable: 4, transient: null, generation: null };
    const advanced = advanceRuntimeStreamCursor(state, transientItem(7, 2), state.targetKey);
    expect(advanced.accepted).toBe(true);
    state = advanced.state;
    expect(runtimeStreamSearchParams(state).toString()).toBe("after=4&include_transient=true&transient_after=2&stream_generation=7");
    expect(advanceRuntimeStreamCursor(state, transientItem(7, 2), state.targetKey).accepted).toBe(false);
    expect(advanceRuntimeStreamCursor(state, transientItem(8, 1), state.targetKey).accepted).toBe(true);
    const terminal = parseRuntimeEventStreamItem({ kind: "event", durable_cursor: 5, transient_cursor: null, envelope: { thread_id: "thread-1", occurred_at_ms: 101, sequence: 5, transient: null, revision: 2, event: { kind: "turn_terminal", turn_id: "turn-1", terminal: "completed", message: null } } })!;
    const cleared = advanceRuntimeStreamCursor(state, terminal, state.targetKey).state;
    expect(cleared.transient).toBeNull();
    expect(cleared.generation).toBeNull();
    expect(advanceRuntimeStreamCursor(state, transientItem(7, 3), "run-b:agent-b").state).toMatchObject({ targetKey: "run-b:agent-b", durable: 0, transient: 3 });
    const lagged = parseRuntimeEventStreamItem({ kind: "error", error: { kind: "unavailable", reason: "lagged", retryable: true } })!;
    expect(advanceRuntimeStreamCursor(state, lagged, state.targetKey)).toEqual({ state, accepted: false });
  });
  it("parses the canonical event envelope and durable cursor", () => {
    const parsed = parseRuntimeEventStreamItem({
      kind: "event",
      durable_cursor: 42,
      transient_cursor: null,
      envelope: {
        thread_id: "thread-1",
        occurred_at_ms: 1_000,
        sequence: 42,
        transient: null,
        revision: 7,
        event: { kind: "turn_started", turn_id: "turn-1" },
      },
    });

    expect(parsed?.kind).toBe("event");
    if (parsed?.kind === "event") {
      expect(parsed.durable_cursor).toBe(42);
      expect(parsed.envelope.event.kind).toBe("turn_started");
    }
  });

  it.each([
    ["missing timestamp", { thread_id: "thread-1", sequence: 1, transient: null, revision: 1, event: { kind: "turn_started" } }],
    ["negative timestamp", { thread_id: "thread-1", occurred_at_ms: -1, sequence: 1, transient: null, revision: 1, event: { kind: "turn_started" } }],
    ["nonfinite timestamp", { thread_id: "thread-1", occurred_at_ms: Number.POSITIVE_INFINITY, sequence: 1, transient: null, revision: 1, event: { kind: "turn_started" } }],
    ["wrong sequence", { thread_id: "thread-1", occurred_at_ms: 1, sequence: "1", transient: null, revision: 1, event: { kind: "turn_started" } }],
    ["wrong transient union", { thread_id: "thread-1", occurred_at_ms: 1, sequence: 1, transient: { binding_id: "b" }, revision: 1, event: { kind: "turn_started" } }],
    ["wrong revision", { thread_id: "thread-1", occurred_at_ms: 1, sequence: 1, transient: null, revision: null, event: { kind: "turn_started" } }],
  ])("rejects %s", (_label, envelope) => {
    expect(parseRuntimeEventStreamItem({ kind: "event", durable_cursor: 1, transient_cursor: null, envelope })).toBeNull();
  });

  it("uses the durable cursor as the next stream resume coordinate", async () => {
    mocks.authenticatedFetch.mockImplementation(() => new Promise(() => {}));
    const stream = createRuntimeEventStream({
      target: { runId: "run 1", agentId: "agent/1" },
      after: 41,
      onEvent: vi.fn(),
      onLifecycleChange: vi.fn(),
      onError: vi.fn(),
    });

    await vi.waitFor(() => expect(mocks.authenticatedFetch).toHaveBeenCalledOnce());
    expect(mocks.authenticatedFetch.mock.calls[0]?.[0]).toBe(
      "http://api.test/api/agent-runs/run%201/agents/agent%2F1/runtime/events/stream/ndjson?after=41&include_transient=true",
    );
    stream.close();
  });

  it("does not dispatch a duplicate accepted-event twice", async () => {
    const item = {
      kind: "event",
      durable_cursor: null,
      transient_cursor: { binding_id: "binding-1", stream_generation: 3, sequence: 1, event_id: "3:1", turn_id: "turn-1" },
      envelope: {
        thread_id: "thread-1",
        occurred_at_ms: 100,
        sequence: null,
        transient: { binding_id: "binding-1", stream_generation: 3, sequence: 1, event_id: "3:1", turn_id: "turn-1" },
        revision: 1,
        event: { kind: "conversation_delta", turn_id: "turn-1", item_id: "item-1", delta: { kind: "agent_message", delta: "x" } },
      },
    };
    const bytes = new TextEncoder().encode(`${JSON.stringify(item)}\n${JSON.stringify(item)}\n`);
    mocks.authenticatedFetch.mockResolvedValue(new Response(new ReadableStream({ start(controller) { controller.enqueue(bytes); controller.close(); } }), { status: 200 }));
    const onEvent = vi.fn();
    const stream = createRuntimeEventStream({
      target: { runId: "run-1", agentId: "agent-1" },
      onEvent,
      onLifecycleChange: vi.fn(),
      onError: vi.fn(),
    });
    await vi.waitFor(() => expect(onEvent).toHaveBeenCalledOnce());
    stream.close();
  });
});
