import { type ReactNode, useCallback, useEffect } from "react";
import { useNavigate, useLocation } from "react-router-dom";
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
  const {
    sessions: sessionHistory,
    activeSessionId,
    removeSession,
  } = useSessionHistoryStore();
  const navigate = useNavigate();
  const location = useLocation();

  useEffect(() => {
    if (currentProjectId) {
      void fetchWorkspaces(currentProjectId);
    }
  }, [currentProjectId, fetchWorkspaces]);

  // 根据 URL 同步 activeView
  useEffect(() => {
    if (location.pathname.startsWith("/session")) {
      if (activeView !== "session") onChangeView("session");
    }
  }, [location.pathname, activeView, onChangeView]);

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

  const handleSessionClick = useCallback(
    (id: string) => {
      navigate(`/session/${id}`);
    },
    [navigate],
  );

  const handleDeleteSession = useCallback(
    async (e: React.MouseEvent, id: string) => {
      e.stopPropagation();
      await removeSession(id);
      if (activeSessionId === id) {
        navigate("/session", { replace: true });
      }
    },
    [removeSession, activeSessionId, navigate],
  );

  const handleNewSession = useCallback(() => {
    navigate("/session");
  }, [navigate]);

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
              <div className="flex items-center justify-between px-2">
                <p className="text-xs uppercase tracking-wider text-muted-foreground">历史会话</p>
                <button
                  type="button"
                  onClick={handleNewSession}
                  className="rounded px-1.5 py-0.5 text-xs text-primary hover:bg-secondary"
                >
                  + 新建
                </button>
              </div>
              {sessionHistory.length === 0 ? (
                <p className="px-2 py-2 text-sm text-muted-foreground">暂无历史会话</p>
              ) : (
                <div className="mt-1 space-y-1">
                  {sessionHistory.map((session) => (
                    <div
                      key={session.id}
                      role="button"
                      tabIndex={0}
                      onClick={() => handleSessionClick(session.id)}
                      onKeyDown={(e) => e.key === "Enter" && handleSessionClick(session.id)}
                      className={`group rounded-md px-3 py-2 text-sm cursor-pointer transition-colors ${
                        activeSessionId === session.id
                          ? "bg-secondary text-foreground"
                          : "hover:bg-secondary/50"
                      }`}
                      title={session.title}
                    >
                      <div className="flex items-start justify-between gap-1">
                        <p className="truncate font-medium text-foreground">{session.title}</p>
                        <button
                          type="button"
                          onClick={(e) => void handleDeleteSession(e, session.id)}
                          className="shrink-0 rounded p-0.5 text-muted-foreground opacity-0 transition-opacity hover:text-destructive group-hover:opacity-100"
                          title="删除会话"
                        >
                          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>
                        </button>
                      </div>
                      <p className="mt-1 text-xs text-muted-foreground">
                        {formatTime(session.updatedAt)}
                      </p>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </nav>

        {/* 后端连接 */}
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
