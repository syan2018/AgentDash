import { beforeEach, describe, expect, it } from "vitest";
import type {
  BackboneEnvelope,
  BackboneEvent,
  PlatformEvent,
} from "../../../generated/backbone-protocol";
import type { SessionEventEnvelope } from "./types";
import { dispatchSessionPlatformEvent } from "./sessionPlatformEventDispatcher";
import {
  TERMINAL_OUTPUT_BUFFER_MAX_CHARS,
  useTerminalStore,
} from "./useTerminalStore";

function resetTerminalStore(): void {
  useTerminalStore.setState({
    terminals: new Map(),
    outputBuffers: new Map(),
    outputBufferBaseOffsets: new Map(),
    outputBufferRevisions: new Map(),
    projectedEventKeys: new Set(),
  });
}

function envelope(event: BackboneEvent): BackboneEnvelope {
  return {
    sessionId: "session-1",
    source: {
      connectorId: "connector",
      connectorType: "test",
      executorId: null,
    },
    trace: {
      turnId: "turn-1",
      entryIndex: 0,
    },
    observedAt: "2026-06-22T00:00:00.000Z",
    event,
  };
}

function platformEvent(seq: number, platform: PlatformEvent): SessionEventEnvelope {
  return {
    session_id: "session-1",
    event_seq: seq,
    occurred_at_ms: seq,
    committed_at_ms: seq,
    session_update_type: "platform",
    turn_id: "turn-1",
    entry_index: 0,
    notification: envelope({
      type: "platform",
      payload: platform,
    }),
  };
}

describe("dispatchSessionPlatformEvent", () => {
  beforeEach(() => {
    resetTerminalStore();
  });

  it("writes terminal_output to capped terminal store", () => {
    const handled = dispatchSessionPlatformEvent(platformEvent(1, {
      kind: "terminal_output",
      data: {
        terminal_id: "term-1",
        data: `${"x".repeat(8)}${"y".repeat(TERMINAL_OUTPUT_BUFFER_MAX_CHARS)}`,
      },
    }));

    const store = useTerminalStore.getState();
    expect(handled).toBe(true);
    expect(store.getOutput("term-1")).toBe("y".repeat(TERMINAL_OUTPUT_BUFFER_MAX_CHARS));
    expect(store.getOutputBaseOffset("term-1")).toBe(8);
  });

  it("updates terminal_state_changed and validates state value", () => {
    useTerminalStore.getState().registerTerminal({
      id: "term-1",
      capability: "interactive",
      cwd: ".",
      state: "running",
      createdAt: 1,
    });

    const handled = dispatchSessionPlatformEvent(platformEvent(1, {
      kind: "terminal_state_changed",
      data: {
        terminal_id: "term-1",
        state: "exited",
        exit_code: 0,
        message: null,
      },
    }));

    const terminal = useTerminalStore.getState().terminals.get("term-1");
    expect(handled).toBe(true);
    expect(terminal?.state).toBe("exited");
    expect(terminal?.exitCode).toBe(0);
  });

  it("does not duplicate output on repeated terminal_output event projection", () => {
    const event = platformEvent(1, {
      kind: "terminal_output",
      data: {
        terminal_id: "term-1",
        data: "hello",
      },
    });

    expect(dispatchSessionPlatformEvent(event)).toBe(true);
    expect(dispatchSessionPlatformEvent(event)).toBe(true);

    expect(useTerminalStore.getState().getOutput("term-1")).toBe("hello");
  });

  it("creates state-only projection for unregistered terminal on terminal_state_changed", () => {
    const handled = dispatchSessionPlatformEvent(platformEvent(1, {
      kind: "terminal_state_changed",
      data: {
        terminal_id: "term-1",
        state: "lost",
        exit_code: null,
        message: null,
      },
    }));

    const terminal = useTerminalStore.getState().terminals.get("term-1");
    expect(handled).toBe(true);
    expect(terminal?.capability).toBe("state_only");
    expect(terminal?.state).toBe("lost");
  });

  it("keeps output and state projection on the same terminal id when output arrives first", () => {
    expect(dispatchSessionPlatformEvent(platformEvent(1, {
      kind: "terminal_output",
      data: {
        terminal_id: "term-running-1",
        data: "ready\n",
      },
    }))).toBe(true);

    expect(dispatchSessionPlatformEvent(platformEvent(2, {
      kind: "terminal_state_changed",
      data: {
        terminal_id: "term-running-1",
        state: "running",
        exit_code: null,
        message: null,
      },
    }))).toBe(true);

    const store = useTerminalStore.getState();
    const terminal = store.terminals.get("term-running-1");
    expect(store.getOutput("term-running-1")).toBe("ready\n");
    expect(terminal?.id).toBe("term-running-1");
    expect(terminal?.capability).toBe("state_only");
    expect(terminal?.state).toBe("running");
  });

  it("does not consume unknown platform events", () => {
    const handled = dispatchSessionPlatformEvent(platformEvent(1, {
      kind: "mailbox_state_changed",
      data: {
        reason: "refresh",
      },
    }));

    expect(handled).toBe(false);
  });
});
