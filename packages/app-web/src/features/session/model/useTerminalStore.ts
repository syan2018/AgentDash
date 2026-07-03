import { create } from "zustand";
import type { TerminalInfo, TerminalProcessState } from "../../../types/terminal";

export const TERMINAL_OUTPUT_BUFFER_MAX_CHARS = 256 * 1024;

function isTerminalClosed(state: TerminalProcessState): boolean {
  return state === "exited" || state === "killed" || state === "lost";
}

function projectionKey(sessionId: string, eventSeq: number): string {
  return `${sessionId}:${eventSeq}`;
}

function retainOutput(data: string): { output: string; baseOffset: number } {
  if (data.length <= TERMINAL_OUTPUT_BUFFER_MAX_CHARS) {
    return { output: data, baseOffset: 0 };
  }
  const dropped = data.length - TERMINAL_OUTPUT_BUFFER_MAX_CHARS;
  return {
    output: data.slice(dropped),
    baseOffset: dropped,
  };
}

interface TerminalStoreState {
  /** session_id → { terminal_id → TerminalInfo } */
  terminals: Map<string, Map<string, TerminalInfo>>;
  /** terminal_id → 有界最近输出 buffer */
  outputBuffers: Map<string, string>;
  /** terminal_id → 当前 buffer 前方已裁掉的字符数 */
  outputBufferBaseOffsets: Map<string, number>;
  /** terminal_id → replaceOutput revision；xterm replay 用于识别重写型输出 */
  outputBufferRevisions: Map<string, number>;
  /** 已投影到 terminal store 的 durable session event key */
  projectedEventKeys: Set<string>;

  registerTerminal: (info: TerminalInfo) => void;
  updateTerminalState: (
    terminalId: string,
    state: TerminalProcessState,
    exitCode?: number,
    sessionId?: string,
  ) => void;
  appendOutput: (terminalId: string, data: string) => void;
  replaceOutput: (terminalId: string, data: string) => void;
  projectOutputEvent: (
    sessionId: string,
    eventSeq: number,
    terminalId: string,
    data: string,
  ) => boolean;
  projectStateEvent: (
    sessionId: string,
    eventSeq: number,
    terminalId: string,
    state: TerminalProcessState,
    exitCode?: number,
  ) => boolean;
  removeTerminal: (terminalId: string) => void;
  getTerminalsForSession: (sessionId: string) => TerminalInfo[];
  getOutput: (terminalId: string) => string;
  getOutputBaseOffset: (terminalId: string) => number;
  getOutputRevision: (terminalId: string) => number;
}

export const useTerminalStore = create<TerminalStoreState>((set, get) => ({
  terminals: new Map(),
  outputBuffers: new Map(),
  outputBufferBaseOffsets: new Map(),
  outputBufferRevisions: new Map(),
  projectedEventKeys: new Set(),

  registerTerminal: (info) =>
    set((state) => {
      const newTerminals = new Map(state.terminals);
      const sessionMap = new Map(newTerminals.get(info.sessionId) ?? []);
      const existing = sessionMap.get(info.id);
      sessionMap.set(info.id, {
        ...existing,
        ...info,
        exitedAt: info.exitedAt ?? (isTerminalClosed(info.state) ? existing?.exitedAt : undefined),
      });
      newTerminals.set(info.sessionId, sessionMap);
      return { terminals: newTerminals };
    }),

  updateTerminalState: (terminalId, newState, exitCode, sessionId) =>
    set((state) => {
      const newTerminals = new Map(state.terminals);
      for (const [sid, sessionMap] of newTerminals) {
        if (sessionMap.has(terminalId)) {
          const updated = new Map(sessionMap);
          const existing = updated.get(terminalId);
          if (!existing) break;
          updated.set(terminalId, {
            ...existing,
            state: newState,
            exitCode,
            exitedAt:
              isTerminalClosed(newState)
                ? Date.now()
                : existing.exitedAt,
          });
          newTerminals.set(sid, updated);
          return { terminals: newTerminals };
        }
      }
      if (sessionId) {
        const sessionMap = new Map(newTerminals.get(sessionId) ?? []);
        sessionMap.set(terminalId, {
          id: terminalId,
          sessionId,
          capability: "state_only",
          cwd: "",
          state: newState,
          exitCode,
          createdAt: Date.now(),
          exitedAt: isTerminalClosed(newState) ? Date.now() : undefined,
        });
        newTerminals.set(sessionId, sessionMap);
      }
      return { terminals: newTerminals };
    }),

  appendOutput: (terminalId, data) =>
    set((state) => {
      const newBuffers = new Map(state.outputBuffers);
      const newBaseOffsets = new Map(state.outputBufferBaseOffsets);
      const prev = newBuffers.get(terminalId) ?? "";
      const prevBaseOffset = newBaseOffsets.get(terminalId) ?? 0;

      if (data.length >= TERMINAL_OUTPUT_BUFFER_MAX_CHARS) {
        const droppedFromData = data.length - TERMINAL_OUTPUT_BUFFER_MAX_CHARS;
        newBuffers.set(terminalId, data.slice(droppedFromData));
        newBaseOffsets.set(terminalId, prevBaseOffset + prev.length + droppedFromData);
        return { outputBuffers: newBuffers, outputBufferBaseOffsets: newBaseOffsets };
      }

      const retainedPrevLength = TERMINAL_OUTPUT_BUFFER_MAX_CHARS - data.length;
      const droppedFromPrev = Math.max(0, prev.length - retainedPrevLength);
      const next = prev.slice(droppedFromPrev) + data;
      newBuffers.set(terminalId, next);
      newBaseOffsets.set(terminalId, prevBaseOffset + droppedFromPrev);
      return { outputBuffers: newBuffers, outputBufferBaseOffsets: newBaseOffsets };
    }),

  replaceOutput: (terminalId, data) =>
    set((state) => {
      const retained = retainOutput(data);
      const newBuffers = new Map(state.outputBuffers);
      const newBaseOffsets = new Map(state.outputBufferBaseOffsets);
      const newRevisions = new Map(state.outputBufferRevisions);
      newBuffers.set(terminalId, retained.output);
      newBaseOffsets.set(terminalId, retained.baseOffset);
      newRevisions.set(terminalId, (newRevisions.get(terminalId) ?? 0) + 1);
      return {
        outputBuffers: newBuffers,
        outputBufferBaseOffsets: newBaseOffsets,
        outputBufferRevisions: newRevisions,
      };
    }),

  projectOutputEvent: (sessionId, eventSeq, terminalId, data) => {
    const key = projectionKey(sessionId, eventSeq);
    if (get().projectedEventKeys.has(key)) return false;
    set((state) => ({
      projectedEventKeys: new Set(state.projectedEventKeys).add(key),
    }));
    get().appendOutput(terminalId, data);
    return true;
  },

  projectStateEvent: (sessionId, eventSeq, terminalId, newState, exitCode) => {
    const key = projectionKey(sessionId, eventSeq);
    if (get().projectedEventKeys.has(key)) return false;
    set((state) => ({
      projectedEventKeys: new Set(state.projectedEventKeys).add(key),
    }));
    get().updateTerminalState(terminalId, newState, exitCode, sessionId);
    return true;
  },

  removeTerminal: (terminalId) =>
    set((state) => {
      const newTerminals = new Map(state.terminals);
      for (const [sid, sessionMap] of newTerminals) {
        if (sessionMap.has(terminalId)) {
          const updated = new Map(sessionMap);
          updated.delete(terminalId);
          newTerminals.set(sid, updated);
          break;
        }
      }
      const newBuffers = new Map(state.outputBuffers);
      newBuffers.delete(terminalId);
      const newBaseOffsets = new Map(state.outputBufferBaseOffsets);
      newBaseOffsets.delete(terminalId);
      const newRevisions = new Map(state.outputBufferRevisions);
      newRevisions.delete(terminalId);
      return {
        terminals: newTerminals,
        outputBuffers: newBuffers,
        outputBufferBaseOffsets: newBaseOffsets,
        outputBufferRevisions: newRevisions,
      };
    }),

  getTerminalsForSession: (sessionId) => {
    const sessionMap = get().terminals.get(sessionId);
    return sessionMap ? Array.from(sessionMap.values()) : [];
  },

  getOutput: (terminalId) => {
    return get().outputBuffers.get(terminalId) ?? "";
  },

  getOutputBaseOffset: (terminalId) => {
    return get().outputBufferBaseOffsets.get(terminalId) ?? 0;
  },

  getOutputRevision: (terminalId) => {
    return get().outputBufferRevisions.get(terminalId) ?? 0;
  },
}));
