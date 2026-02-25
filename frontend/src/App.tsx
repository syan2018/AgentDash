import { useEffect } from 'react';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { AppLayout } from './components/layout/AppLayout';
import { DashboardPage } from './pages/DashboardPage';
import { useCoordinatorStore } from './stores/coordinatorStore';
import { useEventStore } from './stores/eventStore';

function App() {
  const { fetchBackends } = useCoordinatorStore();
  const { connect } = useEventStore();

  useEffect(() => {
    fetchBackends();
    connect();
  }, [fetchBackends, connect]);

  return (
    <BrowserRouter>
      <Routes>
        <Route element={<AppLayout />}>
          <Route path="/" element={<DashboardPage />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

export default App;
