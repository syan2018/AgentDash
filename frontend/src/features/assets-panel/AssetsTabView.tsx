/**
 * AssetsTabView — 统一 Assets 页主壳。
 *
 * 布局：
 * - 顶部：页面 header（标题 + 项目上下文）
 * - 左侧：类目竖排 NavLink 列表（Workflow / Canvas / MCP Preset）
 * - 右侧：`<Outlet />` 渲染选中类目对应的 CategoryPanel
 *
 * 类目切换通过路由 URL 同步（`/dashboard/assets/:category`）。
 * 空 projectId 时展示 "请选择项目" 空态，对齐其他 Tab view 的风格。
 *
 * 本 PR（PR3）仅提供壳 + 占位；具体列表数据留给 PR4/PR5。
 */

import { useMemo } from "react";
import { NavLink, Outlet } from "react-router-dom";
import { useProjectStore } from "../../stores/projectStore";

// 类目元数据——路由 segment 与展示文案解耦，便于后续新增 / 调整顺序
const CATEGORIES: Array<{
  /** URL segment，与 App.tsx 的路由定义对齐 */
  segment: string;
  /** 展示标签 */
  label: string;
  /** 辅助描述，左栏小字 */
  hint: string;
}> = [
  { segment: "workflow", label: "Workflow", hint: "Lifecycle + Workflow 模板" },
  { segment: "canvas", label: "Canvas", hint: "可视化资产" },
  { segment: "mcp-preset", label: "MCP Preset", hint: "MCP Server 模板" },
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
              {currentProject.name} · 统一管理 Workflow / Canvas / MCP Preset 等项目级可复用资产
            </p>
          </div>
        </div>
      </header>

      {/* 主体：左类目栏 + 右 Outlet */}
      <div className="flex flex-1 overflow-hidden">
        {/* 左侧类目栏 */}
        <aside className="flex w-56 shrink-0 flex-col gap-2 border-r border-border bg-background/60 p-3">
          <p className="px-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground">类目</p>
          <nav className="flex flex-col gap-1">
            {CATEGORIES.map((cat) => (
              <NavLink
                key={cat.segment}
                to={`/dashboard/assets/${cat.segment}`}
                className={({ isActive }) =>
                  `flex flex-col gap-0.5 rounded-[10px] border px-3 py-2.5 text-sm transition-colors ${
                    isActive
                      ? "border-primary/20 bg-secondary/70 font-medium text-foreground"
                      : "border-transparent text-muted-foreground hover:border-border hover:bg-secondary/40 hover:text-foreground"
                  }`
                }
              >
                <span>{cat.label}</span>
                <span className="text-[11px] text-muted-foreground">{cat.hint}</span>
              </NavLink>
            ))}
          </nav>
        </aside>

        {/* 右侧内容区：子路由对应的 CategoryPanel */}
        <main className="flex-1 overflow-y-auto">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

export default AssetsTabView;
