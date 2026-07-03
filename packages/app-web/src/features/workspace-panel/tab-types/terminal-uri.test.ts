import { describe, expect, it } from "vitest";
import {
  buildCommandOutputReplayTerminalId,
  buildCommandOutputReplayTerminalUri,
  buildInteractiveTerminalUri,
  parseTerminalUri,
} from "./terminal-uri";

describe("terminal-uri", () => {
  it("round-trips interactive terminal uris", () => {
    expect(buildInteractiveTerminalUri("term-1")).toBe("terminal://term-1");
    expect(parseTerminalUri("terminal://term-1")).toEqual({
      terminalId: "term-1",
      mode: "terminal",
    });
  });

  it("round-trips read-only command output replay uris", () => {
    const itemId = "turn_1:cmd/1";

    expect(buildCommandOutputReplayTerminalUri(itemId)).toBe("terminal://output/turn_1%3Acmd%2F1");
    expect(buildCommandOutputReplayTerminalId(itemId)).toBe("command-output:turn_1:cmd/1");
    expect(parseTerminalUri("terminal://output/turn_1%3Acmd%2F1")).toEqual({
      terminalId: "command-output:turn_1:cmd/1",
      mode: "output",
      itemId,
    });
  });

  it("rejects malformed replay uris instead of throwing", () => {
    expect(parseTerminalUri("terminal://output/%E0%A4%A")).toBeNull();
  });
});
