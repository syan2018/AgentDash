import { create } from "zustand";

import type {
  ActivityCompletionPolicy,
  ActivityDefinition,
  ActivityExecutorSpec,
  ActivityJoinPolicy,
  WorkflowGraph,
  ActivityTransition,
  ArtifactBinding,
  HookRulePreset,
  WorkflowContract,
  AgentProcedure,
  WorkflowRun,
  WorkflowTargetKind,
  WorkflowValidationResult,
} from "../types";
import {
  createWorkflowGraph,
  createAgentProcedure,
  deleteWorkflowGraph,
  deleteAgentProcedure,
  fetchWorkflowGraphs,
  fetchAgentProcedures,
  fetchWorkflowRunsBySession,
  fetchHookPresets,
  getWorkflowGraph,
  startWorkflowRun,
  updateWorkflowGraph,
  updateAgentProcedure,
  validateWorkflowGraph,
} from "../services/workflow";
import { findUnboundedCycles } from "../features/workflow/model/dag-layout";
import type { ValidationIssue } from "../types";

// ─── Draft types ─────────────────────────────────────────

export interface WorkflowEditorDraft {
  id: string | null;
  project_id: string;
  key: string;
  name: string;
  description: string;
  target_kinds: WorkflowTargetKind[];
  contract: WorkflowContract;
}

export interface LifecycleEditorDraft {
  id: string | null;
  project_id: string;
  key: string;
  name: string;
  description: string;
  target_kinds: WorkflowTargetKind[];
  entry_activity_key: string;
  activities: ActivityDefinition[];
  transitions: ActivityTransition[];
}

export interface LifecycleDraftSeed {
  key?: string;
  name?: string;
  initial_activity_key?: string;
}

// ─── Selection 模型 ───────────────────────────────────
//
// Lifecycle 编辑器中 activity 节点与 transition 边都是一等编辑对象，
// inspector 根据 selection 路由到 ActivityInspector 或 TransitionInspector。

export interface ActivitySelection {
  kind: "activity";
  activityKey: string;
}

export interface TransitionSelection {
  kind: "transition";
  transitionId: string;
}

export type LifecycleSelection = ActivitySelection | TransitionSelection;

/**
 * Transition 没有后端 stable id；前端用 `${from}-->${to}#${idx}` 派生。
 * `idx` 取自 lifecycle.transitions 数组中的索引，保证同 from-to 多边可区分。
 */
export function transitionId(transition: ActivityTransition, idx: number): string {
  return `${transition.from}-->${transition.to}#${idx}`;
}

function findTransitionIndex(transitions: ActivityTransition[], id: string): number {
  return transitions.findIndex((t, idx) => transitionId(t, idx) === id);
}

// ─── 客户端级 validation 增强：未设阈值的环 ────────────

function buildUnboundedCycleIssues(
  activities: ActivityDefinition[],
  transitions: ActivityTransition[],
): ValidationIssue[] {
  const cycles = findUnboundedCycles({ activities, transitions });
  const issues: ValidationIssue[] = [];
  for (const cycle of cycles) {
    const message = `环 [${cycle.activityKeys.join(" → ")}] 未设置 max_traversals 或 iteration_policy.max_attempts，运行时无收敛阈值`;
    for (const key of cycle.activityKeys) {
      issues.push({
        code: "cycle_unbounded",
        message,
        field_path: `activities[${key}]`,
        severity: "warning",
      });
    }
  }
  return issues;
}

// ─── Executor × CompletionPolicy 联动 ─────────────────

function isCompletionPolicyCompatible(
  policy: ActivityCompletionPolicy,
  executorKind: ActivityExecutorSpec["kind"],
): boolean {
  if (executorKind === "agent") {
    return policy.kind !== "human_decision";
  }
  if (executorKind === "function") {
    return policy.kind === "output_ports" || policy.kind === "executor_terminal";
  }
  // human
  return policy.kind === "human_decision";
}

/**
 * 切换 executor.kind 时确保 completion_policy 仍然合法；
 * 不兼容则按 kind 给出默认 policy 并标记 reset=true，UI 据此 toast 提示。
 */
export function ensurePolicyForExecutor(
  current: ActivityCompletionPolicy,
  executorKind: ActivityExecutorSpec["kind"],
): { policy: ActivityCompletionPolicy; reset: boolean } {
  if (isCompletionPolicyCompatible(current, executorKind)) {
    return { policy: current, reset: false };
  }
  if (executorKind === "human") {
    return { policy: { kind: "human_decision", decision_port: "decision" }, reset: true };
  }
  return { policy: { kind: "executor_terminal" }, reset: true };
}

/**
 * human_decision policy 的 decision_port 必须在 activity.output_ports 中存在，
 * 否则节点上看不到对应 handle、下游 condition 也无 port 可绑。
 * - 旧 policy 已是 human_decision 且 decision_port 改名 → 把 output_ports 中同 key 的项改名
 * - 否则 → 不存在则 append 一个，存在则保留
 */
function reconcileDecisionPort(
  activity: ActivityDefinition,
  oldPolicy: ActivityCompletionPolicy,
  newPolicy: ActivityCompletionPolicy,
): ActivityDefinition {
  if (newPolicy.kind !== "human_decision") return activity;
  const newKey = newPolicy.decision_port;
  if (!newKey) return activity;
  const oldKey = oldPolicy.kind === "human_decision" ? oldPolicy.decision_port : null;
  let ports = activity.output_ports;

  if (oldKey && oldKey !== newKey && ports.some((p) => p.key === oldKey)) {
    if (ports.some((p) => p.key === newKey)) {
      // 已经有同名端口，直接删掉旧的占位条目
      ports = ports.filter((p) => p.key !== oldKey);
    } else {
      ports = ports.map((p) => (p.key === oldKey ? { ...p, key: newKey } : p));
    }
  }
  if (!ports.some((p) => p.key === newKey)) {
    ports = [
      ...ports,
      { key: newKey, description: "Human decision result", gate_strategy: "existence" },
    ];
  }
  return ports === activity.output_ports ? activity : { ...activity, output_ports: ports };
}

// ─── Unified Lifecycle Editor ────────────────────────────
//
// 单 editor state：前端向用户呈现"一个 editor 编辑一个 workflow 资产"，内部
// 按后端双实体 schema 保存（先 upsert 每个 Agent activity 对应的 workflow，再
// upsert activity lifecycle）。
//
// 每个 activity 关联的 workflow contract 放在
// `workflowDraftsByActivityKey[activity.key]`，索引以 activity.key 为准
// （不是 procedure_key，避免新建 activity 时 procedure_key 还未定型的情况）。

export interface LifecycleEditorState {
  draft: LifecycleEditorDraft | null;
  /** 每个 activity 对应的 workflow contract draft，按 activity.key 索引 */
  workflowDraftsByActivityKey: Record<string, WorkflowEditorDraft>;
  /**
   * Inspector 路由的统一选中模型（activity 节点 / transition 边互斥）。
   * activity key 由 `selection.kind === "activity"` 派生。
   */
  selection: LifecycleSelection | null;
  /** 原 lifecycle definition id；null 表示新建态 */
  originalId: string | null;
  validation: WorkflowValidationResult | null;
  isSaving: boolean;
  isValidating: boolean;
  dirty: boolean;
  isLoading: boolean;
  error: string | null;
}

function activitySelection(activityKey: string | null): {
  selection: LifecycleSelection | null;
} {
  if (!activityKey) return { selection: null };
  return {
    selection: { kind: "activity", activityKey },
  };
}

function emptyLifecycleEditor(): LifecycleEditorState {
  return {
    draft: null,
    workflowDraftsByActivityKey: {},
    selection: null,
    originalId: null,
    validation: null,
    isSaving: false,
    isValidating: false,
    dirty: false,
    isLoading: false,
    error: null,
  };
}

function definitionToDraft(definition: AgentProcedure): WorkflowEditorDraft {
  return {
    id: definition.id,
    project_id: definition.project_id,
    key: definition.key,
    name: definition.name,
    description: definition.description,
    target_kinds: [...definition.target_kinds],
    contract: structuredClone(definition.contract),
  };
}

export function createEmptyLifecycleDraft(projectId = "", seed: LifecycleDraftSeed = {}): LifecycleEditorDraft {
  const initialActivityKey = seed.initial_activity_key ?? "";
  return {
    id: null,
    project_id: projectId,
    key: seed.key ?? "",
    name: seed.name ?? "",
    description: "",
    target_kinds: ["story"],
    entry_activity_key: initialActivityKey,
    activities: [{
      key: initialActivityKey,
      description: "",
      executor: {
        kind: "agent",
        procedure_key: "",
        session_policy: "spawn_child",
      },
      output_ports: [],
      input_ports: [],
      completion_policy: { kind: "executor_terminal" },
      iteration_policy: { max_attempts: 1, artifact_alias: "latest" },
      join_policy: "all",
    }],
    transitions: [],
  };
}

function lifecycleToDraft(definition: WorkflowGraph): LifecycleEditorDraft {
  return {
    id: definition.id,
    project_id: definition.project_id,
    key: definition.key,
    name: definition.name,
    description: definition.description,
    target_kinds: [...definition.target_kinds],
    entry_activity_key: definition.entry_activity_key,
    activities: structuredClone(definition.activities),
    transitions: structuredClone(definition.transitions ?? []),
  };
}

/**
 * 为 activity 创建一个对应的空 workflow contract draft。
 * 约定：activity 新建时自动派生 procedure_key = <lifecycle_key>_<activity_key>。
 */
export function createActivityWorkflowDraft(
  projectId: string,
  lifecycleKey: string,
  activityKey: string,
  targetKinds: WorkflowTargetKind[] = ["story"],
): WorkflowEditorDraft {
  const ProcedureKey = lifecycleKey && activityKey ? `${lifecycleKey}_${activityKey}` : activityKey || "";
  return {
    id: null,
    project_id: projectId,
    key: ProcedureKey,
    name: activityKey || "Untitled",
    description: "",
    target_kinds: [...targetKinds],
    contract: {
      injection: { context_bindings: [] },
      hook_rules: [],
      capability_config: { tool_directives: [], mount_directives: [] },
      output_ports: [],
      input_ports: [],
    },
  };
}

// ─── Collection helpers ──────────────────────────────────

function upsert<T extends { id: string }>(list: T[], next: T): T[] {
  const idx = list.findIndex((item) => item.id === next.id);
  if (idx >= 0) {
    const updated = [...list];
    updated[idx] = next;
    return updated;
  }
  return [next, ...list];
}

function upsertRun(
  runsBySessionId: Record<string, WorkflowRun[]>,
  run: WorkflowRun,
): Record<string, WorkflowRun[]> {
  const key = run.session_id;
  const existing = runsBySessionId[key] ?? [];
  const nextRuns = existing.some((item) => item.id === run.id)
    ? existing.map((item) => (item.id === run.id ? run : item))
    : [run, ...existing];
  nextRuns.sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime());
  return { ...runsBySessionId, [key]: nextRuns };
}

function rewriteTransitionConditionActivity(
  condition: ActivityTransition["condition"],
  oldKey: string,
  newKey: string,
): ActivityTransition["condition"] {
  switch (condition.kind) {
    case "artifact_field_equals":
    case "human_decision_equals":
    case "agent_signal_equals":
      return condition.activity === oldKey
        ? { ...condition, activity: newKey }
        : condition;
    case "always":
      return condition;
  }
}

// ─── Store ───────────────────────────────────────────────

interface WorkflowState {
  definitions: AgentProcedure[];
  lifecycleDefinitions: WorkflowGraph[];
  runsBySessionId: Record<string, WorkflowRun[]>;
  hookPresets: HookRulePreset[];
  isLoading: boolean;
  error: string | null;

  /** 合并后的统一 editor state */
  lifecycleEditor: LifecycleEditorState;

  fetchHookPresets: () => Promise<HookRulePreset[]>;
  fetchDefinitions: (opts?: { projectId?: string; targetKind?: WorkflowTargetKind }) => Promise<AgentProcedure[]>;
  fetchLifecycles: (opts?: { projectId?: string; targetKind?: WorkflowTargetKind }) => Promise<WorkflowGraph[]>;
  fetchRunsBySession: (sessionId: string) => Promise<WorkflowRun[]>;
  startRun: (input: {
    lifecycle_id?: string;
    lifecycle_key?: string;
    session_id: string;
    project_id: string;
  }) => Promise<WorkflowRun | null>;

  removeDefinition: (id: string) => Promise<boolean>;
  removeLifecycle: (id: string) => Promise<boolean>;

  // ── Unified Lifecycle Editor actions ──
  openLifecycleForm: (projectId: string, seed?: LifecycleDraftSeed) => void;
  openLifecycleById: (id: string) => Promise<void>;
  selectLifecycleActivity: (activityKey: string | null) => void;
  selectLifecycleTransition: (transitionId: string | null) => void;
  updateLifecycleEditorDraft: (patch: Partial<LifecycleEditorDraft>) => void;
  updateLifecycleEditorActivity: (activityKey: string, patch: Partial<ActivityDefinition>) => void;
  updateActivityWorkflowDraft: (activityKey: string, patch: Partial<WorkflowEditorDraft>) => void;
  addLifecycleEditorActivity: (opts?: { activityKey?: string; initialFromWorkflow?: AgentProcedure }) => string | null;
  removeLifecycleEditorActivity: (activityKey: string) => void;
  cloneWorkflowIntoActivity: (activityKey: string, source: AgentProcedure) => void;

  // ── Activity 内嵌字段编辑（粒度更细的 setter，避免 inspector 自己 patch 大对象） ──
  setActivityExecutor: (
    activityKey: string,
    executor: ActivityExecutorSpec,
  ) => { reset: boolean; previous: ActivityCompletionPolicy } | null;
  setActivityCompletionPolicy: (activityKey: string, policy: ActivityCompletionPolicy) => void;
  setActivityIterationPolicy: (
    activityKey: string,
    patch: Partial<ActivityDefinition["iteration_policy"]>,
  ) => void;
  setActivityJoinPolicy: (activityKey: string, policy: ActivityJoinPolicy) => void;

  // ── Transition 编辑（基于派生 transitionId 寻址） ──
  updateLifecycleEditorTransition: (id: string, patch: Partial<ActivityTransition>) => void;
  setTransitionKind: (id: string, kind: ActivityTransition["kind"]) => void;
  addArtifactBinding: (id: string, binding: ArtifactBinding) => void;
  updateArtifactBinding: (id: string, idx: number, patch: Partial<ArtifactBinding>) => void;
  removeArtifactBinding: (id: string, idx: number) => void;

  validateLifecycleBundle: () => Promise<WorkflowValidationResult | null>;
  saveLifecycleBundle: () => Promise<WorkflowGraph | null>;
  closeLifecycleEditor: () => void;
}

export const useWorkflowStore = create<WorkflowState>((set, get) => ({
  definitions: [],
  lifecycleDefinitions: [],
  runsBySessionId: {},
  hookPresets: [],
  isLoading: false,
  error: null,

  lifecycleEditor: emptyLifecycleEditor(),

  // ── Data fetching ──

  fetchHookPresets: async () => {
    try {
      const presets = await fetchHookPresets();
      set({ hookPresets: presets });
      return presets;
    } catch (error) {
      set({ error: (error as Error).message });
      return [];
    }
  },

  fetchDefinitions: async (opts) => {
    try {
      const definitions = await fetchAgentProcedures(opts);
      set((state) => {
        const targetKind = opts?.targetKind;
        const next = targetKind
          ? [...state.definitions.filter((item) => !item.target_kinds.includes(targetKind)), ...definitions]
          : definitions;
        return { definitions: next };
      });
      return definitions;
    } catch (error) {
      set({ error: (error as Error).message });
      return [];
    }
  },

  fetchLifecycles: async (opts) => {
    try {
      const lifecycleDefinitions = await fetchWorkflowGraphs(opts);
      set((state) => {
        const targetKind = opts?.targetKind;
        const next = targetKind
          ? [...state.lifecycleDefinitions.filter((item) => !item.target_kinds.includes(targetKind)), ...lifecycleDefinitions]
          : lifecycleDefinitions;
        return { lifecycleDefinitions: next };
      });
      return lifecycleDefinitions;
    } catch (error) {
      set({ error: (error as Error).message });
      return [];
    }
  },

  fetchRunsBySession: async (sessionId) => {
    try {
      const runs = await fetchWorkflowRunsBySession(sessionId);
      set((state) => ({
        runsBySessionId: { ...state.runsBySessionId, [sessionId]: runs },
      }));
      return runs;
    } catch (error) {
      set({ error: (error as Error).message });
      return [];
    }
  },

  startRun: async (input) => {
    set({ error: null });
    try {
      const run = await startWorkflowRun(input);
      set((state) => ({ runsBySessionId: upsertRun(state.runsBySessionId, run) }));
      return run;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  removeDefinition: async (id) => {
    set({ error: null });
    try {
      await deleteAgentProcedure(id);
      set((state) => ({ definitions: state.definitions.filter((item) => item.id !== id) }));
      return true;
    } catch (error) {
      set({ error: (error as Error).message });
      return false;
    }
  },

  removeLifecycle: async (id) => {
    set({ error: null });
    try {
      await deleteWorkflowGraph(id);
      set((state) => ({ lifecycleDefinitions: state.lifecycleDefinitions.filter((item) => item.id !== id) }));
      return true;
    } catch (error) {
      set({ error: (error as Error).message });
      return false;
    }
  },

  // ── Unified Lifecycle Editor ──

  openLifecycleForm: (projectId, seed = {}) => {
    const draft = createEmptyLifecycleDraft(projectId, seed);
    const activityKey = draft.activities[0]?.key ?? "";
    const lifecycleKey = draft.key || "__new__";
    const drafts: Record<string, WorkflowEditorDraft> = {};
    if (activityKey) {
      drafts[activityKey] = createActivityWorkflowDraft(projectId, lifecycleKey, activityKey, draft.target_kinds);
      // 同时把 agent activity 的 procedure_key 派生出来
      draft.activities[0].executor = {
        kind: "agent",
        procedure_key: drafts[activityKey].key,
        session_policy: "spawn_child",
      };
    }
    set({
      lifecycleEditor: {
        ...emptyLifecycleEditor(),
        draft,
        workflowDraftsByActivityKey: drafts,
        ...activitySelection(activityKey || null),
      },
    });
  },

  openLifecycleById: async (id) => {
    set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, isLoading: true, error: null } }));
    try {
      const definition = await getWorkflowGraph(id);
      const draft = lifecycleToDraft(definition);
      // 拉取项目下所有 workflow definitions（用于 agent activity executor.procedure_key → contract 映射）
      const wfDefs = await fetchAgentProcedures({ projectId: definition.project_id });
      const wfByKey = new Map(wfDefs.map((d) => [d.key, d]));
      const drafts: Record<string, WorkflowEditorDraft> = {};
      for (const activity of draft.activities) {
        const wfKey = activity.executor.kind === "agent" ? activity.executor.procedure_key.trim() : "";
        if (!wfKey) {
          drafts[activity.key] = createActivityWorkflowDraft(definition.project_id, draft.key, activity.key, draft.target_kinds);
          continue;
        }
        const wf = wfByKey.get(wfKey);
        if (wf) {
          drafts[activity.key] = definitionToDraft(wf);
        } else {
          // procedure_key 引用了未加载到的 workflow，落回空 draft（保留 key）
          const fallback = createActivityWorkflowDraft(definition.project_id, draft.key, activity.key, draft.target_kinds);
          fallback.key = wfKey;
          drafts[activity.key] = fallback;
        }
      }

      set((state) => ({
        lifecycleDefinitions: upsert(state.lifecycleDefinitions, definition),
        definitions: wfDefs,
        lifecycleEditor: {
          ...emptyLifecycleEditor(),
          draft,
          workflowDraftsByActivityKey: drafts,
          ...activitySelection(draft.activities[0]?.key ?? null),
          originalId: definition.id,
        },
      }));
    } catch (error) {
      set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, error: (error as Error).message, isLoading: false } }));
    }
  },

  selectLifecycleActivity: (activityKey) => {
    set((s) => ({
      lifecycleEditor: {
        ...s.lifecycleEditor,
        ...activitySelection(activityKey),
      },
    }));
  },

  selectLifecycleTransition: (id) => {
    set((s) => ({
      lifecycleEditor: {
        ...s.lifecycleEditor,
        selection: id ? { kind: "transition", transitionId: id } : null,
      },
    }));
  },

  updateLifecycleEditorDraft: (patch) => {
    set((s) => {
      if (!s.lifecycleEditor.draft) return s;
      const nextDraft = { ...s.lifecycleEditor.draft, ...patch };
      // target_kinds 改变时同步到所有 activity workflow drafts，保证 contract/lifecycle 一致
      let nextDrafts = s.lifecycleEditor.workflowDraftsByActivityKey;
      if (patch.target_kinds) {
        nextDrafts = { ...nextDrafts };
        for (const k of Object.keys(nextDrafts)) {
          nextDrafts[k] = { ...nextDrafts[k], target_kinds: [...patch.target_kinds] };
        }
      }
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: nextDraft,
          workflowDraftsByActivityKey: nextDrafts,
          dirty: true,
        },
      };
    });
  },

  updateLifecycleEditorActivity: (activityKey, patch) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;

      // 重命名 activity：连带 transitions 引用、entry_activity_key、selection、workflowDraftsByActivityKey 索引
      const nextActivities = draft.activities.map((activity) =>
        activity.key === activityKey ? { ...activity, ...patch } : activity,
      );
      let nextTransitions = draft.transitions;
      let nextEntry = draft.entry_activity_key;
      let nextSelection = s.lifecycleEditor.selection;
      let nextDrafts = s.lifecycleEditor.workflowDraftsByActivityKey;

      if (patch.key && patch.key !== activityKey) {
        const newKey = patch.key;
        nextTransitions = draft.transitions.map((transition) => ({
          ...transition,
          from: transition.from === activityKey ? newKey : transition.from,
          to: transition.to === activityKey ? newKey : transition.to,
          condition: rewriteTransitionConditionActivity(transition.condition, activityKey, newKey),
          artifact_bindings: transition.artifact_bindings.map((binding) => ({
            ...binding,
            from_activity: binding.from_activity === activityKey ? newKey : binding.from_activity,
          })),
        }));
        if (nextEntry === activityKey) nextEntry = newKey;
        if (nextSelection?.kind === "activity" && nextSelection.activityKey === activityKey) {
          nextSelection = { kind: "activity", activityKey: newKey };
        }
        if (nextDrafts[activityKey]) {
          const moved = { ...nextDrafts };
          moved[newKey] = moved[activityKey];
          delete moved[activityKey];
          nextDrafts = moved;
        }
      }

      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: {
            ...draft,
            activities: nextActivities,
            transitions: nextTransitions,
            entry_activity_key: nextEntry,
          },
          workflowDraftsByActivityKey: nextDrafts,
          selection: nextSelection,
          dirty: true,
        },
      };
    });
  },

  updateActivityWorkflowDraft: (activityKey, patch) => {
    set((s) => {
      const current = s.lifecycleEditor.workflowDraftsByActivityKey[activityKey];
      if (!current) return s;
      const next = { ...current, ...patch };
      // 若 contract.output_ports / input_ports 变化，同步到 activity
      let nextDraft = s.lifecycleEditor.draft;
      if (patch.contract && nextDraft) {
        const newOut = patch.contract.output_ports;
        const newIn = patch.contract.input_ports;
        if (newOut || newIn) {
          nextDraft = {
            ...nextDraft,
            activities: nextDraft.activities.map((activity) =>
              activity.key === activityKey
                ? {
                    ...activity,
                    output_ports: newOut ?? activity.output_ports,
                    input_ports: newIn ?? activity.input_ports,
                  }
                : activity,
            ),
          };
        }
      }
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: nextDraft,
          workflowDraftsByActivityKey: {
            ...s.lifecycleEditor.workflowDraftsByActivityKey,
            [activityKey]: next,
          },
          dirty: true,
        },
      };
    });
    return;
  },

  addLifecycleEditorActivity: (opts = {}) => {
    const state = get();
    const draft = state.lifecycleEditor.draft;
    if (!draft) return null;
    const usedKeys = new Set(draft.activities.map((activity) => activity.key));
    const baseKey = opts.activityKey?.trim() || `activity_${draft.activities.length + 1}`;
    let candidate = baseKey;
    let i = 2;
    while (usedKeys.has(candidate)) {
      candidate = `${baseKey}_${i}`;
      i += 1;
    }
    const activityKey = candidate;
    const lifecycleKey = draft.key || "__new__";
    const wfDraft = opts.initialFromWorkflow
      ? {
          ...definitionToDraft(opts.initialFromWorkflow),
          id: null,
          key: `${lifecycleKey}_${activityKey}`,
          name: opts.initialFromWorkflow.name,
          project_id: draft.project_id,
        }
      : createActivityWorkflowDraft(draft.project_id, lifecycleKey, activityKey, draft.target_kinds);
    const newActivity: ActivityDefinition = {
      key: activityKey,
      description: "",
      executor: {
        kind: "agent",
        procedure_key: wfDraft.key,
        session_policy: "spawn_child",
      },
      output_ports: [...wfDraft.contract.output_ports],
      input_ports: [...wfDraft.contract.input_ports],
      completion_policy: { kind: "executor_terminal" },
      iteration_policy: { max_attempts: 1, artifact_alias: "latest" },
      join_policy: "all",
    };
    set((s) => ({
      lifecycleEditor: {
        ...s.lifecycleEditor,
        draft: {
          ...draft,
          activities: [...draft.activities, newActivity],
          entry_activity_key: draft.entry_activity_key || activityKey,
        },
        workflowDraftsByActivityKey: {
          ...s.lifecycleEditor.workflowDraftsByActivityKey,
          [activityKey]: wfDraft,
        },
        ...activitySelection(activityKey),
        dirty: true,
      },
    }));
    return activityKey;
  },

  removeLifecycleEditorActivity: (activityKey) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;
      const nextActivities = draft.activities.filter((activity) => activity.key !== activityKey);
      const nextEdges = draft.transitions.filter(
        (transition) => transition.from !== activityKey && transition.to !== activityKey,
      );
      const nextDrafts = { ...s.lifecycleEditor.workflowDraftsByActivityKey };
      delete nextDrafts[activityKey];
      const nextEntry =
        draft.entry_activity_key === activityKey ? nextActivities[0]?.key ?? "" : draft.entry_activity_key;
      const fallbackKey = nextActivities[0]?.key ?? null;
      const prevSelection = s.lifecycleEditor.selection;
      // 删除当前选中 activity 时落到首个 activity；删除其他 activity 时若选中是 transition 且引用被删 activity 则清空。
      let nextSelectionPair: { selection: LifecycleSelection | null };
      if (prevSelection?.kind === "activity") {
        nextSelectionPair = activitySelection(
          prevSelection.activityKey === activityKey ? fallbackKey : prevSelection.activityKey,
        );
      } else if (prevSelection?.kind === "transition") {
        const idx = findTransitionIndex(draft.transitions, prevSelection.transitionId);
        const t = idx >= 0 ? draft.transitions[idx] : null;
        if (t && (t.from === activityKey || t.to === activityKey)) {
          nextSelectionPair = { selection: null };
        } else {
          nextSelectionPair = { selection: prevSelection };
        }
      } else {
        nextSelectionPair = { selection: null };
      }
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: {
            ...draft,
            activities: nextActivities,
            transitions: nextEdges,
            entry_activity_key: nextEntry,
          },
          workflowDraftsByActivityKey: nextDrafts,
          ...nextSelectionPair,
          dirty: true,
        },
      };
    });
  },

  cloneWorkflowIntoActivity: (activityKey, source) => {
    set((s) => {
      const current = s.lifecycleEditor.workflowDraftsByActivityKey[activityKey];
      if (!current || !s.lifecycleEditor.draft) return s;
      // Clone：保留新 activity 自己的 key/name/project_id，复制 contract
      const next: WorkflowEditorDraft = {
        ...current,
        target_kinds: [...source.target_kinds],
        contract: structuredClone(source.contract),
      };
      // 同步 ports 到 activity
      const nextDraft = {
        ...s.lifecycleEditor.draft,
        activities: s.lifecycleEditor.draft.activities.map((activity) =>
          activity.key === activityKey
            ? {
                ...activity,
                output_ports: [...next.contract.output_ports],
                input_ports: [...next.contract.input_ports],
                executor:
                  activity.executor.kind === "agent"
                    ? { ...activity.executor, procedure_key: next.key }
                    : activity.executor,
              }
            : activity,
        ),
      };
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: nextDraft,
          workflowDraftsByActivityKey: {
            ...s.lifecycleEditor.workflowDraftsByActivityKey,
            [activityKey]: next,
          },
          dirty: true,
        },
      };
    });
  },

  // ── Activity 内嵌字段编辑 ─────────────────────────────

  setActivityExecutor: (activityKey, executor) => {
    const state = get();
    const draft = state.lifecycleEditor.draft;
    const activity = draft?.activities.find((a) => a.key === activityKey);
    if (!draft || !activity) return null;
    const { policy: nextPolicy, reset } = ensurePolicyForExecutor(activity.completion_policy, executor.kind);
    const previous = activity.completion_policy;
    set((s) => {
      if (!s.lifecycleEditor.draft) return s;
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: {
            ...s.lifecycleEditor.draft,
            activities: s.lifecycleEditor.draft.activities.map((a) => {
              if (a.key !== activityKey) return a;
              const next = { ...a, executor, completion_policy: nextPolicy };
              return reconcileDecisionPort(next, a.completion_policy, nextPolicy);
            }),
          },
          dirty: true,
        },
      };
    });
    return { reset, previous };
  },

  setActivityCompletionPolicy: (activityKey, policy) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: {
            ...draft,
            activities: draft.activities.map((a) => {
              if (a.key !== activityKey) return a;
              const next = { ...a, completion_policy: policy };
              return reconcileDecisionPort(next, a.completion_policy, policy);
            }),
          },
          dirty: true,
        },
      };
    });
  },

  setActivityIterationPolicy: (activityKey, patch) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: {
            ...draft,
            activities: draft.activities.map((a) =>
              a.key === activityKey ? { ...a, iteration_policy: { ...a.iteration_policy, ...patch } } : a,
            ),
          },
          dirty: true,
        },
      };
    });
  },

  setActivityJoinPolicy: (activityKey, policy) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: {
            ...draft,
            activities: draft.activities.map((a) =>
              a.key === activityKey ? { ...a, join_policy: policy } : a,
            ),
          },
          dirty: true,
        },
      };
    });
  },

  // ── Transition 编辑 ──────────────────────────────────

  updateLifecycleEditorTransition: (id, patch) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;
      const idx = findTransitionIndex(draft.transitions, id);
      if (idx < 0) return s;
      const next = [...draft.transitions];
      next[idx] = { ...next[idx], ...patch };
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: { ...draft, transitions: next },
          dirty: true,
        },
      };
    });
  },

  setTransitionKind: (id, kind) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;
      const idx = findTransitionIndex(draft.transitions, id);
      if (idx < 0) return s;
      const t = draft.transitions[idx];
      if (t.kind === kind) return s;
      const next = [...draft.transitions];
      next[idx] = {
        ...t,
        kind,
        // artifact → flow 时清空 bindings；flow → artifact 保留默认空数组（用户后续手动添加）
        artifact_bindings: kind === "artifact" ? t.artifact_bindings : [],
      };
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: { ...draft, transitions: next },
          dirty: true,
        },
      };
    });
  },

  addArtifactBinding: (id, binding) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;
      const idx = findTransitionIndex(draft.transitions, id);
      if (idx < 0) return s;
      const t = draft.transitions[idx];
      const next = [...draft.transitions];
      next[idx] = { ...t, artifact_bindings: [...t.artifact_bindings, binding] };
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: { ...draft, transitions: next },
          dirty: true,
        },
      };
    });
  },

  updateArtifactBinding: (id, bindingIdx, patch) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;
      const idx = findTransitionIndex(draft.transitions, id);
      if (idx < 0) return s;
      const t = draft.transitions[idx];
      if (bindingIdx < 0 || bindingIdx >= t.artifact_bindings.length) return s;
      const nextBindings = [...t.artifact_bindings];
      nextBindings[bindingIdx] = { ...nextBindings[bindingIdx], ...patch };
      const next = [...draft.transitions];
      next[idx] = { ...t, artifact_bindings: nextBindings };
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: { ...draft, transitions: next },
          dirty: true,
        },
      };
    });
  },

  removeArtifactBinding: (id, bindingIdx) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;
      const idx = findTransitionIndex(draft.transitions, id);
      if (idx < 0) return s;
      const t = draft.transitions[idx];
      if (bindingIdx < 0 || bindingIdx >= t.artifact_bindings.length) return s;
      const nextBindings = t.artifact_bindings.filter((_, i) => i !== bindingIdx);
      const next = [...draft.transitions];
      next[idx] = { ...t, artifact_bindings: nextBindings };
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: { ...draft, transitions: next },
          dirty: true,
        },
      };
    });
  },

  validateLifecycleBundle: async () => {
    const state = get();
    const { draft } = state.lifecycleEditor;
    if (!draft) return null;
    set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, isValidating: true, error: null } }));
    try {
      const serverResult = await validateWorkflowGraph({
        project_id: draft.project_id,
        key: draft.key,
        name: draft.name,
        description: draft.description,
        target_kinds: draft.target_kinds,
        entry_activity_key: draft.entry_activity_key,
        activities: draft.activities,
        transitions: draft.transitions,
      });
      const cycleIssues = buildUnboundedCycleIssues(draft.activities, draft.transitions);
      const result: WorkflowValidationResult = cycleIssues.length === 0
        ? serverResult
        : { ...serverResult, issues: [...serverResult.issues, ...cycleIssues] };
      set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, validation: result, isValidating: false } }));
      return result;
    } catch (error) {
      set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, error: (error as Error).message, isValidating: false } }));
      return null;
    }
  },

  saveLifecycleBundle: async () => {
    const state = get();
    const { draft, workflowDraftsByActivityKey, originalId } = state.lifecycleEditor;
    if (!draft) return null;
    set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, isSaving: true, error: null } }));
    try {
      // 1) 先 upsert 每个 Agent activity 关联的 workflow contract。
      const updatedDrafts: Record<string, WorkflowEditorDraft> = { ...workflowDraftsByActivityKey };
      const activitiesAfterSave: ActivityDefinition[] = [];
      for (const activity of draft.activities) {
        if (activity.executor.kind !== "agent") {
          activitiesAfterSave.push(activity);
          continue;
        }
        const wfDraft = updatedDrafts[activity.key];
        if (!wfDraft) {
          activitiesAfterSave.push(activity);
          continue;
        }
        const saved = wfDraft.id
          ? await updateAgentProcedure(wfDraft.id, {
              name: wfDraft.name || activity.key,
              description: wfDraft.description,
              binding_kinds: wfDraft.target_kinds,
              contract: wfDraft.contract,
            })
          : await createAgentProcedure({
              project_id: wfDraft.project_id,
              key: wfDraft.key,
              name: wfDraft.name || activity.key,
              description: wfDraft.description,
              target_kinds: wfDraft.target_kinds,
              contract: wfDraft.contract,
            });
        updatedDrafts[activity.key] = definitionToDraft(saved);
        activitiesAfterSave.push({
          ...activity,
          executor: { ...activity.executor, procedure_key: saved.key },
        });
        set((s) => ({ definitions: upsert(s.definitions, saved) }));
      }

      // 2) 再 upsert activity lifecycle
      const nextLifecycle = originalId
        ? await updateWorkflowGraph(originalId, {
            name: draft.name,
            description: draft.description,
            binding_kinds: draft.target_kinds,
            entry_activity_key: draft.entry_activity_key,
            activities: activitiesAfterSave,
            transitions: draft.transitions,
          })
        : await createWorkflowGraph({
            project_id: draft.project_id,
            key: draft.key,
            name: draft.name,
            description: draft.description,
            target_kinds: draft.target_kinds,
            entry_activity_key: draft.entry_activity_key,
            activities: activitiesAfterSave,
            transitions: draft.transitions,
          });

      set((s) => ({
        lifecycleDefinitions: upsert(s.lifecycleDefinitions, nextLifecycle),
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: lifecycleToDraft(nextLifecycle),
          workflowDraftsByActivityKey: updatedDrafts,
          originalId: nextLifecycle.id,
          validation: null,
          isSaving: false,
          dirty: false,
        },
      }));
      return nextLifecycle;
    } catch (error) {
      set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, error: (error as Error).message, isSaving: false } }));
      return null;
    }
  },

  closeLifecycleEditor: () => {
    set({ lifecycleEditor: emptyLifecycleEditor() });
  },
}));
