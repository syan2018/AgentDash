/* eslint-disable react-refresh/only-export-components */
import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";

import type { TabTypeDescriptor } from "../tab-type-registry";
import { TerminalIcon } from "./icons";
import { useTerminalStore } from "../../session/model/useTerminalStore";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";

const API_BASE = "/api";

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

/**
 * 交互式终端视图。
 *
 * 输出写入模型：useTerminalStore.outputBuffers 是唯一 source of truth，
 * 所有写入 xterm 的数据都通过 useEffect 增量追加（lastWrittenLen 对齐），
 * 不存在第二条写入路径，从而避免重复绘制。
 */
function TerminalView({ terminalId: initialTerminalId, sessionId, tabId }: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  // state 驱动 store 订阅；ref 供 onData/onResize 等事件闭包同步读取
  const [activeId, setActiveId] = useState(initialTerminalId);
  const realIdRef = useRef(initialTerminalId);
  const [status, setStatus] = useState<"connecting" | "running" | "exited" | "error">("connecting");
  const lastWrittenLenRef = useRef(0);

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
      if (!id || id === "new") return;
      void fetch(`${API_BASE}/terminals/${id}/input`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ data }),
      }).catch(() => {});
    });

    term.onResize(({ cols, rows }) => {
      const id = realIdRef.current;
      if (!id || id === "new") return;
      void fetch(`${API_BASE}/terminals/${id}/resize`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ cols, rows }),
      }).catch(() => {});
    });

    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
    });
    resizeObserver.observe(containerRef.current);

    if (initialTerminalId === "new" && sessionId) {
      void spawnTerminal(sessionId, term, fitAddon, setStatus, tabId, realIdRef, setActiveId);
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

  useEffect(() => {
    const term = xtermRef.current;
    if (!term) return;
    // output 为空意味着还没有数据（新终端刚 spawn、或尚未收到 SSE）
    if (!output) return;
    const pending = output.slice(lastWrittenLenRef.current);
    if (pending.length > 0) {
      term.write(pending);
      lastWrittenLenRef.current = output.length;
    }
  }, [output]);

  // ---------- 终端状态同步 ----------
  const terminalState = useTerminalStore((s) => {
    if (activeId === "new") return null;
    for (const sessionMap of s.terminals.values()) {
      const t = sessionMap.get(activeId);
      if (t) return t;
    }
    return null;
  });

  useEffect(() => {
    if (terminalState?.state === "exited" || terminalState?.state === "killed") {
      setStatus("exited");
    } else if (terminalState?.state === "running") {
      setStatus("running");
    }
  }, [terminalState?.state]);

  // ---------- 外部 prop 同步（promote / tab layout 恢复） ----------
  useEffect(() => {
    if (initialTerminalId !== "new" && initialTerminalId !== realIdRef.current) {
      realIdRef.current = initialTerminalId;
      lastWrittenLenRef.current = 0;
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
          {status === "connecting"
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

async function spawnTerminal(
  sessionId: string,
  term: Terminal,
  fitAddon: FitAddon,
  setStatus: (s: "connecting" | "running" | "exited" | "error") => void,
  tabId?: string,
  realIdRef?: React.MutableRefObject<string>,
  setActiveId?: (id: string) => void,
) {
  try {
    const dims = fitAddon.proposeDimensions();
    const resp = await fetch(`${API_BASE}/sessions/${sessionId}/terminals`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        cols: dims?.cols ?? 80,
        rows: dims?.rows ?? 24,
      }),
    });
    if (!resp.ok) {
      const err = await resp.json();
      term.write(`\r\n\x1b[31mFailed to spawn terminal: ${err.error}\x1b[0m\r\n`);
      setStatus("error");
      return;
    }
    const data = (await resp.json()) as { terminalId: string; processId?: number };
    const realId = data.terminalId;

    if (realIdRef) realIdRef.current = realId;

    useTerminalStore.getState().registerTerminal({
      id: realId,
      sessionId,
      cwd: ".",
      state: "running",
      processId: data.processId,
      createdAt: Date.now(),
    });

    if (tabId) {
      useWorkspaceTabStore.getState().updateTabUri(tabId, `terminal://${realId}`);
    }

    // setActiveId 放最后：确保 store 已注册终端，useEffect[output] 切换订阅后能立即读到数据
    if (setActiveId) setActiveId(realId);

    setStatus("running");
  } catch (e) {
    term.write(`\r\n\x1b[31mNetwork error: ${e}\x1b[0m\r\n`);
    setStatus("error");
  }
}

export const terminalTabType: TabTypeDescriptor = {
  typeId: "terminal",
  label: "终端",
  icon: TerminalIcon,
  allowMultiple: true,
  pinned: false,

  renderContent: (props) => {
    const parsed = terminalTabType.parseUri?.(props.uri);
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

  resolveTitle: (uri) => {
    const id = uri.replace("terminal://", "");
    return id && id !== "new" ? `终端: ${id.slice(0, 8)}` : "新终端";
  },

  parseUri: (uri) => {
    const terminalId = uri.replace("terminal://", "");
    return terminalId ? { terminalId } : null;
  },

  buildUri: ({ terminalId }) => `terminal://${terminalId ?? "new"}`,
  defaultUri: "terminal://new",
  menuOrder: 30,
};
