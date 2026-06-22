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

  it("把 terminal_output 写入 capped terminal store", () => {
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

  it("更新 terminal_state_changed 并校验状态值", () => {
    useTerminalStore.getState().registerTerminal({
      id: "term-1",
      sessionId: "session-1",
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

    const terminal = useTerminalStore
      .getState()
      .getTerminalsForSession("session-1")[0];
    expect(handled).toBe(true);
    expect(terminal?.state).toBe("exited");
    expect(terminal?.exitCode).toBe(0);
  });

  it("未知 platform event 不消费", () => {
    const handled = dispatchSessionPlatformEvent(platformEvent(1, {
      kind: "mailbox_state_changed",
      data: {
        reason: "refresh",
      },
    }));

    expect(handled).toBe(false);
  });
});
