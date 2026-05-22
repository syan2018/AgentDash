/**
 * 新路由 `/workflow/:id` 对应的页面容器。
 *
 * 包一层 back-nav + `LifecycleEditorShell`。
 * 编辑器内部自行负责加载 lifecycle + workflow bundle。
 */

import { useCallback, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { ConfirmDialog } from "@agentdash/ui";

import { LifecycleEditorShell } from "../features/workflow/lifecycle-editor-shell";
import { useProjectStore } from "../stores/projectStore";
import { useWorkflowStore } from "../stores/workflowStore";

export function LifecycleEditorShellPage() {
  const { id } = useParams<{ id: string }>();
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const isDirty = useWorkflowStore((s) => s.lifecycleEditor.dirty);
  const [leaveConfirmOpen, setLeaveConfirmOpen] = useState(false);

  const lifecycleId = id ?? "new";
  const seedKey = searchParams.get("key") ?? undefined;
  const seedName = searchParams.get("name") ?? undefined;
  const seedInitialActivityKey = searchParams.get("activity") ?? searchParams.get("step") ?? undefined;

  const handleBack = useCallback(() => {
    if (isDirty) {
      setLeaveConfirmOpen(true);
      return;
    }
    navigate("/dashboard/assets/workflow");
  }, [isDirty, navigate]);

  const confirmBack = useCallback(() => {
    setLeaveConfirmOpen(false);
    navigate("/dashboard/assets/workflow");
  }, [navigate]);

  const handleSaved = useCallback(
    (savedId: string) => {
      if (lifecycleId === "new") {
        navigate(`/workflow/${savedId}`, { replace: true });
      }
    },
    [lifecycleId, navigate],
  );

  return (
    <>
      <div className="flex h-full flex-col overflow-hidden">
        <div className="shrink-0 border-b border-border px-6 py-3">
          <button
            type="button"
            onClick={handleBack}
            className="inline-flex items-center gap-1.5 rounded-[8px] px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
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
            >
              <path d="m15 18-6-6 6-6" />
            </svg>
            返回 Workflow 资产
          </button>
        </div>

        <div className="flex-1 overflow-hidden">
          <LifecycleEditorShell
            lifecycleId={lifecycleId}
            seed={{ key: seedKey, name: seedName, initial_activity_key: seedInitialActivityKey }}
            projectId={currentProjectId ?? ""}
            onSaved={handleSaved}
          />
        </div>
      </div>
      <ConfirmDialog
        open={leaveConfirmOpen}
        title="离开 Workflow 编辑器"
        description="当前 Workflow 有未保存修改，确定离开吗？"
        confirmLabel="离开"
        tone="danger"
        onClose={() => setLeaveConfirmOpen(false)}
        onConfirm={confirmBack}
      />
    </>
  );
}
