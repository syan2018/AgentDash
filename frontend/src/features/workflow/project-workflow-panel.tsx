/**
 * ProjectWorkflowPanel — 项目工作流概览
 *
 * Lifecycle/Workflow 绑定已迁移到 Agent Hub（Agent-Link 模型）。
 * 此面板仅展示当前项目可用的 Lifecycle 定义概览。
 */

import { useEffect } from "react";
import { useWorkflowStore } from "../../stores/workflowStore";

export function ProjectWorkflowPanel({ projectId }: { projectId: string }) {
  const lifecycles = useWorkflowStore((s) => s.lifecycleDefinitions);
  const fetchLifecycles = useWorkflowStore((s) => s.fetchLifecycles);

  useEffect(() => {
    void fetchLifecycles();
  }, [projectId, fetchLifecycles]);

  // status 字段自 migration 0013 起已废弃；全部视为可用。
  const activeLifecycles = lifecycles;

  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground">
        工作流绑定已通过 Agent Hub 管理。以下是当前可用的 Lifecycle 定义：
      </p>
      {activeLifecycles.length === 0 ? (
        <p className="text-xs text-muted-foreground italic">暂无活跃的 Lifecycle 定义</p>
      ) : (
        <div className="flex flex-wrap gap-2">
          {activeLifecycles.map((lc) => (
            <span
              key={lc.id}
              className="rounded-full border border-primary/20 bg-primary/5 px-2.5 py-1 text-xs text-primary"
            >
              {lc.name} ({lc.steps.length} steps)
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
