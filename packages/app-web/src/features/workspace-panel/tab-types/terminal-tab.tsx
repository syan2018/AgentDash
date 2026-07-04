/* eslint-disable react-refresh/only-export-components */
import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";

import { authenticatedFetch } from "../../../api/client";
import { asRecord, requireStringField } from "../../../api/mappers";
import { buildApiPath } from "../../../api/origin";
import {
  agentRunScopedPath,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";
import type { TerminalCapability, TerminalSpawnResult } from "../../../types/terminal";
import type { TabTypeDescriptor } from "../tab-type-registry";
import { useWorkspaceData } from "../workspace-data-context";
import { TerminalIcon } from "./icons";
import {
  buildInteractiveTerminalUri,
  parseTerminalUri,
} from "./terminal-uri";
import { useTerminalStore } from "../../session/model/useTerminalStore";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";

const XTERM_THEME = {
  background: "#ffffff",
  foreground: "#1e1e2e",
  cursor: "#6e6e7e",
  cursorAccent: "#ffffff",
  selectionBackground: "#d0d5dd",
  selectionForeground: "#1e1e2e",
  black: "#3c3c43",
  red: "#d1242f",
  green: "#1a7f37",
  yellow: "#9a6700",
  blue: "#0969da",
  magenta: "#8250df",
  cyan: "#0598bc",
  white: "#8b949e",
  brightBlack: "#57606a",
  brightRed: "#cf222e",
  brightGreen: "#116329",
  brightYellow: "#7d4e00",
  brightBlue: "#0550ae",
  brightMagenta: "#6639ba",
  brightCyan: "#0079b4",
  brightWhite: "#6e7781",
} as const;

interface TerminalViewProps {
  terminalId: string;
  sessionId?: string;
  tabId?: string;
}

function resolveTerminalTitle(uri: string): string {
  const parsed = parseTerminalUri(uri);
  if (parsed?.mode === "output") {
    return `输出: ${parsed.itemId.slice(0, 8)}`;
  }
  const id = parsed?.terminalId ?? "";
  return id && id !== "new" ? `终端: ${id.slice(0, 8)}` : "新终端";
}

/**
 * 交互式终端视图。
 *
 * 输出写入模型：useTerminalStore.outputBuffers 是唯一 source of truth，
 * 所有写入 xterm 的数据都通过 useEffect 增量追加（lastWrittenLen 对齐），
 * 不存在第二条写入路径，从而避免重复绘制。
 */
function TerminalView({ terminalId: initialTerminalId, tabId }: TerminalViewProps) {
  const { agentRunRuntimeTarget } = useWorkspaceData();
  const containerRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  // state 驱动 store 订阅；ref 供 onData/onResize 等事件闭包同步读取
  const [activeId, setActiveId] = useState(initialTerminalId);
  const realIdRef = useRef(initialTerminalId);
  const isInteractiveRef = useRef(false);
  const [status, setStatus] = useState<"connecting" | "running" | "exited" | "error">("connecting");
  const lastWrittenOffsetRef = useRef(0);
  const lastOutputRevisionRef = useRef(0);

  // ---------- xterm 实例生命周期（仅挂载/卸载） ----------
  useEffect(() => {
    if (!containerRef.current) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: "'Cascadia Code', 'JetBrains Mono', 'Fira Code', monospace",
      theme: XTERM_THEME,
      allowProposedApi: true,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(new WebLinksAddon());

    term.open(containerRef.current);
    fitAddon.fit();

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    term.onData((data) => {
      const id = realIdRef.current;
      if (!isInteractiveRef.current || !id || id === "new") return;
      void authenticatedFetch(buildApiPath(`/terminals/${encodeURIComponent(id)}/input`), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ data }),
      }).catch(() => {});
    });

    term.onResize(({ cols, rows }) => {
      const id = realIdRef.current;
      if (!isInteractiveRef.current || !id || id === "new") return;
      void authenticatedFetch(buildApiPath(`/terminals/${encodeURIComponent(id)}/resize`), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ cols, rows }),
      }).catch(() => {});
    });

    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
    });
    resizeObserver.observe(containerRef.current);

    if (initialTerminalId === "new") {
      if (!agentRunRuntimeTarget) {
        term.write("\r\n\x1b[31mAgentRun runtime target 不可用，无法创建终端。\x1b[0m\r\n");
        setStatus("error");
      } else {
        void spawnTerminal(
          agentRunRuntimeTarget,
          term,
          fitAddon,
          setStatus,
          tabId,
          realIdRef,
          setActiveId,
        );
      }
    } else if (initialTerminalId !== "new") {
      // 已有终端：回放通过 useEffect[output] 自动触发，此处只需设状态
      setStatus("running");
    }

    return () => {
      resizeObserver.disconnect();
      term.dispose();
      xtermRef.current = null;
      fitAddonRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ---------- 唯一的 xterm 写入路径：store outputBuffer → 增量 write ----------
  const output = useTerminalStore((s) => s.getOutput(activeId));
  const outputBaseOffset = useTerminalStore((s) => s.getOutputBaseOffset(activeId));
  const outputRevision = useTerminalStore((s) => s.getOutputRevision(activeId));

  useEffect(() => {
    const term = xtermRef.current;
    if (!term) return;
    if (lastOutputRevisionRef.current !== outputRevision) {
      term.clear();
      lastWrittenOffsetRef.current = outputBaseOffset;
      lastOutputRevisionRef.current = outputRevision;
    }
    // output 为空意味着还没有数据（新终端刚 spawn、或尚未收到会话事件）
    if (!output) return;
    const retainedEndOffset = outputBaseOffset + output.length;
    const pendingStart = Math.max(0, lastWrittenOffsetRef.current - outputBaseOffset);
    const pending = output.slice(pendingStart);
    if (pending.length > 0) {
      term.write(pending);
      lastWrittenOffsetRef.current = retainedEndOffset;
    }
  }, [output, outputBaseOffset, outputRevision]);

  // ---------- 终端状态同步 ----------
  const terminalState = useTerminalStore((s) => {
    if (activeId === "new") return null;
    for (const sessionMap of s.terminals.values()) {
      const t = sessionMap.get(activeId);
      if (t) return t;
    }
    return null;
  });
  const terminalCapability: TerminalCapability = terminalState?.capability ?? (
    activeId === "new" ? "interactive" : "state_only"
  );
  const isInteractiveTerminal = terminalCapability === "interactive";

  useEffect(() => {
    isInteractiveRef.current = isInteractiveTerminal && activeId !== "new";
    if (xtermRef.current) {
      xtermRef.current.options.disableStdin = !isInteractiveTerminal;
    }
  }, [activeId, isInteractiveTerminal]);

  useEffect(() => {
    if (terminalCapability === "read_only_output") {
      setStatus(terminalState?.state === "running" ? "running" : "exited");
    } else if (
      terminalState?.state === "exited" ||
      terminalState?.state === "killed" ||
      terminalState?.state === "lost"
    ) {
      setStatus("exited");
    } else if (terminalState?.state === "running") {
      setStatus("running");
    }
  }, [terminalCapability, terminalState?.state]);

  // ---------- 外部 prop 同步（promote / tab layout 恢复） ----------
  useEffect(() => {
    if (initialTerminalId !== "new" && initialTerminalId !== realIdRef.current) {
      realIdRef.current = initialTerminalId;
      lastWrittenOffsetRef.current = 0;
      lastOutputRevisionRef.current = 0;
      if (xtermRef.current) xtermRef.current.clear();
      setActiveId(initialTerminalId);
      // 切换 activeId 后，useEffect[output] 会自动从 0 回放
    }
  }, [initialTerminalId]);

  return (
    <div className="flex h-full flex-col bg-white">
      <div className="flex items-center gap-2 border-b border-border px-3 py-1 text-xs text-muted-foreground">
        <span
          className={`inline-block h-1.5 w-1.5 rounded-full ${
            status === "running"
              ? "bg-emerald-500"
              : status === "exited"
                ? "bg-zinc-600"
                : status === "error"
                  ? "bg-red-500"
                  : "bg-amber-500 animate-pulse"
          }`}
        />
        <span>
          {terminalCapability === "read_only_output"
            ? terminalState?.state === "running" ? "只读输出同步中" : "只读输出"
            : terminalCapability === "state_only"
              ? stateOnlyLabel(terminalState?.state, terminalState?.exitCode)
              : status === "connecting"
                ? "连接中..."
                : status === "running"
                  ? "运行中"
                  : status === "exited"
                    ? `已退出${terminalState?.exitCode !== undefined ? ` (${terminalState.exitCode})` : ""}`
                    : "错误"}
        </span>
        <span className="ml-auto font-mono text-muted-foreground/50">
          {activeId !== "new" ? activeId.slice(0, 12) : ""}
        </span>
      </div>
      <div ref={containerRef} className="flex-1 overflow-hidden p-1" />
    </div>
  );
}

type AgentRunTerminalSpawnResult = TerminalSpawnResult & {
  runtime_session_id: string;
};

async function spawnTerminal(
  agentRunTarget: AgentRunRuntimeTarget,
  term: Terminal,
  fitAddon: FitAddon,
  setStatus: (s: "connecting" | "running" | "exited" | "error") => void,
  tabId?: string,
  realIdRef?: React.MutableRefObject<string>,
  setActiveId?: (id: string) => void,
) {
  try {
    const dims = fitAddon.proposeDimensions();
    const resp = await authenticatedFetch(
      buildApiPath(agentRunScopedPath(agentRunTarget, "/runtime/terminals")),
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          cols: dims?.cols ?? 80,
          rows: dims?.rows ?? 24,
        }),
      },
    );
    if (!resp.ok) {
      const message = await readErrorMessage(resp);
      term.write(`\r\n\x1b[31mFailed to spawn terminal: ${message}\x1b[0m\r\n`);
      setStatus("error");
      return;
    }
    const data = mapTerminalSpawnResult(await readJsonRecord(resp));
    const realId = data.terminal_id;

    if (realIdRef) realIdRef.current = realId;

    useTerminalStore.getState().registerTerminal({
      id: realId,
      sessionId: data.runtime_session_id,
      capability: "interactive",
      cwd: ".",
      state: "running",
      processId: data.process_id,
      createdAt: Date.now(),
    });

    if (tabId) {
      const uri = buildInteractiveTerminalUri(realId);
      useWorkspaceTabStore.getState().updateTabUri(tabId, uri, resolveTerminalTitle(uri));
    }

    // setActiveId 放最后：确保 store 已注册终端，useEffect[output] 切换订阅后能立即读到数据
    if (setActiveId) setActiveId(realId);

    setStatus("running");
  } catch (e) {
    term.write(`\r\n\x1b[31mNetwork error: ${errorMessage(e)}\x1b[0m\r\n`);
    setStatus("error");
  }
}

async function readJsonRecord(resp: Response): Promise<Record<string, unknown>> {
  const text = await resp.text();
  if (!text.trim()) {
    throw new Error(`HTTP ${resp.status} ${resp.statusText || "empty response body"}`);
  }

  let raw: unknown;
  try {
    raw = JSON.parse(text) as unknown;
  } catch {
    throw new Error(`HTTP ${resp.status} returned invalid JSON: ${text.slice(0, 200)}`);
  }

  const record = asRecord(raw);
  if (!record) {
    throw new Error(`HTTP ${resp.status} returned non-object JSON`);
  }
  return record;
}

async function readErrorMessage(resp: Response): Promise<string> {
  const text = await resp.text();
  if (!text.trim()) {
    return `HTTP ${resp.status} ${resp.statusText || "empty response body"}`;
  }

  try {
    const raw: unknown = JSON.parse(text) as unknown;
    const record = asRecord(raw);
    const error = record?.error;
    if (typeof error === "string" && error.trim()) {
      return error;
    }
  } catch {
    return text.slice(0, 200);
  }

  return text.slice(0, 200);
}

function mapTerminalSpawnResult(raw: Record<string, unknown>): AgentRunTerminalSpawnResult {
  const processId = raw.process_id;
  return {
    terminal_id: requireStringField(raw, "terminal_id"),
    runtime_session_id: requireStringField(raw, "runtime_session_id"),
    process_id: typeof processId === "number" && Number.isFinite(processId) ? processId : undefined,
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export const terminalTabType: TabTypeDescriptor = {
  typeId: "terminal",
  label: "终端",
  icon: TerminalIcon,
  allowMultiple: true,
  pinned: false,

  renderContent: (props) => {
    const parsed = parseTerminalUri(props.uri);
    const terminalId = parsed?.terminalId ?? "new";
    return (
      <TerminalView
        key={props.tabId}
        terminalId={terminalId}
        sessionId={props.sessionId ?? undefined}
        tabId={props.tabId}
      />
    );
  },

  resolveTitle: resolveTerminalTitle,

  parseUri: (uri): Record<string, string> | null => {
    const parsed = parseTerminalUri(uri);
    if (!parsed) return null;
    if (parsed.mode === "output") {
      return {
        terminalId: parsed.terminalId,
        mode: parsed.mode,
        itemId: parsed.itemId,
      };
    }
    return { terminalId: parsed.terminalId, mode: parsed.mode };
  },

  buildUri: ({ terminalId }) => buildInteractiveTerminalUri(terminalId ?? "new"),
  defaultUri: "terminal://new",
  menuOrder: 30,
};

function stateOnlyLabel(state: string | undefined, exitCode: number | undefined): string {
  if (state === "exited" || state === "killed" || state === "lost") {
    return `历史状态: ${state}${exitCode !== undefined ? ` (${exitCode})` : ""}`;
  }
  if (state === "running" || state === "starting") {
    return `历史状态: ${state}`;
  }
  return "历史状态";
}
