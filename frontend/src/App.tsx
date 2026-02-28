import { useCallback, useEffect, useRef } from "react";
import { BrowserRouter, Routes, Route, Navigate, useNavigate, useLocation, useParams } from "react-router-dom";
import { WorkspaceLayout, type WorkspaceView } from "./components/layout/workspace-layout";
import { DashboardPage } from "./pages/DashboardPage";
import { StoryPage } from "./pages/StoryPage";
import { SessionPage } from "./pages/SessionPage";
import { useProjectStore } from "./stores/projectStore";
import { useCoordinatorStore } from "./stores/coordinatorStore";
import { useEventStore } from "./stores/eventStore";
import { useSessionHistoryStore } from "./stores/sessionHistoryStore";

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
      } else {
        navigate("/session");
      }
    },
    [navigate],
  );

  return (
    <WorkspaceLayout activeView={activeView} onChangeView={handleChangeView}>
      <Routes>
        <Route path="/" element={<DashboardPage />} />
        <Route path="/story/:storyId" element={<StoryPage />} />
        <Route path="/session" element={<SessionPage />} />
        <Route path="/session/:sessionId" element={<SessionRouteWrapper />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
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
