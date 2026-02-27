import { useEffect, useState } from "react";
import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { WorkspaceLayout } from "./components/layout/workspace-layout";
import { DashboardPage } from "./pages/DashboardPage";
import { StoryPage } from "./pages/StoryPage";
import { SessionPage } from "./pages/SessionPage";
import { useProjectStore } from "./stores/projectStore";
import { useCoordinatorStore } from "./stores/coordinatorStore";
import { useEventStore } from "./stores/eventStore";

function AppContent() {
  const { fetchProjects } = useProjectStore();
  const { fetchBackends } = useCoordinatorStore();
  const { connect } = useEventStore();
  const [activeView, setActiveView] = useState<"dashboard" | "session">("dashboard");

  useEffect(() => {
    void fetchBackends();
    void fetchProjects();
    connect();
  }, [fetchBackends, fetchProjects, connect]);

  return (
    <WorkspaceLayout activeView={activeView} onChangeView={setActiveView}>
      {activeView === "dashboard" ? (
        <Routes>
          <Route path="/" element={<DashboardPage />} />
          <Route path="/story/:storyId" element={<StoryPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      ) : (
        <SessionPage />
      )}
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
