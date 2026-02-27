/**
 * 会话历史记录存储
 *
 * 管理用户会话历史，用于左侧栏历史会话列表展示
 */

import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface SessionHistory {
  id: string;
  title: string;
  preview: string;
  timestamp: number;
}

interface SessionHistoryState {
  sessions: SessionHistory[];
  addSession: (session: Omit<SessionHistory, "timestamp">) => void;
  removeSession: (id: string) => void;
  clearSessions: () => void;
}

export const useSessionHistoryStore = create<SessionHistoryState>()(
  persist(
    (set) => ({
      sessions: [],
      addSession: (session) =>
        set((state) => ({
          sessions: [
            { ...session, timestamp: Date.now() },
            ...state.sessions.slice(0, 49), // 最多保留 50 条
          ],
        })),
      removeSession: (id) =>
        set((state) => ({
          sessions: state.sessions.filter((s) => s.id !== id),
        })),
      clearSessions: () => set({ sessions: [] }),
    }),
    {
      name: "agentdash-session-history",
    },
  ),
);
