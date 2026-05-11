import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { normalizeIdentifier, uniqueIdentifier } from "./model/naming";
import type {
  LifecycleDefinition,
  WorkflowTemplate,
} from "../../types";

/**
 * Workflow Tab —— 统一的 Workflow 资产列表（PR2）。
 *
 * 前端收束了原先 "Lifecycle" + "Workflow" 双 tab 的展示；这里只以 Lifecycle
 * 为展示粒度（每个 Lifecycle = 一个 Workflow 资产），编辑跳转到新的
 * `/workflow/:id` 路由。独立 Workflow contract 不再单独列出（Clone 语义）。
 */
export function WorkflowTabView() {
  const navigate = useNavigate();
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const templates = useWorkflowStore((s) => s.templates);
  const definitions = useWorkflowStore((s) => s.definitions);
  const lifecycles = useWorkflowStore((s) => s.lifecycleDefinitions);
  const error = useWorkflowStore((s) => s.error);
  const fetchTemplates = useWorkflowStore((s) => s.fetchTemplates);
  const fetchDefinitions = useWorkflowStore((s) => s.fetchDefinitions);
  const fetchLifecycles = useWorkflowStore((s) => s.fetchLifecycles);
  const bootstrapTemplate = useWorkflowStore((s) => s.bootstrapTemplate);
  const removeLifecycle = useWorkflowStore((s) => s.removeLifecycle);

  const [message, setMessage] = useState<string | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<{ id: string; name: string } | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [createName, setCreateName] = useState("");
  const [createKey, setCreateKey] = useState("");
  const [createKeyTouched, setCreateKeyTouched] = useState(false);
  const [createInitialStepName, setCreateInitialStepName] = useState("start");

  useEffect(() => {
    void fetchTemplates();
    if (!currentProjectId) return;
    void fetchDefinitions({ projectId: currentProjectId });
    void fetchLifecycles({ projectId: currentProjectId });
  }, [currentProjectId, fetchTemplates, fetchDefinitions, fetchLifecycles]);

  useEffect(() => {
    if (!message) return;
    const t = setTimeout(() => setMessage(null), 4000);
    return () => clearTimeout(t);
  }, [message]);

  const handleBootstrapAll = async (list: WorkflowTemplate[]) => {
    if (!currentProjectId || list.length === 0) return;
    setBusyKey("__all__");
    const registered: string[] = [];
    for (const tpl of list) {
      const lc = await bootstrapTemplate(tpl.key, currentProjectId);
      if (lc) registered.push(tpl.name);
    }
    if (registered.length > 0) {
      setMessage(
        registered.length === list.length
          ? `已注册 ${registered.length} 个内置 Bundle：${registered.join("、")}`
          : `已注册 ${registered.length}/${list.length}：${registered.join("、")}`,
      );
    }
    setBusyKey(null);
  };

  const handleDelete = useCallback(async () => {
    if (!confirmDelete) return;
    setBusyKey(`delete:${confirmDelete.id}`);
    const ok = await removeLifecycle(confirmDelete.id);
    if (ok) setMessage(`已删除：${confirmDelete.name}`);
    setConfirmDelete(null);
    setBusyKey(null);
  }, [confirmDelete, removeLifecycle]);

  const openCreateDialog = useCallback(() => {
    setCreateName("");
    setCreateKey("");
    setCreateKeyTouched(false);
    setCreateInitialStepName("start");
    setCreateOpen(true);
  }, []);

  const handleNameChange = useCallback(
    (value: string) => {
      setCreateName(value);
      if (!createKeyTouched) {
        setCreateKey(normalizeIdentifier(value, "workflow"));
      }
    },
    [createKeyTouched],
  );

  const handleCreate = useCallback(() => {
    const usedKeys = lifecycles.map((lc) => lc.key);
    const key = uniqueIdentifier(
      createKey || createName,
      usedKeys,
      "workflow",
    );
    const name = createName.trim() || key;
    const stepKey = uniqueIdentifier(createInitialStepName, [], "start");
    const params = new URLSearchParams({ key, name, step: stepKey });
    setCreateOpen(false);
    navigate(`/workflow/new?${params.toString()}`);
  }, [createInitialStepName, createKey, createName, lifecycles, navigate]);

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-foreground">请选择或创建项目</h2>
          <p className="mt-2 text-sm text-muted-foreground">在左侧面板选择一个项目开始使用</p>
        </div>
      </div>
    );
  }

  const unregisteredTemplates = templates.filter(
    (tpl) => !lifecycles.some((lc) => lc.key === tpl.lifecycle.key),
  );

  return (
    <>
      <div className="flex h-full flex-col overflow-hidden">
        <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-6">
          <div className="flex items-center gap-2.5">
            <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              WORKFLOW
            </span>
            <div>
              <h2 className="text-sm font-semibold tracking-tight text-foreground">Workflow 资产</h2>
              <p className="text-xs text-muted-foreground">
                {lifecycles.length} 个 Workflow · {definitions.length} 个内部 contract
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {unregisteredTemplates.length > 0 && (
              <div className="relative group">
                <button
                  type="button"
                  className="h-9 rounded-[10px] border border-border bg-background px-3.5 text-sm text-foreground transition-colors hover:bg-secondary"
                  onClick={() => void handleBootstrapAll(unregisteredTemplates)}
                  disabled={busyKey != null}
                  title={unregisteredTemplates.map((tpl) => tpl.name).join("、")}
                >
                  {busyKey === "__all__"
                    ? `注册中…(${unregisteredTemplates.length})`
                    : `注册内置 Bundle (${unregisteredTemplates.length})`}
                </button>
              </div>
            )}
            <button
              type="button"
              onClick={openCreateDialog}
              className="h-9 rounded-[10px] border border-primary bg-primary px-3.5 text-sm text-primary-foreground transition-colors hover:opacity-95"
            >
              + Workflow
            </button>
          </div>
        </header>

        <div className="flex-1 overflow-y-auto">
          <div className="px-6 py-4 space-y-4">
            {message && (
              <div className="flex items-center justify-between rounded-[10px] border border-emerald-300/30 bg-emerald-500/5 px-3 py-2">
                <p className="text-xs text-emerald-600">{message}</p>
                <button type="button" onClick={() => setMessage(null)} className="ml-2 text-xs text-emerald-600/60 hover:text-emerald-600">×</button>
              </div>
            )}
            {error && (
              <div className="rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2">
                <p className="text-xs text-destructive">{error}</p>
              </div>
            )}

            <WorkflowAssetGrid
              items={lifecycles}
              onEdit={(lc) => navigate(`/workflow/${lc.id}`)}
              onDelete={(lc) => setConfirmDelete({ id: lc.id, name: lc.name })}
            />
          </div>
        </div>
      </div>

      {/* 删除确认 */}
      {confirmDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={() => setConfirmDelete(null)}>
          <div className="w-[360px] rounded-[14px] border border-border bg-background p-5 shadow-xl" onClick={(e) => e.stopPropagation()}>
            <h3 className="text-sm font-semibold text-foreground">确认删除</h3>
            <p className="mt-2 text-xs leading-5 text-muted-foreground">
              确定要删除 Workflow{" "}
              <span className="font-medium text-foreground">{confirmDelete.name}</span> 吗？此操作不可撤销。
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setConfirmDelete(null)}
                className="rounded-[8px] border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => void handleDelete()}
                disabled={busyKey != null}
                className="rounded-[8px] border border-destructive/30 bg-destructive px-3 py-1.5 text-xs text-destructive-foreground transition-colors hover:opacity-90 disabled:opacity-50"
              >
                {busyKey != null ? "删除中…" : "删除"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 新建对话框 */}
      {createOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={() => setCreateOpen(false)}>
          <div className="w-[380px] rounded-[14px] border border-border bg-background p-5 shadow-xl" onClick={(e) => e.stopPropagation()}>
            <h3 className="text-sm font-semibold text-foreground">新建 Workflow</h3>
            <div className="mt-4 space-y-3">
              <div>
                <label className="agentdash-form-label">名称</label>
                <input
                  autoFocus
                  value={createName}
                  onChange={(event) => handleNameChange(event.target.value)}
                  className="agentdash-form-input"
                  placeholder="My Workflow"
                />
              </div>
              <div>
                <label className="agentdash-form-label">Key</label>
                <input
                  value={createKey}
                  onChange={(event) => {
                    setCreateKeyTouched(true);
                    setCreateKey(event.target.value);
                  }}
                  className="agentdash-form-input"
                  placeholder="my_workflow"
                />
              </div>
              <div>
                <label className="agentdash-form-label">起始 Step</label>
                <input
                  value={createInitialStepName}
                  onChange={(event) => setCreateInitialStepName(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") handleCreate();
                    if (event.key === "Escape") setCreateOpen(false);
                  }}
                  className="agentdash-form-input"
                  placeholder="start"
                />
              </div>
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setCreateOpen(false)}
                className="rounded-[8px] border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary"
              >
                取消
              </button>
              <button
                type="button"
                onClick={handleCreate}
                className="rounded-[8px] border border-primary bg-primary px-3 py-1.5 text-xs text-primary-foreground transition-colors hover:opacity-90"
              >
                创建
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

/* ─── Workflow 资产卡片网格 ─── */

function WorkflowAssetGrid({
  items,
  onEdit,
  onDelete,
}: {
  items: LifecycleDefinition[];
  onEdit: (lc: LifecycleDefinition) => void;
  onDelete: (lc: LifecycleDefinition) => void;
}) {
  if (items.length === 0) {
    return (
      <div className="rounded-[12px] border border-dashed border-border px-4 py-8 text-center text-sm text-muted-foreground">
        暂无 Workflow 定义
      </div>
    );
  }

  const sorted = items.slice().sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {sorted.map((lc) => (
        <button
          key={lc.id}
          type="button"
          onClick={() => onEdit(lc)}
          className="w-full rounded-[12px] border border-border bg-background p-3.5 text-left transition-all hover:border-primary/25 hover:bg-secondary/35"
        >
          <p className="truncate text-sm font-medium leading-6 text-foreground">{lc.name}</p>
          <p className="mt-0.5 truncate text-xs text-muted-foreground">{lc.key}</p>
          {lc.description && (
            <p className="mt-1.5 line-clamp-2 text-xs leading-5 text-muted-foreground">{lc.description}</p>
          )}

          <div className="mt-3 flex items-center justify-between border-t border-border/70 pt-2.5 text-xs text-muted-foreground">
            <span>
              {lc.steps.length} 个 Step
              {lc.source === "builtin_seed" && <span className="ml-2 rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-700">builtin</span>}
            </span>
            <div className="flex gap-1" onClick={(e) => e.stopPropagation()}>
              <span
                role="button"
                tabIndex={0}
                onClick={() => onDelete(lc)}
                onKeyDown={(e) => { if (e.key === "Enter") onDelete(lc); }}
                className="rounded-[6px] px-1.5 py-0.5 text-[10px] text-destructive transition-colors hover:bg-destructive/10"
              >
                删除
              </span>
            </div>
          </div>
        </button>
      ))}
    </div>
  );
}
