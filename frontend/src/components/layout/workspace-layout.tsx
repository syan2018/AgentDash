import { type ReactNode, useCallback, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { ThemeToggle } from "../ui/theme-toggle";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import { useEventStore } from "../../stores/eventStore";
import { useSessionHistoryStore } from "../../stores/sessionHistoryStore";
import { ProjectSelector } from "../../features/project/project-selector";

export type WorkspaceView = "dashboard" | "session" | "settings";

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

  const navButtonClass = (active: boolean) =>
    `w-full rounded-[10px] border px-3 py-2.5 text-left text-sm transition-colors ${
      active
        ? "border-border bg-background text-foreground"
        : "border-transparent text-foreground hover:border-border hover:bg-background/80"
    }`;

  return (
    <div className="flex h-screen w-full overflow-hidden bg-background">
      <aside className="flex h-full w-72 flex-col border-r border-border bg-background">
        {/* 头部 */}
        <div className="border-b border-border px-4 py-4">
          <div className="flex items-center gap-2">
            <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              APP
            </span>
            <h1 className="text-lg font-semibold tracking-tight text-foreground">AgentDash</h1>
          </div>
          <p className="mt-2 text-xs text-muted-foreground">{streamStatusLabel}</p>
        </div>

        <nav className="flex-1 space-y-4 overflow-y-auto p-3">
          {/* 导航 */}
          <div className="rounded-[12px] border border-border bg-secondary/35 p-2.5">
            <p className="px-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground">导航</p>
            <div className="mt-2 space-y-1.5">
              <button
                type="button"
                onClick={() => onChangeView("dashboard")}
                className={navButtonClass(activeView === "dashboard")}
              >
                看板
              </button>
              <button
                type="button"
                onClick={() => onChangeView("session")}
                className={navButtonClass(activeView === "session")}
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
            <div className="rounded-[12px] border border-border bg-secondary/35 p-2.5">
              <div className="flex items-center justify-between px-1">
                <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">历史会话</p>
                <button
                  type="button"
                  onClick={handleNewSession}
                  className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                >
                  + 新建
                </button>
              </div>
              {sessionHistory.length === 0 ? (
                <p className="mt-2 rounded-[10px] border border-dashed border-border px-3 py-3 text-sm text-muted-foreground">暂无历史会话</p>
              ) : (
                <div className="mt-2 space-y-1.5">
                  {sessionHistory.map((session) => (
                    <div
                      key={session.id}
                      role="button"
                      tabIndex={0}
                      onClick={() => handleSessionClick(session.id)}
                      onKeyDown={(e) => e.key === "Enter" && handleSessionClick(session.id)}
                      className={`group cursor-pointer rounded-[10px] border px-3 py-2.5 text-sm transition-colors ${
                        activeSessionId === session.id
                          ? "border-border bg-background text-foreground"
                          : "border-transparent hover:border-border hover:bg-background/80"
                      }`}
                      title={session.title}
                    >
                      <div className="flex items-start justify-between gap-1">
                        <p className="truncate font-medium text-foreground">{session.title}</p>
                        <button
                          type="button"
                          onClick={(e) => void handleDeleteSession(e, session.id)}
                          className="shrink-0 rounded-[8px] border border-transparent p-1 text-muted-foreground opacity-0 transition-all hover:border-border hover:text-destructive group-hover:opacity-100"
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
          <div className="rounded-[12px] border border-border bg-secondary/35 p-2.5">
          <p className="px-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground">后端连接</p>
          {backends.length === 0 && <p className="mt-2 rounded-[10px] border border-dashed border-border px-3 py-3 text-sm text-muted-foreground">暂无后端</p>}
          <div className="mt-2 space-y-1.5">
            {backends.map((backend) => {
              const executorCount = backend.capabilities?.executors.filter(e => e.available).length ?? 0;
              return (
                <div
                  key={backend.id}
                  className="rounded-[10px] border border-transparent bg-background/80 px-3 py-2.5 text-sm"
                >
                  <div className="flex items-center gap-2">
                    <span
                      className={`inline-block h-2 w-2 shrink-0 rounded-full ${backend.online ? "bg-emerald-500" : "bg-muted-foreground/30"}`}
                      title={backend.online ? "在线" : "离线"}
                    />
                    <p className="truncate font-medium text-foreground">{backend.name}</p>
                  </div>
                  <p className="mt-0.5 truncate pl-4 text-xs text-muted-foreground">
                    {backend.online
                      ? `${executorCount} 个执行器可用`
                      : backend.backend_type === "local" ? "本机" : "远程"}
                  </p>
                </div>
              );
            })}
          </div>
          </div>
        </div>

        <div className="border-t border-border p-3">
          <button
            type="button"
            onClick={() => onChangeView("settings")}
            className={`mb-2 flex w-full items-center gap-2 rounded-[10px] border px-3 py-2.5 text-sm transition-colors ${
              activeView === "settings"
                ? "border-border bg-background text-foreground"
                : "border-transparent text-muted-foreground hover:border-border hover:bg-background/80 hover:text-foreground"
            }`}
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>
            设置
          </button>
          <ThemeToggle />
        </div>
      </aside>

      <main className="flex-1 overflow-hidden">{children}</main>
    </div>
  );
}
