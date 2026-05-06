import { useEffect } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";

import { LifecycleDagEditor } from "../features/workflow/lifecycle-dag-editor";
import { useWorkflowStore } from "../stores/workflowStore";
import { useProjectStore } from "../stores/projectStore";

export function LifecycleEditorPage() {
  const { definitionId } = useParams<{ definitionId: string }>();
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const currentProjectId = useProjectStore((state) => state.currentProjectId);
  const editorDraft = useWorkflowStore((state) => state.lcEditor.draft);
  const openNewDraft = useWorkflowStore((state) => state.openNewLifecycleDraft);
  const openEditDraft = useWorkflowStore((state) => state.openEditLifecycleDraft);
  const isLoading = useWorkflowStore((state) => state.lcEditor.isLoading);
  const isDirty = useWorkflowStore((state) => state.lcEditor.dirty);

  const isNew = definitionId === "new";
  const seedKey = searchParams.get("key") ?? undefined;
  const seedName = searchParams.get("name") ?? undefined;
  const seedInitialStepKey = searchParams.get("step") ?? undefined;
  const handleBack = () => {
    if (isDirty && !window.confirm("当前 Lifecycle 有未保存修改，确定离开吗？")) {
      return;
    }
    navigate("/dashboard/assets/workflow");
  };

  useEffect(() => {
    if (isNew) {
      openNewDraft(currentProjectId ?? "", {
        key: seedKey,
        name: seedName,
        initial_step_key: seedInitialStepKey,
      });
    } else if (definitionId) {
      void openEditDraft(definitionId);
    }
  }, [currentProjectId, definitionId, isNew, openEditDraft, openNewDraft, seedInitialStepKey, seedKey, seedName]);

  if (isLoading && !editorDraft) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <div className="mx-auto h-7 w-7 animate-spin rounded-full border-2 border-primary border-t-transparent" />
          <p className="mt-3 text-sm text-muted-foreground">正在加载 Lifecycle Definition...</p>
        </div>
      </div>
    );
  }

  if (!editorDraft) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="max-w-sm text-center">
          <p className="text-sm text-muted-foreground">未找到 Lifecycle Definition</p>
          <button
            type="button"
            onClick={handleBack}
            className="mt-4 agentdash-button-secondary"
          >
            返回 Workflow 资产
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* 顶部导航栏 */}
      <div className="shrink-0 border-b border-border px-6 py-3">
        <div className="flex items-center justify-between">
          <button
            type="button"
            onClick={handleBack}
            className="inline-flex items-center gap-1.5 rounded-[8px] px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m15 18-6-6 6-6"/></svg>
            返回 Workflow 资产
          </button>
          <p className="text-sm text-muted-foreground">
            Lifecycle 编辑器 — {editorDraft.name || editorDraft.key || "新建"}
          </p>
        </div>
      </div>

      {/* DAG 编辑器（全宽全高） */}
      <div className="flex-1 overflow-hidden">
        <LifecycleDagEditor />
      </div>
    </div>
  );
}
