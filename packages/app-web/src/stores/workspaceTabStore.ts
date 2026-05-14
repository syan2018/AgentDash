/**
 * 工作空间 Tab 状态管理
 *
 * 管理 SessionPage 右栏动态 Tab 实例的生命周期：
 * 增/删/激活/排序/URI 更新，以及从后端 session meta 恢复/持久化。
 */

import { create } from "zustand";
import { tabTypeRegistry, type TabInstance, type SessionTabLayout } from "../features/workspace-panel/tab-type-registry";
import { saveSessionTabLayout, loadSessionTabLayout } from "../services/session";

let nextTabSeq = 1;
function generateTabId(): string {
  return `tab_${Date.now()}_${nextTabSeq++}`;
}

// ─── 默认钉选 Tab 生成 ─────────────────────────────────

function createDefaultPinnedTabs(): TabInstance[] {
  const pinnedTypes = tabTypeRegistry.listTypes().filter((t) => t.pinned);
  return pinnedTypes.map((type) => ({
    id: generateTabId(),
    typeId: type.typeId,
    uri: type.defaultUri ?? type.buildUri({}),
    title: type.label,
    pinned: true,
  }));
}

// ─── Store ──────────────────────────────────────────────

interface WorkspaceTabState {
  tabs: TabInstance[];
  activeTabId: string | null;
  sessionId: string | null;

  /** 初始化：从后端恢复或生成默认状态 */
  initialize: (sessionId: string | null, saved?: SessionTabLayout | null) => void;
  /** 添加新 Tab 实例，返回实例 ID */
  addTab: (typeId: string, uri?: string, activate?: boolean) => string;
  /** 关闭 Tab（钉选 Tab 不可关闭） */
  closeTab: (tabId: string) => void;
  /** 激活 Tab */
  activateTab: (tabId: string) => void;
  /** 按 URI 查找并激活同类型 Tab，不存在则新建 */
  openOrActivate: (typeId: string, uri: string) => string;
  /** 拖拽排序后更新顺序 */
  reorderTabs: (fromIndex: number, toIndex: number) => void;
  /** 更新 Tab 的 URI（导航到新位置） */
  updateTabUri: (tabId: string, uri: string) => void;
  /** 导出当前布局用于持久化 */
  exportLayout: () => SessionTabLayout;
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
  sessionId: null,

  initialize: (sessionId, saved) => {
    // 尝试从后端加载已保存的布局（异步，不阻塞初始化）
    if (!saved && sessionId) {
      void loadSessionTabLayout(sessionId).then((loaded) => {
        if (loaded && get().sessionId === sessionId && get().tabs.every((t) => t.pinned)) {
          get().initialize(sessionId, loaded);
        }
      });
    }

    if (saved && saved.tabs.length > 0) {
      const tabs: TabInstance[] = saved.tabs.map((item) => ({
        id: generateTabId(),
        typeId: item.type_id,
        uri: item.uri,
        title: item.title,
        pinned: item.pinned,
      }));

      const pinnedTypes = tabTypeRegistry.listTypes().filter((t) => t.pinned);
      for (const type of pinnedTypes) {
        if (!tabs.some((t) => t.typeId === type.typeId)) {
          tabs.unshift({
            id: generateTabId(),
            typeId: type.typeId,
            uri: type.defaultUri ?? type.buildUri({}),
            title: type.label,
            pinned: true,
          });
        }
      }

      const activeTab = saved.active_tab_uri
        ? tabs.find((t) => t.uri === saved.active_tab_uri)
        : null;

      set({
        tabs,
        activeTabId: activeTab?.id ?? tabs[0]?.id ?? null,
        sessionId,
      });
    } else {
      const tabs = createDefaultPinnedTabs();
      set({
        tabs,
        activeTabId: tabs[0]?.id ?? null,
        sessionId,
      });
    }
  },

  addTab: (typeId, uri, activate = true) => {
    const type = tabTypeRegistry.getType(typeId);
    if (!type) return "";

    if (!type.allowMultiple) {
      const existing = get().tabs.find((t) => t.typeId === typeId);
      if (existing) {
        if (uri) {
          set((s) => ({
            tabs: s.tabs.map((t) =>
              t.id === existing.id
                ? { ...t, uri, title: type.resolveTitle(uri) }
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

    const tabUri = uri ?? type.defaultUri ?? type.buildUri({});
    const newTab: TabInstance = {
      id: generateTabId(),
      typeId,
      uri: tabUri,
      title: type.resolveTitle(tabUri),
      pinned: false,
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

  openOrActivate: (typeId, uri) => {
    const existing = get().tabs.find(
      (t) => t.typeId === typeId && t.uri === uri,
    );
    if (existing) {
      set({ activeTabId: existing.id });
      return existing.id;
    }
    return get().addTab(typeId, uri, true);
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

  updateTabUri: (tabId, uri) => {
    set((s) => {
      const tab = s.tabs.find((t) => t.id === tabId);
      if (!tab) return s;
      const type = tabTypeRegistry.getType(tab.typeId);
      return {
        tabs: s.tabs.map((t) =>
          t.id === tabId
            ? { ...t, uri, title: type?.resolveTitle(uri) ?? t.title }
            : t,
        ),
      };
    });
  },

  exportLayout: (): SessionTabLayout => {
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
    const sid = get().sessionId;
    if (!sid) return;
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      persistTimer = null;
      const layout = get().exportLayout();
      void saveSessionTabLayout(sid, layout);
    }, PERSIST_DEBOUNCE_MS);
  },

  reset: () => {
    if (persistTimer) {
      clearTimeout(persistTimer);
      persistTimer = null;
    }
    set({ tabs: [], activeTabId: null, sessionId: null });
  },
}));
