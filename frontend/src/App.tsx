import { Suspense, lazy, useEffect } from "react";
import { BrowserRouter, Routes, Route, Navigate, useParams } from "react-router-dom";
import { WorkspaceLayout } from "./components/layout/workspace-layout";
import { useProjectStore } from "./stores/projectStore";
import { useCoordinatorStore } from "./stores/coordinatorStore";
import { useEventStore } from "./stores/eventStore";
import { useCurrentUserStore } from "./stores/currentUserStore";
import { useAuthStore } from "./stores/authStore";
import { getStoredToken, clearStoredToken, type ApiHttpError } from "./api/client";
import { LoginPage } from "./pages/LoginPage";

// ─── 懒加载页面组件 ────────────────────────────────────

const DashboardPage = lazy(async () => {
  const module = await import("./pages/DashboardPage");
  return { default: module.DashboardPage };
});

const StoryPage = lazy(async () => {
  const module = await import("./pages/StoryPage");
  return { default: module.StoryPage };
});

const SessionPage = lazy(async () => {
  const module = await import("./pages/SessionPage");
  return { default: module.SessionPage };
});

const SettingsPage = lazy(async () => {
  const module = await import("./pages/SettingsPage");
  return { default: module.SettingsPage };
});

const ProjectSettingsPage = lazy(async () => {
  const module = await import("./pages/ProjectSettingsPage");
  return { default: module.ProjectSettingsPage };
});

const AgentTabView = lazy(async () => {
  const m = await import("./features/agent/agent-tab-view");
  return { default: m.AgentTabView };
});

const StoryTabView = lazy(async () => {
  const m = await import("./features/story/story-tab-view");
  return { default: m.StoryTabView };
});

const WorkflowTabView = lazy(async () => {
  const m = await import("./features/workflow/workflow-tab-view");
  return { default: m.WorkflowTabView };
});

const WorkflowEditorPage = lazy(async () => {
  const m = await import("./pages/WorkflowEditorPage");
  return { default: m.WorkflowEditorPage };
});

const LifecycleEditorPage = lazy(async () => {
  const m = await import("./pages/LifecycleEditorPage");
  return { default: m.LifecycleEditorPage };
});

// ─── 通用加载占位 ──────────────────────────────────────

function RouteFallback() {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center">
        <div className="mx-auto h-7 w-7 animate-spin rounded-full border-2 border-primary border-t-transparent" />
        <p className="mt-3 text-sm text-muted-foreground">正在加载页面...</p>
      </div>
    </div>
  );
}

function BootstrapErrorState({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div className="flex h-full items-center justify-center bg-background">
      <div className="max-w-md rounded-[16px] border border-destructive/20 bg-destructive/5 p-6 text-center">
        <h2 className="text-lg font-semibold text-foreground">无法完成身份初始化</h2>
        <p className="mt-2 text-sm text-muted-foreground">{message}</p>
        <button
          type="button"
          onClick={onRetry}
          className="mt-4 rounded-[10px] border border-border bg-background px-4 py-2 text-sm text-foreground transition-colors hover:bg-secondary"
        >
          重新加载
        </button>
      </div>
    </div>
  );
}

// ─── /session/:sessionId 路由包装器 ───────────────────

function SessionRouteWrapper() {
  const { sessionId } = useParams<{ sessionId: string }>();
  return <SessionPage sessionId={sessionId} />;
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
  const { fetchBackends } = useCoordinatorStore();
  const { connect, disconnect } = useEventStore();

  useEffect(() => {
    void Promise.allSettled([fetchBackends(), fetchProjects()]);
  }, [fetchBackends, fetchProjects]);

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
        <Route element={<WorkspaceLayout />}>
          <Route index element={<Navigate to="/dashboard/agent" replace />} />

          <Route path="/dashboard" element={<DashboardPage />}>
            <Route index element={<Navigate to="agent" replace />} />
            <Route path="agent" element={<AgentTabView />} />
            <Route path="story" element={<StoryTabView />} />
            <Route path="workflow" element={<WorkflowTabView />} />
          </Route>

          <Route path="/story/:storyId" element={<StoryPage />} />

          <Route path="/workflow-editor/:definitionId" element={<WorkflowEditorPage />} />
          <Route path="/lifecycle-editor/:definitionId" element={<LifecycleEditorPage />} />

          <Route path="/session/:sessionId" element={<SessionRouteWrapper />} />

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
    <BrowserRouter>
      <AuthGate>
        <AppContent />
      </AuthGate>
    </BrowserRouter>
  );
}

export default App;
