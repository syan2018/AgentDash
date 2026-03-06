import { Suspense, lazy, useCallback, useEffect, useRef } from "react";
import { BrowserRouter, Routes, Route, Navigate, useNavigate, useLocation, useParams } from "react-router-dom";
import { WorkspaceLayout, type WorkspaceView } from "./components/layout/workspace-layout";
import { useProjectStore } from "./stores/projectStore";
import { useCoordinatorStore } from "./stores/coordinatorStore";
import { useEventStore } from "./stores/eventStore";
import { useSessionHistoryStore } from "./stores/sessionHistoryStore";

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

function SessionRouteWrapper() {
  const { sessionId } = useParams<{ sessionId: string }>();
  return <SessionPage sessionId={sessionId} />;
}

function AppContent() {
  const { fetchProjects } = useProjectStore();
  const { fetchBackends } = useCoordinatorStore();
  const { connect } = useEventStore();
  const reloadSessions = useSessionHistoryStore(state => state.reload);
  const navigate = useNavigate();
  const location = useLocation();

  // 用于防止重复加载的 ref
  const hasLoadedSessionsRef = useRef(false);

  const activeView: WorkspaceView = location.pathname.startsWith("/session")
    ? "session"
    : location.pathname.startsWith("/settings")
      ? "settings"
      : "dashboard";

  useEffect(() => {
    void fetchBackends();
    void fetchProjects();
    connect();
  }, [fetchBackends, fetchProjects, connect]);

  // 使用 useCallback 稳定 reloadSessions 调用，避免循环依赖
  const loadSessionsOnce = useCallback(() => {
    if (activeView === "session" && !hasLoadedSessionsRef.current) {
      hasLoadedSessionsRef.current = true;
      void reloadSessions();
    }
  }, [activeView, reloadSessions]);

  useEffect(() => {
    loadSessionsOnce();
  }, [loadSessionsOnce]);

  // 当离开 session 视图时重置标记，允许下次进入时重新加载
  useEffect(() => {
    if (activeView !== "session") {
      hasLoadedSessionsRef.current = false;
    }
  }, [activeView]);

  const handleChangeView = useCallback(
    (view: WorkspaceView) => {
      if (view === "dashboard") {
        navigate("/");
      } else if (view === "settings") {
        navigate("/settings");
      } else {
        navigate("/session");
      }
    },
    [navigate],
  );

  return (
    <WorkspaceLayout activeView={activeView} onChangeView={handleChangeView}>
      <Suspense fallback={<RouteFallback />}>
        <Routes>
          <Route path="/" element={<DashboardPage />} />
          <Route path="/story/:storyId" element={<StoryPage />} />
          <Route path="/session" element={<SessionPage />} />
          <Route path="/session/:sessionId" element={<SessionRouteWrapper />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </Suspense>
    </WorkspaceLayout>
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
