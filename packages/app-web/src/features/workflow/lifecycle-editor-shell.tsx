/**
 * LifecycleEditorShell —— 统一的 Activity Lifecycle 编辑器入口。
 *
 * 单一布局：左侧 DAG 画布固定常驻，右侧 sidebar 由 store 的 selection 模型驱动：
 *   - selection.kind === "activity"   → ActivityInspector
 *   - selection.kind === "transition" → TransitionInspector
 *   - selection === null              → LifecycleHeader（顶层信息）
 *
 * 不再有 Form / DAG 双模式；不再读写 sticky_dag。1 个 activity 也直接画 1 个节点。
 *
 * 保存语义：单 save → 内部先 upsert 每个 Agent activity 的 AgentProcedure，再 upsert lifecycle。
 */

import { useCallback, useEffect } from "react";

import type {
  ActivityDefinition,
  ActivityTransition,
  WorkflowTargetKind,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import type { LifecycleDraftSeed, LifecycleSelection } from "../../stores/workflowStore";
import { transitionId as deriveTransitionId } from "../../stores/workflowStore";
import { LifecycleDagCanvas } from "./ui/lifecycle-dag-canvas";
import { ActivityInspector } from "./ui/activity-inspector";
import { TransitionInspector } from "./ui/transition-inspector";
import { ValidationPanel } from "./ui/validation-panel";
import {
  TARGET_KIND_LABEL,
  TARGET_KIND_OPTIONS,
} from "./shared-labels";

export interface LifecycleEditorShellProps {
  /** "new" 表示新建；否则是 lifecycle definition id */
  lifecycleId: string | "new";
  /** 新建时的种子：key / name / initial_activity_key */
  seed?: LifecycleDraftSeed;
  /** 项目 id（仅新建模式使用） */
  projectId: string;
  /** 保存成功后回调 —— 外层可选择 navigate 到新 id */
  onSaved?: (lifecycleId: string) => void;
}

export function LifecycleEditorShell({
  lifecycleId,
  seed,
  projectId,
  onSaved,
}: LifecycleEditorShellProps) {
  const draft = useWorkflowStore((s) => s.lifecycleEditor.draft);
  const procedureDraftsByActivityKey = useWorkflowStore((s) => s.lifecycleEditor.procedureDraftsByActivityKey);
  const selection = useWorkflowStore((s) => s.lifecycleEditor.selection);
  const validation = useWorkflowStore((s) => s.lifecycleEditor.validation);
  const isSaving = useWorkflowStore((s) => s.lifecycleEditor.isSaving);
  const isValidating = useWorkflowStore((s) => s.lifecycleEditor.isValidating);
  const isDirty = useWorkflowStore((s) => s.lifecycleEditor.dirty);
  const isLoading = useWorkflowStore((s) => s.lifecycleEditor.isLoading);
  const error = useWorkflowStore((s) => s.lifecycleEditor.error);

  const hookPresets = useWorkflowStore((s) => s.hookPresets);
  const allProcedureDefs = useWorkflowStore((s) => s.definitions);

  const fetchHookPresets = useWorkflowStore((s) => s.fetchHookPresets);
  const fetchDefinitions = useWorkflowStore((s) => s.fetchDefinitions);
  const openLifecycleForm = useWorkflowStore((s) => s.openLifecycleForm);
  const openLifecycleById = useWorkflowStore((s) => s.openLifecycleById);
  const selectLifecycleActivity = useWorkflowStore((s) => s.selectLifecycleActivity);
  const selectLifecycleTransition = useWorkflowStore((s) => s.selectLifecycleTransition);
  const updateLifecycleEditorDraft = useWorkflowStore((s) => s.updateLifecycleEditorDraft);
  const updateLifecycleEditorActivity = useWorkflowStore((s) => s.updateLifecycleEditorActivity);
  const updateActivityProcedureDraft = useWorkflowStore((s) => s.updateActivityProcedureDraft);
  const addLifecycleEditorActivity = useWorkflowStore((s) => s.addLifecycleEditorActivity);
  const removeLifecycleEditorActivity = useWorkflowStore((s) => s.removeLifecycleEditorActivity);
  const setActivityExecutor = useWorkflowStore((s) => s.setActivityExecutor);
  const setActivityCompletionPolicy = useWorkflowStore((s) => s.setActivityCompletionPolicy);
  const setActivityIterationPolicy = useWorkflowStore((s) => s.setActivityIterationPolicy);
  const setActivityJoinPolicy = useWorkflowStore((s) => s.setActivityJoinPolicy);
  const updateLifecycleEditorTransition = useWorkflowStore((s) => s.updateLifecycleEditorTransition);
  const setTransitionKind = useWorkflowStore((s) => s.setTransitionKind);
  const addArtifactBinding = useWorkflowStore((s) => s.addArtifactBinding);
  const updateArtifactBinding = useWorkflowStore((s) => s.updateArtifactBinding);
  const removeArtifactBinding = useWorkflowStore((s) => s.removeArtifactBinding);
  const validateLifecycleBundle = useWorkflowStore((s) => s.validateLifecycleBundle);
  const saveLifecycleBundle = useWorkflowStore((s) => s.saveLifecycleBundle);
  const closeLifecycleEditor = useWorkflowStore((s) => s.closeLifecycleEditor);

  // ── 加载 hook presets + AgentProcedure definitions ──
  useEffect(() => {
    if (hookPresets.length === 0) void fetchHookPresets();
  }, [hookPresets.length, fetchHookPresets]);

  useEffect(() => {
    if (projectId) void fetchDefinitions({ projectId });
  }, [fetchDefinitions, projectId]);

  // ── 加载 lifecycle bundle ──
  useEffect(() => {
    if (lifecycleId === "new") {
      openLifecycleForm(projectId, seed);
    } else {
      void openLifecycleById(lifecycleId);
    }
    return () => {
      closeLifecycleEditor();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lifecycleId]);

  // ── 保存 ──
  const handleSave = useCallback(async () => {
    const result = await validateLifecycleBundle();
    if (result && result.issues.some((i) => i.severity === "error")) return;
    const saved = await saveLifecycleBundle();
    if (saved) onSaved?.(saved.id);
  }, [validateLifecycleBundle, saveLifecycleBundle, onSaved]);

  // Ctrl+S
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        if (!isSaving) void handleSave();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [handleSave, isSaving]);

  // beforeunload
  useEffect(() => {
    if (!isDirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [isDirty]);

  if (isLoading && !draft) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          {/* eslint-disable-next-line no-restricted-syntax -- spinner 圆形 */}
          <div className="mx-auto h-7 w-7 animate-spin rounded-full border-2 border-primary border-t-transparent" />
          <p className="mt-3 text-sm text-muted-foreground">正在加载 Workflow...</p>
        </div>
      </div>
    );
  }

  if (!draft) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-sm text-muted-foreground">未找到 Workflow 定义</p>
      </div>
    );
  }

  const isNew = !useWorkflowStore.getState().lifecycleEditor.originalId;
  const hasErrors = validation?.issues.some((i) => i.severity === "error") ?? false;
  const availableProcedures = allProcedureDefs.filter((definition) => definition.project_id === draft.project_id);

  // selection 派生：transition selection 时把 transition 对象解析出来
  const selectedActivityKey =
    selection?.kind === "activity" ? selection.activityKey : null;
  const selectedTransitionId =
    selection?.kind === "transition" ? selection.transitionId : null;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <TopBar
        title={`Workflow 编辑器 — ${draft.name || draft.key || "新建"}`}
        isDirty={isDirty}
        isSaving={isSaving}
        isValidating={isValidating}
        hasErrors={hasErrors}
        onValidate={() => void validateLifecycleBundle()}
        onSave={() => void handleSave()}
      />

      {error && (
        <div className="shrink-0 border-b border-destructive/30 bg-destructive/5 px-6 py-2">
          <p className="text-xs text-destructive">{error}</p>
        </div>
      )}

      <div className="flex h-full min-h-0 flex-1">
        {/* 左：DAG 画布 */}
        <div className="relative flex-1">
          <LifecycleDagCanvas
            storageKey={draft.key || "__new__"}
            activities={draft.activities}
            transitions={draft.transitions}
            entryActivityKey={draft.entry_activity_key}
            procedureDefs={availableProcedures}
            selectedActivityKey={selectedActivityKey}
            selectedTransitionId={selectedTransitionId}
            validationIssues={validation?.issues ?? []}
            onSelectActivity={selectLifecycleActivity}
            onSelectTransition={selectLifecycleTransition}
            onActivitiesChange={(next) => updateLifecycleEditorDraft({ activities: next })}
            onEdgesChange={(next) => updateLifecycleEditorDraft({ transitions: next })}
            onAddActivity={() => addLifecycleEditorActivity()}
            bottomLeftOverlay={
              validation && validation.issues.length > 0 ? (
                <div className="max-h-40 w-96 overflow-y-auto rounded-[8px] border border-border bg-background/95 shadow-sm backdrop-blur-sm">
                  <ValidationPanel issues={validation.issues} />
                </div>
              ) : null
            }
          />
        </div>

        {/* 右：Inspector / Lifecycle 配置（按 selection 路由） */}
        <div className="flex w-96 shrink-0 flex-col border-l border-border bg-background">
          <SidebarRouter
            selection={selection}
            draft={draft}
            procedureDraftsByActivityKey={procedureDraftsByActivityKey}
            availableProcedures={availableProcedures}
            hookPresets={hookPresets}
            isNew={isNew}
            onLifecycleChange={updateLifecycleEditorDraft}
            onActivityChange={updateLifecycleEditorActivity}
            onProcedureDraftChange={updateActivityProcedureDraft}
            onSetActivityExecutor={setActivityExecutor}
            onSetActivityCompletionPolicy={setActivityCompletionPolicy}
            onSetActivityIterationPolicy={setActivityIterationPolicy}
            onSetActivityJoinPolicy={setActivityJoinPolicy}
            onSetEntry={(activityKey) =>
              updateLifecycleEditorDraft({ entry_activity_key: activityKey })
            }
            onRemoveActivity={removeLifecycleEditorActivity}
            onSelectActivity={selectLifecycleActivity}
            onSelectTransition={selectLifecycleTransition}
            onTransitionChange={updateLifecycleEditorTransition}
            onSetTransitionKind={setTransitionKind}
            onAddBinding={addArtifactBinding}
            onUpdateBinding={updateArtifactBinding}
            onRemoveBinding={removeArtifactBinding}
          />
        </div>
      </div>
    </div>
  );
}

// ─── Top Bar ────────────────────────────────────────────

function TopBar({
  title,
  isDirty,
  isSaving,
  isValidating,
  hasErrors,
  onValidate,
  onSave,
}: {
  title: string;
  isDirty: boolean;
  isSaving: boolean;
  isValidating: boolean;
  hasErrors: boolean;
  onValidate: () => void;
  onSave: () => void;
}) {
  return (
    <div className="flex shrink-0 items-center justify-between border-b border-border bg-background px-6 py-3">
      <div className="flex items-center gap-3">
        <p className="text-sm font-semibold tracking-tight text-foreground">{title}</p>
        {isDirty && (
          <span className="rounded-[8px] bg-warning/10 px-2 py-0.5 text-[10px] text-warning">
            未保存
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onValidate}
          disabled={isValidating}
          className="agentdash-button-secondary text-sm"
        >
          {isValidating ? "校验中…" : "校验"}
        </button>
        <button
          type="button"
          onClick={onSave}
          disabled={isSaving || hasErrors}
          className="agentdash-button-primary text-sm"
        >
          {isSaving ? "保存中…" : "保存"}
        </button>
      </div>
    </div>
  );
}

// ─── Sidebar Router ─────────────────────────────────────

interface SidebarRouterProps {
  selection: LifecycleSelection | null;
  draft: NonNullable<ReturnType<typeof useWorkflowStore.getState>["lifecycleEditor"]["draft"]>;
  procedureDraftsByActivityKey: ReturnType<
    typeof useWorkflowStore.getState
  >["lifecycleEditor"]["procedureDraftsByActivityKey"];
  availableProcedures: ReturnType<typeof useWorkflowStore.getState>["definitions"];
  hookPresets: ReturnType<typeof useWorkflowStore.getState>["hookPresets"];
  isNew: boolean;
  onLifecycleChange: (patch: Partial<SidebarRouterProps["draft"]>) => void;
  onActivityChange: (activityKey: string, patch: Partial<ActivityDefinition>) => void;
  onProcedureDraftChange: (
    activityKey: string,
    patch: Partial<SidebarRouterProps["procedureDraftsByActivityKey"][string]>,
  ) => void;
  onSetActivityExecutor: ReturnType<typeof useWorkflowStore.getState>["setActivityExecutor"];
  onSetActivityCompletionPolicy: ReturnType<
    typeof useWorkflowStore.getState
  >["setActivityCompletionPolicy"];
  onSetActivityIterationPolicy: ReturnType<
    typeof useWorkflowStore.getState
  >["setActivityIterationPolicy"];
  onSetActivityJoinPolicy: ReturnType<typeof useWorkflowStore.getState>["setActivityJoinPolicy"];
  onSetEntry: (activityKey: string) => void;
  onRemoveActivity: (activityKey: string) => void;
  onSelectActivity: (activityKey: string | null) => void;
  onSelectTransition: (transitionId: string | null) => void;
  onTransitionChange: (id: string, patch: Partial<ActivityTransition>) => void;
  onSetTransitionKind: (id: string, kind: ActivityTransition["kind"]) => void;
  onAddBinding: ReturnType<typeof useWorkflowStore.getState>["addArtifactBinding"];
  onUpdateBinding: ReturnType<typeof useWorkflowStore.getState>["updateArtifactBinding"];
  onRemoveBinding: ReturnType<typeof useWorkflowStore.getState>["removeArtifactBinding"];
}

function SidebarRouter(props: SidebarRouterProps) {
  const {
    selection,
    draft,
    procedureDraftsByActivityKey,
    availableProcedures,
    hookPresets,
    isNew,
    onLifecycleChange,
    onActivityChange,
    onProcedureDraftChange,
    onSetActivityExecutor,
    onSetActivityCompletionPolicy,
    onSetActivityIterationPolicy,
    onSetActivityJoinPolicy,
    onSetEntry,
    onRemoveActivity,
    onSelectActivity,
    onSelectTransition,
    onTransitionChange,
    onSetTransitionKind,
    onAddBinding,
    onUpdateBinding,
    onRemoveBinding,
  } = props;

  // selection.kind === "activity" 路由
  if (selection?.kind === "activity") {
    const activity = draft.activities.find((a) => a.key === selection.activityKey);
    const procedureDraft = procedureDraftsByActivityKey[selection.activityKey];
    if (!activity || !procedureDraft) {
      return <SidebarPlaceholder text="找不到选中的 activity，请重新选择" />;
    }
    return (
      <ActivityInspector
        activity={activity}
        procedureDraft={procedureDraft}
        isEntry={activity.key === draft.entry_activity_key}
        availableProcedures={availableProcedures}
        hookPresets={hookPresets}
        targetKinds={draft.target_kinds}
        projectId={draft.project_id}
        onActivityChange={(patch) => onActivityChange(activity.key, patch)}
        onProcedureDraftChange={(patch) => onProcedureDraftChange(activity.key, patch)}
        onSetExecutor={(executor) => onSetActivityExecutor(activity.key, executor)}
        onSetCompletionPolicy={(policy) => onSetActivityCompletionPolicy(activity.key, policy)}
        onSetIterationPolicy={(patch) => onSetActivityIterationPolicy(activity.key, patch)}
        onSetJoinPolicy={(policy) => onSetActivityJoinPolicy(activity.key, policy)}
        onSetEntry={() => onSetEntry(activity.key)}
        onRemove={() => onRemoveActivity(activity.key)}
        onClose={() => onSelectActivity(null)}
      />
    );
  }

  // selection.kind === "transition" 路由
  if (selection?.kind === "transition") {
    const transition = findTransitionById(draft.transitions, selection.transitionId);
    if (!transition) {
      return <SidebarPlaceholder text="找不到选中的 transition，请重新选择" />;
    }
    return (
      <TransitionInspector
        transition={transition}
        activities={draft.activities}
        onClose={() => onSelectTransition(null)}
        onSetKind={(kind) => onSetTransitionKind(selection.transitionId, kind)}
        onConditionChange={(condition) =>
          onTransitionChange(selection.transitionId, { condition })
        }
        onMaxTraversalsChange={(max_traversals) =>
          onTransitionChange(selection.transitionId, { max_traversals: max_traversals ?? undefined })
        }
        onAddBinding={(binding) => onAddBinding(selection.transitionId, binding)}
        onUpdateBinding={(idx, patch) => onUpdateBinding(selection.transitionId, idx, patch)}
        onRemoveBinding={(idx) => onRemoveBinding(selection.transitionId, idx)}
      />
    );
  }

  // 无选中 → LifecycleHeader
  return <LifecycleHeader draft={draft} isNew={isNew} onChange={onLifecycleChange} />;
}

function findTransitionById(
  transitions: ActivityTransition[],
  id: string,
): ActivityTransition | null {
  const idx = transitions.findIndex((t, i) => deriveTransitionId(t, i) === id);
  return idx >= 0 ? transitions[idx] : null;
}

function SidebarPlaceholder({ text }: { text: string }) {
  return (
    <div className="flex h-full items-center justify-center px-4">
      <p className="text-center text-xs text-muted-foreground">{text}</p>
    </div>
  );
}

// ─── Lifecycle 顶层信息 header ─────────────────────────

function LifecycleHeader({
  draft,
  isNew,
  onChange,
}: {
  draft: NonNullable<ReturnType<typeof useWorkflowStore.getState>["lifecycleEditor"]["draft"]>;
  isNew: boolean;
  onChange: (patch: Partial<typeof draft>) => void;
}) {
  const toggleKind = (value: WorkflowTargetKind) => {
    const cur = draft.target_kinds;
    if (cur.includes(value)) {
      const next = cur.filter((k) => k !== value);
      if (next.length > 0) onChange({ target_kinds: next });
    } else {
      onChange({ target_kinds: [...cur, value] });
    }
  };

  return (
    <section className="flex-1 space-y-3 overflow-y-auto p-4">
      <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
        Lifecycle 信息
      </p>

      <div>
        <label className="agentdash-form-label">Key</label>
        <input
          value={draft.key}
          onChange={(e) => onChange({ key: e.target.value })}
          disabled={!isNew}
          className="agentdash-form-input disabled:opacity-60"
          placeholder="my_workflow"
        />
      </div>

      <div>
        <label className="agentdash-form-label">名称</label>
        <input
          value={draft.name}
          onChange={(e) => onChange({ name: e.target.value })}
          className="agentdash-form-input"
          placeholder="My Workflow"
        />
      </div>

      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={draft.description}
          onChange={(e) => onChange({ description: e.target.value })}
          rows={2}
          className="agentdash-form-textarea"
          placeholder="这个 Workflow 做什么"
        />
      </div>

      <div>
        <label className="agentdash-form-label">挂载类型</label>
        <div className="flex flex-wrap gap-2">
          {TARGET_KIND_OPTIONS.map((kind) => {
            const checked = draft.target_kinds.includes(kind);
            return (
              <label
                key={kind}
                className={`flex cursor-pointer items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-xs transition-colors ${
                  checked
                    ? "border-primary/40 bg-primary/5 text-foreground"
                    : "border-border bg-background text-muted-foreground hover:border-primary/20"
                }`}
              >
                <input
                  type="checkbox"
                  checked={checked}
                  onChange={() => toggleKind(kind)}
                  className="sr-only"
                />
                {TARGET_KIND_LABEL[kind]}
              </label>
            );
          })}
        </div>
      </div>

      <div>
        <label className="agentdash-form-label">入口节点</label>
        <input
          value={draft.entry_activity_key}
          onChange={(e) => onChange({ entry_activity_key: e.target.value })}
          list="lifecycle-entry-activity-opts-shell"
          className="agentdash-form-input"
          placeholder="start"
        />
        <datalist id="lifecycle-entry-activity-opts-shell">
          {draft.activities.filter((a) => a.key).map((a) => (
            <option key={a.key} value={a.key} />
          ))}
        </datalist>
      </div>
    </section>
  );
}
