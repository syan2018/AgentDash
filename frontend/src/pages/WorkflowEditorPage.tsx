import { useEffect } from "react";
import { useParams, useNavigate } from "react-router-dom";

import { useWorkflowStore } from "../stores/workflowStore";
import { WorkflowEditor } from "../features/workflow/workflow-editor";

export function WorkflowEditorPage() {
  const { definitionId } = useParams<{ definitionId: string }>();
  const navigate = useNavigate();
  const editorDraft = useWorkflowStore((s) => s.editorDraft);
  const openNewDraft = useWorkflowStore((s) => s.openNewDraft);
  const openEditDraft = useWorkflowStore((s) => s.openEditDraft);
  const isLoading = useWorkflowStore((s) => s.isLoading);

  const isNew = definitionId === "new";

  useEffect(() => {
    if (isNew) {
      openNewDraft();
    } else if (definitionId) {
      void openEditDraft(definitionId);
    }
  }, [definitionId, isNew, openEditDraft, openNewDraft]);

  if (isLoading && !editorDraft) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <div className="mx-auto h-7 w-7 animate-spin rounded-full border-2 border-primary border-t-transparent" />
          <p className="mt-3 text-sm text-muted-foreground">正在加载 Workflow...</p>
        </div>
      </div>
    );
  }

  if (!editorDraft) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="max-w-sm text-center">
          <p className="text-sm text-muted-foreground">未找到 Workflow Definition</p>
          <button
            type="button"
            onClick={() => navigate("/dashboard/workflow")}
            className="mt-4 agentdash-button-secondary"
          >
            返回 Workflow 列表
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Top bar with back navigation */}
      <div className="shrink-0 border-b border-border px-6 py-3">
        <button
          type="button"
          onClick={() => navigate("/dashboard/workflow")}
          className="inline-flex items-center gap-1.5 rounded-[8px] px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m15 18-6-6 6-6"/></svg>
          返回 Workflow 列表
        </button>
      </div>

      {/* Scrollable editor area */}
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-4xl px-6 py-6">
          <WorkflowEditorPageContent />
        </div>
      </div>
    </div>
  );
}

function WorkflowEditorPageContent() {
  return <WorkflowEditor />;
}
