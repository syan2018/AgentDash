import { useCoordinatorStore } from '../../stores/coordinatorStore';
import { useEventStore } from '../../stores/eventStore';

export function Sidebar() {
  const { backends, currentBackendId, selectBackend } = useCoordinatorStore();
  const { connected } = useEventStore();

  return (
    <aside className="w-64 h-screen bg-sidebar text-white flex flex-col">
      <div className="p-4 border-b border-white/10">
        <h1 className="text-lg font-bold tracking-tight">AgentDash</h1>
        <div className="mt-1 flex items-center gap-2 text-xs text-white/60">
          <span
            className={`inline-block w-2 h-2 rounded-full ${
              connected ? 'bg-success' : 'bg-danger'
            }`}
          />
          {connected ? '已连接' : '未连接'}
        </div>
      </div>

      <nav className="flex-1 overflow-y-auto p-2">
        <div className="px-2 py-1 text-xs font-semibold text-white/40 uppercase tracking-wider">
          后端列表
        </div>
        {backends.length === 0 && (
          <p className="px-3 py-2 text-sm text-white/40">暂无后端连接</p>
        )}
        {backends.map((b) => (
          <button
            key={b.id}
            onClick={() => selectBackend(b.id)}
            className={`w-full text-left px-3 py-2 rounded-md text-sm transition-colors ${
              currentBackendId === b.id
                ? 'bg-primary text-white'
                : 'text-white/70 hover:bg-sidebar-hover'
            }`}
          >
            <div className="font-medium">{b.name}</div>
            <div className="text-xs opacity-60 truncate">{b.endpoint}</div>
          </button>
        ))}
      </nav>

      <div className="p-3 border-t border-white/10">
        <button className="w-full px-3 py-2 bg-primary hover:bg-primary-hover text-white text-sm rounded-md transition-colors">
          + 添加后端
        </button>
      </div>
    </aside>
  );
}
