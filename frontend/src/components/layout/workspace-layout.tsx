import React, { useEffect, useMemo, useRef, useState } from "react";
import { NavLink, Outlet, useLocation, useMatch, useNavigate, type NavLinkRenderProps } from "react-router-dom";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import { useEventStore } from "../../stores/eventStore";
import { useCurrentUserStore } from "../../stores/currentUserStore";
import { useActiveSessionsStore } from "../../stores/activeSessionsStore";
import { useTheme } from "../../hooks/use-theme";
import { ProjectCreateDrawer } from "../../features/project/project-selector";
import type { Project, ProjectSessionEntry } from "../../types";

// ─── 视图导航定义 ──────────────────────────────────────────
type NavKey = "agent" | "story" | "assets" | "routine";

interface NavItem {
  key: NavKey;
  label: string;
  defaultPath: string;
  pathPrefixes: string[];
  icon: React.ReactNode;
}

// Icon 统一走 feather-style inline SVG，业务感更强：
//  - Agent  → sparkles（AI / 代理）
//  - Story  → scroll（故事脚本 / 叙事）
//  - Assets → workflow 连线节点（工作流资产）
//  - Routine→ list-checks（日常例行勾选）
const NAV_ITEMS: NavItem[] = [
  {
    key: "agent",
    label: "Agent",
    defaultPath: "/dashboard/agent",
    pathPrefixes: ["/dashboard/agent", "/session/"],
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 3v4" />
        <path d="m8 5 2 2 2-2" opacity="0" />
        <path d="M5 8l2 2" />
        <path d="M17 8l-2 2" />
        <path d="M12 21a7 7 0 0 0 7-7c0-3-2-5-5-6-1-1-1-3 0-5-4 0-9 3-9 8a7 7 0 0 0 7 10Z" />
        <circle cx="12" cy="14" r="1.2" fill="currentColor" />
      </svg>
    ),
  },
  {
    key: "story",
    label: "Story",
    defaultPath: "/dashboard/story",
    pathPrefixes: ["/dashboard/story", "/story/"],
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
        <path d="M8 21h11a2 2 0 0 0 2-2v-1a2 2 0 0 0-2-2H8v5Z" />
        <path d="M8 16V5a2 2 0 1 0-4 0v14a2 2 0 0 0 2 2" />
        <path d="M11 7h6" />
        <path d="M11 11h6" />
      </svg>
    ),
  },
  {
    key: "assets",
    label: "Assets",
    defaultPath: "/dashboard/assets",
    pathPrefixes: ["/dashboard/assets", "/workflow/"],
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
        <rect x="3" y="3" width="6" height="6" rx="1" />
        <rect x="15" y="4" width="6" height="6" rx="1" />
        <rect x="9" y="15" width="6" height="6" rx="1" />
        <path d="M6 9v3a2 2 0 0 0 2 2h4" />
        <path d="M18 10v2a2 2 0 0 1-2 2h-4" />
      </svg>
    ),
  },
  {
    key: "routine",
    label: "Routine",
    defaultPath: "/dashboard/routine",
    pathPrefixes: ["/dashboard/routine"],
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
        <path d="m3 7 3 3 5-5" />
        <path d="m3 14 3 3 5-5" />
        <path d="M14 7h7" />
        <path d="M14 14h7" />
        <path d="M14 21h7" />
      </svg>
    ),
  },
];

// 底栏共享 popover 的 key
type FooterPanelKey = "backend" | "stream" | "user" | "theme";

export function WorkspaceLayout() {
  const location = useLocation();
  const { projects, currentProjectId, selectProject } = useProjectStore();
  const { fetchWorkspaces } = useWorkspaceStore();
  const { backends } = useCoordinatorStore();
  const { connectionState } = useEventStore();
  const { currentUser } = useCurrentUserStore();
  const { sessions, loadForProject, clearForProject } = useActiveSessionsStore();

  const [activeFooterPanel, setActiveFooterPanel] = useState<FooterPanelKey | null>(null);

  const isSettingsRoute = location.pathname === "/settings";
  const rememberedPath = useMemo(() => {
    if (!isSettingsRoute) {
      return `${location.pathname}${location.search}${location.hash}`;
    }
    const state = location.state as { return_to?: string } | null;
    return state?.return_to ?? "/dashboard/agent";
  }, [isSettingsRoute, location.hash, location.pathname, location.search, location.state]);

  useEffect(() => {
    if (currentProjectId) {
      void fetchWorkspaces(currentProjectId);
    }
  }, [currentProjectId, fetchWorkspaces]);

  // 项目切换时同步活跃会话（与 agent-tab-view 共享 store，竞态由 store 兜底）
  const prevProjectIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (!currentProjectId) return;
    if (prevProjectIdRef.current === currentProjectId) return;
    prevProjectIdRef.current = currentProjectId;
    clearForProject(currentProjectId);
    void loadForProject(currentProjectId);
  }, [currentProjectId, clearForProject, loadForProject]);

  // 路由高亮匹配：useMatch 调用顺序必须稳定
  const agentDashboardMatch = useMatch("/dashboard/agent");
  const sessionRouteMatch = useMatch("/session/:sessionId");
  const storyDashboardMatch = useMatch("/dashboard/story");
  const storyRouteMatch = useMatch("/story/:storyId");
  const assetsDashboardMatch = useMatch("/dashboard/assets/*");
  const unifiedWorkflowEditorMatch = useMatch("/workflow/:id");
  const routineDashboardMatch = useMatch("/dashboard/routine");

  const activeMap: Record<NavKey, boolean> = {
    agent: !!agentDashboardMatch || !!sessionRouteMatch,
    story: !!storyDashboardMatch || !!storyRouteMatch,
    assets: !!assetsDashboardMatch || !!unifiedWorkflowEditorMatch,
    routine: !!routineDashboardMatch,
  };

  const navTargets = useMemo<Record<NavKey, string>>(() => {
    const result = {} as Record<NavKey, string>;
    for (const item of NAV_ITEMS) {
      if (!isSettingsRoute) {
        result[item.key] = item.defaultPath;
        continue;
      }
      const matched = item.pathPrefixes.some((p) => rememberedPath.startsWith(p));
      result[item.key] = matched ? rememberedPath : item.defaultPath;
    }
    return result;
  }, [isSettingsRoute, rememberedPath]);

  const toggleFooterPanel = (key: FooterPanelKey) => {
    setActiveFooterPanel((prev) => (prev === key ? null : key));
  };

  return (
    <div className="flex h-screen w-full overflow-hidden bg-background">
      <aside className="relative flex h-full w-72 flex-col border-r border-border bg-background">
        {/* 顶部：项目下拉（置顶替代原 header） */}
        <ProjectDropdown
          projects={projects}
          currentProjectId={currentProjectId}
          onSelect={selectProject}
        />

        {/* 横排 Nav：切换感加强 —— 激活项用 primary 填充 pill */}
        <div className="grid grid-cols-4 gap-1 border-b border-border px-2 py-2">
          {NAV_ITEMS.map((item) => {
            const isActive = activeMap[item.key];
            return (
              <NavLink
                key={item.key}
                to={navTargets[item.key]}
                title={item.label}
                aria-label={item.label}
                className={() =>
                  `flex h-10 items-center justify-center rounded-[10px] transition-all ${
                    isActive
                      ? "bg-primary text-primary-foreground shadow-sm"
                      : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
                  }`
                }
              >
                {item.icon}
              </NavLink>
            );
          })}
        </div>

        {/* Session 快捷列表 */}
        <SessionShortcutList sessions={sessions} />

        {/* 底栏共享 popover 面板 */}
        {activeFooterPanel && (
          <div className="border-t border-border bg-background">
            <FooterPanelContent
              panel={activeFooterPanel}
              onClose={() => setActiveFooterPanel(null)}
              backends={backends}
              connectionState={connectionState}
            />
          </div>
        )}

        {/* 底栏横排按钮 */}
        <FooterIconBar
          activePanel={activeFooterPanel}
          onTogglePanel={toggleFooterPanel}
          backends={backends}
          connectionState={connectionState}
          currentUser={currentUser}
          rememberedPath={rememberedPath}
        />
      </aside>

      <main className="flex-1 overflow-hidden">
        <Outlet />
      </main>
    </div>
  );
}

// ─── 顶部：项目下拉 ───────────────────────────────────────

interface ProjectDropdownProps {
  projects: Project[];
  currentProjectId: string | null;
  onSelect: (id: string) => void;
}

function ProjectDropdown({ projects, currentProjectId, onSelect }: ProjectDropdownProps) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);

  const current = projects.find((p) => p.id === currentProjectId) ?? null;

  // 点击外部关闭
  useEffect(() => {
    if (!open) return;
    const handler = (event: MouseEvent) => {
      if (!wrapperRef.current) return;
      if (!wrapperRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  return (
    <div ref={wrapperRef} className="relative border-b border-border">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-3 py-3 text-left transition-colors hover:bg-secondary/40"
      >
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px] bg-primary/10 text-primary">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
          </svg>
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-semibold text-foreground">
            {current?.name ?? "未选择项目"}
          </p>
          <p className="truncate text-[10px] text-muted-foreground">
            {current ? (current.description || `${projects.length} 个项目可选`) : "请选择项目"}
          </p>
        </div>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={`shrink-0 text-muted-foreground transition-transform ${open ? "rotate-180" : ""}`}
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>

      {open && (
        <div className="absolute left-2 right-2 top-full z-20 mt-1 rounded-[10px] border border-border bg-background shadow-lg">
          <div className="flex items-center justify-between px-3 py-2">
            <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">项目</span>
            <button
              type="button"
              onClick={() => {
                setOpen(false);
                setIsCreateOpen(true);
              }}
              className="rounded-[6px] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            >
              + 新建
            </button>
          </div>

          <div className="max-h-80 overflow-y-auto border-t border-border py-1">
            {projects.length === 0 && (
              <p className="px-3 py-3 text-xs text-muted-foreground">暂无项目</p>
            )}
            {projects.map((project) => {
              const isActive = currentProjectId === project.id;
              return (
                <div
                  key={project.id}
                  className={`group flex items-center gap-1 px-2 ${isActive ? "" : ""}`}
                >
                  <button
                    type="button"
                    onClick={() => {
                      onSelect(project.id);
                      setOpen(false);
                    }}
                    className={`flex min-w-0 flex-1 items-center gap-2 rounded-[8px] px-2 py-1.5 text-left transition-colors ${
                      isActive ? "bg-primary/5 text-foreground" : "text-foreground hover:bg-secondary/50"
                    }`}
                  >
                    <span
                      className={`shrink-0 ${isActive ? "text-primary" : "text-transparent"}`}
                      aria-hidden="true"
                    >
                      <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M20 6 9 17l-5-5" />
                      </svg>
                    </span>
                    <span className="min-w-0 flex-1">
                      <p className="truncate text-sm font-medium">{project.name}</p>
                      {project.description && (
                        <p className="truncate text-[10px] text-muted-foreground">{project.description}</p>
                      )}
                    </span>
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      onSelect(project.id);
                      navigate(`/projects/${project.id}/settings`);
                      setOpen(false);
                    }}
                    className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-[6px] text-muted-foreground opacity-0 transition-opacity hover:bg-secondary hover:text-foreground group-hover:opacity-100"
                    aria-label="项目设置"
                    title="项目设置"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <circle cx="12" cy="12" r="1" />
                      <circle cx="19" cy="12" r="1" />
                      <circle cx="5" cy="12" r="1" />
                    </svg>
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      )}

      <ProjectCreateDrawer open={isCreateOpen} onClose={() => setIsCreateOpen(false)} />
    </div>
  );
}

// ─── Session 快捷列表（紧凑单行） ──────────────────────────

function SessionShortcutList({ sessions }: { sessions: ProjectSessionEntry[] }) {
  const navigate = useNavigate();
  const location = useLocation();

  const sessionRouteMatch = useMatch("/session/:sessionId");
  const activeSessionId = sessionRouteMatch?.params.sessionId ?? null;

  const sorted = useMemo(() => {
    return [...sessions].sort((a, b) => {
      const ta = a.last_activity ?? 0;
      const tb = b.last_activity ?? 0;
      return tb - ta;
    });
  }, [sessions]);

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="flex items-center justify-between px-3 pb-1 pt-2.5">
        <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">最近会话</span>
        {sorted.length > 0 && (
          <span className="text-[10px] text-muted-foreground/70">{sorted.length}</span>
        )}
      </div>
      {sorted.length === 0 ? (
        <p className="px-3 py-2 text-xs text-muted-foreground">暂无活跃会话</p>
      ) : (
        <div className="flex-1 overflow-y-auto">
          {sorted.map((session) => {
            const isActive = session.session_id === activeSessionId;
            const title = session.session_title?.trim() || "无标题会话";
            const agent = session.agent_display_name?.trim() || null;
            const time = formatRelativeTime(session.last_activity);
            return (
              <button
                key={session.session_id}
                type="button"
                onClick={() => {
                  if (location.pathname === `/session/${session.session_id}`) return;
                  navigate(`/session/${session.session_id}`);
                }}
                className={`flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors ${
                  isActive ? "bg-primary/5" : "hover:bg-secondary/40"
                }`}
                title={agent ? `${title} · ${agent}` : title}
              >
                <SessionStatusDot status={session.execution_status} />
                <span className="min-w-0 flex-1 truncate text-[13px] text-foreground">{title}</span>
                {agent && (
                  <span className="shrink-0 rounded-[4px] bg-secondary/60 px-1 text-[10px] leading-[1.4] text-muted-foreground">
                    {agent.slice(0, 2)}
                  </span>
                )}
                <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">{time}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

function SessionStatusDot({ status }: { status: ProjectSessionEntry["execution_status"] }) {
  const base = "h-1.5 w-1.5 shrink-0 rounded-full";
  switch (status) {
    case "running":
      return (
        <span className="relative flex h-1.5 w-1.5 shrink-0">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
          <span className={`${base} bg-emerald-500`} />
        </span>
      );
    case "completed":
      return <span className={`${base} bg-blue-500`} />;
    case "failed":
      return <span className={`${base} bg-red-500`} />;
    case "interrupted":
      return <span className={`${base} bg-amber-400`} />;
    default:
      return <span className={`${base} bg-muted-foreground/25`} />;
  }
}

function formatRelativeTime(timestamp: number | null): string {
  if (timestamp == null) return "—";
  const ts = timestamp < 1e12 ? timestamp * 1000 : timestamp;
  const diffMs = Date.now() - ts;
  if (diffMs < 0) return "刚刚";
  const seconds = Math.floor(diffMs / 1000);
  if (seconds < 60) return "刚刚";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d`;
  const date = new Date(ts);
  return `${date.getMonth() + 1}/${date.getDate()}`;
}

// ─── 底栏：横排 icon 按钮 ──────────────────────────────────

interface FooterIconBarProps {
  activePanel: FooterPanelKey | null;
  onTogglePanel: (key: FooterPanelKey) => void;
  backends: import("../../types").BackendConfig[];
  connectionState: string;
  currentUser: ReturnType<typeof useCurrentUserStore.getState>["currentUser"];
  rememberedPath: string;
}

function FooterIconBar({ activePanel, onTogglePanel, backends, connectionState, currentUser, rememberedPath }: FooterIconBarProps) {
  const { theme } = useTheme();
  const backendOnline = backends.filter((b) => b.online).length;
  const backendDotClass = backendOnline > 0 ? "bg-emerald-500" : "bg-muted-foreground/30";
  const streamDotClass =
    connectionState === "connected"
      ? "bg-emerald-500"
      : connectionState === "reconnecting" || connectionState === "connecting"
        ? "bg-amber-400"
        : "bg-muted-foreground/30";
  const userInitial = currentUser
    ? (currentUser.display_name?.trim()?.[0] || currentUser.email?.trim()?.[0] || currentUser.user_id[0] || "?")
        .toUpperCase()
    : null;

  return (
    <div className="flex items-center gap-0.5 border-t border-border px-2 py-1.5">
      <FooterIconButton
        label="后端连接"
        active={activePanel === "backend"}
        onClick={() => onTogglePanel("backend")}
      >
        <span className="relative">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <rect x="2" y="3" width="20" height="8" rx="2" />
            <rect x="2" y="13" width="20" height="8" rx="2" />
            <path d="M6 7h.01" />
            <path d="M6 17h.01" />
          </svg>
          <span className={`absolute -right-0.5 -top-0.5 h-1.5 w-1.5 rounded-full ring-2 ring-background ${backendDotClass}`} />
        </span>
      </FooterIconButton>

      <FooterIconButton
        label="事件流"
        active={activePanel === "stream"}
        onClick={() => onTogglePanel("stream")}
      >
        <span className="relative">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <path d="M22 12h-4l-3 9-6-18-3 9H2" />
          </svg>
          <span className={`absolute -right-0.5 -top-0.5 h-1.5 w-1.5 rounded-full ring-2 ring-background ${streamDotClass}`} />
        </span>
      </FooterIconButton>

      {currentUser && (
        <FooterIconButton
          label="当前身份"
          active={activePanel === "user"}
          onClick={() => onTogglePanel("user")}
        >
          <span className="flex h-5 w-5 items-center justify-center rounded-full bg-secondary text-[10px] font-semibold text-foreground">
            {userInitial}
          </span>
        </FooterIconButton>
      )}

      <div className="flex-1" />

      <NavLink
        to="/settings"
        state={{ return_to: rememberedPath }}
        title="设置"
        aria-label="设置"
        className={({ isActive }: NavLinkRenderProps) =>
          `flex h-8 w-8 items-center justify-center rounded-[8px] transition-colors ${
            isActive
              ? "bg-secondary text-foreground"
              : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
          }`
        }
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
          <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
          <circle cx="12" cy="12" r="3" />
        </svg>
      </NavLink>

      <FooterIconButton
        label="主题"
        active={activePanel === "theme"}
        onClick={() => onTogglePanel("theme")}
      >
        <ThemeIcon theme={theme} />
      </FooterIconButton>
    </div>
  );
}

function FooterIconButton({
  label,
  active,
  onClick,
  children,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      aria-pressed={active}
      className={`flex h-8 w-8 items-center justify-center rounded-[8px] transition-colors ${
        active
          ? "bg-secondary text-foreground"
          : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

function ThemeIcon({ theme }: { theme: "light" | "dark" | "system" }) {
  if (theme === "light") {
    return (
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2" />
        <path d="M12 20v2" />
        <path d="m4.93 4.93 1.41 1.41" />
        <path d="m17.66 17.66 1.41 1.41" />
        <path d="M2 12h2" />
        <path d="M20 12h2" />
        <path d="m4.93 19.07 1.41-1.41" />
        <path d="m17.66 6.34 1.41-1.41" />
      </svg>
    );
  }
  if (theme === "dark") {
    return (
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
      </svg>
    );
  }
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <rect x="2" y="3" width="20" height="14" rx="2" />
      <path d="M8 21h8" />
      <path d="M12 17v4" />
    </svg>
  );
}

// ─── 底栏共享 popover 内容 ─────────────────────────────────

interface FooterPanelContentProps {
  panel: FooterPanelKey;
  onClose: () => void;
  backends: import("../../types").BackendConfig[];
  connectionState: string;
}

function FooterPanelContent({ panel, onClose, backends, connectionState }: FooterPanelContentProps) {
  return (
    <div className="px-3 py-2.5">
      <div className="mb-1.5 flex items-center justify-between">
        <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
          {panel === "backend" && "后端连接"}
          {panel === "stream" && "事件流"}
          {panel === "user" && "当前身份"}
          {panel === "theme" && "主题"}
        </span>
        <button
          type="button"
          onClick={onClose}
          className="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
          aria-label="关闭"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
          </svg>
        </button>
      </div>

      {panel === "backend" && <BackendPanel backends={backends} />}
      {panel === "stream" && <StreamPanel connectionState={connectionState} />}
      {panel === "user" && <UserPanel />}
      {panel === "theme" && <ThemePanel />}
    </div>
  );
}

function BackendPanel({ backends }: { backends: import("../../types").BackendConfig[] }) {
  const [expandedId, setExpandedId] = useState<string | null>(
    backends.length === 1 ? backends[0].id : null,
  );

  if (backends.length === 0) {
    return <p className="rounded-[8px] border border-dashed border-border px-3 py-2 text-xs text-muted-foreground">暂无后端</p>;
  }

  return (
    <div className="space-y-1">
      {backends.map((backend) => {
        const isExpanded = expandedId === backend.id;
        const executors = backend.capabilities?.executors ?? [];
        const availableCount = executors.filter((e) => e.available).length;
        const roots = backend.accessible_roots ?? [];
        return (
          <div key={backend.id} className="rounded-[8px] border border-border/60">
            <button
              type="button"
              className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-sm transition-colors hover:bg-secondary/30"
              onClick={() => setExpandedId((prev) => (prev === backend.id ? null : backend.id))}
            >
              <span
                className={`inline-block h-2 w-2 shrink-0 rounded-full ${backend.online ? "bg-emerald-500" : "bg-muted-foreground/30"}`}
              />
              <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">{backend.name}</span>
              <span className="shrink-0 text-[10px] text-muted-foreground">
                {backend.online
                  ? `${availableCount} 执行器`
                  : backend.backend_type === "local"
                    ? "本机"
                    : "远程"}
              </span>
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="10"
                height="10"
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
            {isExpanded && (
              <div className="space-y-2 border-t border-border/60 px-2.5 py-2">
                {backend.online && executors.length > 0 && (
                  <div>
                    <p className="text-[10px] uppercase tracking-wider text-muted-foreground">执行器</p>
                    <div className="mt-1 flex flex-wrap gap-1">
                      {executors.map((ex) => (
                        <span
                          key={ex.id}
                          className={`inline-block rounded-[6px] border px-1.5 py-0.5 text-[10px] ${
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
                        <p key={root} className="truncate text-[10px] text-muted-foreground" title={root}>
                          {root.replace(/^\\\\\?\\/, "")}
                        </p>
                      ))}
                    </div>
                  </div>
                )}
                <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
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
  );
}

function StreamPanel({ connectionState }: { connectionState: string }) {
  const label =
    connectionState === "connected"
      ? "已连接"
      : connectionState === "reconnecting"
        ? "重连中…"
        : connectionState === "connecting"
          ? "连接中…"
          : "未连接";
  const dotClass =
    connectionState === "connected"
      ? "bg-emerald-500"
      : connectionState === "reconnecting" || connectionState === "connecting"
        ? "bg-amber-400 animate-pulse"
        : "bg-muted-foreground/30";
  return (
    <div className="flex items-center gap-2 rounded-[8px] border border-border/60 px-2.5 py-2">
      <span className={`inline-block h-2 w-2 rounded-full ${dotClass}`} />
      <span className="text-xs text-foreground">{label}</span>
      <span className="ml-auto text-[10px] text-muted-foreground">SSE</span>
    </div>
  );
}

function UserPanel() {
  const { currentUser } = useCurrentUserStore();
  if (!currentUser) return null;

  const title = currentUser.display_name?.trim() || currentUser.email?.trim() || currentUser.user_id;
  const subtitle = currentUser.email?.trim() || currentUser.user_id;
  const modeLabel = currentUser.auth_mode === "enterprise" ? "企业模式" : "个人模式";
  const groupCount = currentUser.groups.length;

  return (
    <div className="space-y-2 rounded-[8px] border border-border/60 px-2.5 py-2">
      <div>
        <p className="truncate text-sm font-medium text-foreground">{title}</p>
        {subtitle !== title && <p className="truncate text-[11px] text-muted-foreground">{subtitle}</p>}
      </div>
      <div className="flex flex-wrap gap-1.5">
        {currentUser.is_admin && (
          <span className="rounded-[6px] border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-700 dark:text-amber-300">
            Admin
          </span>
        )}
        <span className="rounded-[6px] border border-border bg-secondary px-2 py-0.5 text-[10px] text-muted-foreground">{modeLabel}</span>
        <span className="rounded-[6px] border border-border bg-secondary/70 px-2 py-0.5 text-[10px] text-muted-foreground">
          provider: {currentUser.provider ?? "unknown"}
        </span>
        <span className="rounded-[6px] border border-border bg-secondary/70 px-2 py-0.5 text-[10px] text-muted-foreground">
          groups: {groupCount}
        </span>
      </div>
      <p className="truncate font-mono text-[10px] text-muted-foreground">{currentUser.user_id}</p>
    </div>
  );
}

function ThemePanel() {
  const { theme, setTheme } = useTheme();
  const options: Array<{ value: "light" | "dark" | "system"; label: string }> = [
    { value: "light", label: "浅色" },
    { value: "dark", label: "深色" },
    { value: "system", label: "系统" },
  ];
  return (
    <div className="flex gap-1 rounded-[8px] border border-border/60 p-1">
      {options.map((option) => {
        const active = option.value === theme;
        return (
          <button
            key={option.value}
            type="button"
            onClick={() => setTheme(option.value)}
            className={`flex-1 rounded-[6px] px-2 py-1.5 text-xs transition-colors ${
              active ? "bg-secondary text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"
            }`}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
