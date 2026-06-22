import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { CommandExecutionCardBody } from "./CommandExecutionCardBody";

describe("CommandExecutionCardBody", () => {
  it("renders bounded output notice while keeping status and exit code visible", () => {
    const html = renderToStaticMarkup(
      <CommandExecutionCardBody
        item={commandItem(
          [
            "command: node big-output.js",
            "cwd: /tmp",
            "state: exited",
            "exit_code: 0",
            "output_truncated: true (omitted_bytes=4096)",
            "bounded preview",
          ].join("\n"),
        )}
      />,
    );

    expect(html).toContain("输出已裁切");
    expect(html).toContain("4.0 KiB");
    expect(html).toContain("status: completed");
    expect(html).toContain("exit: 0");
    expect(html).toContain("bounded preview");
  });
});

function commandItem(aggregatedOutput: string): Extract<ThreadItem, { type: "commandExecution" }> {
  return {
    type: "commandExecution",
    id: "cmd-1",
    command: "node big-output.js",
    cwd: "/tmp",
    processId: null,
    source: "agent",
    status: "completed",
    commandActions: [],
    aggregatedOutput,
    exitCode: 0,
    durationMs: 10,
  };
}
