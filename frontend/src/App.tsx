import { Suspense, lazy, useEffect } from "react";
import { BrowserRouter, Routes, Route, Navigate, useParams } from "react-router-dom";
import { WorkspaceLayout } from "./components/layout/workspace-layout";
import { useProjectStore } from "./stores/projectStore";
import { useCoordinatorStore } from "./stores/coordinatorStore";
import { useEventStore } from "./stores/eventStore";

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

const AgentTabView = lazy(async () => {
  const m = await import("./features/agent/agent-tab-view");
  return { default: m.AgentTabView };
});

const StoryTabView = lazy(async () => {
  const m = await import("./features/story/story-tab-view");
  return { default: m.StoryTabView };
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

// ─── /session/:sessionId 路由包装器 ───────────────────

function SessionRouteWrapper() {
  const { sessionId } = useParams<{ sessionId: string }>();
  return <SessionPage sessionId={sessionId} />;
}

// ─── 应用主路由结构 ────────────────────────────────────

function AppContent() {
  const { fetchProjects } = useProjectStore();
  const { fetchBackends } = useCoordinatorStore();
  const { connect } = useEventStore();

  // 应用初始化：拉取项目列表、后端列表，建立 SSE 连接
  useEffect(() => {
    void fetchBackends();
    void fetchProjects();
    connect();
  }, [fetchBackends, fetchProjects, connect]);

  return (
    <Suspense fallback={<RouteFallback />}>
      <Routes>
        {/* WorkspaceLayout 作为所有主要页面的 Layout Route */}
        <Route element={<WorkspaceLayout />}>
          {/* 根路径重定向到 Agent Tab */}
          <Route index element={<Navigate to="/dashboard/agent" replace />} />

          {/* Dashboard 容器路由，包含 Agent / Story 子 Tab */}
          <Route path="/dashboard" element={<DashboardPage />}>
            <Route index element={<Navigate to="agent" replace />} />
            <Route path="agent" element={<AgentTabView />} />
            <Route path="story" element={<StoryTabView />} />
          </Route>

          {/* Story 详情页 */}
          <Route path="/story/:storyId" element={<StoryPage />} />

          {/* Session 独立全屏页 */}
          <Route path="/session/:sessionId" element={<SessionRouteWrapper />} />

          {/* 设置页 */}
          <Route path="/settings" element={<SettingsPage />} />

          {/* 未匹配路由重定向到默认 Tab */}
          <Route path="*" element={<Navigate to="/dashboard/agent" replace />} />
        </Route>
      </Routes>
    </Suspense>
  );
}

function App() {
  return (
    <BrowserRouter>
      <AppContent />
    </BrowserRouter>
  );
}

export default App;
