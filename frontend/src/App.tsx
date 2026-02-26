import { useEffect, useState } from "react";
import { WorkspaceLayout } from "./components/layout/workspace-layout";
import { DashboardPage } from "./pages/DashboardPage";
import { SessionPage } from "./pages/SessionPage";
import { useCoordinatorStore } from "./stores/coordinatorStore";
import { useEventStore } from "./stores/eventStore";

function App() {
  const { fetchBackends } = useCoordinatorStore();
  const { connect } = useEventStore();
  const [activeView, setActiveView] = useState<"dashboard" | "session">("dashboard");

  useEffect(() => {
    void fetchBackends();
    connect();
  }, [fetchBackends, connect]);

  return (
    <WorkspaceLayout activeView={activeView} onChangeView={setActiveView}>
      {activeView === "dashboard" ? <DashboardPage /> : <SessionPage />}
    </WorkspaceLayout>
  );
}

export default App;
