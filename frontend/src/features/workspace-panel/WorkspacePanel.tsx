/**
 * WorkspacePanel v2 — 浏览器化动态 Tab 容器
 *
 * 顶部 TabBar 分为钉选区（上下文/审计）和动态区（Canvas/VFS/Terminal 等），
 * 动态 Tab 支持 dnd-kit 拖拽排序、关闭，以及 "+" 按钮新建。
 * 下方 AddressBar 展示当前 Tab 的 URI。
 * 内容区根据 TabTypeDescriptor 渲染对应组件。
 */

import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef } from "react";
import { useWorkspaceTabStore } from "../../stores/workspaceTabStore";
import { tabTypeRegistry } from "./tab-type-registry";
import { registerBuiltinTabTypes } from "./tab-types";
import { WorkspaceDataProvider, type WorkspaceData } from "./workspace-data-context";
import { TabBar } from "./TabBar";
import { AddressBar } from "./AddressBar";
import type { WorkspacePanelHandle, WorkspacePanelProps } from "./workspace-panel-types";

registerBuiltinTabTypes();

export const WorkspacePanel = forwardRef<WorkspacePanelHandle, WorkspacePanelProps>(
  function WorkspacePanel(props, ref) {
    const {
      sessionId,
      contextSnapshot,
      ownerStory,
      ownerProjectName,
      executorSummary,
      runtimeSurface,
      vfs,
      hookRuntime,
      sessionCapabilities,
      workflowRuns,
      activeCanvasId,
    } = props;

    const tabs = useWorkspaceTabStore((s) => s.tabs);
    const activeTabId = useWorkspaceTabStore((s) => s.activeTabId);
    const storeSessionId = useWorkspaceTabStore((s) => s.sessionId);

    const prevCanvasIdRef = useRef<string | null>(null);

    // 首次挂载或 session 切换时初始化 Tab 状态
    useEffect(() => {
      if (storeSessionId !== sessionId) {
        useWorkspaceTabStore.getState().initialize(sessionId);
      }
    }, [sessionId, storeSessionId]);

    // 外部命令式 API：按类型打开或激活 Tab
    useImperativeHandle(ref, () => ({
      openTab: (typeId: string, uri?: string) => {
        const s = useWorkspaceTabStore.getState();
        if (uri) {
          s.openOrActivate(typeId, uri);
        } else {
          const type = tabTypeRegistry.getType(typeId);
          if (type) {
            const defaultUri = type.defaultUri ?? type.buildUri({});
            s.openOrActivate(typeId, defaultUri);
          }
        }
      },
    }), []);

    // activeCanvasId 变化时，自动打开/激活 Canvas Tab
    useEffect(() => {
      if (!activeCanvasId || activeCanvasId === prevCanvasIdRef.current) return;
      prevCanvasIdRef.current = activeCanvasId;
      const uri = `canvas://${activeCanvasId}`;
      useWorkspaceTabStore.getState().openOrActivate("canvas", uri);
    }, [activeCanvasId]);

    const handleAddTab = useCallback((typeId: string) => {
      useWorkspaceTabStore.getState().addTab(typeId);
    }, []);

    const handleActivate = useCallback((tabId: string) => {
      useWorkspaceTabStore.getState().activateTab(tabId);
    }, []);

    const handleClose = useCallback((tabId: string) => {
      useWorkspaceTabStore.getState().closeTab(tabId);
    }, []);

    const handleReorder = useCallback((fromIndex: number, toIndex: number) => {
      useWorkspaceTabStore.getState().reorderTabs(fromIndex, toIndex);
    }, []);

    const activeTab = useMemo(
      () => tabs.find((t) => t.id === activeTabId) ?? null,
      [tabs, activeTabId],
    );

    const workspaceData: WorkspaceData = useMemo(() => ({
      sessionId,
      contextSnapshot,
      ownerStory,
      ownerProjectName,
      executorSummary,
      runtimeSurface,
      vfs,
      hookRuntime,
      sessionCapabilities,
      workflowRuns,
      activeCanvasId,
    }), [
      sessionId, contextSnapshot, ownerStory, ownerProjectName,
      executorSummary, runtimeSurface, vfs, hookRuntime,
      sessionCapabilities, workflowRuns, activeCanvasId,
    ]);

    // 渲染当前激活 Tab 的内容
    const activeContent = useMemo(() => {
      if (!activeTab) return null;
      const type = tabTypeRegistry.getType(activeTab.typeId);
      if (!type) return null;
      return type.renderContent({
        uri: activeTab.uri,
        tabId: activeTab.id,
        sessionId,
        isActive: true,
      });
    }, [activeTab, sessionId]);

    return (
      <WorkspaceDataProvider value={workspaceData}>
        <div className="flex h-full flex-col bg-background">
          <TabBar
            tabs={tabs}
            activeTabId={activeTabId}
            onActivate={handleActivate}
            onClose={handleClose}
            onReorder={handleReorder}
            onAddTab={handleAddTab}
          />
          <AddressBar tab={activeTab} />
          <div className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
            {activeContent}
          </div>
        </div>
      </WorkspaceDataProvider>
    );
  },
);
