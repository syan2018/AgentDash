/**
 * 活跃会话列表 Store
 *
 * 维护当前项目的所有活跃会话（涵盖 project / story / task 三个层级），
 * 数据来源于 GET /api/projects/{id}/sessions。
 *
 * 竞态保护：每次 loadForProject 记录请求发起时的 projectId，
 * resolve 时比对当前 projectId，不一致则丢弃旧数据。
 */

import { create } from "zustand";
import type { ProjectSessionEntry } from "../types";
import { fetchProjectSessions } from "../services/session";

interface ActiveSessionsState {
  sessions: ProjectSessionEntry[];
  isLoading: boolean;
  /** 当前已加载数据所属的项目 ID，用于判断数据是否过期 */
  loadedProjectId: string | null;

  /** 加载指定项目的活跃会话，带竞态保护 */
  loadForProject: (projectId: string) => Promise<void>;

  /** 切换项目时立即清空旧数据，避免短暂展示错误内容 */
  clearForProject: (projectId: string) => void;

  /** SSE 实时更新单条会话状态 */
  updateSessionStatus: (
    sessionId: string,
    status: ProjectSessionEntry["execution_status"],
  ) => void;
}

export const useActiveSessionsStore = create<ActiveSessionsState>((set, get) => ({
  sessions: [],
  isLoading: false,
  loadedProjectId: null,

  clearForProject: (projectId: string) => {
    // 切换到新项目时立即清空，不展示上一个项目的残留数据
    set({ sessions: [], loadedProjectId: projectId, isLoading: true });
  },

  loadForProject: async (projectId: string) => {
    const requestedFor = projectId;
    set({ isLoading: true, loadedProjectId: projectId });
    try {
      const sessions = await fetchProjectSessions(projectId);
      if (get().loadedProjectId !== requestedFor) return;
      set({ sessions, isLoading: false });
    } catch (e) {
      if (get().loadedProjectId !== requestedFor) return;
      // 不静默吞掉错误：记录日志，设为空列表，让 UI 展示错误状态
      console.error(`[activeSessionsStore] 加载项目 ${projectId} 的会话列表失败:`, e);
      set({ sessions: [], isLoading: false });
    }
  },

  updateSessionStatus: (sessionId, status) => {
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.session_id === sessionId ? { ...s, execution_status: status } : s,
      ),
    }));
  },
}));
