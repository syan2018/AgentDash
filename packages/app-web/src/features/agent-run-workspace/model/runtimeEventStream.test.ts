import { beforeEach, describe, expect, it, vi } from "vitest";

import { createRuntimeEventStream, parseRuntimeEventStreamItem } from "./runtimeEventStream";

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
  it("parses the canonical event envelope and durable cursor", () => {
    const parsed = parseRuntimeEventStreamItem({
      kind: "event",
      durable_cursor: 42,
      envelope: {
        thread_id: "thread-1",
        sequence: 42,
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
      "http://api.test/api/agent-runs/run%201/agents/agent%2F1/runtime/events/stream/ndjson?after=41&include_transient=false",
    );
    stream.close();
  });
});
