/**
 * 工作空间 Tab 状态管理
 *
 * 管理 AgentRun workspace 右栏动态 Tab 实例的生命周期：
 * 增/删/激活/排序/URI 更新，以及按 AgentRun workspace key 恢复/持久化。
 */

import { create } from "zustand";
import type { TabInstance, WorkspaceTabLayout } from "../features/workspace-runtime";
import {
  loadWorkspaceTabLayout,
  saveWorkspaceTabLayout,
} from "../services/agentRunWorkspaceLayout";

let nextTabSeq = 1;
function generateTabId(): string {
  return `tab_${Date.now()}_${nextTabSeq++}`;
}

export interface WorkspaceTabTypeLayoutDescriptor {
  typeId: string;
  label: string;
  allowMultiple: boolean;
  pinned: boolean;
  defaultUri: string;
  canCreateUri?: (uri: string) => boolean;
}

export interface WorkspaceTabLayoutOptions {
  tabTypes: WorkspaceTabTypeLayoutDescriptor[];
  resolveTitle: (typeId: string, uri: string) => string;
}

function fallbackTitle(typeId: string, uri: string): string {
  return uri.trim() || typeId;
}

function resolveTitle(
  typeId: string,
  uri: string,
  options?: WorkspaceTabLayoutOptions,
): string {
  return options?.resolveTitle(typeId, uri) ?? fallbackTitle(typeId, uri);
}

function findTabType(
  typeId: string,
  options?: WorkspaceTabLayoutOptions,
): WorkspaceTabTypeLayoutDescriptor | null {
  return options?.tabTypes.find((type) => type.typeId === typeId) ?? null;
}

function canUseTabUri(
  typeId: string,
  uri: string,
  options?: WorkspaceTabLayoutOptions,
): boolean {
  const type = findTabType(typeId, options);
  if (options && !type) return false;
  return !type?.canCreateUri || type.canCreateUri(uri);
}

// ─── 默认钉选 Tab 生成 ─────────────────────────────────

function createDefaultPinnedTabs(options?: WorkspaceTabLayoutOptions): TabInstance[] {
  const pinnedTypes = options?.tabTypes.filter((type) => type.pinned) ?? [];
  return pinnedTypes.map((type) => ({
    id: generateTabId(),
    typeId: type.typeId,
    uri: type.defaultUri,
    title: type.label,
    pinned: true,
    refreshRevision: 0,
  }));
}

// ─── Store ──────────────────────────────────────────────

interface WorkspaceTabState {
  tabs: TabInstance[];
  activeTabId: string | null;
  workspaceKey: string | null;

  /** 初始化：从用户设置恢复或生成默认状态 */
  initialize: (
    workspaceKey: string | null,
    saved?: WorkspaceTabLayout | null,
    options?: WorkspaceTabLayoutOptions,
  ) => void;
  /** 添加新 Tab 实例，返回实例 ID */
  addTab: (
    typeId: string,
    uri?: string,
    activate?: boolean,
    options?: WorkspaceTabLayoutOptions,
  ) => string;
  /** 关闭 Tab（钉选 Tab 不可关闭） */
  closeTab: (tabId: string) => void;
  /** 激活 Tab */
  activateTab: (tabId: string) => void;
  /** 按 URI 查找并激活同类型 Tab，不存在则新建 */
  openOrActivate: (typeId: string, uri: string, options?: WorkspaceTabLayoutOptions) => string;
  /** 触发指定 Tab 内容重拉，不改变 URI 或布局持久化结果 */
  refreshTab: (tabId: string) => void;
  /** 拖拽排序后更新顺序 */
  reorderTabs: (fromIndex: number, toIndex: number) => void;
  /** 更新 Tab 的 URI（导航到新位置） */
  updateTabUri: (tabId: string, uri: string, title?: string) => void;
  /** 清理当前 runtime 下不可再打开的动态 Tab */
  pruneInvalidTabs: (options: WorkspaceTabLayoutOptions) => void;
  /** 导出当前布局用于持久化 */
  exportLayout: () => WorkspaceTabLayout;
  /** 防抖持久化到后端 */
  schedulePersist: () => void;
  /** 重置状态 */
  reset: () => void;
}

let persistTimer: ReturnType<typeof setTimeout> | null = null;
const PERSIST_DEBOUNCE_MS = 1500;

export const useWorkspaceTabStore = create<WorkspaceTabState>()((set, get) => ({
  tabs: [],
  activeTabId: null,
  workspaceKey: null,

  initialize: (workspaceKey, saved, options) => {
    // 尝试加载已保存的布局（异步，不阻塞初始化）
    if (!saved && workspaceKey) {
      void loadWorkspaceTabLayout(workspaceKey)
        .then((loaded) => {
          if (
            loaded
            && get().workspaceKey === workspaceKey
            && get().tabs.every((t) => t.pinned)
          ) {
            get().initialize(workspaceKey, loaded, options);
          }
        })
        .catch((error: unknown) => {
          console.error("加载 workspace tab layout 失败", error);
        });
    }

    if (saved && saved.tabs.length > 0) {
      const tabs: TabInstance[] = saved.tabs
        .filter((item) => item.pinned || canUseTabUri(item.type_id, item.uri, options))
        .map((item) => ({
          id: generateTabId(),
          typeId: item.type_id,
          uri: item.uri,
          title: item.title,
          pinned: item.pinned,
          refreshRevision: 0,
        }));

      const pinnedTypes = options?.tabTypes.filter((type) => type.pinned) ?? [];
      for (const type of pinnedTypes) {
        if (!tabs.some((t) => t.typeId === type.typeId)) {
          tabs.unshift({
            id: generateTabId(),
            typeId: type.typeId,
            uri: type.defaultUri,
            title: type.label,
            pinned: true,
            refreshRevision: 0,
          });
        }
      }

      const activeTab = saved.active_tab_uri
        ? tabs.find((t) => t.uri === saved.active_tab_uri)
        : null;

      set({
        tabs,
        activeTabId: activeTab?.id ?? tabs[0]?.id ?? null,
        workspaceKey,
      });
    } else {
      const tabs = createDefaultPinnedTabs(options);
      set({
        tabs,
        activeTabId: tabs[0]?.id ?? null,
        workspaceKey,
      });
    }
  },

  addTab: (typeId, uri, activate = true, options) => {
    const type = findTabType(typeId, options);
    if (options && !type) return "";

    if (type && !type.allowMultiple) {
      const existing = get().tabs.find((t) => t.typeId === typeId);
      if (existing) {
        if (uri) {
          set((s) => ({
            tabs: s.tabs.map((t) =>
              t.id === existing.id
                ? { ...t, uri, title: resolveTitle(typeId, uri, options) }
                : t,
            ),
            activeTabId: activate ? existing.id : s.activeTabId,
          }));
        } else if (activate) {
          set({ activeTabId: existing.id });
        }
        return existing.id;
      }
    }

    const tabUri = uri ?? type?.defaultUri ?? "";
    if (!canUseTabUri(typeId, tabUri, options)) {
      return "";
    }
    const newTab: TabInstance = {
      id: generateTabId(),
      typeId,
      uri: tabUri,
      title: resolveTitle(typeId, tabUri, options),
      pinned: false,
      refreshRevision: 0,
    };

    set((s) => ({
      tabs: [...s.tabs, newTab],
      activeTabId: activate ? newTab.id : s.activeTabId,
    }));

    get().schedulePersist();
    return newTab.id;
  },

  closeTab: (tabId) => {
    const state = get();
    const tab = state.tabs.find((t) => t.id === tabId);
    if (!tab || tab.pinned) return;

    const index = state.tabs.indexOf(tab);
    const nextTabs = state.tabs.filter((t) => t.id !== tabId);

    let nextActiveId = state.activeTabId;
    if (state.activeTabId === tabId) {
      const neighbor = nextTabs[Math.min(index, nextTabs.length - 1)];
      nextActiveId = neighbor?.id ?? null;
    }

    set({ tabs: nextTabs, activeTabId: nextActiveId });
    get().schedulePersist();
  },

  activateTab: (tabId) => {
    if (get().tabs.some((t) => t.id === tabId)) {
      set({ activeTabId: tabId });
    }
  },

  openOrActivate: (typeId, uri, options) => {
    const type = findTabType(typeId, options);
    if (options && !type) {
      return "";
    }
    if (!canUseTabUri(typeId, uri, options)) {
      return "";
    }
    const existing = get().tabs.find(
      (t) => t.typeId === typeId && t.uri === uri,
    );
    if (existing) {
      set({ activeTabId: existing.id });
      return existing.id;
    }
    return get().addTab(typeId, uri, true, options);
  },

  refreshTab: (tabId) => {
    set((s) => ({
      tabs: s.tabs.map((tab) =>
        tab.id === tabId
          ? { ...tab, refreshRevision: tab.refreshRevision + 1 }
          : tab,
      ),
    }));
  },

  reorderTabs: (fromIndex, toIndex) => {
    set((s) => {
      const tabs = [...s.tabs];
      const [moved] = tabs.splice(fromIndex, 1);
      tabs.splice(toIndex, 0, moved);
      return { tabs };
    });
    get().schedulePersist();
  },

  updateTabUri: (tabId, uri, title) => {
    set((s) => {
      const tab = s.tabs.find((t) => t.id === tabId);
      if (!tab) return s;
      return {
        tabs: s.tabs.map((t) =>
          t.id === tabId
            ? { ...t, uri, title: title ?? t.title }
            : t,
        ),
      };
    });
  },

  pruneInvalidTabs: (options) => {
    const state = get();
    const nextTabs = state.tabs.filter(
      (tab) => tab.pinned || canUseTabUri(tab.typeId, tab.uri, options),
    );
    if (nextTabs.length === state.tabs.length) return;
    const activeStillValid = nextTabs.some((tab) => tab.id === state.activeTabId);
    set({
      tabs: nextTabs,
      activeTabId: activeStillValid ? state.activeTabId : nextTabs[0]?.id ?? null,
    });
    get().schedulePersist();
  },

  exportLayout: (): WorkspaceTabLayout => {
    const state = get();
    const activeTab = state.tabs.find((t) => t.id === state.activeTabId);
    return {
      tabs: state.tabs.map((t) => ({
        type_id: t.typeId,
        uri: t.uri,
        title: t.title,
        pinned: t.pinned,
      })),
      active_tab_uri: activeTab?.uri ?? null,
    };
  },

  schedulePersist: () => {
    const workspaceKey = get().workspaceKey;
    if (!workspaceKey) return;
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      persistTimer = null;
      if (get().workspaceKey !== workspaceKey) return;
      const layout = get().exportLayout();
      void saveWorkspaceTabLayout(workspaceKey, layout).catch((error: unknown) => {
        console.error("保存 workspace tab layout 失败", error);
      });
    }, PERSIST_DEBOUNCE_MS);
  },

  reset: () => {
    if (persistTimer) {
      clearTimeout(persistTimer);
      persistTimer = null;
    }
    set({ tabs: [], activeTabId: null, workspaceKey: null });
  },
}));
