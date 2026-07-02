import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { NavLink, Outlet, useLocation, useMatch, useNavigate } from "react-router-dom";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import { useEventStore } from "../../stores/eventStore";
import { useCurrentUserStore } from "../../stores/currentUserStore";
import { ProjectCreateDrawer } from "../../features/project/project-selector";
import { listProjectBackendAccess } from "../../services/backendAccess";
import type { Project, ProjectBackendAccess } from "../../types";
import { SidebarFooter, type FooterPanelKey } from "./SidebarFooter";
import { AgentRunShortcutList } from "./AgentRunShortcutList";
import { AppErrorBoundary } from "../error/AppErrorBoundary";
import { selectSidebarBackendGroups } from "./sidebarBackendVisibility";

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
    pathPrefixes: ["/dashboard/agent", "/agent/", "/run/", "/subject/", "/agent-runs/"],
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

export function WorkspaceLayout() {
  const location = useLocation();
  const { projects, currentProjectId, selectProject } = useProjectStore();
  const { fetchWorkspaces } = useWorkspaceStore();
  const { backends } = useCoordinatorStore();
  const { connectionState } = useEventStore();
  const { currentUser } = useCurrentUserStore();

  const [activeFooterPanel, setActiveFooterPanel] = useState<FooterPanelKey | null>(null);
  const [backendAccesses, setBackendAccesses] = useState<ProjectBackendAccess[]>([]);

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

  const refreshBackendAccesses = useCallback(() => {
    if (!currentProjectId) {
      setBackendAccesses([]);
      return;
    }
    let alive = true;
    void listProjectBackendAccess(currentProjectId)
      .then((items) => {
        if (alive) setBackendAccesses(items);
      })
      .catch(() => {
        if (alive) setBackendAccesses([]);
      });
    return () => {
      alive = false;
    };
  }, [currentProjectId]);

  useEffect(() => refreshBackendAccesses(), [refreshBackendAccesses]);

  useEffect(() => {
    if (activeFooterPanel === "backend") {
      return refreshBackendAccesses();
    }
    return undefined;
  }, [activeFooterPanel, refreshBackendAccesses]);

  // 路由高亮匹配：useMatch 调用顺序必须稳定
  const agentDashboardMatch = useMatch("/dashboard/agent");
  const agentRouteMatch = useMatch("/agent/:agentId");
  const runRouteMatch = useMatch("/run/:runId");
  const subjectRouteMatch = useMatch("/subject/:kind/:id");
  const agentRunRouteMatch = useMatch("/agent-runs/:runId/:agentId");
  const storyDashboardMatch = useMatch("/dashboard/story");
  const storyRouteMatch = useMatch("/story/:storyId");
  const assetsDashboardMatch = useMatch("/dashboard/assets/*");
  const unifiedWorkflowEditorMatch = useMatch("/workflow/:id");
  const routineDashboardMatch = useMatch("/dashboard/routine");

  const activeMap: Record<NavKey, boolean> = {
    agent: !!agentDashboardMatch || !!agentRouteMatch || !!runRouteMatch || !!subjectRouteMatch || !!agentRunRouteMatch,
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

  const backendGroups = useMemo(
    () => selectSidebarBackendGroups(backends, backendAccesses, currentUser),
    [backends, backendAccesses, currentUser],
  );

  const toggleFooterPanel = (key: FooterPanelKey) => {
    setActiveFooterPanel((prev) => (prev === key ? null : key));
  };

  return (
    <div className="flex h-full w-full overflow-hidden bg-background">
      <aside className="relative z-10 flex h-full w-72 flex-col bg-sidebar text-sidebar-foreground shadow-md">
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
                      ? "bg-card text-primary shadow-sm"
                      : "text-muted-foreground hover:bg-card/50 hover:text-foreground"
                  }`
                }
              >
                {item.icon}
                <span className="text-[10px] font-medium leading-none tracking-wide">{item.label}</span>
              </NavLink>
            );
          })}
        </div>

        {/* AgentRun 快捷列表 */}
        <AgentRunShortcutList projectId={currentProjectId} />

        {/* 底栏 */}
        <SidebarFooter
          activePanel={activeFooterPanel}
          onTogglePanel={toggleFooterPanel}
          onClosePanel={() => setActiveFooterPanel(null)}
          backendGroups={backendGroups}
          connectionState={connectionState}
          currentUser={currentUser}
          rememberedPath={rememberedPath}
        />
      </aside>

      <main className="flex-1 overflow-hidden">
        <AppErrorBoundary resetKeys={[location.pathname]} title="此页面出错了">
          <Outlet />
        </AppErrorBoundary>
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
  member: "Member",
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
  const projectSettingsMatch = useMatch("/projects/:projectId/settings");
  const [open, setOpen] = useState(false);
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [focusedProjectId, setFocusedProjectId] = useState<string | null>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const switchBtnRef = useRef<HTMLButtonElement>(null);
  const [popupPos, setPopupPos] = useState<{ top: number; left: number } | null>(null);

  const current = projects.find((p) => p.id === currentProjectId) ?? null;
  const otherProjects = projects.filter((p) => p.id !== currentProjectId);

  const handleSelectProject = (project: Project, isActive: boolean) => {
    onSelect(project.id);
    if (projectSettingsMatch && !isActive) {
      navigate(`/projects/${project.id}/settings`);
    }
    if (!isActive) setOpen(false);
  };

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
          onClick={() => handleSelectProject(project, isActive)}
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
