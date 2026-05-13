/**
 * CanvasCategoryPanel — Assets 页 Canvas 类目实装（PR4）。
 *
 * 实现决策：
 * - Canvas 的 list + detail 已在 `ProjectCanvasManager` 内部以左右分栏实现。
 * - 不再抽出新的 list 组件，也不引入额外子路由——PRD 要求"最小改动 + URL 拓扑不分裂"。
 * - 本面板直接复用 `ProjectCanvasManager`，选中即进入编辑态（等同于"编辑"操作）。
 * - 原有 `/dashboard/canvas` 深链已在 App.tsx redirect 到 `/dashboard/assets/canvas`，
 *   用户体验 = 原 Canvas Tab 等价。
 *
 * 对齐 PRD：
 * - 列表项展示：title / description / files 计数 / bindings 计数 / 更新时间（ProjectCanvasManager 已实现）
 * - 行动作：选中 = 编辑（ProjectCanvasManager 选中展开 detail / 绑定编辑），删除按钮也已存在
 * - 复制：Canvas 后端暂未提供 duplicate API —— 留 TODO 给 PR5 / 后续任务接
 *
 * NOTE: 未来若需要独立编辑子路由（如 `/dashboard/assets/canvas/:id`），
 * 只需在 App.tsx 为 Canvas panel 新增嵌套子路由，并把 ProjectCanvasManager 拆成
 * list + detail 两个组件。当前无此需求。
 */

import { useMemo } from "react";

import { ProjectCanvasManager } from "../../canvas-panel/ProjectCanvasManager";
import { useProjectStore } from "../../../stores/projectStore";

export function CanvasCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const projects = useProjectStore((s) => s.projects);

  const currentProject = useMemo(
    () => projects.find((p) => p.id === currentProjectId) ?? null,
    [currentProjectId, projects],
  );

  if (!currentProjectId || !currentProject) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="text-center text-sm text-muted-foreground">
          请选择项目后查看 Canvas 资产
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      <header className="space-y-1">
        <h2 className="text-base font-semibold tracking-tight text-foreground">Canvas 资产</h2>
        <p className="text-xs text-muted-foreground">
          {currentProject.name} · 项目级可视化 Canvas 模板，支持文件 / 数据绑定 / 运行时预览的在线编辑。
        </p>
      </header>

      <ProjectCanvasManager
        projectId={currentProject.id}
        projectName={currentProject.name}
      />
    </div>
  );
}

export default CanvasCategoryPanel;
