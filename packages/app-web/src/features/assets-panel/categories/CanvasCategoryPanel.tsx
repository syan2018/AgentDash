/**
 * CanvasCategoryPanel — Assets 页 Canvas 类目入口。
 * ProjectCanvasManager 负责个人 Canvas、项目共用 Canvas、运行时预览和源编辑组合。
 */

import { useMemo } from "react";

import { ProjectCanvasManager } from "../../canvas-panel/ProjectCanvasManager";
import { useExtensionRuntimeStore } from "../../extension-runtime/model/extensionRuntimeStore";
import { useProjectStore } from "../../../stores/projectStore";
import { SelectProjectEmpty } from "../_shared/SelectProjectEmpty";

export function CanvasCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const projects = useProjectStore((s) => s.projects);

  const currentProject = useMemo(
    () => projects.find((p) => p.id === currentProjectId) ?? null,
    [currentProjectId, projects],
  );

  if (!currentProjectId || !currentProject) {
    return <SelectProjectEmpty assetLabel="Canvas 资产" />;
  }

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      <header className="space-y-1">
        <h2 className="text-base font-semibold tracking-tight text-foreground">Canvas 资产</h2>
        <p className="text-xs text-muted-foreground">
          {currentProject.name} · 我的 Canvas 与项目共用 Canvas。
        </p>
      </header>

      <ProjectCanvasManager
        projectId={currentProject.id}
        projectName={currentProject.name}
        onExtensionRuntimeRefresh={(projectId) => useExtensionRuntimeStore.getState().fetchProject(projectId)}
      />
    </div>
  );
}

export default CanvasCategoryPanel;
