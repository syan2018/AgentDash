/* eslint-disable react-refresh/only-export-components */
import { useEffect, useRef, useCallback, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";

import type { TabTypeDescriptor } from "../tab-type-registry";
import { TerminalIcon } from "./icons";
import { useTerminalStore } from "../../session/model/useTerminalStore";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";

const API_BASE = "/api";

interface TerminalViewProps {
  terminalId: string;
  sessionId?: string;
  tabId?: string;
}

function TerminalView({ terminalId, sessionId, tabId }: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const [status, setStatus] = useState<"connecting" | "running" | "exited" | "error">("connecting");
  const sendInput = useCallback(
    async (data: string) => {
      try {
        await fetch(`${API_BASE}/terminals/${terminalId}/input`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ data }),
        });
      } catch {
        /* network error — terminal probably dead */
      }
    },
    [terminalId],
  );

  const sendResize = useCallback(
    async (cols: number, rows: number) => {
      try {
        await fetch(`${API_BASE}/terminals/${terminalId}/resize`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ cols, rows }),
        });
      } catch {
        /* ignore */
      }
    },
    [terminalId],
  );

  useEffect(() => {
    if (!containerRef.current) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: "'Cascadia Code', 'JetBrains Mono', 'Fira Code', monospace",
      theme: {
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
      },
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
      void sendInput(data);
    });

    term.onResize(({ cols, rows }) => {
      void sendResize(cols, rows);
    });

    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
    });
    resizeObserver.observe(containerRef.current);

    if (terminalId === "new" && sessionId) {
      void spawnTerminal(sessionId, term, fitAddon, setStatus, tabId);
    } else if (terminalId !== "new") {
      setStatus("running");
    }

    return () => {
      resizeObserver.disconnect();
      term.dispose();
      xtermRef.current = null;
      fitAddonRef.current = null;
    };
  }, [terminalId, sessionId, sendInput, sendResize]);

  // SSE 输出订阅通过 useTerminalStore + useSessionStream 已有的 platform event 管道
  const terminalState = useTerminalStore((s) => {
    const allTerminals = s.terminals;
    for (const sessionMap of allTerminals.values()) {
      const t = sessionMap.get(terminalId);
      if (t) return t;
    }
    return null;
  });

  // 当 output buffer 更新时写入 xterm
  const output = useTerminalStore((s) => s.getOutput(terminalId));
  const lastWrittenLenRef = useRef(0);

  useEffect(() => {
    if (!xtermRef.current || !output) return;
    const newData = output.slice(lastWrittenLenRef.current);
    if (newData) {
      xtermRef.current.write(newData);
      lastWrittenLenRef.current = output.length;
    }
  }, [output]);

  useEffect(() => {
    if (terminalState?.state === "exited" || terminalState?.state === "killed") {
      setStatus("exited");
    } else if (terminalState?.state === "running") {
      setStatus("running");
    }
  }, [terminalState?.state]);

  return (
    <div className="flex h-full flex-col bg-white">
      {/* Status bar */}
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
        <span className="ml-auto font-mono text-muted-foreground/50">{terminalId.slice(0, 12)}</span>
      </div>

      {/* Terminal container */}
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

    // 注册到 store，以便 SSE platform event 能路由输出到这个终端
    useTerminalStore.getState().registerTerminal({
      id: realId,
      sessionId,
      cwd: ".",
      state: "running",
      processId: data.processId,
      createdAt: Date.now(),
    });

    // 更新 Tab URI 为真正的 terminalId，使 input/resize/output 全部路由正确
    if (tabId) {
      useWorkspaceTabStore.getState().updateTabUri(tabId, `terminal://${realId}`);
    }

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
