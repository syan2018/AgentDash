import { beforeEach, describe, expect, it } from "vitest";
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

describe("useTerminalStore", () => {
  beforeEach(() => {
    resetTerminalStore();
  });

  it("preserves terminal output under capacity", () => {
    const store = useTerminalStore.getState();

    store.appendOutput("term-1", "hello");
    store.appendOutput("term-1", " world");

    expect(useTerminalStore.getState().getOutput("term-1")).toBe("hello world");
    expect(useTerminalStore.getState().getOutputBaseOffset("term-1")).toBe(0);
  });

  it("retains only recent output and tracks dropped prefix length when over capacity", () => {
    const store = useTerminalStore.getState();
    const initial = "a".repeat(TERMINAL_OUTPUT_BUFFER_MAX_CHARS - 2);

    store.appendOutput("term-1", initial);
    store.appendOutput("term-1", "bcdef");

    const nextStore = useTerminalStore.getState();
    const output = nextStore.getOutput("term-1");
    expect(output).toHaveLength(TERMINAL_OUTPUT_BUFFER_MAX_CHARS);
    expect(output.endsWith("bcdef")).toBe(true);
    expect(nextStore.getOutputBaseOffset("term-1")).toBe(3);
  });

  it("drops chunk head when single chunk exceeds capacity", () => {
    const store = useTerminalStore.getState();
    const chunk = `${"x".repeat(10)}${"y".repeat(TERMINAL_OUTPUT_BUFFER_MAX_CHARS)}`;

    store.appendOutput("term-1", chunk);

    const nextStore = useTerminalStore.getState();
    expect(nextStore.getOutput("term-1")).toBe("y".repeat(TERMINAL_OUTPUT_BUFFER_MAX_CHARS));
    expect(nextStore.getOutputBaseOffset("term-1")).toBe(10);
  });

  it("cleans output and base offset when terminal is removed", () => {
    const store = useTerminalStore.getState();
    store.registerTerminal({
      id: "term-1",
      capability: "interactive",
      cwd: ".",
      state: "running",
      createdAt: 1,
    });
    store.appendOutput("term-1", "hello");

    useTerminalStore.getState().removeTerminal("term-1");

    const nextStore = useTerminalStore.getState();
    expect(nextStore.getOutput("term-1")).toBe("");
    expect(nextStore.getOutputBaseOffset("term-1")).toBe(0);
    expect(nextStore.terminals.get("term-1")).toBeUndefined();
  });

  it("replaces read-only replay output and bumps revision", () => {
    const store = useTerminalStore.getState();

    store.replaceOutput("term-1", "first");
    const afterFirst = useTerminalStore.getState();
    expect(afterFirst.getOutput("term-1")).toBe("first");
    expect(afterFirst.getOutputRevision("term-1")).toBe(1);

    afterFirst.replaceOutput("term-1", "second");
    const afterSecond = useTerminalStore.getState();
    expect(afterSecond.getOutput("term-1")).toBe("second");
    expect(afterSecond.getOutputRevision("term-1")).toBe(2);
  });

  it("creates state-only projection for unknown terminal state event", () => {
    const store = useTerminalStore.getState();

    store.updateTerminalState("term-1", "exited", 0);

    const terminal = useTerminalStore.getState().terminals.get("term-1");
    expect(terminal?.id).toBe("term-1");
    expect(terminal?.capability).toBe("state_only");
    expect(terminal?.state).toBe("exited");
    expect(terminal?.exitCode).toBe(0);
  });

  it("idempotently projects terminal output by event_seq", () => {
    const store = useTerminalStore.getState();

    expect(store.projectOutputEvent(10, "term-1", "hello")).toBe(true);
    expect(useTerminalStore.getState().projectOutputEvent(10, "term-1", "hello")).toBe(false);

    expect(useTerminalStore.getState().getOutput("term-1")).toBe("hello");
  });

  it("idempotently projects terminal state by event_seq", () => {
    const store = useTerminalStore.getState();

    expect(store.projectStateEvent(10, "term-1", "running")).toBe(true);
    expect(useTerminalStore.getState().projectStateEvent(10, "term-1", "exited", 0)).toBe(false);

    const terminal = useTerminalStore.getState().terminals.get("term-1");
    expect(terminal?.id).toBe("term-1");
    expect(terminal?.state).toBe("running");
    expect(terminal?.exitCode).toBeUndefined();
  });
});
