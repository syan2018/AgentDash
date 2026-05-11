import { create } from "zustand";

import type {
  HookRulePreset,
  LifecycleDefinition,
  LifecycleEdge,
  LifecycleStepDefinition,
  WorkflowContract,
  WorkflowDefinition,
  WorkflowRun,
  WorkflowTargetKind,
  WorkflowTemplate,
  WorkflowValidationResult,
} from "../types";
import {
  activateWorkflowStep,
  bootstrapWorkflowTemplate,
  completeWorkflowStep,
  createLifecycleDefinition,
  createWorkflowDefinition,
  deleteLifecycleDefinition,
  deleteWorkflowDefinition,
  fetchLifecycleDefinitions,
  fetchWorkflowDefinitions,
  fetchWorkflowRunsBySession,
  fetchHookPresets,
  fetchWorkflowTemplates,
  getLifecycleDefinition,
  startWorkflowRun,
  updateLifecycleDefinition,
  updateWorkflowDefinition,
  validateLifecycleDefinition,
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
  entry_step_key: string;
  steps: LifecycleStepDefinition[];
  edges: LifecycleEdge[];
}

export interface LifecycleDraftSeed {
  key?: string;
  name?: string;
  initial_step_key?: string;
}

// ─── Unified Lifecycle Editor ────────────────────────────
//
// 单 editor state：前端向用户呈现"一个 editor 编辑一个 workflow 资产"，内部
// 按后端双实体 schema 保存（先 upsert 每 step 对应的 workflow，再 upsert lifecycle）。
//
// 每个 step 关联的 workflow contract 放在 `workflowDraftsByStepKey[stepKey]`，
// key 为 step.key（不是 workflow_key，避免新建 step 时 key 还未定型的情况）。

export interface LifecycleEditorState {
  draft: LifecycleEditorDraft | null;
  /** 每个 step 对应的 workflow contract draft，按 step.key 索引 */
  workflowDraftsByStepKey: Record<string, WorkflowEditorDraft>;
  /** 当前选中 step key（inspector 面板渲染用） */
  selectedStepKey: string | null;
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
    workflowDraftsByStepKey: {},
    selectedStepKey: null,
    originalId: null,
    validation: null,
    isSaving: false,
    isValidating: false,
    dirty: false,
    isLoading: false,
    error: null,
  };
}

function emptyCapabilityConfig(): WorkflowContract["capability_config"] {
  return { tool_directives: [], mount_directives: [] };
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
  const initialStepKey = seed.initial_step_key ?? "";
  return {
    id: null,
    project_id: projectId,
    key: seed.key ?? "",
    name: seed.name ?? "",
    description: "",
    target_kinds: ["story"],
    entry_step_key: initialStepKey,
    steps: [{
      key: initialStepKey,
      description: "",
      workflow_key: null,
      node_type: "agent_node",
      output_ports: [],
      input_ports: [],
      capability_config: emptyCapabilityConfig(),
    }],
    edges: [],
  };
}

function lifecycleToDraft(definition: LifecycleDefinition): LifecycleEditorDraft {
  return {
    id: definition.id,
    project_id: definition.project_id,
    key: definition.key,
    name: definition.name,
    description: definition.description,
    target_kinds: [...definition.target_kinds],
    entry_step_key: definition.entry_step_key,
    steps: structuredClone(definition.steps),
    edges: structuredClone(definition.edges ?? []),
  };
}

/**
 * 为 step 创建一个对应的空 workflow contract draft。
 * 约定：step 新建时自动派生 workflow_key = <lifecycle_key>.<step_key>。
 */
export function createStepWorkflowDraft(
  projectId: string,
  lifecycleKey: string,
  stepKey: string,
  targetKinds: WorkflowTargetKind[] = ["story"],
): WorkflowEditorDraft {
  const workflowKey = lifecycleKey && stepKey ? `${lifecycleKey}.${stepKey}` : stepKey || "";
  return {
    id: null,
    project_id: projectId,
    key: workflowKey,
    name: stepKey || "Untitled",
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

// ─── Store ───────────────────────────────────────────────

interface WorkflowState {
  templates: WorkflowTemplate[];
  definitions: WorkflowDefinition[];
  lifecycleDefinitions: LifecycleDefinition[];
  runsBySessionId: Record<string, WorkflowRun[]>;
  hookPresets: HookRulePreset[];
  isLoading: boolean;
  error: string | null;

  /** 合并后的统一 editor state */
  lifecycleEditor: LifecycleEditorState;

  fetchHookPresets: () => Promise<HookRulePreset[]>;
  fetchTemplates: () => Promise<WorkflowTemplate[]>;
  fetchDefinitions: (opts?: { projectId?: string; targetKind?: WorkflowTargetKind }) => Promise<WorkflowDefinition[]>;
  fetchLifecycles: (opts?: { projectId?: string; targetKind?: WorkflowTargetKind }) => Promise<LifecycleDefinition[]>;
  bootstrapTemplate: (builtinKey: string, projectId: string) => Promise<LifecycleDefinition | null>;
  fetchRunsBySession: (sessionId: string) => Promise<WorkflowRun[]>;
  startRun: (input: {
    lifecycle_id?: string;
    lifecycle_key?: string;
    session_id: string;
    project_id: string;
  }) => Promise<WorkflowRun | null>;
  activateStep: (input: { run_id: string; step_key: string }) => Promise<WorkflowRun | null>;
  completeStep: (input: {
    run_id: string;
    step_key: string;
    summary?: string;
  }) => Promise<WorkflowRun | null>;

  removeDefinition: (id: string) => Promise<boolean>;
  removeLifecycle: (id: string) => Promise<boolean>;

  // ── Unified Lifecycle Editor actions ──
  openLifecycleForm: (projectId: string, seed?: LifecycleDraftSeed) => void;
  openLifecycleById: (id: string) => Promise<void>;
  selectLifecycleStep: (stepKey: string | null) => void;
  updateLifecycleEditorDraft: (patch: Partial<LifecycleEditorDraft>) => void;
  updateLifecycleEditorStep: (stepKey: string, patch: Partial<LifecycleStepDefinition>) => void;
  updateStepWorkflowDraft: (stepKey: string, patch: Partial<WorkflowEditorDraft>) => void;
  addLifecycleEditorStep: (opts?: { stepKey?: string; initialFromWorkflow?: WorkflowDefinition }) => string | null;
  removeLifecycleEditorStep: (stepKey: string) => void;
  cloneWorkflowIntoStep: (stepKey: string, source: WorkflowDefinition) => void;
  validateLifecycleBundle: () => Promise<WorkflowValidationResult | null>;
  saveLifecycleBundle: () => Promise<LifecycleDefinition | null>;
  closeLifecycleEditor: () => void;
}

export const useWorkflowStore = create<WorkflowState>((set, get) => ({
  templates: [],
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

  fetchTemplates: async () => {
    try {
      const templates = await fetchWorkflowTemplates();
      set({ templates });
      return templates;
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
      const lifecycleDefinitions = await fetchLifecycleDefinitions(opts);
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

  bootstrapTemplate: async (builtinKey, projectId) => {
    set({ error: null });
    try {
      const lifecycle = await bootstrapWorkflowTemplate(builtinKey, projectId);
      const [definitions, lifecycleDefinitions] = await Promise.all([
        fetchWorkflowDefinitions({ projectId }),
        fetchLifecycleDefinitions({ projectId }),
      ]);
      set({ definitions, lifecycleDefinitions });
      return lifecycle;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
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

  activateStep: async (input) => {
    set({ error: null });
    try {
      const run = await activateWorkflowStep(input);
      set((state) => ({ runsBySessionId: upsertRun(state.runsBySessionId, run) }));
      return run;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  completeStep: async (input) => {
    set({ error: null });
    try {
      const run = await completeWorkflowStep(input);
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
      await deleteLifecycleDefinition(id);
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
    const stepKey = draft.steps[0]?.key ?? "";
    const lifecycleKey = draft.key || "__new__";
    const drafts: Record<string, WorkflowEditorDraft> = {};
    if (stepKey) {
      drafts[stepKey] = createStepWorkflowDraft(projectId, lifecycleKey, stepKey, draft.target_kinds);
      // 同时把 step.workflow_key 派生出来
      draft.steps[0].workflow_key = drafts[stepKey].key;
    }
    set({
      lifecycleEditor: {
        ...emptyLifecycleEditor(),
        draft,
        workflowDraftsByStepKey: drafts,
        selectedStepKey: stepKey || null,
      },
    });
  },

  openLifecycleById: async (id) => {
    set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, isLoading: true, error: null } }));
    try {
      const definition = await getLifecycleDefinition(id);
      const draft = lifecycleToDraft(definition);
      // 拉取项目下所有 workflow definitions（用于 step.workflow_key → contract 映射）
      const wfDefs = await fetchWorkflowDefinitions({ projectId: definition.project_id });
      const wfByKey = new Map(wfDefs.map((d) => [d.key, d]));
      const drafts: Record<string, WorkflowEditorDraft> = {};
      for (const step of draft.steps) {
        const wfKey = step.workflow_key?.trim();
        if (!wfKey) {
          drafts[step.key] = createStepWorkflowDraft(definition.project_id, draft.key, step.key, draft.target_kinds);
          continue;
        }
        const wf = wfByKey.get(wfKey);
        if (wf) {
          drafts[step.key] = definitionToDraft(wf);
        } else {
          // workflow_key 引用了未加载到的 workflow，落回空 draft（保留 key）
          const fallback = createStepWorkflowDraft(definition.project_id, draft.key, step.key, draft.target_kinds);
          fallback.key = wfKey;
          drafts[step.key] = fallback;
        }
      }

      set((state) => ({
        lifecycleDefinitions: upsert(state.lifecycleDefinitions, definition),
        definitions: wfDefs,
        lifecycleEditor: {
          ...emptyLifecycleEditor(),
          draft,
          workflowDraftsByStepKey: drafts,
          selectedStepKey: draft.steps[0]?.key ?? null,
          originalId: definition.id,
        },
      }));
    } catch (error) {
      set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, error: (error as Error).message, isLoading: false } }));
    }
  },

  selectLifecycleStep: (stepKey) => {
    set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, selectedStepKey: stepKey } }));
  },

  updateLifecycleEditorDraft: (patch) => {
    set((s) => {
      if (!s.lifecycleEditor.draft) return s;
      const nextDraft = { ...s.lifecycleEditor.draft, ...patch };
      // target_kinds 改变时同步到所有 step workflow drafts，保证 contract/lifecycle 一致
      let nextDrafts = s.lifecycleEditor.workflowDraftsByStepKey;
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
          workflowDraftsByStepKey: nextDrafts,
          dirty: true,
        },
      };
    });
  },

  updateLifecycleEditorStep: (stepKey, patch) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;

      // 重命名 step：连带 edges 引用、entry_step_key、selectedStepKey、workflowDraftsByStepKey 索引
      const nextSteps = draft.steps.map((step) =>
        step.key === stepKey ? { ...step, ...patch } : step,
      );
      let nextEdges = draft.edges;
      let nextEntry = draft.entry_step_key;
      let nextSelected = s.lifecycleEditor.selectedStepKey;
      let nextDrafts = s.lifecycleEditor.workflowDraftsByStepKey;

      if (patch.key && patch.key !== stepKey) {
        const newKey = patch.key;
        nextEdges = draft.edges.map((e) => ({
          ...e,
          from_node: e.from_node === stepKey ? newKey : e.from_node,
          to_node: e.to_node === stepKey ? newKey : e.to_node,
        }));
        if (nextEntry === stepKey) nextEntry = newKey;
        if (nextSelected === stepKey) nextSelected = newKey;
        if (nextDrafts[stepKey]) {
          const moved = { ...nextDrafts };
          moved[newKey] = moved[stepKey];
          delete moved[stepKey];
          nextDrafts = moved;
        }
      }

      // 同步 workflow draft 的 ports（port 以 step 为真相）
      const stepAfter = nextSteps.find((step) => step.key === (patch.key ?? stepKey));
      if (stepAfter) {
        const wfDraftKey = patch.key ?? stepKey;
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
            steps: nextSteps,
            edges: nextEdges,
            entry_step_key: nextEntry,
          },
          workflowDraftsByStepKey: nextDrafts,
          selectedStepKey: nextSelected,
          dirty: true,
        },
      };
    });
  },

  updateStepWorkflowDraft: (stepKey, patch) => {
    set((s) => {
      const current = s.lifecycleEditor.workflowDraftsByStepKey[stepKey];
      if (!current) return s;
      const next = { ...current, ...patch };
      // 若 contract.output_ports / input_ports 变化，同步到 step
      let nextDraft = s.lifecycleEditor.draft;
      if (patch.contract && nextDraft) {
        const newOut = patch.contract.output_ports;
        const newIn = patch.contract.input_ports;
        if (newOut || newIn) {
          nextDraft = {
            ...nextDraft,
            steps: nextDraft.steps.map((step) =>
              step.key === stepKey
                ? {
                    ...step,
                    output_ports: newOut ?? step.output_ports,
                    input_ports: newIn ?? step.input_ports,
                  }
                : step,
            ),
          };
        }
      }
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: nextDraft,
          workflowDraftsByStepKey: {
            ...s.lifecycleEditor.workflowDraftsByStepKey,
            [stepKey]: next,
          },
          dirty: true,
        },
      };
    });
    return;
  },

  addLifecycleEditorStep: (opts = {}) => {
    const state = get();
    const draft = state.lifecycleEditor.draft;
    if (!draft) return null;
    const usedKeys = new Set(draft.steps.map((s) => s.key));
    const baseKey = opts.stepKey?.trim() || `step_${draft.steps.length + 1}`;
    let candidate = baseKey;
    let i = 2;
    while (usedKeys.has(candidate)) {
      candidate = `${baseKey}_${i}`;
      i += 1;
    }
    const stepKey = candidate;
    const lifecycleKey = draft.key || "__new__";
    const wfDraft = opts.initialFromWorkflow
      ? {
          ...definitionToDraft(opts.initialFromWorkflow),
          id: null,
          key: `${lifecycleKey}.${stepKey}`,
          name: opts.initialFromWorkflow.name,
          project_id: draft.project_id,
        }
      : createStepWorkflowDraft(draft.project_id, lifecycleKey, stepKey, draft.target_kinds);
    const newStep: LifecycleStepDefinition = {
      key: stepKey,
      description: "",
      workflow_key: wfDraft.key,
      node_type: "agent_node",
      output_ports: [...wfDraft.contract.output_ports],
      input_ports: [...wfDraft.contract.input_ports],
      capability_config: { tool_directives: [], mount_directives: [] },
    };
    set((s) => ({
      lifecycleEditor: {
        ...s.lifecycleEditor,
        draft: {
          ...draft,
          steps: [...draft.steps, newStep],
          entry_step_key: draft.entry_step_key || stepKey,
        },
        workflowDraftsByStepKey: {
          ...s.lifecycleEditor.workflowDraftsByStepKey,
          [stepKey]: wfDraft,
        },
        selectedStepKey: stepKey,
        dirty: true,
      },
    }));
    return stepKey;
  },

  removeLifecycleEditorStep: (stepKey) => {
    set((s) => {
      const draft = s.lifecycleEditor.draft;
      if (!draft) return s;
      const nextSteps = draft.steps.filter((step) => step.key !== stepKey);
      const nextEdges = draft.edges.filter(
        (e) => e.from_node !== stepKey && e.to_node !== stepKey,
      );
      const nextDrafts = { ...s.lifecycleEditor.workflowDraftsByStepKey };
      delete nextDrafts[stepKey];
      const nextEntry =
        draft.entry_step_key === stepKey ? nextSteps[0]?.key ?? "" : draft.entry_step_key;
      const nextSelected =
        s.lifecycleEditor.selectedStepKey === stepKey
          ? nextSteps[0]?.key ?? null
          : s.lifecycleEditor.selectedStepKey;
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: {
            ...draft,
            steps: nextSteps,
            edges: nextEdges,
            entry_step_key: nextEntry,
          },
          workflowDraftsByStepKey: nextDrafts,
          selectedStepKey: nextSelected,
          dirty: true,
        },
      };
    });
  },

  cloneWorkflowIntoStep: (stepKey, source) => {
    set((s) => {
      const current = s.lifecycleEditor.workflowDraftsByStepKey[stepKey];
      if (!current || !s.lifecycleEditor.draft) return s;
      // Clone：保留新 step 自己的 key/name/project_id，复制 contract
      const next: WorkflowEditorDraft = {
        ...current,
        target_kinds: [...source.target_kinds],
        contract: structuredClone(source.contract),
      };
      // 同步 ports 到 step
      const nextDraft = {
        ...s.lifecycleEditor.draft,
        steps: s.lifecycleEditor.draft.steps.map((step) =>
          step.key === stepKey
            ? {
                ...step,
                output_ports: [...next.contract.output_ports],
                input_ports: [...next.contract.input_ports],
              }
            : step,
        ),
      };
      return {
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: nextDraft,
          workflowDraftsByStepKey: {
            ...s.lifecycleEditor.workflowDraftsByStepKey,
            [stepKey]: next,
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
      const result = await validateLifecycleDefinition({
        project_id: draft.project_id,
        key: draft.key,
        name: draft.name,
        description: draft.description,
        target_kinds: draft.target_kinds,
        entry_step_key: draft.entry_step_key,
        steps: draft.steps,
        edges: draft.edges,
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
    const { draft, workflowDraftsByStepKey, originalId } = state.lifecycleEditor;
    if (!draft) return null;
    set((s) => ({ lifecycleEditor: { ...s.lifecycleEditor, isSaving: true, error: null } }));
    try {
      // 1) 先 upsert 每个 step 关联的 workflow（agent_node / phase_node 一视同仁）
      // domain `LifecycleStepDefinition.workflow_key` 从未限定 node_type；唯一硬约束
      // 是 entry step 必须是 agent_node（由 validate_lifecycle_definition 强制）。
      const updatedDrafts: Record<string, WorkflowEditorDraft> = { ...workflowDraftsByStepKey };
      const stepsAfterSave: LifecycleStepDefinition[] = [];
      for (const step of draft.steps) {
        const wfDraft = updatedDrafts[step.key];
        if (!wfDraft) {
          stepsAfterSave.push(step);
          continue;
        }
        const saved = wfDraft.id
          ? await updateWorkflowDefinition(wfDraft.id, {
              name: wfDraft.name || step.key,
              description: wfDraft.description,
              binding_kinds: wfDraft.target_kinds,
              contract: wfDraft.contract,
            })
          : await createWorkflowDefinition({
              project_id: wfDraft.project_id,
              key: wfDraft.key,
              name: wfDraft.name || step.key,
              description: wfDraft.description,
              target_kinds: wfDraft.target_kinds,
              contract: wfDraft.contract,
            });
        updatedDrafts[step.key] = definitionToDraft(saved);
        stepsAfterSave.push({ ...step, workflow_key: saved.key });
        set((s) => ({ definitions: upsert(s.definitions, saved) }));
      }

      // 2) 再 upsert lifecycle
      const nextLifecycle = originalId
        ? await updateLifecycleDefinition(originalId, {
            name: draft.name,
            description: draft.description,
            binding_kinds: draft.target_kinds,
            entry_step_key: draft.entry_step_key,
            steps: stepsAfterSave,
            edges: draft.edges,
          })
        : await createLifecycleDefinition({
            project_id: draft.project_id,
            key: draft.key,
            name: draft.name,
            description: draft.description,
            target_kinds: draft.target_kinds,
            entry_step_key: draft.entry_step_key,
            steps: stepsAfterSave,
            edges: draft.edges,
          });

      set((s) => ({
        lifecycleDefinitions: upsert(s.lifecycleDefinitions, nextLifecycle),
        lifecycleEditor: {
          ...s.lifecycleEditor,
          draft: lifecycleToDraft(nextLifecycle),
          workflowDraftsByStepKey: updatedDrafts,
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
