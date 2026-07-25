import { create } from "zustand";
import type { TerminalInfo, TerminalProcessState } from "../../../types/terminal";

export const TERMINAL_OUTPUT_BUFFER_MAX_CHARS = 256 * 1024;

function isTerminalClosed(state: TerminalProcessState): boolean {
  return state === "exited" || state === "killed" || state === "lost";
}

function projectionKey(streamIdentity: string, eventSeq: number): string {
  return `${streamIdentity}:${eventSeq}`;
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
  /** terminal_id -> TerminalInfo (flat, terminal_id is globally unique) */
  terminals: Map<string, TerminalInfo>;
  /** terminal_id -> bounded output buffer */
  outputBuffers: Map<string, string>;
  /** terminal_id -> characters dropped from the front of the buffer */
  outputBufferBaseOffsets: Map<string, number>;
  /** terminal_id -> replaceOutput revision; xterm replay uses this to detect rewrites */
  outputBufferRevisions: Map<string, number>;
  /** projected durable session event keys */
  projectedEventKeys: Set<string>;

  registerTerminal: (info: TerminalInfo) => void;
  updateTerminalState: (
    terminalId: string,
    state: TerminalProcessState,
    exitCode?: number,
  ) => void;
  updateTerminalAvailability: (
    terminalId: string,
    availability: "online" | "offline" | "reconciling",
  ) => void;
  appendOutput: (terminalId: string, data: string) => void;
  replaceOutput: (terminalId: string, data: string) => void;
  projectOutputEvent: (
    streamIdentity: string,
    eventSeq: number,
    terminalId: string,
    data: string,
  ) => boolean;
  projectStateEvent: (
    streamIdentity: string,
    eventSeq: number,
    terminalId: string,
    state: TerminalProcessState,
    exitCode?: number,
  ) => boolean;
  removeTerminal: (terminalId: string) => void;
  getTerminalsForAgentRun: (runId: string, agentId: string) => TerminalInfo[];
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
      const existing = newTerminals.get(info.id);
      newTerminals.set(info.id, {
        ...existing,
        ...info,
        exitedAt: info.exitedAt ?? (isTerminalClosed(info.state) ? existing?.exitedAt : undefined),
      });
      return { terminals: newTerminals };
    }),

  updateTerminalState: (terminalId, newState, exitCode) =>
    set((state) => {
      const newTerminals = new Map(state.terminals);
      const existing = newTerminals.get(terminalId);
      if (existing) {
        newTerminals.set(terminalId, {
          ...existing,
          state: newState,
          exitCode,
          exitedAt:
            isTerminalClosed(newState)
              ? Date.now()
              : existing.exitedAt,
        });
      } else {
        // Create state-only projection for unknown terminals
        newTerminals.set(terminalId, {
          id: terminalId,
          capability: "state_only",
          cwd: "",
          state: newState,
          exitCode,
          createdAt: Date.now(),
          exitedAt: isTerminalClosed(newState) ? Date.now() : undefined,
        });
      }
      return { terminals: newTerminals };
    }),

  updateTerminalAvailability: (terminalId, availability) =>
    set((state) => {
      const existing = state.terminals.get(terminalId);
      if (!existing) return state;
      const terminals = new Map(state.terminals);
      terminals.set(terminalId, { ...existing, availability });
      return { terminals };
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

  projectOutputEvent: (streamIdentity, eventSeq, terminalId, data) => {
    const key = projectionKey(streamIdentity, eventSeq);
    if (get().projectedEventKeys.has(key)) return false;
    set((state) => ({
      projectedEventKeys: new Set(state.projectedEventKeys).add(key),
    }));
    get().appendOutput(terminalId, data);
    return true;
  },

  projectStateEvent: (streamIdentity, eventSeq, terminalId, newState, exitCode) => {
    const key = projectionKey(streamIdentity, eventSeq);
    if (get().projectedEventKeys.has(key)) return false;
    set((state) => ({
      projectedEventKeys: new Set(state.projectedEventKeys).add(key),
    }));
    get().updateTerminalState(terminalId, newState, exitCode);
    return true;
  },

  removeTerminal: (terminalId) =>
    set((state) => {
      const newTerminals = new Map(state.terminals);
      newTerminals.delete(terminalId);
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

  getTerminalsForAgentRun: (runId, agentId) => {
    const terminals = get().terminals;
    return Array.from(terminals.values()).filter(
      (t) => t.runId === runId && t.agentId === agentId,
    );
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
