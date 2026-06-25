/**
 * WorkspacePanel v2 — 浏览器化动态 Tab 容器
 *
 * 顶部 TabBar 分为钉选区（上下文/审计）和动态区（Canvas/VFS/Terminal 等），
 * 动态 Tab 支持 dnd-kit 拖拽排序、关闭，以及 "+" 按钮新建。
 * 下方 AddressBar 展示当前 Tab 的 URI。
 * 内容区根据 TabTypeDescriptor 渲染对应组件。
 */

import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useState } from "react";
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
import type { WorkspaceTabLayoutOptions } from "../../stores/workspaceTabStore";
import {
  activeCanvasMountIdsFromRuntimeSurface,
  canvasMountIdFromPresentationUri,
  selectCanvasModuleOpenOptions,
  openUserCanvasModule,
  type CanvasModuleOpenOption,
} from "./model/canvasModuleOpen";
import {
  useWorkspaceModuleStore,
} from "../workspace-module/model/workspaceModuleStore";
import { idleProjectWorkspaceModulesState } from "../workspace-module/model/types";

registerBuiltinTabTypes();

export const WorkspacePanel = forwardRef<WorkspacePanelHandle, WorkspacePanelProps>(
  function WorkspacePanel(props, ref) {
    const { runtimeData, onWorkspaceModuleOpened } = props;
    const { projectId, sessionId, extensionRuntime } = runtimeData;

    const tabs = useWorkspaceTabStore((s) => s.tabs);
    const activeTabId = useWorkspaceTabStore((s) => s.activeTabId);
    const storeSessionId = useWorkspaceTabStore((s) => s.sessionId);
    const registrySnapshot = useTabTypeRegistrySnapshot();
    const fetchWorkspaceModules = useWorkspaceModuleStore((s) => s.fetchProject);
    const storedWorkspaceModuleState = useWorkspaceModuleStore(
      useCallback((s) => projectId ? s.byProjectId[projectId] ?? null : null, [projectId]),
    );
    const [canvasOpenBusyKey, setCanvasOpenBusyKey] = useState<string | null>(null);
    const [canvasOpenError, setCanvasOpenError] = useState<string | null>(null);

    const activeCanvasMountIds = useMemo(
      () => activeCanvasMountIdsFromRuntimeSurface(runtimeData.runtimeSurface),
      [runtimeData.runtimeSurface],
    );
    const runtimeCanvasSurfaceReady = runtimeData.runtimeStatus === "ready";

    const tabLayoutOptions: WorkspaceTabLayoutOptions = useMemo(() => ({
      tabTypes: registrySnapshot.map((type) => ({
        typeId: type.typeId,
        label: type.label,
        allowMultiple: type.allowMultiple,
        pinned: type.pinned,
        defaultUri: type.defaultUri ?? type.buildUri({}),
        canCreateUri: type.typeId === "canvas"
          ? (uri) => canvasMountIdFromPresentationUri(uri) !== null
          : type.canCreateUri,
      })),
      resolveTitle: (typeId, uri) => {
        const type = registrySnapshot.find((descriptor) => descriptor.typeId === typeId);
        return type?.resolveTitle(uri) ?? uri;
      },
    }), [registrySnapshot]);

    // 首次挂载或 session 切换时初始化 Tab 状态
    useEffect(() => {
      if (storeSessionId !== sessionId) {
        useWorkspaceTabStore.getState().initialize(sessionId, null, tabLayoutOptions);
      }
    }, [sessionId, storeSessionId, tabLayoutOptions]);

    useEffect(() => {
      if (!runtimeCanvasSurfaceReady || storeSessionId !== sessionId) return;
      useWorkspaceTabStore.getState().pruneInvalidTabs(tabLayoutOptions);
    }, [runtimeCanvasSurfaceReady, sessionId, storeSessionId, tabLayoutOptions]);

    const workspaceModuleState = useMemo(() => {
      if (storedWorkspaceModuleState) return storedWorkspaceModuleState;
      if (!projectId) return idleProjectWorkspaceModulesState();
      return {
        project_id: projectId,
        status: "idle" as const,
        modules: [],
        error: null,
      };
    }, [projectId, storedWorkspaceModuleState]);

    useEffect(() => {
      if (!projectId) return;
      if (workspaceModuleState.status !== "idle") return;
      void fetchWorkspaceModules(projectId);
    }, [fetchWorkspaceModules, projectId, workspaceModuleState.status]);

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
      openTab: (typeId: string, uri?: string, options?: { refreshContent?: boolean }) => {
        const s = useWorkspaceTabStore.getState();
        let tabId = "";
        if (uri) {
          tabId = s.openOrActivate(typeId, uri, tabLayoutOptions);
        } else {
          const type = tabTypeRegistry.getType(typeId);
          if (type) {
            const defaultUri = type.defaultUri ?? type.buildUri({});
            tabId = s.openOrActivate(typeId, defaultUri, tabLayoutOptions);
          }
        }
        if (tabId && options?.refreshContent) {
          useWorkspaceTabStore.getState().refreshTab(tabId);
        }
      },
    }), [tabLayoutOptions]);

    const handleAddTab = useCallback((typeId: string) => {
      useWorkspaceTabStore.getState().addTab(typeId, undefined, true, tabLayoutOptions);
    }, [tabLayoutOptions]);

    const canvasOptions = useMemo(
      () => runtimeCanvasSurfaceReady
        ? selectCanvasModuleOpenOptions(workspaceModuleState.modules, activeCanvasMountIds)
        : [],
      [activeCanvasMountIds, runtimeCanvasSurfaceReady, workspaceModuleState.modules],
    );

    const handleOpenCanvasModule = useCallback(async (option: CanvasModuleOpenOption) => {
      const busyKey = `${option.module_id}:${option.view_key}`;
      setCanvasOpenBusyKey(busyKey);
      setCanvasOpenError(null);
      try {
        await openUserCanvasModule({
          runtimeSessionId: sessionId,
          option,
          openOrActivate: (typeId, uri, refreshContent) => {
            const tabId = useWorkspaceTabStore
              .getState()
              .openOrActivate(typeId, uri, tabLayoutOptions);
            if (tabId && refreshContent) {
              useWorkspaceTabStore.getState().refreshTab(tabId);
            }
          },
        });
        onWorkspaceModuleOpened?.();
        return true;
      } catch (error: unknown) {
        setCanvasOpenError(error instanceof Error ? error.message : "Canvas 打开失败。");
        return false;
      } finally {
        setCanvasOpenBusyKey(null);
      }
    }, [
      onWorkspaceModuleOpened,
      sessionId,
      tabLayoutOptions,
    ]);

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

    const workspaceData: WorkspaceData = useMemo(() => {
      const agentRef = runtimeData.lifecycleAgent?.agent_ref ?? null;
      return {
        ...runtimeData,
        agentRunCanvasBridgeBase: agentRef && runtimeData.projectId
          ? {
              run_id: agentRef.run_id,
              agent_id: agentRef.agent_id,
              project_id: runtimeData.projectId,
            }
          : null,
        refreshAgentRunWorkspace: onWorkspaceModuleOpened
          ? async () => {
              onWorkspaceModuleOpened();
            }
          : null,
      };
    }, [onWorkspaceModuleOpened, runtimeData]);

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
        refreshRevision: activeTab.refreshRevision,
      });
    }, [activeTab, registrySnapshot, sessionId]);

    return (
      <WorkspaceDataProvider value={workspaceData}>
        <div className="flex h-full flex-col bg-background">
          <TabBar
            tabs={tabs}
            tabTypes={registrySnapshot}
            activeTabId={activeTabId}
            onActivate={handleActivate}
            onClose={handleClose}
            onReorder={handleReorder}
            onAddTab={handleAddTab}
            canvasOptions={canvasOptions}
            canvasOptionsStatus={workspaceModuleState.status}
            canvasOpenBusyKey={canvasOpenBusyKey}
            canvasOpenError={canvasOpenError}
            onOpenCanvasModule={handleOpenCanvasModule}
          />
          <AddressBar tab={activeTab} tabTypes={registrySnapshot} />
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
