import { type ReactNode, useEffect } from "react";
import { ThemeToggle } from "../ui/theme-toggle";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import { useEventStore } from "../../stores/eventStore";
import { useSessionHistoryStore } from "../../stores/sessionHistoryStore";
import { ProjectSelector } from "../../features/project/project-selector";

export type WorkspaceView = "dashboard" | "session";

interface WorkspaceLayoutProps {
  children: ReactNode;
  activeView: WorkspaceView;
  onChangeView: (view: WorkspaceView) => void;
}

export function WorkspaceLayout({ children, activeView, onChangeView }: WorkspaceLayoutProps) {
  const { projects, currentProjectId, selectProject } = useProjectStore();
  const { fetchWorkspaces } = useWorkspaceStore();
  const { backends } = useCoordinatorStore();
  const { connectionState } = useEventStore();
  const { sessions: sessionHistory } = useSessionHistoryStore();

  useEffect(() => {
    if (currentProjectId) {
      void fetchWorkspaces(currentProjectId);
    }
  }, [currentProjectId, fetchWorkspaces]);

  const streamStatusLabel =
    connectionState === "connected"
      ? "事件流已连接"
      : connectionState === "reconnecting"
        ? "事件流重连中…"
        : connectionState === "connecting"
          ? "事件流连接中…"
          : "事件流未连接";

  const formatTime = (timestamp: number) => {
    const date = new Date(timestamp);
    return `${date.getMonth() + 1}/${date.getDate()} ${date.getHours().toString().padStart(2, "0")}:${date.getMinutes().toString().padStart(2, "0")}`;
  };

  return (
    <div className="flex h-screen w-full overflow-hidden bg-background">
      <aside className="flex h-full w-64 flex-col border-r border-border bg-card">
        {/* 头部 */}
        <div className="border-b border-border px-4 py-3">
          <h1 className="text-lg font-semibold tracking-tight text-foreground">AgentDash</h1>
          <p className="mt-1 text-xs text-muted-foreground">{streamStatusLabel}</p>
        </div>

        <nav className="flex-1 space-y-4 overflow-y-auto p-3">
          {/* 导航 */}
          <div>
            <p className="px-2 text-xs uppercase tracking-wider text-muted-foreground">导航</p>
            <div className="mt-1 space-y-1">
              <button
                type="button"
                onClick={() => onChangeView("dashboard")}
                className={`w-full rounded-md px-3 py-2 text-left text-sm transition-colors ${
                  activeView === "dashboard"
                    ? "bg-secondary text-foreground"
                    : "text-foreground hover:bg-secondary/70"
                }`}
              >
                看板
              </button>
              <button
                type="button"
                onClick={() => onChangeView("session")}
                className={`w-full rounded-md px-3 py-2 text-left text-sm transition-colors ${
                  activeView === "session"
                    ? "bg-secondary text-foreground"
                    : "text-foreground hover:bg-secondary/70"
                }`}
              >
                会话
              </button>
            </div>
          </div>

          {/* 看板模式：项目选择 */}
          {activeView === "dashboard" && (
            <ProjectSelector
              projects={projects}
              currentProjectId={currentProjectId}
              onSelect={selectProject}
            />
          )}

          {/* 会话模式：历史会话列表 */}
          {activeView === "session" && (
            <div>
              <p className="px-2 text-xs uppercase tracking-wider text-muted-foreground">历史会话</p>
              {sessionHistory.length === 0 ? (
                <p className="px-2 py-2 text-sm text-muted-foreground">暂无历史会话</p>
              ) : (
                <div className="mt-1 space-y-1">
                  {sessionHistory.map((session) => (
                    <div
                      key={session.id}
                      className="rounded-md px-3 py-2 text-sm hover:bg-secondary/50 cursor-pointer"
                      title={session.preview}
                    >
                      <p className="truncate font-medium text-foreground">{session.title}</p>
                      <p className="truncate text-xs text-muted-foreground">{session.preview}</p>
                      <p className="mt-1 text-xs text-muted-foreground">{formatTime(session.timestamp)}</p>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </nav>

        {/* 后端连接 - 移到底部单独一栏 */}
        <div className="border-t border-border p-3">
          <p className="px-2 text-xs uppercase tracking-wider text-muted-foreground">后端连接</p>
          {backends.length === 0 && <p className="px-2 py-2 text-sm text-muted-foreground">暂无后端</p>}
          <div className="mt-1 space-y-1">
            {backends.map((backend) => (
              <div
                key={backend.id}
                className="rounded-md px-3 py-2 text-sm"
              >
                <p className="truncate font-medium text-foreground">{backend.name}</p>
                <p className="truncate text-xs text-muted-foreground">{backend.endpoint}</p>
              </div>
            ))}
          </div>
        </div>

        <div className="border-t border-border p-3">
          <ThemeToggle />
        </div>
      </aside>

      <main className="flex-1 overflow-hidden">{children}</main>
    </div>
  );
}
