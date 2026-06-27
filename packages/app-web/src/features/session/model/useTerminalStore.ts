import { create } from "zustand";
import type { TerminalInfo, TerminalProcessState } from "../../../types/terminal";

export const TERMINAL_OUTPUT_BUFFER_MAX_CHARS = 256 * 1024;

interface TerminalStoreState {
  /** session_id → { terminal_id → TerminalInfo } */
  terminals: Map<string, Map<string, TerminalInfo>>;
  /** terminal_id → 有界最近输出 buffer */
  outputBuffers: Map<string, string>;
  /** terminal_id → 当前 buffer 前方已裁掉的字符数 */
  outputBufferBaseOffsets: Map<string, number>;

  registerTerminal: (info: TerminalInfo) => void;
  updateTerminalState: (
    terminalId: string,
    state: TerminalProcessState,
    exitCode?: number,
  ) => void;
  appendOutput: (terminalId: string, data: string) => void;
  removeTerminal: (terminalId: string) => void;
  getTerminalsForSession: (sessionId: string) => TerminalInfo[];
  getOutput: (terminalId: string) => string;
  getOutputBaseOffset: (terminalId: string) => number;
}

export const useTerminalStore = create<TerminalStoreState>((set, get) => ({
  terminals: new Map(),
  outputBuffers: new Map(),
  outputBufferBaseOffsets: new Map(),

  registerTerminal: (info) =>
    set((state) => {
      const newTerminals = new Map(state.terminals);
      const sessionMap = new Map(newTerminals.get(info.sessionId) ?? []);
      sessionMap.set(info.id, info);
      newTerminals.set(info.sessionId, sessionMap);
      return { terminals: newTerminals };
    }),

  updateTerminalState: (terminalId, newState, exitCode) =>
    set((state) => {
      const newTerminals = new Map(state.terminals);
      for (const [sid, sessionMap] of newTerminals) {
        if (sessionMap.has(terminalId)) {
          const updated = new Map(sessionMap);
          const existing = updated.get(terminalId)!;
          updated.set(terminalId, {
            ...existing,
            state: newState,
            exitCode,
            exitedAt:
              newState === "exited" || newState === "killed" || newState === "lost"
                ? Date.now()
                : existing.exitedAt,
          });
          newTerminals.set(sid, updated);
          break;
        }
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
      return {
        terminals: newTerminals,
        outputBuffers: newBuffers,
        outputBufferBaseOffsets: newBaseOffsets,
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
}));
