/**
 * 命令执行 body — 终端输出区域 + 元信息 footer
 *
 * 从原 CommandExecutionCard 提取的 body 部分，header 已移交 ToolCallCardShell。
 */

import { memo, useEffect, useRef, useState, useCallback } from "react";
import type { AgentDashThreadItem, ThreadItem } from "../../../../generated/backbone-protocol";
import { useTerminalStore } from "../../model/useTerminalStore";
import { formatBytes, parseBoundedOutputText, type BoundedOutputInfo } from "../../model/boundedOutput";
import {
  buildCommandOutputReplayTerminalId,
  buildCommandOutputReplayTerminalUri,
} from "../../../workspace-panel/tab-types/terminal-uri";
import { useSessionWorkspacePanelAction } from "../SessionWorkspacePanelActionContext";
import { CB } from "./cardBodyTokens";
import { extractTerminalIdFromItem, parseTerminalItemMeta } from "../../model/terminalItemMeta";

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
  const replayCreatedAtRef = useRef<number | null>(null);
  const [collapsed, setCollapsed] = useState(false);
  const openWorkspacePanel = useSessionWorkspacePanelAction();
  const status = item.status;
  const isRunning = status === "inProgress";

  const rawAggregated = outputText ?? ("aggregatedOutput" in item ? item.aggregatedOutput ?? undefined : undefined);

  // 新协议：从 aggregatedOutput 元数据中提取真实 terminal_id
  const realTerminalId = extractTerminalIdFromItem(item as { aggregatedOutput?: string | null; processId?: string | null });
  const terminalMeta = parseTerminalItemMeta(rawAggregated);
  const hasNewProtocol = realTerminalId != null;

  // 从 terminal store 订阅真实终端输出（live session 或 durable 事件回放时有效）
  const storeOutput = useTerminalStore((s) =>
    realTerminalId ? s.getOutput(realTerminalId) : "",
  );

  // 决定渲染内容：
  // 新协议：store 实时输出 → read 操作内联输出 → 无
  // 旧协议：直接用 rawAggregated
  const renderedOutput = hasNewProtocol
    ? (storeOutput || terminalMeta.outputContent || undefined)
    : rawAggregated;

  const boundedOutput = parseBoundedOutputText(renderedOutput);

  // --- replay terminal：始终注册，供 "查看输出" 使用 ---
  const replayTerminalId = buildCommandOutputReplayTerminalId(item.id);
  const replayTerminalUri = buildCommandOutputReplayTerminalUri(item.id);
  const canOpenOutputReplay = Boolean(sessionId && openWorkspacePanel);

  useEffect(() => {
    if (!sessionId) return;
    if (replayCreatedAtRef.current == null) {
      replayCreatedAtRef.current = Date.now();
    }
    const store = useTerminalStore.getState();
    store.registerTerminal({
      id: replayTerminalId,
      sessionId,
      capability: "read_only_output",
      cwd: item.cwd ?? "",
      state: isRunning ? "running" : "exited",
      exitCode: item.exitCode ?? undefined,
      linkedItemId: item.id,
      createdAt: replayCreatedAtRef.current,
      exitedAt: isRunning ? undefined : Date.now(),
    });

    // 写入 renderedOutput（真实终端输出）而非 rawAggregated（协议元数据）
    const contentToWrite = renderedOutput;
    if (contentToWrite == null) return;
    const currentOutput = store.getOutput(replayTerminalId);
    if (contentToWrite === currentOutput) return;
    if (contentToWrite.startsWith(currentOutput)) {
      store.appendOutput(replayTerminalId, contentToWrite.slice(currentOutput.length));
      return;
    }
    store.replaceOutput(replayTerminalId, contentToWrite);
  }, [
    item.cwd,
    item.exitCode,
    item.id,
    isRunning,
    renderedOutput,
    replayTerminalId,
    sessionId,
  ]);

  const handlePromote = useCallback(() => {
    if (!openWorkspacePanel || !sessionId) return;
    openWorkspacePanel({
      typeId: "terminal",
      uri: replayTerminalUri,
      options: { refreshContent: true },
    });
  }, [openWorkspacePanel, sessionId, replayTerminalUri]);

  useEffect(() => {
    if (outputRef.current && isRunning) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [renderedOutput, isRunning]);

  const lineCount = renderedOutput ? renderedOutput.split("\n").length : 0;
  const shouldCollapse = lineCount > 80;
  const maxH = collapsed ? "max-h-16" : shouldCollapse ? "max-h-96" : "max-h-64";

  return (
    <div className={CB.sectionGap}>
      {boundedOutput && (
        <BoundedOutputNotice info={boundedOutput} />
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
        <span className={statusClassName(status)}>
          status: {status}
        </span>
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
          disabled={!canOpenOutputReplay}
          title={canOpenOutputReplay ? "在终端面板中查看完整输出" : "当前页面没有工作区面板"}
          className={`ml-auto ${CB.actionButton}`}
        >
          查看输出
        </button>
      </div>
    </div>
  );
});

function BoundedOutputNotice({ info }: { info: BoundedOutputInfo }) {
  const parts = ["输出已裁切"];
  if (info.omittedBytes != null) {
    parts.push(`省略 ${formatBytes(info.omittedBytes)}`);
  }
  if (info.policy) {
    parts.push(`policy: ${info.policy}`);
  }

  return (
    <div className={`rounded-[6px] border border-warning/25 bg-warning/5 px-2 py-1.5 ${CB.meta}`}>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className={CB.statusWarning}>{parts.join(" · ")}</span>
        {info.lifecyclePath && (
          <code className="max-w-full truncate text-[10px] text-muted-foreground/60">
            {info.lifecyclePath}
          </code>
        )}
      </div>
    </div>
  );
}

function statusClassName(status: CommandItem["status"]): string {
  if (status === "completed") return CB.statusSuccess;
  if (status === "failed") return CB.statusFailed;
  if (status === "inProgress") return CB.statusWarning;
  return CB.statusNeutral;
}
