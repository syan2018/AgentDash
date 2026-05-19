import React, { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { NavLink, Outlet, useLocation, useMatch, useNavigate, type NavLinkRenderProps } from "react-router-dom";
import { StatusDot } from "@agentdash/ui";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import { useEventStore } from "../../stores/eventStore";
import { useCurrentUserStore } from "../../stores/currentUserStore";
import { useSidebarSessionsStore } from "../../stores/sidebarSessionsStore";
import { useTheme } from "../../hooks/use-theme";
import { ProjectCreateDrawer } from "../../features/project/project-selector";
import type { Project, ProjectSessionEntry } from "../../types";
import {
  buildSessionShortcutRows,
  type SessionShortcutRow,
} from "./session-shortcut-rows";

// ─── 视图导航定义 ──────────────────────────────────────────
type NavKey = "agent" | "story" | "assets" | "routine";

interface NavItem {
  key: NavKey;
  label: string;
  defaultPath: string;
  pathPrefixes: string[];
  icon: React.ReactNode;
}

// Icon 与项目其他地方一致：feather-style，stroke 1.8 / 18px。
//  - Agent  → bot（智能体）
//  - Story  → book-open（故事 / 叙事）
//  - Assets → layers（资产堆叠）
//  - Routine→ check-square（例行勾选）
const NAV_ITEMS: NavItem[] = [
  {
    key: "agent",
    label: "Agent",
    defaultPath: "/dashboard/agent",
    pathPrefixes: ["/dashboard/agent", "/session/"],
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 8V4H8" />
        <rect x="4" y="8" width="16" height="12" rx="2" />
        <path d="M2 14h2" />
        <path d="M20 14h2" />
        <path d="M15 13v2" />
        <path d="M9 13v2" />
      </svg>
    ),
  },
  {
    key: "story",
    label: "Story",
    defaultPath: "/dashboard/story",
    pathPrefixes: ["/dashboard/story", "/story/"],
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" />
        <path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" />
      </svg>
    ),
  },
  {
    key: "assets",
    label: "Assets",
    defaultPath: "/dashboard/assets",
    pathPrefixes: ["/dashboard/assets", "/workflow/"],
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.91a1 1 0 0 0 0-1.83Z" />
        <path d="M2 12.5l8.58 3.91a2 2 0 0 0 1.66 0L22 12.5" />
        <path d="M2 17l8.58 3.91a2 2 0 0 0 1.66 0L22 17" />
      </svg>
    ),
  },
  {
    key: "routine",
    label: "Routine",
    defaultPath: "/dashboard/routine",
    pathPrefixes: ["/dashboard/routine"],
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <rect x="3" y="3" width="18" height="18" rx="2" />
        <path d="m9 12 2 2 4-4" />
      </svg>
    ),
  },
];

// 底栏共享 popover：事件流移除（无人关注），仅保留后端 + 主题
type FooterPanelKey = "backend" | "theme";

export function WorkspaceLayout() {
  const location = useLocation();
  const { projects, currentProjectId, selectProject } = useProjectStore();
  const { fetchWorkspaces } = useWorkspaceStore();
  const { backends } = useCoordinatorStore();
  const { connectionState } = useEventStore();
  const { currentUser } = useCurrentUserStore();
  const { sessions, loadForProject, clearForProject } = useSidebarSessionsStore();

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

  // Sidebar 会话列表：独立 store，独立刷新生命周期（与 agent-tab-view 脱钩）
  const prevProjectIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (!currentProjectId) return;
    if (prevProjectIdRef.current === currentProjectId) return;
    prevProjectIdRef.current = currentProjectId;
    clearForProject(currentProjectId);
    void loadForProject(currentProjectId);
  }, [currentProjectId, clearForProject, loadForProject]);

  // 定时轮询（30s），保证 sidebar 最近会话不过期；Agent tab 各自按需刷新
  useEffect(() => {
    if (!currentProjectId) return;
    const interval = window.setInterval(() => {
      void loadForProject(currentProjectId);
    }, 30_000);
    return () => window.clearInterval(interval);
  }, [currentProjectId, loadForProject]);

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
        {/* 头部：品牌 */}
        <div className="flex items-center gap-2 border-b border-border px-4 py-3.5">
          <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            APP
          </span>
          <h1 className="text-lg font-semibold tracking-tight text-foreground">AgentDash</h1>
        </div>

        {/* 项目下拉 */}
        <ProjectDropdown
          projects={projects}
          currentProjectId={currentProjectId}
          onSelect={selectProject}
        />

        {/* 横排 Nav：icon + 文字标签，占两行 */}
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
                  `flex h-14 flex-col items-center justify-center gap-1 rounded-[10px] transition-all ${
                    isActive
                      ? "bg-primary text-primary-foreground shadow-sm"
                      : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
                  }`
                }
              >
                {item.icon}
                <span className="text-[10px] font-medium leading-none tracking-wide">{item.label}</span>
              </NavLink>
            );
          })}
        </div>

        {/* Session 快捷列表：flex-1 填充中段，内部自滚 */}
        <SessionShortcutList sessions={sessions} />

        {/* 底栏 */}
        <SidebarFooter
          activePanel={activeFooterPanel}
          onTogglePanel={toggleFooterPanel}
          onClosePanel={() => setActiveFooterPanel(null)}
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

const PROJECT_ROLE_LABELS: Record<NonNullable<Project["access"]["role"]>, string> = {
  owner: "Owner",
  editor: "Editor",
  viewer: "Viewer",
};
const PROJECT_VISIBILITY_LABELS: Record<Project["visibility"], string> = {
  private: "私有",
  template_visible: "模板可见",
};
function describeProjectAccess(project: Project): string {
  if (project.access.via_admin_bypass) return "管理员旁路";
  if (project.access.role) return PROJECT_ROLE_LABELS[project.access.role];
  if (project.access.via_template_visibility) return "模板访客";
  return "仅查看";
}

function ProjectDropdown({ projects, currentProjectId, onSelect }: ProjectDropdownProps) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [focusedProjectId, setFocusedProjectId] = useState<string | null>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const switchBtnRef = useRef<HTMLButtonElement>(null);
  const [popupPos, setPopupPos] = useState<{ top: number; left: number } | null>(null);

  const current = projects.find((p) => p.id === currentProjectId) ?? null;
  const otherProjects = projects.filter((p) => p.id !== currentProjectId);

  useEffect(() => {
    if (!open) return;
    const update = () => {
      if (!switchBtnRef.current) return;
      const rect = switchBtnRef.current.getBoundingClientRect();
      setPopupPos({ top: Math.round(rect.top), left: Math.round(rect.right + 8) });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handler = (event: MouseEvent) => {
      const target = event.target as Node;
      if (popoverRef.current?.contains(target)) return;
      if (switchBtnRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const renderCard = (project: Project, isActive: boolean) => {
    const isFocused = focusedProjectId === project.id;
    const showSettingsButton = isActive || isFocused;
    return (
      <div
        key={project.id}
        className={`flex items-center justify-between rounded-[10px] border px-3 py-2.5 text-sm transition-colors ${
          isActive
            ? "border-primary/20 bg-background"
            : "border-transparent bg-transparent hover:border-border hover:bg-background/80"
        }`}
        onMouseEnter={() => setFocusedProjectId(project.id)}
        onMouseLeave={() =>
          setFocusedProjectId((value) => (value === project.id ? null : value))
        }
        onFocusCapture={() => setFocusedProjectId(project.id)}
        onBlurCapture={(event) => {
          const nextTarget = event.relatedTarget as Node | null;
          if (!nextTarget || !event.currentTarget.contains(nextTarget)) {
            setFocusedProjectId((value) => (value === project.id ? null : value));
          }
        }}
      >
        <button
          type="button"
          onClick={() => {
            onSelect(project.id);
            if (!isActive) setOpen(false);
          }}
          className="min-w-0 flex-1 text-left text-foreground"
        >
          <p className="truncate font-medium">{project.name}</p>
          <p className="truncate text-xs text-muted-foreground">
            {project.description || `ID: ${project.id}`}
          </p>
          <div className="mt-2 flex flex-wrap gap-1.5">
            <span className="rounded-[8px] border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
              {describeProjectAccess(project)}
            </span>
            {project.is_template && (
              <span className="rounded-[8px] border border-warning/20 bg-warning/10 px-2 py-0.5 text-[10px] text-warning">
                模板
              </span>
            )}
            <span className="rounded-[8px] border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
              {PROJECT_VISIBILITY_LABELS[project.visibility]}
            </span>
          </div>
        </button>
        {showSettingsButton && (
          <button
            type="button"
            onClick={() => {
              onSelect(project.id);
              navigate(`/projects/${project.id}/settings`);
              setOpen(false);
            }}
            className="ml-2 inline-flex h-7 w-7 items-center justify-center rounded-[8px] border border-border bg-secondary text-sm leading-none text-muted-foreground transition-colors hover:text-foreground"
            aria-label="打开项目设置"
            title="打开项目设置"
          >
            ⋯
          </button>
        )}
      </div>
    );
  };

  return (
    <div className="relative space-y-1.5 border-b border-border px-3 py-3">
      <div className="flex items-center justify-between px-1">
        <p className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">项目</p>
        <div className="flex items-center gap-1.5">
          {otherProjects.length > 0 && (
            <button
              ref={switchBtnRef}
              type="button"
              onClick={() => setOpen((v) => !v)}
              aria-pressed={open}
              className={`flex items-center gap-1 rounded-[8px] border px-2 py-1 text-xs transition-colors ${
                open
                  ? "border-border bg-secondary text-foreground"
                  : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground"
              }`}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="11"
                height="11"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="m17 3 4 4-4 4" />
                <path d="M21 7H9" />
                <path d="m7 21-4-4 4-4" />
                <path d="M3 17h12" />
              </svg>
              <span>切换</span>
              <span className="text-[10px] text-muted-foreground/80">{otherProjects.length}</span>
            </button>
          )}
          <button
            type="button"
            onClick={() => setIsCreateOpen(true)}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            + 新建
          </button>
        </div>
      </div>

      {/* 当前项目卡片 */}
      {current ? (
        renderCard(current, true)
      ) : (
        <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-sm text-muted-foreground">
          {projects.length === 0 ? "暂无项目" : "未选择项目"}
        </p>
      )}

      <ProjectCreateDrawer open={isCreateOpen} onClose={() => setIsCreateOpen(false)} />

      {/* 切换项目 popup：Portal 到 body + fixed 定位 */}
      {open &&
        otherProjects.length > 0 &&
        popupPos &&
        createPortal(
          <div
            ref={popoverRef}
            style={{ position: "fixed", top: popupPos.top, left: popupPos.left }}
            className="z-50 w-80 overflow-hidden rounded-[12px] border border-border bg-background shadow-xl"
          >
            <div className="flex items-center justify-between px-4 pb-2 pt-3">
              <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                切换项目
              </span>
              <button
                type="button"
                onClick={() => setOpen(false)}
                className="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
                aria-label="关闭"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M18 6 6 18" />
                  <path d="m6 6 12 12" />
                </svg>
              </button>
            </div>
            <div className="max-h-[70vh] space-y-1 overflow-y-auto px-2 pb-2">
              {otherProjects.map((project) => renderCard(project, false))}
            </div>
          </div>,
          document.body,
        )}
    </div>
  );
}

// ─── Session 快捷列表（容器高度自适应 + 末尾 ...） ──────────

function isUuidLike(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

function getShortcutAgentLabel(session: ProjectSessionEntry): string | null {
  const displayName = session.agent_display_name?.trim();
  if (displayName) return displayName;

  const agentKey = session.agent_key?.trim();
  if (agentKey && !isUuidLike(agentKey)) return agentKey;
  return null;
}

function getShortcutOwnerLabel(session: ProjectSessionEntry): string | null {
  if (session.owner_type === "story") {
    return session.owner_title?.trim() ? `Story · ${session.owner_title.trim()}` : "Story";
  }
  if (session.owner_type === "task") {
    const taskTitle = session.owner_title?.trim() || "Task";
    const storyTitle = session.story_title?.trim();
    return storyTitle ? `Task · ${storyTitle} / ${taskTitle}` : `Task · ${taskTitle}`;
  }
  return null;
}

function getShortcutIndentClass(depth: number): string {
  if (depth <= 0) return "pl-2.5";
  if (depth === 1) return "pl-5";
  return "pl-8";
}

function estimateShortcutRowHeight(row: SessionShortcutRow): number {
  const titleLength = row.session.session_title?.trim().length ?? 0;
  const hasMeta = Boolean(
    row.isCompanion ||
      getShortcutAgentLabel(row.session) ||
      getShortcutOwnerLabel(row.session),
  );
  if (titleLength > 34 || hasMeta) return 58;
  return 42;
}

function SessionShortcutList({ sessions }: { sessions: ProjectSessionEntry[] }) {
  const navigate = useNavigate();
  const location = useLocation();
  const listRef = useRef<HTMLDivElement>(null);
  const rowsRef = useRef<Map<string, HTMLButtonElement>>(new Map());
  const [rowHeights, setRowHeights] = useState<Map<string, number>>(new Map());
  const [containerH, setContainerH] = useState(0);

  const sessionRouteMatch = useMatch("/session/:sessionId");
  const activeSessionId = sessionRouteMatch?.params.sessionId ?? null;

  const rows = useMemo(() => buildSessionShortcutRows(sessions), [sessions]);

  // 监听容器高度变化
  useEffect(() => {
    const el = listRef.current;
    if (!el) return;
    const update = () => setContainerH(el.clientHeight);
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // 测量每行实际高度（记录到 id → height 的 Map）；DOM 变动时重算
  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      const map = new Map<string, number>();
      rowsRef.current.forEach((el, id) => {
        map.set(id, el.offsetHeight);
      });
      setRowHeights((prev) => {
        // 仅当有差异时才 setState，避免无意义重渲染
        if (prev.size === map.size) {
          let same = true;
          for (const [k, v] of map) {
            if (prev.get(k) !== v) {
              same = false;
              break;
            }
          }
          if (same) return prev;
        }
        return map;
      });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [rows]);

  // 用已知行高 + 容器高度决定可见数量；未知行用保守估算
  const { displayed, hasMore } = useMemo(() => {
    if (rows.length === 0 || containerH <= 0) {
      return { displayed: rows, hasMore: false };
    }
    const estH = (row: SessionShortcutRow) =>
      rowHeights.get(row.session.session_id) ?? estimateShortcutRowHeight(row);
    let acc = 0;
    let count = 0;
    for (const row of rows) {
      const h = estH(row);
      if (acc + h > containerH) break;
      acc += h;
      count += 1;
    }
    if (count >= rows.length) {
      return { displayed: rows, hasMore: false };
    }
    return { displayed: rows.slice(0, Math.max(1, count)), hasMore: true };
  }, [rows, containerH, rowHeights]);

  return (
    <div className="flex min-h-0 flex-1 flex-col border-b border-border">
      {/* 标题行：左右各 px-4，与 ProjectDropdown 对齐 */}
      <div className="flex shrink-0 items-center justify-between px-4 pb-1.5 pt-3">
        <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">最近会话</span>
        {rows.length > 0 && (
          <span className="text-[10px] text-muted-foreground/70">
            {hasMore ? `${displayed.length} / ${rows.length}` : rows.length}
          </span>
        )}
      </div>
      {rows.length === 0 ? (
        <p className="px-4 pb-3 text-xs text-muted-foreground">暂无活跃会话</p>
      ) : (
        <>
          <div ref={listRef} className="min-h-0 flex-1 overflow-hidden px-3">
            {displayed.map((row) => {
              const { session } = row;
              const isActive = session.session_id === activeSessionId;
              const title = session.session_title?.trim() || "无标题会话";
              const agent = getShortcutAgentLabel(session);
              const owner = getShortcutOwnerLabel(session);
              const time = formatRelativeTime(session.last_activity);
              const indentClass = getShortcutIndentClass(row.depth);
              const metaParts = [
                row.isCompanion ? "Subagent" : null,
                agent,
                owner,
              ].filter((part): part is string => Boolean(part));
              const meta = metaParts.join(" · ");
              return (
                <button
                  key={session.session_id}
                  ref={(el) => {
                    if (el) rowsRef.current.set(session.session_id, el);
                    else rowsRef.current.delete(session.session_id);
                  }}
                  type="button"
                  onClick={() => {
                    if (location.pathname === `/session/${session.session_id}`) return;
                    navigate(`/session/${session.session_id}`);
                  }}
                  className={`flex w-full flex-col gap-1 rounded-[8px] py-2 pr-2.5 text-left transition-colors ${indentClass} ${
                    isActive ? "bg-primary/10" : "hover:bg-secondary/50"
                  }`}
                  title={meta ? `${title} · ${meta}` : title}
                >
                  <div className="flex items-start gap-2">
                    {row.isCompanion && (
                      <span className="mt-[3px] shrink-0 text-[11px] leading-none text-primary/70">
                        ↳
                      </span>
                    )}
                    <SessionStatusDot status={session.execution_status} />
                    <span className="min-w-0 flex-1 whitespace-normal break-words text-[13px] leading-[1.35] text-foreground line-clamp-2">
                      {title}
                    </span>
                    <span className="mt-[1px] shrink-0 text-[10px] tabular-nums text-muted-foreground">{time}</span>
                  </div>
                  {meta && (
                    <p className="ml-3.5 whitespace-normal break-words text-[11px] leading-[1.35] text-muted-foreground line-clamp-2">
                      {meta}
                    </p>
                  )}
                </button>
              );
            })}
          </div>
          {/* 固定按钮槽：无论 hasMore 与否都占相同高度，列表容器尺寸稳定 */}
          <div className="flex h-7 shrink-0 items-center justify-center px-3 pb-1">
            {hasMore && (
              <button
                type="button"
                onClick={() => navigate("/dashboard/agent")}
                title={`查看全部会话（还有 ${rows.length - displayed.length} 个）`}
                className="flex w-full items-center justify-center rounded-[8px] py-1 text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                  <circle cx="5" cy="12" r="1.5" />
                  <circle cx="12" cy="12" r="1.5" />
                  <circle cx="19" cy="12" r="1.5" />
                </svg>
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}

function SessionStatusDot({ status }: { status: ProjectSessionEntry["execution_status"] }) {
  switch (status) {
    case "running":
      return <StatusDot tone="success" pulse className="shrink-0" />;
    case "completed":
      return <StatusDot tone="info" className="shrink-0" />;
    case "failed":
      return <StatusDot tone="danger" className="shrink-0" />;
    case "interrupted":
      return <StatusDot tone="warning" className="shrink-0" />;
    default:
      return <StatusDot tone="muted" className="shrink-0" />;
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

// ─── 底栏：UserCard 常驻 + IconBar + Portal overlay popup ───

interface SidebarFooterProps {
  activePanel: FooterPanelKey | null;
  onTogglePanel: (key: FooterPanelKey) => void;
  onClosePanel: () => void;
  backends: import("../../types").BackendConfig[];
  connectionState: string;
  currentUser: ReturnType<typeof useCurrentUserStore.getState>["currentUser"];
  rememberedPath: string;
}

function SidebarFooter({
  activePanel,
  onTogglePanel,
  onClosePanel,
  backends,
  connectionState,
  currentUser,
  rememberedPath,
}: SidebarFooterProps) {
  const footerRef = useRef<HTMLDivElement>(null);
  const overlayRef = useRef<HTMLDivElement>(null);
  const { theme } = useTheme();

  const [anchor, setAnchor] = useState<{ top: number; left: number; right: number } | null>(null);

  useEffect(() => {
    if (!activePanel) return;
    const update = () => {
      if (!footerRef.current) return;
      const rect = footerRef.current.getBoundingClientRect();
      setAnchor({ top: Math.round(rect.top), left: Math.round(rect.left), right: Math.round(rect.right) });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [activePanel]);

  useEffect(() => {
    if (!activePanel) return;
    const handler = (event: MouseEvent) => {
      const target = event.target as Node;
      if (overlayRef.current?.contains(target)) return;
      if (footerRef.current?.contains(target)) return;
      onClosePanel();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [activePanel, onClosePanel]);

  const backendOnline = backends.filter((b) => b.online).length;
  const backendDotClass = backendOnline > 0 ? "bg-emerald-500" : "bg-muted-foreground/30";

  const panelTitle =
    activePanel === "backend" ? "后端连接" : activePanel === "theme" ? "主题" : "";

  return (
    <>
      <div ref={footerRef} className="border-t border-border">
        {currentUser && <UserCard />}
        <div className="flex items-center gap-0.5 border-t border-border/60 px-2 py-1.5">
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
            label="主题"
            active={activePanel === "theme"}
            onClick={() => onTogglePanel("theme")}
          >
            <ThemeIcon theme={theme} />
          </FooterIconButton>

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
        </div>
      </div>

      {/* Portal overlay：从 footer 上方向上浮出，平面化内部内容（无嵌套 card） */}
      {activePanel &&
        anchor &&
        createPortal(
          <div
            ref={overlayRef}
            style={{
              position: "fixed",
              left: anchor.left + 8,
              width: anchor.right - anchor.left - 16,
              bottom: window.innerHeight - anchor.top + 6,
              maxHeight: `calc(${anchor.top}px - 16px)`,
            }}
            className="z-40 flex flex-col overflow-hidden rounded-[12px] border border-border bg-background shadow-2xl"
          >
            <div className="flex items-center justify-between px-4 pb-2 pt-3">
              <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                {panelTitle}
              </span>
              <button
                type="button"
                onClick={onClosePanel}
                className="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
                aria-label="关闭"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M18 6 6 18" />
                  <path d="m6 6 12 12" />
                </svg>
              </button>
            </div>
            <div className="flex-1 overflow-y-auto px-2 pb-3">
              {activePanel === "backend" && (
                <BackendPanel backends={backends} connectionState={connectionState} />
              )}
              {activePanel === "theme" && <ThemePanel />}
            </div>
          </div>,
          document.body,
        )}
    </>
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

// ─── 常驻 UserCard ─────────────────────────────────────────

function UserCard() {
  const { currentUser } = useCurrentUserStore();
  if (!currentUser) return null;

  const title = currentUser.display_name?.trim() || currentUser.email?.trim() || currentUser.user_id;
  const subtitle = currentUser.email?.trim() || currentUser.user_id;
  const initial = (title?.[0] || "?").toUpperCase();
  const modeLabel = currentUser.auth_mode === "enterprise" ? "企业" : "个人";

  return (
    <div className="flex items-center gap-2 px-3 py-2">
      {/* eslint-disable-next-line no-restricted-syntax -- 用户头像 */}
      <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-secondary text-xs font-semibold text-foreground">
        {initial}
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-xs font-medium text-foreground">{title}</p>
        {subtitle !== title && (
          <p className="truncate text-[10px] text-muted-foreground">{subtitle}</p>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {currentUser.is_admin && (
          <span className="rounded-[4px] border border-warning/30 bg-warning/10 px-1 py-0.5 text-[9px] text-warning">
            Admin
          </span>
        )}
        <span className="rounded-[4px] border border-border bg-secondary px-1 py-0.5 text-[9px] text-muted-foreground">
          {modeLabel}
        </span>
      </div>
    </div>
  );
}

// ─── BackendPanel：平面化（行 + 分割线，无嵌套 card） ────────

function BackendPanel({
  backends,
  connectionState,
}: {
  backends: import("../../types").BackendConfig[];
  connectionState: string;
}) {
  const [expandedId, setExpandedId] = useState<string | null>(
    backends.length === 1 ? backends[0].id : null,
  );

  const streamLabel =
    connectionState === "connected"
      ? "已连接"
      : connectionState === "reconnecting"
        ? "重连中…"
        : connectionState === "connecting"
          ? "连接中…"
          : "未连接";
  const streamDotClass =
    connectionState === "connected"
      ? "bg-emerald-500"
      : connectionState === "reconnecting" || connectionState === "connecting"
        ? "bg-amber-400 animate-pulse"
        : "bg-muted-foreground/30";

  return (
    <div>
      {backends.length === 0 ? (
        <p className="px-2 py-2 text-xs text-muted-foreground">暂无后端</p>
      ) : (
        <div>
          {backends.map((backend) => {
            const isExpanded = expandedId === backend.id;
            const executors = backend.capabilities?.executors ?? [];
            const availableCount = executors.filter((e) => e.available).length;
            const roots = backend.accessible_roots ?? [];
            return (
              <div key={backend.id}>
                <button
                  type="button"
                  className="flex w-full items-center gap-2 rounded-[8px] px-2 py-1.5 text-left text-sm transition-colors hover:bg-secondary/50"
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
                  <div className="space-y-2 px-2 pb-2 pt-1 text-[11px]">
                    {backend.online && executors.length > 0 && (
                      <div>
                        <p className="mb-1 text-[10px] uppercase tracking-wider text-muted-foreground">执行器</p>
                        <div className="flex flex-wrap gap-1">
                          {executors.map((ex) => (
                            <span
                              key={ex.id}
                              className={`inline-block rounded-[6px] px-1.5 py-0.5 text-[10px] ${
                                ex.available
                                  ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                                  : "bg-secondary text-muted-foreground"
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
                        <p className="mb-0.5 text-[10px] uppercase tracking-wider text-muted-foreground">可访问路径</p>
                        <div className="space-y-0.5">
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
      )}

      {/* 事件流状态：作为 backend 面板里的元信息行（无独立 card） */}
      <div className="mt-2 flex items-center gap-2 border-t border-border/60 px-2 pt-2">
        <span className={`inline-block h-1.5 w-1.5 rounded-full ${streamDotClass}`} />
        <span className="text-[11px] text-muted-foreground">事件流 · {streamLabel}</span>
        <span className="ml-auto text-[10px] text-muted-foreground/70">SSE</span>
      </div>
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
    <div className="flex gap-1 px-1 pt-1">
      {options.map((option) => {
        const active = option.value === theme;
        return (
          <button
            key={option.value}
            type="button"
            onClick={() => setTheme(option.value)}
            className={`flex-1 rounded-[8px] px-2 py-1.5 text-xs transition-colors ${
              active
                ? "bg-secondary text-foreground shadow-sm"
                : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
            }`}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
