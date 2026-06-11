/**
 * WorkspacePanel v2 — 浏览器化动态 Tab 容器
 *
 * 顶部 TabBar 分为钉选区（上下文/审计）和动态区（Canvas/VFS/Terminal 等），
 * 动态 Tab 支持 dnd-kit 拖拽排序、关闭，以及 "+" 按钮新建。
 * 下方 AddressBar 展示当前 Tab 的 URI。
 * 内容区根据 TabTypeDescriptor 渲染对应组件。
 */

import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo } from "react";
import { useWorkspaceTabStore } from "../../stores/workspaceTabStore";
import {
  tabTypeRegistry,
  useTabTypeRegistrySnapshot,
} from "./tab-type-registry";
import { createExtensionTabDescriptors } from "../extension-runtime";
import { registerBuiltinTabTypes } from "./tab-types";
import { WorkspaceDataProvider, type WorkspaceData } from "./workspace-data-context";
import { TabBar } from "./TabBar";
import { AddressBar } from "./AddressBar";
import type { WorkspacePanelHandle, WorkspacePanelProps } from "./workspace-panel-types";

registerBuiltinTabTypes();

export const WorkspacePanel = forwardRef<WorkspacePanelHandle, WorkspacePanelProps>(
  function WorkspacePanel(props, ref) {
    const { runtimeData } = props;
    const { projectId, sessionId, extensionRuntime } = runtimeData;

    const tabs = useWorkspaceTabStore((s) => s.tabs);
    const activeTabId = useWorkspaceTabStore((s) => s.activeTabId);
    const storeSessionId = useWorkspaceTabStore((s) => s.sessionId);
    const registrySnapshot = useTabTypeRegistrySnapshot();

    // 首次挂载或 session 切换时初始化 Tab 状态
    useEffect(() => {
      if (storeSessionId !== sessionId) {
        useWorkspaceTabStore.getState().initialize(sessionId);
      }
    }, [sessionId, storeSessionId]);

    useEffect(() => {
      if (
        !projectId
        || (extensionRuntime.status !== "ready" && extensionRuntime.status !== "refreshing")
      ) {
        return;
      }
      const ownerKey = `project-extension-runtime:${projectId}`;
      const descriptors = createExtensionTabDescriptors({
        projection: extensionRuntime.projection,
      });
      tabTypeRegistry.registerContribution(ownerKey, descriptors);
      return () => tabTypeRegistry.unregisterContribution(ownerKey);
    }, [extensionRuntime.projection, extensionRuntime.status, projectId]);

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

    const workspaceData: WorkspaceData = useMemo(() => runtimeData, [runtimeData]);

    // 渲染当前激活 Tab 的内容
    const activeContent = useMemo(() => {
      if (!activeTab) return null;
      const type = registrySnapshot.find((descriptor) => descriptor.typeId === activeTab.typeId);
      if (!type) {
        return (
          <UnavailableTabState
            typeId={activeTab.typeId}
            uri={activeTab.uri}
          />
        );
      }
      return type.renderContent({
        uri: activeTab.uri,
        tabId: activeTab.id,
        sessionId,
        isActive: true,
      });
    }, [activeTab, registrySnapshot, sessionId]);

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

function UnavailableTabState({ typeId, uri }: { typeId: string; uri: string }) {
  return (
    <div className="flex h-full min-h-[180px] items-center justify-center bg-background p-6">
      <div className="max-w-sm rounded-[8px] border border-border bg-secondary/25 px-4 py-3 text-sm">
        <p className="font-medium text-foreground">Workspace tab 不可用</p>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          {typeId} 没有可用的 tab descriptor，可能对应插件已停用或尚未加载。
        </p>
        <p className="mt-2 truncate font-mono text-[11px] text-muted-foreground">{uri}</p>
      </div>
    </div>
  );
}
