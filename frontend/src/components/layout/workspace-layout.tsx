import type { ReactNode } from "react";
import { ThemeToggle } from "../ui/theme-toggle";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import { useEventStore } from "../../stores/eventStore";

interface WorkspaceLayoutProps {
  children: ReactNode;
}

export function WorkspaceLayout({ children }: WorkspaceLayoutProps) {
  const { backends, currentBackendId, selectBackend } = useCoordinatorStore();
  const { connected } = useEventStore();

  return (
    <div className="flex h-screen w-full overflow-hidden bg-background">
      <aside className="flex h-full w-64 flex-col border-r border-border bg-card">
        <div className="border-b border-border px-4 py-3">
          <h1 className="text-lg font-semibold tracking-tight text-foreground">AgentDashboard</h1>
          <p className="mt-1 text-xs text-muted-foreground">{connected ? "事件流已连接" : "事件流未连接"}</p>
        </div>

        <nav className="flex-1 space-y-1 overflow-y-auto p-3">
          <p className="px-2 text-xs uppercase tracking-wider text-muted-foreground">后端连接</p>
          {backends.length === 0 && <p className="px-2 py-3 text-sm text-muted-foreground">暂无后端配置</p>}
          {backends.map((backend) => (
            <button
              key={backend.id}
              type="button"
              onClick={() => selectBackend(backend.id)}
              className={`w-full rounded-md px-3 py-2 text-left text-sm transition-colors ${
                currentBackendId === backend.id
                  ? "bg-primary text-primary-foreground"
                  : "text-foreground hover:bg-secondary"
              }`}
            >
              <p className="truncate font-medium">{backend.name}</p>
              <p className="truncate text-xs opacity-75">{backend.endpoint}</p>
            </button>
          ))}
        </nav>

        <div className="border-t border-border p-3">
          <ThemeToggle />
        </div>
      </aside>

      <main className="flex-1 overflow-hidden">{children}</main>
    </div>
  );
}
