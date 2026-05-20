/**
 * LifecycleEditorShell —— 统一的 Workflow 资产编辑器入口。
 *
 * 按 activity 规模自适应布局：
 *   - Form 模式：activities.length === 1 && transitions.length === 0 && !sticky_dag
 *     - 头部 Lifecycle 基础信息 + 单 activity 的所有 contract panel 平铺
 *   - DAG 模式：进入后永不回退（sticky_dag = true）
 *     - 左侧 DAG 画布；右侧选中 step 的 inline inspector（不再开抽屉）
 *
 * sticky_dag 粘性：使用 localStorage 记忆；首次从 Form 升 DAG 后置 true，
 * 之后即便删回 1 step 也保持 DAG。
 *
 * 保存语义：单 save → 内部先 upsert 每个 Agent activity 的 workflow，再 upsert lifecycle。
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import type {
  ActivityDefinition,
  ActivityTransition,
  WorkflowDefinition,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import type { LifecycleDraftSeed } from "../../stores/workflowStore";
import { LifecycleDagCanvas } from "./ui/lifecycle-dag-canvas";
import { StepInspector } from "./ui/step-inspector";
import { ValidationPanel } from "./ui/validation-panel";
import {
  TARGET_KIND_LABEL,
  TARGET_KIND_OPTIONS,
} from "./shared-labels";
import type { WorkflowTargetKind } from "../../types";

const STICKY_DAG_PREFIX = "agentdash:editor-dag-sticky:";

function readStickyDag(lifecycleKey: string): boolean {
  try {
    return localStorage.getItem(STICKY_DAG_PREFIX + lifecycleKey) === "1";
  } catch {
    return false;
  }
}

function writeStickyDag(lifecycleKey: string, value: boolean) {
  try {
    if (value) localStorage.setItem(STICKY_DAG_PREFIX + lifecycleKey, "1");
    else localStorage.removeItem(STICKY_DAG_PREFIX + lifecycleKey);
  } catch {
    // 忽略
  }
}

function migrateStickyDag(fromKey: string, toKey: string) {
  try {
    const v = localStorage.getItem(STICKY_DAG_PREFIX + fromKey);
    if (v === "1") {
      localStorage.setItem(STICKY_DAG_PREFIX + toKey, "1");
      localStorage.removeItem(STICKY_DAG_PREFIX + fromKey);
    }
  } catch {
    // 忽略
  }
}

export interface LifecycleEditorShellProps {
  /** "new" 表示新建；否则是 lifecycle definition id */
  lifecycleId: string | "new";
  /** 新建时的种子：key / name / initial_step_key */
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
  const workflowDraftsByStepKey = useWorkflowStore((s) => s.lifecycleEditor.workflowDraftsByStepKey);
  const selectedStepKey = useWorkflowStore((s) => s.lifecycleEditor.selectedStepKey);
  const originalId = useWorkflowStore((s) => s.lifecycleEditor.originalId);
  const validation = useWorkflowStore((s) => s.lifecycleEditor.validation);
  const isSaving = useWorkflowStore((s) => s.lifecycleEditor.isSaving);
  const isValidating = useWorkflowStore((s) => s.lifecycleEditor.isValidating);
  const isDirty = useWorkflowStore((s) => s.lifecycleEditor.dirty);
  const isLoading = useWorkflowStore((s) => s.lifecycleEditor.isLoading);
  const error = useWorkflowStore((s) => s.lifecycleEditor.error);

  const hookPresets = useWorkflowStore((s) => s.hookPresets);
  const allWorkflowDefs = useWorkflowStore((s) => s.definitions);

  const fetchHookPresets = useWorkflowStore((s) => s.fetchHookPresets);
  const fetchDefinitions = useWorkflowStore((s) => s.fetchDefinitions);
  const openLifecycleForm = useWorkflowStore((s) => s.openLifecycleForm);
  const openLifecycleById = useWorkflowStore((s) => s.openLifecycleById);
  const selectLifecycleStep = useWorkflowStore((s) => s.selectLifecycleStep);
  const updateLifecycleEditorDraft = useWorkflowStore((s) => s.updateLifecycleEditorDraft);
  const updateLifecycleEditorStep = useWorkflowStore((s) => s.updateLifecycleEditorStep);
  const updateStepWorkflowDraft = useWorkflowStore((s) => s.updateStepWorkflowDraft);
  const addLifecycleEditorStep = useWorkflowStore((s) => s.addLifecycleEditorStep);
  const removeLifecycleEditorStep = useWorkflowStore((s) => s.removeLifecycleEditorStep);
  const cloneWorkflowIntoStep = useWorkflowStore((s) => s.cloneWorkflowIntoStep);
  const validateLifecycleBundle = useWorkflowStore((s) => s.validateLifecycleBundle);
  const saveLifecycleBundle = useWorkflowStore((s) => s.saveLifecycleBundle);
  const closeLifecycleEditor = useWorkflowStore((s) => s.closeLifecycleEditor);

  // Sticky-dag：按 lifecycle key 存 localStorage；新建用 __new__ 桶
  const [stickyDag, setStickyDag] = useState<boolean>(false);

  // ── 加载 hook presets + workflow definitions ──
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

  // ── 读取 sticky-dag（draft 就绪后） ──
  useEffect(() => {
    if (!draft) return;
    setStickyDag(readStickyDag(draft.key || "__new__"));
  }, [draft?.key, draft]);

  // ── 模式判定 ──
  const mode: "form" | "dag" = useMemo(() => {
    if (!draft) return "form";
    const stepCount = draft.activities.length;
    const edgeCount = draft.transitions.length;
    if (stickyDag) return "dag";
    if (stepCount <= 1 && edgeCount === 0) return "form";
    return "dag";
  }, [draft, stickyDag]);

  // ── 升级 DAG：Form 下"+加 step"触发 ──
  const upgradeToDag = useCallback(() => {
    const key = draft?.key || "__new__";
    writeStickyDag(key, true);
    setStickyDag(true);
  }, [draft?.key]);

  // ── 保存 ──
  const handleSave = useCallback(async () => {
    const result = await validateLifecycleBundle();
    if (result && result.issues.some((i) => i.severity === "error")) return;
    const saved = await saveLifecycleBundle();
    if (saved) {
      // 迁移 sticky-dag bucket：__new__ → 真实 key
      if (!originalId) {
        migrateStickyDag("__new__", saved.key);
      }
      onSaved?.(saved.id);
    }
  }, [originalId, validateLifecycleBundle, saveLifecycleBundle, onSaved]);

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
          {/* eslint-disable-next-line no-restricted-syntax -- 圆形 spinner，rounded-full 是必需视觉形态 */}
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

  const isNew = originalId === null;
  const hasErrors = validation?.issues.some((i) => i.severity === "error") ?? false;

  // ── 子组件 props ──

  const availableWorkflows = allWorkflowDefs.filter(
    (d) => d.project_id === draft.project_id,
  );

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* 顶部操作栏 */}
      <div className="flex shrink-0 items-center justify-between border-b border-border bg-background px-6 py-3">
        <div className="flex items-center gap-3">
          <p className="text-sm font-semibold tracking-tight text-foreground">
            Workflow 编辑器 — {draft.name || draft.key || "新建"}
          </p>
          <span className="rounded-[6px] border border-border bg-secondary/60 px-1.5 py-0.5 text-[10px] text-muted-foreground">
            {mode === "form" ? "Form 模式" : "DAG 模式"}
          </span>
          {isDirty && (
            <span className="rounded-[8px] bg-warning/10 px-2 py-0.5 text-[10px] text-warning">
              未保存
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void validateLifecycleBundle()}
            disabled={isValidating}
            className="agentdash-button-secondary text-sm"
          >
            {isValidating ? "校验中…" : "校验"}
          </button>
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={isSaving || hasErrors}
            className="agentdash-button-primary text-sm"
          >
            {isSaving ? "保存中…" : "保存"}
          </button>
        </div>
      </div>

      {error && (
        <div className="shrink-0 border-b border-destructive/30 bg-destructive/5 px-6 py-2">
          <p className="text-xs text-destructive">{error}</p>
        </div>
      )}

      {mode === "form" ? (
        <FormLayout
          draft={draft}
          isNew={isNew}
          availableWorkflows={availableWorkflows}
          workflowDraft={
            draft.activities[0] ? workflowDraftsByStepKey[draft.activities[0].key] ?? null : null
          }
          hookPresets={hookPresets}
          validation={validation}
          onLifecycleChange={updateLifecycleEditorDraft}
          onStepChange={(patch) => {
            const firstKey = draft.activities[0]?.key;
            if (firstKey) updateLifecycleEditorStep(firstKey, patch);
          }}
          onWorkflowChange={(patch) => {
            const firstKey = draft.activities[0]?.key;
            if (firstKey) updateStepWorkflowDraft(firstKey, patch);
          }}
          onCloneFromWorkflow={(wf) => {
            const firstKey = draft.activities[0]?.key;
            if (firstKey) cloneWorkflowIntoStep(firstKey, wf);
          }}
          onAddStep={() => {
            upgradeToDag();
            addLifecycleEditorStep();
          }}
        />
      ) : (
        <DagLayout
          draft={draft}
          availableWorkflows={availableWorkflows}
          workflowDraftsByStepKey={workflowDraftsByStepKey}
          selectedStepKey={selectedStepKey}
          hookPresets={hookPresets}
          validation={validation}
          onLifecycleChange={updateLifecycleEditorDraft}
          onSelectStep={selectLifecycleStep}
          onStepChange={(stepKey, patch) => updateLifecycleEditorStep(stepKey, patch)}
          onWorkflowChange={(stepKey, patch) => updateStepWorkflowDraft(stepKey, patch)}
          onCloneFromWorkflow={(stepKey, wf) => cloneWorkflowIntoStep(stepKey, wf)}
          onAddStep={() => addLifecycleEditorStep()}
          onRemoveStep={(stepKey) => removeLifecycleEditorStep(stepKey)}
        />
      )}
    </div>
  );
}

// ─── Form 模式布局 ─────────────────────────────────────

function FormLayout(props: {
  draft: NonNullable<ReturnType<typeof useWorkflowStore.getState>["lifecycleEditor"]["draft"]>;
  isNew: boolean;
  availableWorkflows: WorkflowDefinition[];
  workflowDraft:
    | ReturnType<typeof useWorkflowStore.getState>["lifecycleEditor"]["workflowDraftsByStepKey"][string]
    | null;
  hookPresets: ReturnType<typeof useWorkflowStore.getState>["hookPresets"];
  validation: ReturnType<typeof useWorkflowStore.getState>["lifecycleEditor"]["validation"];
  onLifecycleChange: (patch: Partial<typeof props.draft>) => void;
  onStepChange: (patch: Partial<ActivityDefinition>) => void;
  onWorkflowChange: (patch: Partial<NonNullable<typeof props.workflowDraft>>) => void;
  onCloneFromWorkflow: (wf: WorkflowDefinition) => void;
  onAddStep: () => void;
}) {
  const {
    draft,
    isNew,
    availableWorkflows,
    workflowDraft,
    hookPresets,
    validation,
    onLifecycleChange,
    onStepChange,
    onWorkflowChange,
    onCloneFromWorkflow,
    onAddStep,
  } = props;

  const step = draft.activities[0];
  if (!step || !workflowDraft) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <button
          type="button"
          onClick={onAddStep}
          className="agentdash-button-primary"
        >
          + 添加起始 Step
        </button>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="mx-auto max-w-4xl space-y-4 px-6 py-6">
        {/* Lifecycle 顶层信息 */}
        <LifecycleHeader
          draft={draft}
          isNew={isNew}
          onChange={onLifecycleChange}
        />

        {validation && validation.issues.length > 0 && (
          <ValidationPanel issues={validation.issues} />
        )}

        {/* Step 升级按钮 */}
        <div className="flex items-center justify-between rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-2">
          <p className="text-xs text-muted-foreground">
            当前为单 step 模式。添加第二个 step 即切到 DAG 编辑。
          </p>
          <button
            type="button"
            onClick={onAddStep}
            className="agentdash-button-secondary text-xs"
          >
            + 添加 Step（进入 DAG）
          </button>
        </div>

        {/* Step Inspector（平铺，Form 模式隐藏 tab、只展示 Detail + 顶部基础信息） */}
        <div className="rounded-[12px] border border-border bg-background">
          <StepInspector
            step={step}
            workflowDraft={workflowDraft}
            isEntry
            hideStepActions
            hideTabs
            availableWorkflows={availableWorkflows}
            hookPresets={hookPresets}
            targetKinds={draft.target_kinds}
            projectId={draft.project_id}
            onStepChange={onStepChange}
            onWorkflowChange={onWorkflowChange}
            onCloneFromWorkflow={onCloneFromWorkflow}
          />
        </div>
      </div>
    </div>
  );
}

// ─── DAG 模式布局 ──────────────────────────────────────

function DagLayout(props: {
  draft: NonNullable<ReturnType<typeof useWorkflowStore.getState>["lifecycleEditor"]["draft"]>;
  availableWorkflows: WorkflowDefinition[];
  workflowDraftsByStepKey: ReturnType<
    typeof useWorkflowStore.getState
  >["lifecycleEditor"]["workflowDraftsByStepKey"];
  selectedStepKey: string | null;
  hookPresets: ReturnType<typeof useWorkflowStore.getState>["hookPresets"];
  validation: ReturnType<typeof useWorkflowStore.getState>["lifecycleEditor"]["validation"];
  onLifecycleChange: (patch: Partial<typeof props.draft>) => void;
  onSelectStep: (stepKey: string | null) => void;
  onStepChange: (stepKey: string, patch: Partial<ActivityDefinition>) => void;
  onWorkflowChange: (
    stepKey: string,
    patch: Partial<
      NonNullable<
        ReturnType<typeof useWorkflowStore.getState>["lifecycleEditor"]["workflowDraftsByStepKey"][string]
      >
    >,
  ) => void;
  onCloneFromWorkflow: (stepKey: string, wf: WorkflowDefinition) => void;
  onAddStep: () => void;
  onRemoveStep: (stepKey: string) => void;
}) {
  const {
    draft,
    availableWorkflows,
    workflowDraftsByStepKey,
    selectedStepKey,
    hookPresets,
    validation,
    onLifecycleChange,
    onSelectStep,
    onStepChange,
    onWorkflowChange,
    onCloneFromWorkflow,
    onAddStep,
    onRemoveStep,
  } = props;

  const selectedStep =
    selectedStepKey ? draft.activities.find((s) => s.key === selectedStepKey) ?? null : null;
  const selectedWorkflowDraft =
    selectedStepKey ? workflowDraftsByStepKey[selectedStepKey] ?? null : null;

  const handleStepsChange = useCallback(
    (nextSteps: ActivityDefinition[]) => {
      onLifecycleChange({ activities: nextSteps });
    },
    [onLifecycleChange],
  );

  const handleEdgesChange = useCallback(
    (nextEdges: ActivityTransition[]) => {
      onLifecycleChange({ transitions: nextEdges });
    },
    [onLifecycleChange],
  );

  return (
    <div className="flex h-full min-h-0 flex-1">
      {/* 左：DAG 画布 */}
      <div className="relative flex-1">
        <LifecycleDagCanvas
          storageKey={draft.key || "__new__"}
          activities={draft.activities}
          transitions={draft.transitions}
          entryActivityKey={draft.entry_activity_key}
          workflowDefs={availableWorkflows}
          selectedStepKey={selectedStepKey}
          onSelectStep={onSelectStep}
          onStepsChange={handleStepsChange}
          onEdgesChange={handleEdgesChange}
          onAddStep={onAddStep}
          bottomLeftOverlay={
            validation && validation.issues.length > 0 ? (
              <div className="max-h-40 w-96 overflow-y-auto rounded-[8px] border border-border bg-background/95 shadow-sm backdrop-blur-sm">
                <ValidationPanel issues={validation.issues} />
              </div>
            ) : null
          }
        />
      </div>

      {/* 右：Inspector / Lifecycle 配置 */}
      <div className="flex w-96 shrink-0 flex-col border-l border-border bg-background">
        {selectedStep && selectedWorkflowDraft ? (
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="min-h-0 flex-1">
              <StepInspector
                step={selectedStep}
                workflowDraft={selectedWorkflowDraft}
                isEntry={selectedStep.key === draft.entry_activity_key}
                availableWorkflows={availableWorkflows}
                hookPresets={hookPresets}
                targetKinds={draft.target_kinds}
                projectId={draft.project_id}
                onStepChange={(patch) => onStepChange(selectedStep.key, patch)}
                onWorkflowChange={(patch) => onWorkflowChange(selectedStep.key, patch)}
                onSetEntry={() => onLifecycleChange({ entry_activity_key: selectedStep.key })}
                onRemove={() => onRemoveStep(selectedStep.key)}
                onClose={() => onSelectStep(null)}
                onCloneFromWorkflow={(wf) => onCloneFromWorkflow(selectedStep.key, wf)}
              />
            </div>
            <TransitionPanel
              activityKey={selectedStep.key}
              activityKeys={draft.activities.map((activity) => activity.key)}
              transitions={draft.transitions.filter((transition) => transition.from === selectedStep.key)}
              onChange={(updated) => {
                onLifecycleChange({
                  transitions: draft.transitions.map((transition) =>
                    transition.from === selectedStep.key && transition.to === updated.to
                      ? updated
                      : transition,
                  ),
                });
              }}
            />
          </div>
        ) : (
          <LifecycleHeader
            draft={draft}
            isNew={false}
            onChange={onLifecycleChange}
            compact
          />
        )}
      </div>
    </div>
  );
}

function TransitionPanel({
  activityKey,
  activityKeys,
  transitions,
  onChange,
}: {
  activityKey: string;
  activityKeys: string[];
  transitions: ActivityTransition[];
  onChange: (transition: ActivityTransition) => void;
}) {
  return (
    <section className="shrink-0 border-t border-border bg-secondary/20 p-3">
      <div className="mb-2 flex items-center justify-between">
        <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          Transitions
        </p>
        <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground">
          {transitions.length}
        </span>
      </div>
      <div className="max-h-52 space-y-2 overflow-y-auto">
        {transitions.length === 0 && (
          <p className="rounded-[8px] border border-dashed border-border bg-background px-3 py-3 text-center text-xs text-muted-foreground">
            选中连线后可在这里编辑条件
          </p>
        )}
        {transitions.map((transition) => (
          <TransitionEditor
            key={`${transition.from}->${transition.to}`}
            activityKey={activityKey}
            activityKeys={activityKeys}
            transition={transition}
            onChange={onChange}
          />
        ))}
      </div>
    </section>
  );
}

function TransitionEditor({
  activityKey,
  activityKeys,
  transition,
  onChange,
}: {
  activityKey: string;
  activityKeys: string[];
  transition: ActivityTransition;
  onChange: (transition: ActivityTransition) => void;
}) {
  const conditionKind = transition.condition.kind;
  const humanCondition =
    transition.condition.kind === "human_decision_equals"
      ? transition.condition
      : null;
  const setConditionKind = (kind: "always" | "human_decision_equals") => {
    if (kind === "always") {
      onChange({ ...transition, condition: { kind } });
      return;
    }
    onChange({
      ...transition,
      condition: {
        kind,
        activity: activityKey,
        decision_port: "decision",
        value: "approved",
      },
    });
  };

  return (
    <div className="rounded-[8px] border border-border bg-background p-2.5">
      <div className="mb-2 flex items-center justify-between gap-2">
        <span className="truncate font-mono text-[11px] text-foreground">
          {transition.from} → {transition.to}
        </span>
        <span className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 text-[10px] text-muted-foreground">
          {transition.kind}
        </span>
      </div>
      <label className="agentdash-form-label">Condition</label>
      <select
        value={conditionKind === "human_decision_equals" ? "human_decision_equals" : "always"}
        onChange={(event) => setConditionKind(event.target.value as "always" | "human_decision_equals")}
        className="agentdash-form-select"
      >
        <option value="always">Always</option>
        <option value="human_decision_equals">Human Decision Equals</option>
      </select>
      {humanCondition && (
        <div className="mt-2 grid grid-cols-3 gap-2">
          <select
            value={humanCondition.activity}
            onChange={(event) =>
              onChange({
                ...transition,
                condition: { ...humanCondition, activity: event.target.value },
              })
            }
            className="agentdash-form-select"
          >
            {activityKeys.map((key) => (
              <option key={key} value={key}>{key}</option>
            ))}
          </select>
          <input
            value={humanCondition.decision_port}
            onChange={(event) =>
              onChange({
                ...transition,
                condition: { ...humanCondition, decision_port: event.target.value },
              })
            }
            className="agentdash-form-input"
            placeholder="decision"
          />
          <select
            value={humanCondition.value}
            onChange={(event) =>
              onChange({
                ...transition,
                condition: { ...humanCondition, value: event.target.value },
              })
            }
            className="agentdash-form-select"
          >
            <option value="approved">approved</option>
            <option value="rejected">rejected</option>
          </select>
        </div>
      )}
    </div>
  );
}

// ─── Lifecycle 顶层信息 header ─────────────────────────

function LifecycleHeader({
  draft,
  isNew,
  compact = false,
  onChange,
}: {
  draft: NonNullable<ReturnType<typeof useWorkflowStore.getState>["lifecycleEditor"]["draft"]>;
  isNew: boolean;
  compact?: boolean;
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
    <div className={compact ? "flex h-full flex-col" : ""}>
      <section className={compact ? "flex-1 space-y-3 overflow-y-auto p-4" : "space-y-3"}>
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
            list="lifecycle-entry-step-opts-shell"
            className="agentdash-form-input"
            placeholder="start"
          />
          <datalist id="lifecycle-entry-step-opts-shell">
            {draft.activities.filter((s) => s.key).map((s) => (
              <option key={s.key} value={s.key} />
            ))}
          </datalist>
        </div>
      </section>
    </div>
  );
}
