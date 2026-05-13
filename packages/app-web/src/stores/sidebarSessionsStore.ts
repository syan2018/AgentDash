/**
 * Sidebar 会话快捷列表 Store
 *
 * 与 activeSessionsStore 脱钩：sidebar 有独立的刷新生命周期（挂载即拉 + 定时轮询），
 * 不受 Agent tab 的 filter/分组等交互影响。两者数据源同一 API，但各自维护 state。
 *
 * TODO(user-filter): 当前后端 SessionBinding 无 `created_by` 字段，无法按当前用户过滤。
 *   后端补齐后在此处加 `?creator=<current_user_id>` 查询或客户端 filter。
 */

import { create } from "zustand";
import type { ProjectSessionEntry } from "../types";
import { fetchProjectSessions } from "../services/session";

interface SidebarSessionsState {
  sessions: ProjectSessionEntry[];
  isLoading: boolean;
  loadedProjectId: string | null;
  loadForProject: (projectId: string) => Promise<void>;
  clearForProject: (projectId: string) => void;
}

export const useSidebarSessionsStore = create<SidebarSessionsState>((set, get) => ({
  sessions: [],
  isLoading: false,
  loadedProjectId: null,

  clearForProject: (projectId: string) => {
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
      console.error(`[sidebarSessionsStore] 加载项目 ${projectId} 的会话列表失败:`, e);
      set({ sessions: [], isLoading: false });
    }
  },
}));
