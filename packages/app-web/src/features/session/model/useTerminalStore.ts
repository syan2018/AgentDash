import { create } from "zustand";
import type { TerminalInfo, TerminalProcessState } from "../../../types/terminal";

interface TerminalStoreState {
  /** session_id → { terminal_id → TerminalInfo } */
  terminals: Map<string, Map<string, TerminalInfo>>;
  /** terminal_id → 累积输出 buffer */
  outputBuffers: Map<string, string>;

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
}

export const useTerminalStore = create<TerminalStoreState>((set, get) => ({
  terminals: new Map(),
  outputBuffers: new Map(),

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
      const prev = newBuffers.get(terminalId) ?? "";
      newBuffers.set(terminalId, prev + data);
      return { outputBuffers: newBuffers };
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
      return { terminals: newTerminals, outputBuffers: newBuffers };
    }),

  getTerminalsForSession: (sessionId) => {
    const sessionMap = get().terminals.get(sessionId);
    return sessionMap ? Array.from(sessionMap.values()) : [];
  },

  getOutput: (terminalId) => {
    return get().outputBuffers.get(terminalId) ?? "";
  },
}));
