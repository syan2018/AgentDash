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
  });
}

describe("useTerminalStore", () => {
  beforeEach(() => {
    resetTerminalStore();
  });

  it("保留未超过容量的终端输出", () => {
    const store = useTerminalStore.getState();

    store.appendOutput("term-1", "hello");
    store.appendOutput("term-1", " world");

    expect(useTerminalStore.getState().getOutput("term-1")).toBe("hello world");
    expect(useTerminalStore.getState().getOutputBaseOffset("term-1")).toBe(0);
  });

  it("超过容量后只保留最新输出并记录裁掉的前缀长度", () => {
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

  it("单次大 chunk 超过容量时丢弃 chunk 头部", () => {
    const store = useTerminalStore.getState();
    const chunk = `${"x".repeat(10)}${"y".repeat(TERMINAL_OUTPUT_BUFFER_MAX_CHARS)}`;

    store.appendOutput("term-1", chunk);

    const nextStore = useTerminalStore.getState();
    expect(nextStore.getOutput("term-1")).toBe("y".repeat(TERMINAL_OUTPUT_BUFFER_MAX_CHARS));
    expect(nextStore.getOutputBaseOffset("term-1")).toBe(10);
  });

  it("删除终端时同步清理输出和 base offset", () => {
    const store = useTerminalStore.getState();
    store.registerTerminal({
      id: "term-1",
      sessionId: "session-1",
      cwd: ".",
      state: "running",
      createdAt: 1,
    });
    store.appendOutput("term-1", "hello");

    useTerminalStore.getState().removeTerminal("term-1");

    const nextStore = useTerminalStore.getState();
    expect(nextStore.getOutput("term-1")).toBe("");
    expect(nextStore.getOutputBaseOffset("term-1")).toBe(0);
    expect(nextStore.getTerminalsForSession("session-1")).toEqual([]);
  });
});
