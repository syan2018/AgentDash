import { useMemo } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { ProjectCanvasManager } from "./ProjectCanvasManager";

export function CanvasTabView() {
  const currentProjectId = useProjectStore((state) => state.currentProjectId);
  const projects = useProjectStore((state) => state.projects);

  const currentProject = useMemo(
    () => projects.find((project) => project.id === currentProjectId) ?? null,
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
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-6">
        <div className="flex items-center gap-2.5">
          <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            CANVAS
          </span>
          <div>
            <h2 className="text-sm font-semibold tracking-tight text-foreground">可视化资产</h2>
            <p className="text-xs text-muted-foreground">
              {currentProject.name} · 统一管理项目级 Canvas、运行时预览与数据绑定
            </p>
          </div>
        </div>
      </header>

      <div className="flex-1 overflow-y-auto px-6 py-4">
        <ProjectCanvasManager
          projectId={currentProject.id}
          projectName={currentProject.name}
        />
      </div>
    </div>
  );
}

export default CanvasTabView;
