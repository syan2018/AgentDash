/**
 * AssetsTabView — 统一 Assets 页主壳。
 *
 * 布局：
 * - 顶部：页面 header（标题 + 项目上下文）
 * - 左侧侧栏分两区：
 *   - 主类目（项目内已有的资产）：Workflow / Canvas / MCP Preset / Skill
 *   - 安装入口（栏底固定）：资源市场 — 专属样式区分语义
 * - 右侧：`<Outlet />` 渲染选中类目对应的 CategoryPanel
 *
 * 类目切换通过路由 URL 同步（`/dashboard/assets/:category`）。
 * 空 projectId 时展示 "请选择项目" 空态，对齐其他 Tab view 的风格。
 */

import { useMemo } from "react";
import { NavLink, Outlet } from "react-router-dom";
import { useProjectStore } from "../../stores/projectStore";

interface CategoryItem {
  segment: string;
  label: string;
  hint: string;
}

const PRIMARY_CATEGORIES: CategoryItem[] = [
  { segment: "workflow", label: "Workflow", hint: "Lifecycle + Workflow 模板" },
  { segment: "canvas", label: "Canvas", hint: "可视化资产" },
  { segment: "mcp-preset", label: "MCP Preset", hint: "MCP Server 模板" },
  { segment: "skill", label: "Skill", hint: "Agent 可读技能包" },
];

const SOURCE_CATEGORIES: CategoryItem[] = [
  { segment: "marketplace", label: "资源市场", hint: "浏览、安装与发布共享资产" },
];

export function AssetsTabView() {
  const currentProjectId = useProjectStore((state) => state.currentProjectId);
  const projects = useProjectStore((state) => state.projects);

  const currentProject = useMemo(
    () => projects.find((p) => p.id === currentProjectId) ?? null,
    [currentProjectId, projects],
  );

  if (!currentProjectId || !currentProject) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-foreground">请选择或创建项目</h2>
          <p className="mt-2 text-sm text-muted-foreground">在左侧面板选择一个项目开始使用</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* 顶部 header 与 CanvasTabView / WorkflowTabView 对齐视觉语言 */}
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-6">
        <div className="flex items-center gap-2.5">
          <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            ASSETS
          </span>
          <div>
            <h2 className="text-sm font-semibold tracking-tight text-foreground">项目资产</h2>
            <p className="text-xs text-muted-foreground">
              {currentProject.name} · 统一管理 Workflow / Canvas / MCP Preset / Skill 等项目级可复用资产
            </p>
          </div>
        </div>
      </header>

      {/* 主体：左类目栏 + 右 Outlet */}
      <div className="flex flex-1 overflow-hidden">
        <aside className="flex w-56 shrink-0 flex-col gap-2 border-r border-border bg-background/60 p-3">
          {/* 主类目 */}
          <p className="px-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground">类目</p>
          <nav className="flex flex-col gap-1">
            {PRIMARY_CATEGORIES.map((cat) => (
              <NavItem key={cat.segment} item={cat} variant="primary" />
            ))}
          </nav>

          {/* 安装入口（栏底固定） */}
          <div className="mt-auto flex flex-col gap-1.5 border-t border-border pt-3">
            <p className="px-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
              安装入口
            </p>
            {SOURCE_CATEGORIES.map((cat) => (
              <NavItem key={cat.segment} item={cat} variant="source" />
            ))}
          </div>
        </aside>

        <main className="flex-1 overflow-y-auto">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

export default AssetsTabView;

/* ─── NavItem ─── */

function NavItem({ item, variant }: { item: CategoryItem; variant: "primary" | "source" }) {
  return (
    <NavLink
      to={`/dashboard/assets/${item.segment}`}
      className={({ isActive }: { isActive: boolean }) =>
        navItemClass(variant, isActive)
      }
    >
      {variant === "source" && <DownloadIcon />}
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="truncate">{item.label}</span>
        <span className="truncate text-[11px] text-muted-foreground">{item.hint}</span>
      </div>
    </NavLink>
  );
}

function navItemClass(variant: "primary" | "source", isActive: boolean): string {
  const base =
    "flex items-center gap-2 rounded-[10px] border px-3 py-2.5 text-sm transition-colors";
  if (variant === "primary") {
    return `${base} ${
      isActive
        ? "border-primary/20 bg-secondary/70 font-medium text-foreground"
        : "border-transparent text-muted-foreground hover:border-border hover:bg-secondary/40 hover:text-foreground"
    }`;
  }
  return `${base} ${
    isActive
      ? "border-primary/30 bg-primary/8 font-medium text-foreground"
      : "border-border bg-secondary/20 text-muted-foreground hover:bg-secondary/40 hover:text-foreground"
  }`;
}

function DownloadIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="shrink-0"
      aria-hidden="true"
    >
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
      <polyline points="7 10 12 15 17 10" />
      <line x1="12" y1="15" x2="12" y2="3" />
    </svg>
  );
}
