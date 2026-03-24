import { useEffect, useState } from "react";
import { NavLink, Outlet, useMatch, type NavLinkRenderProps } from "react-router-dom";
import { ThemeToggle } from "../ui/theme-toggle";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import { useEventStore } from "../../stores/eventStore";
import { ProjectSelector } from "../../features/project/project-selector";

// WorkspaceView 类型已废弃，改为 React Router NavLink 驱动导航

export function WorkspaceLayout() {
  const { projects, currentProjectId, selectProject } = useProjectStore();
  const { fetchWorkspaces } = useWorkspaceStore();
  const { backends } = useCoordinatorStore();
  const { connectionState } = useEventStore();

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

  // /session/:id 和 /story/:storyId 页面下，对应 Tab 应保持高亮
  // 使用 useMatch 而非 NavLink 的 isActive，以支持跨路由的高亮继承
  // 这些 route match 必须始终逐个调用，不能放进 || / && 短路表达式里，
  // 否则不同路由下会改变 Hook 调用顺序，直接触发 React Hook 规则错误。
  const agentDashboardMatch = useMatch("/dashboard/agent");
  const sessionRouteMatch = useMatch("/session/:sessionId");
  const storyDashboardMatch = useMatch("/dashboard/story");
  const storyRouteMatch = useMatch("/story/:storyId");

  const isAgentActive =
    !!agentDashboardMatch ||
    !!sessionRouteMatch;     // 全屏 Session 从 Agent Tab 发起，高亮 Agent
  const isStoryActive =
    !!storyDashboardMatch ||
    !!storyRouteMatch;       // Story 详情页从 Story Tab 进入，高亮 Story

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

        <nav className="flex flex-1 flex-col gap-0 overflow-y-auto">
          {/* 项目选择器：全局上下文 */}
          <div className="p-3">
            <ProjectSelector
              projects={projects}
              currentProjectId={currentProjectId}
              onSelect={selectProject}
            />
          </div>

          {/* Tab 导航：竖排列表项，对齐项目选择器的视觉语言 */}
          <div className="px-3">
            <div className="space-y-1.5 rounded-[12px] border border-border bg-secondary/35 p-2.5">
              <p className="px-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground">视图</p>
              <NavLink
                to="/dashboard/agent"
                className={() =>
                  `flex w-full items-center gap-2.5 rounded-[10px] border px-3 py-2.5 text-sm transition-colors ${
                    isAgentActive
                      ? "border-primary/20 bg-background font-medium text-foreground"
                      : "border-transparent text-muted-foreground hover:border-border hover:bg-background/80 hover:text-foreground"
                  }`
                }
              >
                Agent
              </NavLink>
              <NavLink
                to="/dashboard/story"
                className={() =>
                  `flex w-full items-center gap-2.5 rounded-[10px] border px-3 py-2.5 text-sm transition-colors ${
                    isStoryActive
                      ? "border-primary/20 bg-background font-medium text-foreground"
                      : "border-transparent text-muted-foreground hover:border-border hover:bg-background/80 hover:text-foreground"
                  }`
                }
              >
                Story
              </NavLink>
            </div>
          </div>
        </nav>

        {/* 后端连接状态 */}
        <div className="border-t border-border p-3">
          <BackendConnectionPanel backends={backends} />
        </div>

        {/* 底部：设置 + 主题切换 */}
        <div className="border-t border-border p-3">
          <NavLink
            to="/settings"
            className={({ isActive }: NavLinkRenderProps) =>
              `mb-2 flex w-full items-center gap-2 rounded-[10px] border px-3 py-2.5 text-sm transition-colors ${
                isActive
                  ? "border-border bg-background text-foreground"
                  : "border-transparent text-muted-foreground hover:border-border hover:bg-background/80 hover:text-foreground"
              }`
            }
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>
            设置
          </NavLink>
          <ThemeToggle />
        </div>
      </aside>

      {/* 主内容区，由 React Router Outlet 填充子路由 */}
      <main className="flex-1 overflow-hidden">
        <Outlet />
      </main>
    </div>
  );
}

// ─── 后端连接面板 ──────────────────────────────────────────

function BackendConnectionPanel({ backends }: { backends: import("../../types").BackendConfig[] }) {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const toggle = (id: string) => setExpandedId((prev) => (prev === id ? null : id));

  return (
    <div className="rounded-[12px] border border-border bg-secondary/35 p-2.5">
      <p className="px-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground">后端连接</p>
      {backends.length === 0 && (
        <p className="mt-2 rounded-[10px] border border-dashed border-border px-3 py-3 text-sm text-muted-foreground">
          暂无后端
        </p>
      )}
      <div className="mt-2 space-y-1.5">
        {backends.map((backend) => {
          const isExpanded = expandedId === backend.id;
          const executors = backend.capabilities?.executors ?? [];
          const availableCount = executors.filter((e) => e.available).length;
          const roots = backend.accessible_roots ?? [];

          return (
            <div key={backend.id} className="rounded-[10px] border border-transparent bg-background/80 text-sm">
              <button
                type="button"
                className="flex w-full items-center gap-2 px-3 py-2.5 text-left"
                onClick={() => toggle(backend.id)}
              >
                <span
                  className={`inline-block h-2 w-2 shrink-0 rounded-full ${backend.online ? "bg-emerald-500" : "bg-muted-foreground/30"}`}
                />
                <span className="min-w-0 flex-1 truncate font-medium text-foreground">{backend.name}</span>
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="12"
                  height="12"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  className={`shrink-0 text-muted-foreground transition-transform ${isExpanded ? "rotate-180" : ""}`}
                >
                  <path d="m6 9 6 6 6-6" />
                </svg>
              </button>

              {!isExpanded && (
                <p className="truncate px-3 pb-2 pl-7 text-xs text-muted-foreground">
                  {backend.online
                    ? `${availableCount} 个执行器可用`
                    : backend.backend_type === "local"
                      ? "本机"
                      : "远程"}
                </p>
              )}

              {isExpanded && (
                <div className="space-y-2 border-t border-border/50 px-3 pb-3 pt-2">
                  {backend.online && executors.length > 0 && (
                    <div>
                      <p className="text-[10px] uppercase tracking-wider text-muted-foreground">执行器</p>
                      <div className="mt-1 flex flex-wrap gap-1">
                        {executors.map((ex) => (
                          <span
                            key={ex.id}
                            className={`inline-block rounded-[6px] border px-1.5 py-0.5 text-[11px] ${
                              ex.available
                                ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                                : "border-border bg-muted/50 text-muted-foreground"
                            }`}
                          >
                            {ex.name}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}

                  {roots.length > 0 && (
                    <div>
                      <p className="text-[10px] uppercase tracking-wider text-muted-foreground">可访问路径</p>
                      <div className="mt-1 space-y-0.5">
                        {roots.map((root) => (
                          <p key={root} className="truncate text-[11px] text-muted-foreground" title={root}>
                            {root.replace(/^\\\\\?\\/, "")}
                          </p>
                        ))}
                      </div>
                    </div>
                  )}

                  <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
                    <span>{backend.backend_type === "local" ? "本机" : "远程"}</span>
                    <span>·</span>
                    <span>{backend.online ? "在线" : "离线"}</span>
                    <span>·</span>
                    <span className="truncate font-mono">{backend.id}</span>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
