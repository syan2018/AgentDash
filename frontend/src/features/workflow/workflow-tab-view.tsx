import { useEffect, useMemo, useState } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { ROLE_LABEL, ROLE_ORDER, DEFAULT_ROLE_BY_TARGET } from "./shared-labels";
import { DetailPanel } from "../../components/ui/detail-panel";
import { WorkflowEditor } from "./workflow-editor";
import { LifecycleEditor } from "./lifecycle-editor";
import type {
  LifecycleDefinition,
  WorkflowAgentRole,
  WorkflowAssignment,
  WorkflowDefinition,
  WorkflowTemplate,
} from "../../types";

const EMPTY_ASSIGNMENTS: WorkflowAssignment[] = [];

function resolveRole(
  item: { recommended_roles?: WorkflowAgentRole[]; target_kind: string },
): WorkflowAgentRole {
  return (
    item.recommended_roles?.[0]
    ?? DEFAULT_ROLE_BY_TARGET[item.target_kind as keyof typeof DEFAULT_ROLE_BY_TARGET]
    ?? "task"
  );
}

function statusStyle(status: string): { bg: string; text: string } {
  if (status === "active") return { bg: "bg-emerald-500/10 border-emerald-300/40", text: "text-emerald-700" };
  if (status === "disabled") return { bg: "bg-amber-500/10 border-amber-300/40", text: "text-amber-700" };
  return { bg: "bg-secondary/40 border-border", text: "text-muted-foreground" };
}

function StatusPill({ status, label }: { status: string; label: string }) {
  const s = statusStyle(status);
  return (
    <span className={`rounded-full border px-2 py-0.5 text-[10px] ${s.bg} ${s.text}`}>
      {label}
    </span>
  );
}

export function WorkflowTabView() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const templates = useWorkflowStore((s) => s.templates);
  const definitions = useWorkflowStore((s) => s.definitions);
  const lifecycles = useWorkflowStore((s) => s.lifecycleDefinitions);
  const assignments = useWorkflowStore(
    (s) => currentProjectId ? (s.assignmentsByProjectId[currentProjectId] ?? EMPTY_ASSIGNMENTS) : EMPTY_ASSIGNMENTS,
  );
  const error = useWorkflowStore((s) => s.error);
  const fetchTemplates = useWorkflowStore((s) => s.fetchTemplates);
  const fetchDefinitions = useWorkflowStore((s) => s.fetchDefinitions);
  const fetchLifecycles = useWorkflowStore((s) => s.fetchLifecycles);
  const fetchProjectAssignments = useWorkflowStore((s) => s.fetchProjectAssignments);
  const bootstrapTemplate = useWorkflowStore((s) => s.bootstrapTemplate);
  const assignLifecycleToProject = useWorkflowStore((s) => s.assignLifecycleToProject);
  const enableDefinition = useWorkflowStore((s) => s.enableDefinition);
  const disableDefinition = useWorkflowStore((s) => s.disableDefinition);
  const enableLifecycle = useWorkflowStore((s) => s.enableLifecycle);
  const disableLifecycle = useWorkflowStore((s) => s.disableLifecycle);
  const openNewDraft = useWorkflowStore((s) => s.openNewDraft);
  const openEditDraft = useWorkflowStore((s) => s.openEditDraft);
  const openNewLifecycleDraft = useWorkflowStore((s) => s.openNewLifecycleDraft);
  const openEditLifecycleDraft = useWorkflowStore((s) => s.openEditLifecycleDraft);
  const editorDraft = useWorkflowStore((s) => s.editorDraft);
  const lifecycleEditorDraft = useWorkflowStore((s) => s.lifecycleEditorDraft);
  const closeDraft = useWorkflowStore((s) => s.closeDraft);
  const closeLifecycleDraft = useWorkflowStore((s) => s.closeLifecycleDraft);

  const [tab, setTab] = useState<"lifecycle" | "workflow">("lifecycle");
  const [message, setMessage] = useState<string | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);

  useEffect(() => {
    void fetchTemplates();
    void fetchDefinitions();
    void fetchLifecycles();
    if (currentProjectId) void fetchProjectAssignments(currentProjectId);
  }, [fetchTemplates, fetchDefinitions, fetchLifecycles, fetchProjectAssignments, currentProjectId]);

  useEffect(() => {
    if (!message) return;
    const t = setTimeout(() => setMessage(null), 4000);
    return () => clearTimeout(t);
  }, [message]);

  const defaultByRole = useMemo(() => {
    const map = new Map<WorkflowAgentRole, LifecycleDefinition | null>();
    for (const role of ROLE_ORDER) {
      const ra = assignments.filter((a) => a.role === role && a.enabled);
      const active = ra.find((a) => a.is_default) ?? ra[0] ?? null;
      map.set(role, active ? lifecycles.find((l) => l.id === active.lifecycle_id) ?? null : null);
    }
    return map;
  }, [assignments, lifecycles]);

  const handleBootstrap = async (tpl: WorkflowTemplate) => {
    setBusyKey(tpl.key);
    const lc = await bootstrapTemplate(tpl.key);
    if (lc) setMessage(`已注册：${tpl.name}`);
    setBusyKey(null);
  };

  const handleAssign = async (lc: LifecycleDefinition, role: WorkflowAgentRole) => {
    if (!currentProjectId) return;
    setBusyKey(`assign:${lc.id}:${role}`);
    const a = await assignLifecycleToProject({
      project_id: currentProjectId,
      lifecycle_id: lc.id,
      role,
      enabled: true,
      is_default: true,
    });
    if (a) setMessage(`已设为 ${ROLE_LABEL[role]} 默认：${lc.name}`);
    setBusyKey(null);
  };

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
        {/* 页头：对齐 Story/Agent 风格 */}
        <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-6">
          <div className="flex items-center gap-2.5">
            <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              WORKFLOW
            </span>
            <div>
              <h2 className="text-sm font-semibold tracking-tight text-foreground">工作流定义</h2>
              <p className="text-xs text-muted-foreground">
                {lifecycles.length} 个 Lifecycle · {definitions.length} 个 Workflow
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {unregisteredTemplates.length > 0 && (
              <div className="relative group">
                <button
                  type="button"
                  className="h-9 rounded-[10px] border border-border bg-background px-3.5 text-sm text-foreground transition-colors hover:bg-secondary"
                  onClick={() => void handleBootstrap(unregisteredTemplates[0])}
                  disabled={busyKey != null}
                >
                  {busyKey === unregisteredTemplates[0].key ? "注册中…" : `注册内置 Bundle (${unregisteredTemplates.length})`}
                </button>
              </div>
            )}
            <button
              type="button"
              onClick={() => { openNewLifecycleDraft(); }}
              className="h-9 rounded-[10px] border border-border bg-background px-3.5 text-sm text-foreground transition-colors hover:bg-secondary"
            >
              + Lifecycle
            </button>
            <button
              type="button"
              onClick={() => { openNewDraft(); }}
              className="h-9 rounded-[10px] border border-primary bg-primary px-3.5 text-sm text-primary-foreground transition-colors hover:opacity-95"
            >
              + Workflow
            </button>
          </div>
        </header>

        {/* 内容区 */}
        <div className="flex-1 overflow-y-auto">
          <div className="px-6 py-4 space-y-4">
            {/* 角色默认绑定摘要 */}
            <div className="flex flex-wrap gap-2">
              {ROLE_ORDER.map((role) => {
                const lc = defaultByRole.get(role);
                return lc ? (
                  <span key={role} className="rounded-full border border-primary/30 bg-primary/10 px-2.5 py-1 text-xs text-primary">
                    {ROLE_LABEL[role]}: {lc.name}
                  </span>
                ) : (
                  <span key={role} className="rounded-full border border-border bg-secondary/40 px-2.5 py-1 text-xs text-muted-foreground">
                    {ROLE_LABEL[role]}: 未配置
                  </span>
                );
              })}
            </div>

            {/* 反馈消息 */}
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

            {/* Tab 切换 */}
            <div className="flex gap-1 rounded-[10px] border border-border bg-secondary/35 p-1">
              <button
                type="button"
                onClick={() => setTab("lifecycle")}
                className={`rounded-[8px] px-3 py-1.5 text-sm transition-colors ${tab === "lifecycle" ? "bg-background font-medium text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"}`}
              >
                Lifecycle ({lifecycles.length})
              </button>
              <button
                type="button"
                onClick={() => setTab("workflow")}
                className={`rounded-[8px] px-3 py-1.5 text-sm transition-colors ${tab === "workflow" ? "bg-background font-medium text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"}`}
              >
                Workflow ({definitions.length})
              </button>
            </div>

            {/* 列表 */}
            {tab === "lifecycle" && (
              <LifecycleCardGrid
                items={lifecycles}
                defaultByRole={defaultByRole}
                busyKey={busyKey}
                onEdit={(lc) => void openEditLifecycleDraft(lc.id)}
                onEnable={(lc) => void enableLifecycle(lc.id)}
                onDisable={(lc) => void disableLifecycle(lc.id)}
                onAssign={handleAssign}
              />
            )}
            {tab === "workflow" && (
              <WorkflowCardGrid
                items={definitions}
                onEdit={(wf) => void openEditDraft(wf.id)}
                onEnable={(wf) => void enableDefinition(wf.id)}
                onDisable={(wf) => void disableDefinition(wf.id)}
              />
            )}
          </div>
        </div>
      </div>

      {/* 编辑器抽屉 */}
      <DetailPanel
        open={editorDraft != null}
        title={editorDraft?.key ? `编辑 Workflow: ${editorDraft.name || editorDraft.key}` : "新建 Workflow"}
        onClose={closeDraft}
        widthClassName="max-w-3xl"
      >
        <WorkflowEditor />
      </DetailPanel>

      <DetailPanel
        open={lifecycleEditorDraft != null}
        title={lifecycleEditorDraft?.key ? `编辑 Lifecycle: ${lifecycleEditorDraft.name || lifecycleEditorDraft.key}` : "新建 Lifecycle"}
        onClose={closeLifecycleDraft}
        widthClassName="max-w-4xl"
      >
        <LifecycleEditor />
      </DetailPanel>
    </>
  );
}

/* ─── Lifecycle 卡片网格 ─── */

const STATUS_LABEL: Record<string, string> = { draft: "草稿", active: "已激活", disabled: "已停用" };

function LifecycleCardGrid({
  items,
  defaultByRole,
  busyKey,
  onEdit,
  onEnable,
  onDisable,
  onAssign,
}: {
  items: LifecycleDefinition[];
  defaultByRole: Map<WorkflowAgentRole, LifecycleDefinition | null>;
  busyKey: string | null;
  onEdit: (lc: LifecycleDefinition) => void;
  onEnable: (lc: LifecycleDefinition) => void;
  onDisable: (lc: LifecycleDefinition) => void;
  onAssign: (lc: LifecycleDefinition, role: WorkflowAgentRole) => Promise<void>;
}) {
  if (items.length === 0) {
    return (
      <div className="rounded-[12px] border border-dashed border-border px-4 py-8 text-center text-sm text-muted-foreground">
        暂无 Lifecycle 定义
      </div>
    );
  }

  const sorted = items.slice().sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {sorted.map((lc) => {
        const role = resolveRole(lc);
        const isDefault = defaultByRole.get(role)?.id === lc.id;
        return (
          <button
            key={lc.id}
            type="button"
            onClick={() => onEdit(lc)}
            className="w-full rounded-[12px] border border-border bg-background p-3.5 text-left transition-all hover:border-primary/25 hover:bg-secondary/35"
          >
            <div className="mb-2 flex items-center gap-1.5">
              <StatusPill status={lc.status} label={STATUS_LABEL[lc.status] ?? lc.status} />
              <span className="rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
                {ROLE_LABEL[role]}
              </span>
              {isDefault && (
                <span className="rounded-full border border-primary/30 bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">默认</span>
              )}
            </div>

            <p className="truncate text-sm font-medium leading-6 text-foreground">{lc.name}</p>
            <p className="mt-0.5 truncate text-xs text-muted-foreground">{lc.key}</p>
            {lc.description && (
              <p className="mt-1.5 line-clamp-2 text-xs leading-5 text-muted-foreground">{lc.description}</p>
            )}

            <div className="mt-3 flex items-center justify-between border-t border-border/70 pt-2.5 text-xs text-muted-foreground">
              <span>{lc.steps.length} 个 Step</span>
              <div className="flex gap-1" onClick={(e) => e.stopPropagation()}>
                {lc.status === "active" ? (
                  <span role="button" tabIndex={0} onClick={() => onDisable(lc)} onKeyDown={(e) => { if (e.key === "Enter") onDisable(lc); }} className="rounded-[6px] px-1.5 py-0.5 text-[10px] text-amber-700 transition-colors hover:bg-amber-500/10">
                    停用
                  </span>
                ) : (
                  <span role="button" tabIndex={0} onClick={() => onEnable(lc)} onKeyDown={(e) => { if (e.key === "Enter") onEnable(lc); }} className="rounded-[6px] px-1.5 py-0.5 text-[10px] text-emerald-700 transition-colors hover:bg-emerald-500/10">
                    激活
                  </span>
                )}
                {!isDefault && (
                  <span
                    role="button"
                    tabIndex={0}
                    onClick={() => void onAssign(lc, role)}
                    onKeyDown={(e) => { if (e.key === "Enter") void onAssign(lc, role); }}
                    className={`rounded-[6px] px-1.5 py-0.5 text-[10px] text-primary transition-colors hover:bg-primary/10 ${busyKey != null ? "pointer-events-none opacity-50" : ""}`}
                  >
                    设为默认
                  </span>
                )}
              </div>
            </div>
          </button>
        );
      })}
    </div>
  );
}

/* ─── Workflow 卡片网格 ─── */

function WorkflowCardGrid({
  items,
  onEdit,
  onEnable,
  onDisable,
}: {
  items: WorkflowDefinition[];
  onEdit: (wf: WorkflowDefinition) => void;
  onEnable: (wf: WorkflowDefinition) => void;
  onDisable: (wf: WorkflowDefinition) => void;
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
      {sorted.map((wf) => {
        const role = resolveRole(wf);
        const bindCount = wf.contract.injection.context_bindings.length;
        const ruleCount = wf.contract.constraints.length;
        const checkCount = wf.contract.completion.checks.length;
        return (
          <button
            key={wf.id}
            type="button"
            onClick={() => onEdit(wf)}
            className="w-full rounded-[12px] border border-border bg-background p-3.5 text-left transition-all hover:border-primary/25 hover:bg-secondary/35"
          >
            <div className="mb-2 flex items-center gap-1.5">
              <StatusPill status={wf.status} label={STATUS_LABEL[wf.status] ?? wf.status} />
              <span className="rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
                {ROLE_LABEL[role]}
              </span>
            </div>

            <p className="truncate text-sm font-medium leading-6 text-foreground">{wf.name}</p>
            <p className="mt-0.5 truncate text-xs text-muted-foreground">{wf.key}</p>
            {wf.description && (
              <p className="mt-1.5 line-clamp-2 text-xs leading-5 text-muted-foreground">{wf.description}</p>
            )}

            <div className="mt-3 flex items-center justify-between border-t border-border/70 pt-2.5 text-xs text-muted-foreground">
              <div className="flex gap-2">
                {bindCount > 0 && <span>{bindCount} bind</span>}
                {ruleCount > 0 && <span>{ruleCount} rule</span>}
                {checkCount > 0 && <span>{checkCount} check</span>}
                {bindCount + ruleCount + checkCount === 0 && <span>空 contract</span>}
              </div>
              <div onClick={(e) => e.stopPropagation()}>
                {wf.status === "active" ? (
                  <span role="button" tabIndex={0} onClick={() => onDisable(wf)} onKeyDown={(e) => { if (e.key === "Enter") onDisable(wf); }} className="rounded-[6px] px-1.5 py-0.5 text-[10px] text-amber-700 transition-colors hover:bg-amber-500/10">
                    停用
                  </span>
                ) : (
                  <span role="button" tabIndex={0} onClick={() => onEnable(wf)} onKeyDown={(e) => { if (e.key === "Enter") onEnable(wf); }} className="rounded-[6px] px-1.5 py-0.5 text-[10px] text-emerald-700 transition-colors hover:bg-emerald-500/10">
                    激活
                  </span>
                )}
              </div>
            </div>
          </button>
        );
      })}
    </div>
  );
}
