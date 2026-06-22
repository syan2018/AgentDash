import { describe, expect, it } from "vitest";
import { formatBytes, parseBoundedOutputText, parseTruncationDetails } from "./boundedOutput";

describe("boundedOutput", () => {
  it("parses AgentToolResult bounded preview markers", () => {
    const info = parseBoundedOutputText(
      [
        "[tool result truncated]",
        "lifecycle_path: lifecycle://session/tool-results/item-1/result.txt",
        "policy: head_tail",
        "",
        "head",
        "[... omitted 2048 bytes ...]",
        "tail",
      ].join("\n"),
    );

    expect(info).toMatchObject({
      truncated: true,
      source: "tool_result",
      lifecyclePath: "lifecycle://session/tool-results/item-1/result.txt",
      policy: "head_tail",
      omittedBytes: 2048,
    });
  });

  it("parses shell and terminal truncation markers", () => {
    expect(parseBoundedOutputText("output_truncated: true (omitted_bytes=4096)")).toMatchObject({
      source: "shell_output",
      omittedBytes: 4096,
    });
    expect(parseBoundedOutputText("[terminal output truncated: omitted_bytes=8192]")).toMatchObject({
      source: "terminal_output",
      omittedBytes: 8192,
    });
  });

  it("parses existing details.truncation shape", () => {
    const info = parseTruncationDetails({
      details: {
        lifecycle_path: "lifecycle://session/tool-results/item-1/result.txt",
        truncation: {
          truncated: true,
          original_bytes: 1000,
          inline_bytes: 200,
          omitted_bytes: 800,
          policy: "head_tail",
        },
      },
    });

    expect(info).toMatchObject({
      lifecyclePath: "lifecycle://session/tool-results/item-1/result.txt",
      originalBytes: 1000,
      inlineBytes: 200,
      omittedBytes: 800,
      policy: "head_tail",
    });
  });

  it("formats byte counts compactly", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KiB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MiB");
  });
});
