/**
 * 命令执行 body — 终端输出区域 + 元信息 footer
 *
 * 从原 CommandExecutionCard 提取的 body 部分，header 已移交 ToolCallCardShell。
 */

import { memo, useEffect, useRef, useState, useCallback } from "react";
import type { AgentDashThreadItem, ThreadItem } from "../../../../generated/backbone-protocol";
import { useWorkspaceTabStore } from "../../../../stores/workspaceTabStore";
import { useTerminalStore } from "../../model/useTerminalStore";
import { CB } from "./cardBodyTokens";

type CommandItem =
  | Extract<ThreadItem, { type: "commandExecution" }>
  | Extract<AgentDashThreadItem, { type: "shellExec" }>;

export interface CommandExecutionCardBodyProps {
  item: CommandItem;
  outputText?: string;
  sessionId?: string;
}

export const CommandExecutionCardBody = memo(function CommandExecutionCardBody({
  item,
  outputText,
  sessionId,
}: CommandExecutionCardBodyProps) {
  const outputRef = useRef<HTMLPreElement>(null);
  const [collapsed, setCollapsed] = useState(false);
  const status = item.status;
  const isRunning = status === "inProgress";
  const renderedOutput = outputText ?? ("aggregatedOutput" in item ? item.aggregatedOutput ?? undefined : undefined);

  const handlePromote = useCallback(() => {
    const promoteId = `promote-${item.id}`;
    const store = useTerminalStore.getState();
    if (!store.getOutput(promoteId) && renderedOutput) {
      store.appendOutput(promoteId, renderedOutput);
    }
    if (sessionId) {
      store.registerTerminal({
        id: promoteId,
        sessionId,
        cwd: item.cwd ?? "platform://",
        state: isRunning ? "running" : "exited",
        exitCode: item.exitCode ?? undefined,
        createdAt: Date.now(),
      });
    }
    useWorkspaceTabStore
      .getState()
      .openOrActivate("terminal", `terminal://${promoteId}`);
  }, [item.id, item.cwd, item.exitCode, renderedOutput, sessionId, isRunning]);

  useEffect(() => {
    if (outputRef.current && isRunning) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [renderedOutput, isRunning]);

  const lineCount = renderedOutput ? renderedOutput.split("\n").length - 1 : 0;
  const shouldCollapse = lineCount > 80;
  const maxH = collapsed ? "max-h-16" : shouldCollapse ? "max-h-96" : "max-h-64";

  return (
    <div className={CB.sectionGap}>
      {item.cwd && (
        <div className={CB.meta}>
          cwd: <span className="font-mono">{item.cwd}</span>
        </div>
      )}

      {(renderedOutput || isRunning) && (
        <div className="relative">
          <pre
            ref={outputRef}
            className={`overflow-auto ${CB.codeBlock} transition-[max-height] ${maxH}`}
          >
            {renderedOutput || (
              <span className="animate-pulse text-muted-foreground/40">
                等待输出...
              </span>
            )}
            {isRunning && (
              <span className="inline-block h-3.5 w-1.5 animate-pulse bg-muted-foreground/40" />
            )}
          </pre>

          {shouldCollapse && (
            <button
              type="button"
              onClick={() => setCollapsed(!collapsed)}
              className={`absolute bottom-1 right-2 ${CB.actionButton} bg-background/80 shadow-sm`}
            >
              {collapsed ? "展开" : "折叠"}
            </button>
          )}
        </div>
      )}

      <div className={`mt-1.5 flex items-center gap-2 ${CB.meta}`}>
        {item.exitCode !== undefined && item.exitCode !== null && (
          <span className={item.exitCode === 0 ? CB.statusSuccess : CB.statusFailed}>
            exit: {item.exitCode}
          </span>
        )}
        {lineCount > 0 && (
          <span>{lineCount} 行</span>
        )}
        <button
          type="button"
          onClick={handlePromote}
          className={`ml-auto ${CB.actionButton}`}
        >
          在终端中查看
        </button>
      </div>
    </div>
  );
});
