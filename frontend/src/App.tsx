import { useEffect } from "react";
import { WorkspaceLayout } from "./components/layout/workspace-layout";
import { DashboardPage } from "./pages/DashboardPage";
import { useCoordinatorStore } from "./stores/coordinatorStore";
import { useEventStore } from "./stores/eventStore";

function App() {
  const { fetchBackends } = useCoordinatorStore();
  const { connect } = useEventStore();

  useEffect(() => {
    void fetchBackends();
    connect();
  }, [fetchBackends, connect]);

  return (
    <WorkspaceLayout>
      <DashboardPage />
    </WorkspaceLayout>
  );
}

export default App;
