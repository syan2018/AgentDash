import { useCoordinatorStore } from '../../stores/coordinatorStore';

export function Header() {
  const { backends, currentBackendId } = useCoordinatorStore();
  const currentBackend = backends.find((b) => b.id === currentBackendId);

  return (
    <header className="h-14 bg-white border-b border-gray-200 flex items-center px-6 shrink-0">
      <div className="flex items-center gap-3">
        <h2 className="text-lg font-semibold text-gray-800">
          {currentBackend ? currentBackend.name : '看板总览'}
        </h2>
        {currentBackend && (
          <span className="px-2 py-0.5 text-xs rounded-full bg-accent/10 text-accent">
            {currentBackend.backend_type === 'local' ? '本地' : '远程'}
          </span>
        )}
      </div>

      <div className="ml-auto flex items-center gap-2">
        <span className="text-sm text-gray-500">AgentDashboard v0.1.0</span>
      </div>
    </header>
  );
}
