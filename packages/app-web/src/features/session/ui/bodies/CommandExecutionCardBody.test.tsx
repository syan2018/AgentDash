import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { SessionWorkspacePanelActionProvider } from "../SessionWorkspacePanelActionProvider";
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
            "lifecycle_path: lifecycle://session/tool-results/turn_001/cmd_001/result.txt",
            "bounded preview",
          ].join("\n"),
        )}
      />,
    );

    expect(html).toContain("输出已裁切");
    expect(html).toContain("4.0 KiB");
    expect(html).toContain("status: completed");
    expect(html).toContain("exit: 0");
    expect(html).toContain("lifecycle://session/tool-results/turn_001/cmd_001/result.txt");
    expect(html).toContain("bounded preview");
  });

  it("renders output replay action without pretending to be an interactive terminal", () => {
    const html = renderToStaticMarkup(
      <SessionWorkspacePanelActionProvider openWorkspacePanel={() => {}}>
        <CommandExecutionCardBody
          item={commandItem("hello")}
          sessionId="session-1"
        />
      </SessionWorkspacePanelActionProvider>,
    );

    expect(html).toContain("查看输出");
    expect(html).not.toContain("在终端中查看");
    expect(html).not.toContain("disabled");
  });

  it("disables output replay action when no page-level workspace panel action exists", () => {
    const html = renderToStaticMarkup(
      <CommandExecutionCardBody
        item={commandItem("hello")}
        sessionId="session-1"
      />,
    );

    expect(html).toContain("查看输出");
    expect(html).toContain("disabled");
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
