import { create } from "zustand";

import type {
  HookRulePreset,
  LifecycleDefinition,
  LifecycleEdge,
  LifecycleStepDefinition,
  WorkflowAgentRole,
  WorkflowContextBinding,
  WorkflowContract,
  WorkflowDefinition,
  WorkflowHookRuleSpec,
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
  getWorkflowDefinition,
  startWorkflowRun,
  updateLifecycleDefinition,
  updateWorkflowDefinition,
  validateLifecycleDefinition,
  validateWorkflowDefinition,
} from "../services/workflow";

// ─── Editor state（消除 workflow / lifecycle 编辑器的镜像重复）───

interface EditorState<T> {
  draft: T | null;
  originalId: string | null;
  validation: WorkflowValidationResult | null;
  isSaving: boolean;
  isValidating: boolean;
  dirty: boolean;
  isLoading: boolean;
  error: string | null;
}

function emptyEditor<T>(): EditorState<T> {
  return {
    draft: null,
    originalId: null,
    validation: null,
    isSaving: false,
    isValidating: false,
    dirty: false,
    isLoading: false,
    error: null,
  };
}

// ─── Draft types ─────────────────────────────────────────

export interface WorkflowEditorDraft {
  id: string | null;
  project_id: string;
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_roles: WorkflowAgentRole[];
  contract: WorkflowContract;
}

export interface LifecycleEditorDraft {
  id: string | null;
  project_id: string;
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_roles: WorkflowAgentRole[];
  entry_step_key: string;
  steps: LifecycleStepDefinition[];
  edges: LifecycleEdge[];
}

export function createEmptyDraft(projectId = ""): WorkflowEditorDraft {
  return {
    id: null,
    project_id: projectId,
    key: "",
    name: "",
    description: "",
    target_kind: "task",
    recommended_roles: ["task"],
    contract: {
      injection: { goal: null, instructions: [], context_bindings: [] },
      hook_rules: [],
      constraints: [],
      completion: { checks: [] },
    },
  };
}

export function definitionToDraft(definition: WorkflowDefinition): WorkflowEditorDraft {
  return {
    id: definition.id,
    project_id: definition.project_id,
    key: definition.key,
    name: definition.name,
    description: definition.description,
    target_kind: definition.target_kind,
    recommended_roles: [...definition.recommended_roles],
    contract: structuredClone(definition.contract),
  };
}

export function createEmptyLifecycleDraft(projectId = ""): LifecycleEditorDraft {
  return {
    id: null,
    project_id: projectId,
    key: "",
    name: "",
    description: "",
    target_kind: "task",
    recommended_roles: ["task"],
    entry_step_key: "",
    steps: [{ key: "", description: "", workflow_key: null, output_ports: [], input_ports: [] }],
    edges: [],
  };
}

export function lifecycleToDraft(definition: LifecycleDefinition): LifecycleEditorDraft {
  return {
    id: definition.id,
    project_id: definition.project_id,
    key: definition.key,
    name: definition.name,
    description: definition.description,
    target_kind: definition.target_kind,
    recommended_roles: [...definition.recommended_roles],
    entry_step_key: definition.entry_step_key,
    steps: structuredClone(definition.steps),
    edges: structuredClone(definition.edges ?? []),
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

  wfEditor: EditorState<WorkflowEditorDraft>;
  lcEditor: EditorState<LifecycleEditorDraft>;

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

  openNewDraft: () => void;
  openEditDraft: (definitionId: string) => Promise<void>;
  closeDraft: () => void;
  updateDraft: (patch: Partial<WorkflowEditorDraft>) => void;
  updateDraftBinding: (bindingIndex: number, patch: Partial<WorkflowContextBinding>) => void;
  addDraftBinding: () => void;
  removeDraftBinding: (bindingIndex: number) => void;
  validateDraft: () => Promise<WorkflowValidationResult | null>;
  saveDraft: () => Promise<WorkflowDefinition | null>;
  removeDefinition: (id: string) => Promise<boolean>;

  addDraftHookRule: (rule: WorkflowHookRuleSpec) => void;
  removeDraftHookRule: (ruleKey: string) => void;
  updateDraftHookRule: (ruleKey: string, patch: Partial<WorkflowHookRuleSpec>) => void;

  openNewLifecycleDraft: () => void;
  openEditLifecycleDraft: (definitionId: string) => Promise<void>;
  closeLifecycleDraft: () => void;
  updateLifecycleDraft: (patch: Partial<LifecycleEditorDraft>) => void;
  updateLifecycleStep: (stepIndex: number, patch: Partial<LifecycleStepDefinition>) => void;
  addLifecycleStep: () => void;
  removeLifecycleStep: (stepIndex: number) => void;
  validateLifecycleDraft: () => Promise<WorkflowValidationResult | null>;
  saveLifecycleDraft: () => Promise<LifecycleDefinition | null>;
  removeLifecycle: (id: string) => Promise<boolean>;

}

export const useWorkflowStore = create<WorkflowState>((set, get) => ({
  templates: [],
  definitions: [],
  lifecycleDefinitions: [],
  runsBySessionId: {},
  hookPresets: [],
  isLoading: false,
  error: null,

  wfEditor: emptyEditor<WorkflowEditorDraft>(),
  lcEditor: emptyEditor<LifecycleEditorDraft>(),

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
          ? [...state.definitions.filter((item) => item.target_kind !== targetKind), ...definitions]
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
          ? [...state.lifecycleDefinitions.filter((item) => item.target_kind !== targetKind), ...lifecycleDefinitions]
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

  // ── Workflow Definition editor ──

  openNewDraft: () => {
    set({ wfEditor: { ...emptyEditor<WorkflowEditorDraft>(), draft: createEmptyDraft() } });
  },

  openEditDraft: async (definitionId) => {
    set((s) => ({ wfEditor: { ...s.wfEditor, isLoading: true, error: null } }));
    try {
      const definition = await getWorkflowDefinition(definitionId);
      set((state) => ({
        definitions: upsert(state.definitions, definition),
        wfEditor: {
          ...emptyEditor<WorkflowEditorDraft>(),
          draft: definitionToDraft(definition),
          originalId: definition.id,
        },
      }));
    } catch (error) {
      set((s) => ({ wfEditor: { ...s.wfEditor, error: (error as Error).message, isLoading: false } }));
    }
  },

  closeDraft: () => {
    set({ wfEditor: emptyEditor<WorkflowEditorDraft>() });
  },

  updateDraft: (patch) => {
    set((state) => {
      if (!state.wfEditor.draft) return state;
      return { wfEditor: { ...state.wfEditor, draft: { ...state.wfEditor.draft, ...patch }, dirty: true } };
    });
  },

  updateDraftBinding: (bindingIndex, patch) => {
    set((state) => {
      const draft = state.wfEditor.draft;
      if (!draft) return state;
      const bindings = draft.contract.injection.context_bindings.map((b, i) =>
        i === bindingIndex ? { ...b, ...patch } : b,
      );
      return {
        wfEditor: {
          ...state.wfEditor,
          draft: { ...draft, contract: { ...draft.contract, injection: { ...draft.contract.injection, context_bindings: bindings } } },
          dirty: true,
        },
      };
    });
  },

  addDraftBinding: () => {
    set((state) => {
      const draft = state.wfEditor.draft;
      if (!draft) return state;
      const newBinding: WorkflowContextBinding = { locator: "", reason: "", required: true, title: null };
      return {
        wfEditor: {
          ...state.wfEditor,
          draft: { ...draft, contract: { ...draft.contract, injection: { ...draft.contract.injection, context_bindings: [...draft.contract.injection.context_bindings, newBinding] } } },
          dirty: true,
        },
      };
    });
  },

  removeDraftBinding: (bindingIndex) => {
    set((state) => {
      const draft = state.wfEditor.draft;
      if (!draft) return state;
      return {
        wfEditor: {
          ...state.wfEditor,
          draft: { ...draft, contract: { ...draft.contract, injection: { ...draft.contract.injection, context_bindings: draft.contract.injection.context_bindings.filter((_, i) => i !== bindingIndex) } } },
          dirty: true,
        },
      };
    });
  },

  addDraftHookRule: (rule) => {
    set((state) => {
      const draft = state.wfEditor.draft;
      if (!draft) return state;
      const existing = draft.contract.hook_rules.some((r) => r.key === rule.key);
      if (existing) return state;
      return {
        wfEditor: {
          ...state.wfEditor,
          draft: { ...draft, contract: { ...draft.contract, hook_rules: [...draft.contract.hook_rules, rule] } },
          dirty: true,
        },
      };
    });
  },

  removeDraftHookRule: (ruleKey) => {
    set((state) => {
      const draft = state.wfEditor.draft;
      if (!draft) return state;
      return {
        wfEditor: {
          ...state.wfEditor,
          draft: { ...draft, contract: { ...draft.contract, hook_rules: draft.contract.hook_rules.filter((r) => r.key !== ruleKey) } },
          dirty: true,
        },
      };
    });
  },

  updateDraftHookRule: (ruleKey, patch) => {
    set((state) => {
      const draft = state.wfEditor.draft;
      if (!draft) return state;
      return {
        wfEditor: {
          ...state.wfEditor,
          draft: {
            ...draft,
            contract: {
              ...draft.contract,
              hook_rules: draft.contract.hook_rules.map((r) =>
                r.key === ruleKey ? { ...r, ...patch } : r,
              ),
            },
          },
          dirty: true,
        },
      };
    });
  },

  validateDraft: async () => {
    const draft = get().wfEditor.draft;
    if (!draft) return null;
    set((s) => ({ wfEditor: { ...s.wfEditor, isValidating: true, error: null } }));
    try {
      const result = await validateWorkflowDefinition({
        project_id: draft.project_id,
        key: draft.key,
        name: draft.name,
        description: draft.description,
        target_kind: draft.target_kind,
        recommended_roles: draft.recommended_roles,
        contract: draft.contract,
      });
      set((s) => ({ wfEditor: { ...s.wfEditor, validation: result, isValidating: false } }));
      return result;
    } catch (error) {
      set((s) => ({ wfEditor: { ...s.wfEditor, error: (error as Error).message, isValidating: false } }));
      return null;
    }
  },

  saveDraft: async () => {
    const { draft, originalId } = get().wfEditor;
    if (!draft) return null;
    set((s) => ({ wfEditor: { ...s.wfEditor, isSaving: true, error: null } }));
    try {
      const definition = originalId
        ? await updateWorkflowDefinition(originalId, {
            name: draft.name,
            description: draft.description,
            recommended_roles: draft.recommended_roles,
            contract: draft.contract,
          })
        : await createWorkflowDefinition({
            project_id: draft.project_id,
            key: draft.key,
            name: draft.name,
            description: draft.description,
            target_kind: draft.target_kind,
            recommended_roles: draft.recommended_roles,
            contract: draft.contract,
          });
      set((state) => ({
        definitions: upsert(state.definitions, definition),
        wfEditor: {
          ...state.wfEditor,
          draft: definitionToDraft(definition),
          originalId: definition.id,
          validation: null,
          isSaving: false,
          dirty: false,
        },
      }));
      return definition;
    } catch (error) {
      set((s) => ({ wfEditor: { ...s.wfEditor, error: (error as Error).message, isSaving: false } }));
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

  // ── Lifecycle Definition editor ──

  openNewLifecycleDraft: () => {
    set({ lcEditor: { ...emptyEditor<LifecycleEditorDraft>(), draft: createEmptyLifecycleDraft() } });
  },

  openEditLifecycleDraft: async (definitionId) => {
    set((s) => ({ lcEditor: { ...s.lcEditor, isLoading: true, error: null } }));
    try {
      const definition = await getLifecycleDefinition(definitionId);
      set((state) => ({
        lifecycleDefinitions: upsert(state.lifecycleDefinitions, definition),
        lcEditor: {
          ...emptyEditor<LifecycleEditorDraft>(),
          draft: lifecycleToDraft(definition),
          originalId: definition.id,
        },
      }));
    } catch (error) {
      set((s) => ({ lcEditor: { ...s.lcEditor, error: (error as Error).message, isLoading: false } }));
    }
  },

  closeLifecycleDraft: () => {
    set({ lcEditor: emptyEditor<LifecycleEditorDraft>() });
  },

  updateLifecycleDraft: (patch) => {
    set((state) => {
      if (!state.lcEditor.draft) return state;
      return { lcEditor: { ...state.lcEditor, draft: { ...state.lcEditor.draft, ...patch }, dirty: true } };
    });
  },

  updateLifecycleStep: (stepIndex, patch) => {
    set((state) => {
      const draft = state.lcEditor.draft;
      if (!draft) return state;
      return {
        lcEditor: {
          ...state.lcEditor,
          draft: { ...draft, steps: draft.steps.map((step, i) => (i === stepIndex ? { ...step, ...patch } : step)) },
          dirty: true,
        },
      };
    });
  },

  addLifecycleStep: () => {
    set((state) => {
      const draft = state.lcEditor.draft;
      if (!draft) return state;
      return {
        lcEditor: {
          ...state.lcEditor,
          draft: { ...draft, steps: [...draft.steps, { key: "", description: "", workflow_key: null, output_ports: [], input_ports: [] }] },
          dirty: true,
        },
      };
    });
  },

  removeLifecycleStep: (stepIndex) => {
    set((state) => {
      const draft = state.lcEditor.draft;
      if (!draft) return state;
      return {
        lcEditor: {
          ...state.lcEditor,
          draft: { ...draft, steps: draft.steps.filter((_, i) => i !== stepIndex) },
          dirty: true,
        },
      };
    });
  },

  validateLifecycleDraft: async () => {
    const draft = get().lcEditor.draft;
    if (!draft) return null;
    set((s) => ({ lcEditor: { ...s.lcEditor, isValidating: true, error: null } }));
    try {
      const result = await validateLifecycleDefinition({
        project_id: draft.project_id,
        key: draft.key,
        name: draft.name,
        description: draft.description,
        target_kind: draft.target_kind,
        recommended_roles: draft.recommended_roles,
        entry_step_key: draft.entry_step_key,
        steps: draft.steps,
        edges: draft.edges,
      });
      set((s) => ({ lcEditor: { ...s.lcEditor, validation: result, isValidating: false } }));
      return result;
    } catch (error) {
      set((s) => ({ lcEditor: { ...s.lcEditor, error: (error as Error).message, isValidating: false } }));
      return null;
    }
  },

  saveLifecycleDraft: async () => {
    const { draft, originalId } = get().lcEditor;
    if (!draft) return null;
    set((s) => ({ lcEditor: { ...s.lcEditor, isSaving: true, error: null } }));
    try {
      const definition = originalId
        ? await updateLifecycleDefinition(originalId, {
            name: draft.name,
            description: draft.description,
            recommended_roles: draft.recommended_roles,
            entry_step_key: draft.entry_step_key,
            steps: draft.steps,
            edges: draft.edges,
          })
        : await createLifecycleDefinition({
            project_id: draft.project_id,
            key: draft.key,
            name: draft.name,
            description: draft.description,
            target_kind: draft.target_kind,
            recommended_roles: draft.recommended_roles,
            entry_step_key: draft.entry_step_key,
            steps: draft.steps,
            edges: draft.edges,
          });
      set((state) => ({
        lifecycleDefinitions: upsert(state.lifecycleDefinitions, definition),
        lcEditor: {
          ...state.lcEditor,
          draft: lifecycleToDraft(definition),
          originalId: definition.id,
          validation: null,
          isSaving: false,
          dirty: false,
        },
      }));
      return definition;
    } catch (error) {
      set((s) => ({ lcEditor: { ...s.lcEditor, error: (error as Error).message, isSaving: false } }));
      return null;
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
}));
