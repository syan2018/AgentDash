import { Suspense, lazy, useEffect } from "react";
import { BrowserRouter, Routes, Route, Navigate, useParams, useSearchParams } from "react-router-dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { Button, StatusScreen } from "@agentdash/ui";
import { queryClient } from "./api/queryClient";
import { AppErrorBoundary } from "./components/error/AppErrorBoundary";
import { WorkspaceLayout } from "./components/layout/workspace-layout";
import { useProjectStore } from "./stores/projectStore";
import { useCoordinatorStore } from "./stores/coordinatorStore";
import { subscribeProjectEvents, useEventStore } from "./stores/eventStore";
import { useCurrentUserStore } from "./stores/currentUserStore";
import { useAuthStore } from "./stores/authStore";
import { useStoryStore } from "./stores/storyStore";
import { getStoredToken, clearStoredToken, type ApiHttpError } from "./api/client";
import { LoginPage } from "./pages/LoginPage";
import { ensureDesktopLocalRuntimeStarted, getDesktopLocalRuntimeClient } from "./desktop/localRuntimeBridge";

// ─── 懒加载页面组件 ────────────────────────────────────

const DashboardPage = lazy(async () => {
  const module = await import("./pages/DashboardPage");
  return { default: module.DashboardPage };
});

const StoryPage = lazy(async () => {
  const module = await import("./pages/StoryPage");
  return { default: module.StoryPage };
});

const AgentRunWorkspacePage = lazy(async () => {
  const module = await import("./pages/AgentRunWorkspacePage");
  return { default: module.AgentRunWorkspacePage };
});

const LifecycleRunPage = lazy(async () => {
  const module = await import("./pages/LifecyclePages");
  return { default: module.LifecycleRunPage };
});

const SubjectExecutionPage = lazy(async () => {
  const module = await import("./pages/LifecyclePages");
  return { default: module.SubjectExecutionPage };
});

const LifecycleAgentPage = lazy(async () => {
  const module = await import("./pages/LifecyclePages");
  return { default: module.LifecycleAgentPage };
});

const SettingsPage = lazy(async () => {
  const module = await import("./pages/SettingsPage");
  return { default: module.SettingsPage };
});

const ProjectSettingsPage = lazy(async () => {
  const module = await import("./pages/ProjectSettingsPage");
  return { default: module.ProjectSettingsPage };
});

const DesignSystemPage = lazy(async () => {
  const module = await import("./pages/DesignSystemPage");
  return { default: module.DesignSystemPage };
});

const AgentTabView = lazy(async () => {
  const m = await import("./features/agent/agent-tab-view");
  return { default: m.AgentTabView };
});

const StoryTabView = lazy(async () => {
  const m = await import("./features/story/story-tab-view");
  return { default: m.StoryTabView };
});

const RoutineTabView = lazy(async () => {
  const m = await import("./features/routine/routine-tab-view");
  return { default: m.RoutineTabView };
});

// Assets 页：Workflow / Canvas / MCP Preset / Skill 四类项目级可复用资产的统一入口。
// 顶层壳 AssetsTabView + 四个类目 Panel 通过嵌套路由切换。
const AssetsTabView = lazy(async () => {
  const m = await import("./features/assets-panel");
  return { default: m.AssetsTabView };
});

const AssetsWorkflowPanel = lazy(async () => {
  const m = await import("./features/assets-panel");
  return { default: m.WorkflowCategoryPanel };
});

const AssetsCanvasPanel = lazy(async () => {
  const m = await import("./features/assets-panel");
  return { default: m.CanvasCategoryPanel };
});

const AssetsMcpPresetPanel = lazy(async () => {
  const m = await import("./features/assets-panel");
  return { default: m.McpPresetCategoryPanel };
});

const AssetsMarketplacePanel = lazy(async () => {
  const m = await import("./features/assets-panel");
  return { default: m.MarketplaceCategoryPanel };
});

const AssetsSkillPanel = lazy(async () => {
  const m = await import("./features/assets-panel");
  return { default: m.SkillCategoryPanel };
});

const AssetsVfsMountPanel = lazy(async () => {
  const m = await import("./features/assets-panel");
  return { default: m.VfsMountCategoryPanel };
});

const AssetsExtensionPanel = lazy(async () => {
  const m = await import("./features/assets-panel");
  return { default: m.ExtensionCategoryPanel };
});

// 统一 Workflow 编辑器（自适应 Form / DAG 布局）
const LifecycleEditorShellPage = lazy(async () => {
  const m = await import("./pages/LifecycleEditorShellPage");
  return { default: m.LifecycleEditorShellPage };
});

// ─── 通用加载占位 ──────────────────────────────────────

function RouteFallback() {
  return <StatusScreen tone="loading" title="正在加载页面…" />;
}

function BootstrapErrorState({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <StatusScreen
      tone="danger"
      title="无法完成身份初始化"
      description={message}
      action={
        <Button variant="secondary" onClick={onRetry}>
          重新加载
        </Button>
      }
    />
  );
}

// ─── /agent-runs/:runId/:agentId → AgentRun workspace route ──

function AgentRunWorkspaceRouteWrapper() {
  const { runId, agentId } = useParams<{ runId: string; agentId: string }>();
  return <AgentRunWorkspacePage runId={runId} agentId={agentId} />;
}

function DraftAgentRunRouteWrapper() {
  const [searchParams] = useSearchParams();
  return (
    <AgentRunWorkspacePage
      draftProjectId={searchParams.get("project_id") ?? undefined}
      draftProjectAgentId={searchParams.get("project_agent_id") ?? undefined}
    />
  );
}

// ─── 认证守卫 ──────────────────────────────────────────
//
// 职责链：
//   1. 拉 LoginMetadata → 判断是否需要登录
//   2. 需要登录 + 无 token → 展示 LoginPage（登录成功后 authStore 设 token + currentUser）
//   3. 有 token（或不需要登录）→ 调 /api/me 获取当前用户
//   4. currentUser 就绪后放行 children
//
// 原则：fetchCurrentUser 只在此处触发一次；AppContent 不再重复调用。

function AuthGate({ children }: { children: React.ReactNode }) {
  const { metadata, isMetadataLoading, fetchMetadata } = useAuthStore();
  const {
    currentUser,
    isLoading: isLoadingCurrentUser,
    hasLoaded: hasLoadedCurrentUser,
    error: currentUserError,
    fetchCurrentUser,
  } = useCurrentUserStore();

  // Step 1: 获取 metadata
  useEffect(() => {
    if (!metadata && !isMetadataLoading) {
      fetchMetadata();
    }
  }, [metadata, isMetadataLoading, fetchMetadata]);

  // Step 2: metadata 就绪 + token 可用（或无需登录）→ 获取用户身份
  const needsLogin = metadata?.requires_login ?? false;
  const hasToken = !!getStoredToken();

  useEffect(() => {
    if (!metadata || isMetadataLoading) return;
    if (needsLogin && !hasToken) return;
    if (hasLoadedCurrentUser || isLoadingCurrentUser) return;

    fetchCurrentUser().catch((err: unknown) => {
      if ((err as ApiHttpError).status === 401 && needsLogin) {
        clearStoredToken();
      }
    });
  }, [metadata, isMetadataLoading, needsLogin, hasToken, hasLoadedCurrentUser, isLoadingCurrentUser, fetchCurrentUser]);

  useEffect(() => {
    if (!currentUser) return;
    if (!getDesktopLocalRuntimeClient()) return;
    ensureDesktopLocalRuntimeStarted(
      getStoredToken() ?? "",
      { currentUserAvailable: true },
    )
      .catch(() => undefined);
  }, [currentUser]);

  // ── 渲染状态机 ──

  if (isMetadataLoading || !metadata) {
    return <RouteFallback />;
  }

  if (needsLogin && !hasToken && !currentUser) {
    return <LoginPage />;
  }

  if (!hasLoadedCurrentUser || isLoadingCurrentUser) {
    return <RouteFallback />;
  }

  if (!currentUser && hasLoadedCurrentUser && currentUserError) {
    const is401 = currentUserError.includes("401") || currentUserError.includes("认证");
    if (needsLogin && is401) {
      clearStoredToken();
      return <LoginPage />;
    }
    return (
      <BootstrapErrorState
        message={currentUserError}
        onRetry={() => void fetchCurrentUser()}
      />
    );
  }

  if (!currentUser) {
    return (
      <BootstrapErrorState
        message={currentUserError ?? "当前服务未返回有效用户身份"}
        onRetry={() => void fetchCurrentUser()}
      />
    );
  }

  return <>{children}</>;
}

// ─── 应用主路由结构 ────────────────────────────────────

function AppContent() {
  const { fetchProjects, currentProjectId } = useProjectStore();
  const { fetchBackends, fetchBackendRuntimeSummaries } = useCoordinatorStore();
  const { connect, disconnect } = useEventStore();
  const handleStateChange = useStoryStore((state) => state.handleStateChange);

  useEffect(() => {
    void Promise.allSettled([fetchBackends(), fetchBackendRuntimeSummaries(), fetchProjects()]);
  }, [fetchBackends, fetchBackendRuntimeSummaries, fetchProjects]);

  useEffect(() => {
    let backendRefreshTimer: ReturnType<typeof setTimeout> | null = null;
    const scheduleBackendRefresh = () => {
      if (backendRefreshTimer) return;
      backendRefreshTimer = setTimeout(() => {
        backendRefreshTimer = null;
        void fetchBackends();
      }, 100);
    };
    const unsubscribe = subscribeProjectEvents((event) => {
      switch (event.type) {
        case "Connected":
        case "BackendRuntimeChanged":
          scheduleBackendRefresh();
          void fetchBackendRuntimeSummaries();
          break;
        case "StateChanged":
          handleStateChange(event.data);
          break;
        case "Heartbeat":
          break;
      }
    });
    return () => {
      unsubscribe();
      if (backendRefreshTimer) {
        clearTimeout(backendRefreshTimer);
      }
    };
  }, [fetchBackends, fetchBackendRuntimeSummaries, handleStateChange]);

  useEffect(() => {
    if (!currentProjectId) {
      disconnect();
      return;
    }
    connect(currentProjectId);
  }, [connect, currentProjectId, disconnect]);

  return (
    <Suspense fallback={<RouteFallback />}>
      <Routes>
        {/* 设计语言预览页：独立路由，不挂业务壳；任务 05-19-frontend-design-language */}
        <Route path="/dev/design-system" element={<DesignSystemPage />} />

        <Route element={<WorkspaceLayout />}>
          <Route index element={<Navigate to="/dashboard/agent" replace />} />

          <Route path="/dashboard" element={<DashboardPage />}>
            <Route index element={<Navigate to="agent" replace />} />
            <Route path="agent" element={<AgentTabView />} />
            <Route path="story" element={<StoryTabView />} />
            {/* Assets 页：Workflow / Canvas / MCP Preset / Skill 四类项目级可复用资产统一入口 */}
            <Route path="assets" element={<AssetsTabView />}>
              <Route index element={<Navigate to="workflow" replace />} />
              <Route path="workflow" element={<AssetsWorkflowPanel />} />
              <Route path="marketplace" element={<AssetsMarketplacePanel />} />
              <Route path="canvas" element={<AssetsCanvasPanel />} />
              <Route path="mcp-preset" element={<AssetsMcpPresetPanel />} />
              <Route path="skill" element={<AssetsSkillPanel />} />
              <Route path="vfs-mount" element={<AssetsVfsMountPanel />} />
              <Route path="extension" element={<AssetsExtensionPanel />} />
            </Route>
            <Route path="routine" element={<RoutineTabView />} />
          </Route>

          <Route path="/story/:storyId" element={<StoryPage />} />

          {/* 统一 Workflow 编辑器路由 */}
          <Route path="/workflow/:id" element={<LifecycleEditorShellPage />} />

          <Route path="/agent-runs/new" element={<DraftAgentRunRouteWrapper />} />
          <Route path="/agent-runs/:runId/:agentId" element={<AgentRunWorkspaceRouteWrapper />} />
          <Route path="/run/:runId" element={<LifecycleRunPage />} />
          <Route path="/subject/:kind/:id" element={<SubjectExecutionPage />} />
          <Route path="/agent/:agentId" element={<LifecycleAgentPage />} />

          <Route path="/settings" element={<SettingsPage />} />
          <Route path="/projects/:projectId/settings" element={<ProjectSettingsPage />} />

          <Route path="*" element={<Navigate to="/dashboard/agent" replace />} />
        </Route>
      </Routes>
    </Suspense>
  );
}

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <AppErrorBoundary>
          <AuthGate>
            <AppContent />
          </AuthGate>
        </AppErrorBoundary>
      </BrowserRouter>
    </QueryClientProvider>
  );
}

export default App;
