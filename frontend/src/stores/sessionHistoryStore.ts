/**
 * 会话历史记录存储
 *
 * 从后端 API 获取会话列表，用于侧边栏历史会话展示。
 * 同时维护 activeSessionId 作为当前打开的会话。
 */

import { create } from "zustand";
import {
  fetchSessions,
  createSession,
  deleteSession as apiDeleteSession,
  type SessionMeta,
} from "../services/session";

interface SessionHistoryState {
  sessions: SessionMeta[];
  isLoading: boolean;
  activeSessionId: string | null;
  error: string | null;

  setActiveSessionId: (id: string | null) => void;
  reload: () => Promise<void>;
  createNew: (title?: string) => Promise<SessionMeta>;
  removeSession: (id: string) => Promise<void>;
}

export type { SessionMeta };

export const useSessionHistoryStore = create<SessionHistoryState>()((set, get) => ({
  sessions: [],
  isLoading: false,
  activeSessionId: null,
  error: null,

  setActiveSessionId: (id) => set({ activeSessionId: id }),

  reload: async () => {
    set({ isLoading: true, error: null });
    try {
      const sessions = await fetchSessions({ excludeBound: true });
      set({ sessions, isLoading: false, error: null });
    } catch (e) {
      set({
        isLoading: false,
        error: e instanceof Error ? e.message : "加载会话历史失败",
      });
    }
  },

  createNew: async (title?: string) => {
    const meta = await createSession(title);
    await get().reload();
    return meta;
  },

  removeSession: async (id: string) => {
    try {
      await apiDeleteSession(id);
      set((state) => ({
        sessions: state.sessions.filter((s) => s.id !== id),
        activeSessionId: state.activeSessionId === id ? null : state.activeSessionId,
        error: null,
      }));
    } catch (e) {
      set({
        error: e instanceof Error ? e.message : "删除会话失败",
      });
      throw e;
    }
  },
}));
