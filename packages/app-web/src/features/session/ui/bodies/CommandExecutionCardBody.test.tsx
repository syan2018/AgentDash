import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AgentDashThreadItem, ThreadItem } from "../../../../generated/backbone-protocol";
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

  it("renders durable shell output after the metadata separator", () => {
    const html = renderToStaticMarkup(
      <CommandExecutionCardBody
        item={shellItem([
          "operation: start",
          "command: echo hello",
          "cwd: main://",
          "state: completed",
          "exit_code: 0",
          "terminal_id: term-1",
          "next_seq: 1",
          "",
          "hello",
        ].join("\n"))}
      />,
    );

    expect(html).toContain("hello");
    expect(html).not.toContain("terminal_id: term-1");
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

function shellItem(
  aggregatedOutput: string,
): Extract<AgentDashThreadItem, { type: "shellExec" }> {
  return {
    type: "shellExec",
    id: "shell-1",
    command: "echo hello",
    cwd: "main://",
    executionMode: "mountExec",
    arguments: { command: "echo hello" },
    status: "completed",
    aggregatedOutput,
    exitCode: 0,
    success: true,
  };
}
