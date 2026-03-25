import { ProjectWorkflowPanel } from "./project-workflow-panel";
import { useProjectStore } from "../../stores/projectStore";

export function WorkflowTabView() {
  const currentProjectId = useProjectStore((state) => state.currentProjectId);

  if (!currentProjectId) {
    return (
      <div className="flex h-full flex-col overflow-hidden">
        <div className="shrink-0 border-b border-border px-6 py-5">
          <div>
            <h2 className="text-lg font-semibold text-foreground">工作流系统</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              Workflow 定义 agent 的注入、Hook 策略与完成检查，Lifecycle 定义步骤编排与推进规则。
            </p>
          </div>
        </div>
        <div className="flex-1 overflow-y-auto px-6 py-5">
          <div className="rounded-[12px] border border-dashed border-amber-300/30 bg-amber-500/5 px-4 py-6 text-center text-sm text-amber-700">
            请先在左侧选择一个项目，以便查看默认 lifecycle assignment 与运行情况。
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="shrink-0 border-b border-border px-6 py-5">
        <div>
          <h2 className="text-lg font-semibold text-foreground">工作流系统</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            面向整个 agent 生命周期管理 Workflow 定义与 Lifecycle 编排。
          </p>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto px-6 py-5">
        <ProjectWorkflowPanel projectId={currentProjectId} />
      </div>
    </div>
  );
}
