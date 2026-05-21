import { create } from "zustand";

import type {
  ActivityDefinition,
  ActivityLifecycleDefinition,
  ActivityTransition,
  HookRulePreset,
  WorkflowContract,
  WorkflowDefinition,
  WorkflowRun,
  WorkflowTargetKind,
  WorkflowValidationResult,
} from "../types";
import {
  createActivityLifecycleDefinition,
  createWorkflowDefinition,
  deleteActivityLifecycleDefinition,
  deleteWorkflowDefinition,
  fetchActivityLifecycleDefinitions,
  fetchWorkflowDefinitions,
  fetchWorkflowRunsBySession,
  fetchHookPresets,
  getActivityLifecycleDefinition,
  startWorkflowRun,
  updateActivityLifecycleDefinition,
  updateWorkflowDefinition,
  validateActivityLifecycleDefinition,
} from "../services/workflow";

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

// ─── Unified Lifecycle Editor ────────────────────────────
//
// 单 editor state：前端向用户呈现"一个 editor 编辑一个 workflow 资产"，内部
// 按后端双实体 schema 保存（先 upsert 每个 Agent activity 对应的 workflow，再
// upsert activity lifecycle）。
//
// 每个 activity 关联的 workflow contract 放在
// `workflowDraftsByActivityKey[activity.key]`，索引以 activity.key 为准
// （不是 workflow_key，避免新建 activity 时 workflow_key 还未定型的情况）。

export interface LifecycleEditorState {
  draft: LifecycleEditorDraft | null;
  /** 每个 activity 对应的 workflow contract draft，按 activity.key 索引 */
  workflowDraftsByActivityKey: Record<string, WorkflowEditorDraft>;
  /** 当前选中 activity key（inspector 面板渲染用） */
  selectedActivityKey: string | null;
  /** 原 lifecycle definition id；null 表示新建态 */
  originalId: string | null;
  validation: WorkflowValidationResult | null;
  isSaving: boolean;
  isValidating: boolean;
  dirty: boolean;
  isLoading: boolean;
  error: string | null;
}

function emptyLifecycleEditor(): LifecycleEditorState {
  return {
    draft: null,
    workflowDraftsByActivityKey: {},
    selectedActivityKey: null,
    originalId: null,
    validation: null,
    isSaving: false,
    isValidating: false,
    dirty: false,
    isLoading: false,
    error: null,
  };
}

function definitionToDraft(definition: WorkflowDefinition): WorkflowEditorDraft {
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
        workflow_key: "",
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

function lifecycleToDraft(definition: ActivityLifecycleDefinition): LifecycleEditorDraft {
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
 * 约定：activity 新建时自动派生 workflow_key = <lifecycle_key>.<activity_key>。
 */
export function createActivityWorkflowDraft(
  projectId: string,
  lifecycleKey: string,
  activityKey: string,
  targetKinds: WorkflowTargetKind[] = ["story"],
): WorkflowEditorDraft {
  const workflowKey = lifecycleKey && activityKey ? `${lifecycleKey}.${activityKey}` : activityKey || "";
  return {
    id: null,
    project_id: projectId,
    key: workflowKey,
    name: activityKey || "Untitled",
    description: "",
    target_kinds: [...targetKinds],
    contract: {
      injection: { guidance: null, context_bindings: [] },
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
  definitions: WorkflowDefinition[];
  lifecycleDefinitions: ActivityLifecycleDefinition[];
  runsBySessionId: Record<string, WorkflowRun[]>;
  hookPresets: HookRulePreset[];
  isLoading: boolean;
  error: string | null;

  /** 合并后的统一 editor state */
  lifecycleEditor: LifecycleEditorState;

  fetchHookPresets: () => Promise<HookRulePreset[]>;
  fetchDefinitions: (opts?: { projectId?: string; targetKind?: WorkflowTargetKind }) => Promise<WorkflowDefinition[]>;
  fetchLifecycles: (opts?: { projectId?: string; targetKind?: WorkflowTargetKind }) => Promise<ActivityLifecycleDefinition[]>;
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
  updateLifecycleEditorDraft: (patch: Partial<LifecycleEditorDraft>) => void;
  updateLifecycleEditorActivity: (activityKey: string, patch: Partial<ActivityDefinition>) => void;
  updateActivityWorkflowDraft: (activityKey: string, patch: Partial<WorkflowEditorDraft>) => void;
  addLifecycleEditorActivity: (opts?: { activityKey?: string; initialFromWorkflow?: WorkflowDefinition }) => string | null;
  removeLifecycleEditorActivity: (activityKey: string) => void;
  cloneWorkflowIntoActivity: (activityKey: string, source: WorkflowDefinition) => void;
  validateLifecycleBundle: () => Promise<WorkflowValidationResult | null>;
  saveLifecycleBundle: () => Promise<ActivityLifecycleDefinition | null>;
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
      const definitions = await fetchWorkflowDefinitions(opts);
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
      const lifecycleDefinitions = await fetchActivityLifecycleDefinitions(opts);
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
      await deleteWorkflowDefinition(id);
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
      await deleteActivityLifecycleDefinition(id);
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
      // 同时把 agent activity 的 workflow_key 派生出来
      draft.activities[0].executor = {
        kind: "agent",
        workflow_key: drafts[activityKey].key,
        session_policy: "spawn_child",
      };
    }
    set({
      lifecycleEditor: {
        ...emptyLifecycleEditor(),
        draft,
        workflowDraftsByActivityKey: drafts,
        selectedActivityKey: activityKey || null,
      },
    });
  },

  openLifecycleById: async (id) => {
    set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, isLoading: true, error: null } }));
    try {
      const definition = await getActivityLifecycleDefinition(id);
      const draft = lifecycleToDraft(definition);
      // 拉取项目下所有 workflow definitions（用于 agent activity executor.workflow_key → contract 映射）
      const wfDefs = await fetchWorkflowDefinitions({ projectId: definition.project_id });
      const wfByKey = new Map(wfDefs.map((d) => [d.key, d]));
      const drafts: Record<string, WorkflowEditorDraft> = {};
      for (const activity of draft.activities) {
        const wfKey = activity.executor.kind === "agent" ? activity.executor.workflow_key.trim() : "";
        if (!wfKey) {
          drafts[activity.key] = createActivityWorkflowDraft(definition.project_id, draft.key, activity.key, draft.target_kinds);
          continue;
        }
        const wf = wfByKey.get(wfKey);
        if (wf) {
          drafts[activity.key] = definitionToDraft(wf);
        } else {
          // workflow_key 引用了未加载到的 workflow，落回空 draft（保留 key）
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
          selectedActivityKey: draft.activities[0]?.key ?? null,
          originalId: definition.id,
        },
      }));
    } catch (error) {
      set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, error: (error as Error).message, isLoading: false } }));
    }
  },

  selectLifecycleActivity: (activityKey) => {
    set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, selectedActivityKey: activityKey } }));
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

      // 重命名 activity：连带 transitions 引用、entry_activity_key、selectedActivityKey、workflowDraftsByActivityKey 索引
      const nextActivities = draft.activities.map((activity) =>
        activity.key === activityKey ? { ...activity, ...patch } : activity,
      );
      let nextTransitions = draft.transitions;
      let nextEntry = draft.entry_activity_key;
      let nextSelected = s.lifecycleEditor.selectedActivityKey;
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
        if (nextSelected === activityKey) nextSelected = newKey;
        if (nextDrafts[activityKey]) {
          const moved = { ...nextDrafts };
          moved[newKey] = moved[activityKey];
          delete moved[activityKey];
          nextDrafts = moved;
        }
      }

      // 同步 workflow draft 的 ports（port 以 activity 为真相）
      const activityAfter = nextActivities.find((activity) => activity.key === (patch.key ?? activityKey));
      if (activityAfter) {
        const wfDraftKey = patch.key ?? activityKey;
        const wfDraft = nextDrafts[wfDraftKey];
        if (wfDraft && (patch.output_ports || patch.input_ports)) {
          nextDrafts = {
            ...nextDrafts,
            [wfDraftKey]: {
              ...wfDraft,
              contract: {
                ...wfDraft.contract,
                output_ports: patch.output_ports ?? wfDraft.contract.output_ports,
                input_ports: patch.input_ports ?? wfDraft.contract.input_ports,
              },
            },
          };
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
          selectedActivityKey: nextSelected,
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
          key: `${lifecycleKey}.${activityKey}`,
          name: opts.initialFromWorkflow.name,
          project_id: draft.project_id,
        }
      : createActivityWorkflowDraft(draft.project_id, lifecycleKey, activityKey, draft.target_kinds);
    const newActivity: ActivityDefinition = {
      key: activityKey,
      description: "",
      executor: {
        kind: "agent",
        workflow_key: wfDraft.key,
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
        selectedActivityKey: activityKey,
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
      const nextSelected =
        s.lifecycleEditor.selectedActivityKey === activityKey
          ? nextActivities[0]?.key ?? null
          : s.lifecycleEditor.selectedActivityKey;
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
          selectedActivityKey: nextSelected,
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
                    ? { ...activity.executor, workflow_key: next.key }
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

  validateLifecycleBundle: async () => {
    const state = get();
    const { draft } = state.lifecycleEditor;
    if (!draft) return null;
    set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, isValidating: true, error: null } }));
    try {
      const result = await validateActivityLifecycleDefinition({
        project_id: draft.project_id,
        key: draft.key,
        name: draft.name,
        description: draft.description,
        target_kinds: draft.target_kinds,
        entry_activity_key: draft.entry_activity_key,
        activities: draft.activities,
        transitions: draft.transitions,
      });
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
          ? await updateWorkflowDefinition(wfDraft.id, {
              name: wfDraft.name || activity.key,
              description: wfDraft.description,
              binding_kinds: wfDraft.target_kinds,
              contract: wfDraft.contract,
            })
          : await createWorkflowDefinition({
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
          executor: { ...activity.executor, workflow_key: saved.key },
        });
        set((s) => ({ definitions: upsert(s.definitions, saved) }));
      }

      // 2) 再 upsert activity lifecycle
      const nextLifecycle = originalId
        ? await updateActivityLifecycleDefinition(originalId, {
            name: draft.name,
            description: draft.description,
            binding_kinds: draft.target_kinds,
            entry_activity_key: draft.entry_activity_key,
            activities: activitiesAfterSave,
            transitions: draft.transitions,
          })
        : await createActivityLifecycleDefinition({
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
