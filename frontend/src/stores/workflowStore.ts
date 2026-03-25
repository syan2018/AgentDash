import { create } from "zustand";

import type {
  BindingKindMetadata,
  LifecycleDefinition,
  LifecycleStepDefinition,
  WorkflowAgentRole,
  WorkflowAttachmentSpec,
  WorkflowAssignment,
  WorkflowContextBinding,
  WorkflowContract,
  WorkflowDefinition,
  WorkflowRecordArtifactType,
  WorkflowRecordPolicy,
  WorkflowRun,
  WorkflowSessionTerminalState,
  WorkflowTargetKind,
  WorkflowTemplate,
  WorkflowValidationResult,
} from "../types";
import {
  activateWorkflowStep,
  assignProjectLifecycle,
  bootstrapWorkflowTemplate,
  completeWorkflowStep,
  createLifecycleDefinition,
  createWorkflowDefinition,
  deleteLifecycleDefinition,
  deleteWorkflowDefinition,
  disableLifecycleDefinition,
  disableWorkflowDefinition,
  enableLifecycleDefinition,
  enableWorkflowDefinition,
  fetchBindingMetadata,
  fetchLifecycleDefinitions,
  fetchProjectWorkflowAssignments,
  fetchWorkflowDefinitions,
  fetchWorkflowRunsByTarget,
  fetchWorkflowTemplates,
  getLifecycleDefinition,
  getWorkflowDefinition,
  startWorkflowRun,
  updateLifecycleDefinition,
  updateWorkflowDefinition,
  validateLifecycleDefinition,
  validateWorkflowDefinition,
} from "../services/workflow";

export interface WorkflowEditorDraft {
  id: string | null;
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_role: WorkflowAgentRole | null;
  contract: WorkflowContract;
  record_policy: WorkflowRecordPolicy;
}

export interface LifecycleEditorDraft {
  id: string | null;
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_role: WorkflowAgentRole | null;
  entry_step_key: string;
  steps: LifecycleStepDefinition[];
}

export function createEmptyDraft(): WorkflowEditorDraft {
  return {
    id: null,
    key: "",
    name: "",
    description: "",
    target_kind: "task",
    recommended_role: "task_execution_worker",
    contract: {
      injection: {
        goal: null,
        instructions: [],
        context_bindings: [],
        session_binding: "not_required",
      },
      hook_policy: {
        constraints: [],
      },
      completion: {
        checks: [],
        default_artifact_type: null,
        default_artifact_title: null,
      },
    },
    record_policy: {
      emit_summary: true,
      emit_journal_update: true,
      emit_archive_suggestion: true,
    },
  };
}

export function definitionToDraft(definition: WorkflowDefinition): WorkflowEditorDraft {
  return {
    id: definition.id,
    key: definition.key,
    name: definition.name,
    description: definition.description,
    target_kind: definition.target_kind,
    recommended_role: definition.recommended_role ?? null,
    contract: structuredClone(definition.contract),
    record_policy: { ...definition.record_policy },
  };
}

function createEmptyLifecycleStep(): LifecycleStepDefinition {
  return {
    key: "",
    title: "",
    description: "",
    primary_workflow_key: "",
    session_binding: "not_required",
    attached_workflows: [],
    transition: {
      policy: {
        kind: "manual",
        next_step_key: null,
        session_terminal_states: [],
        action_key: null,
      },
      on_failure: null,
    },
  };
}

export function createEmptyLifecycleDraft(): LifecycleEditorDraft {
  return {
    id: null,
    key: "",
    name: "",
    description: "",
    target_kind: "task",
    recommended_role: "task_execution_worker",
    entry_step_key: "",
    steps: [createEmptyLifecycleStep()],
  };
}

export function lifecycleToDraft(definition: LifecycleDefinition): LifecycleEditorDraft {
  return {
    id: definition.id,
    key: definition.key,
    name: definition.name,
    description: definition.description,
    target_kind: definition.target_kind,
    recommended_role: definition.recommended_role ?? null,
    entry_step_key: definition.entry_step_key,
    steps: structuredClone(definition.steps),
  };
}

interface WorkflowState {
  templates: WorkflowTemplate[];
  definitions: WorkflowDefinition[];
  lifecycleDefinitions: LifecycleDefinition[];
  assignmentsByProjectId: Record<string, WorkflowAssignment[]>;
  runsByTargetKey: Record<string, WorkflowRun[]>;
  isLoading: boolean;
  error: string | null;

  editorDraft: WorkflowEditorDraft | null;
  editorOriginalId: string | null;
  editorValidation: WorkflowValidationResult | null;
  editorIsSaving: boolean;
  editorIsValidating: boolean;
  editorDirty: boolean;
  editorIsLoading: boolean;
  editorError: string | null;

  lifecycleEditorDraft: LifecycleEditorDraft | null;
  lifecycleEditorOriginalId: string | null;
  lifecycleEditorValidation: WorkflowValidationResult | null;
  lifecycleEditorIsSaving: boolean;
  lifecycleEditorIsValidating: boolean;
  lifecycleEditorDirty: boolean;
  lifecycleEditorIsLoading: boolean;
  lifecycleEditorError: string | null;

  bindingMetadata: BindingKindMetadata[];
  bindingMetadataLoaded: boolean;

  fetchTemplates: () => Promise<WorkflowTemplate[]>;
  fetchDefinitions: (targetKind?: WorkflowTargetKind) => Promise<WorkflowDefinition[]>;
  fetchLifecycles: (targetKind?: WorkflowTargetKind) => Promise<LifecycleDefinition[]>;
  bootstrapTemplate: (builtinKey: string) => Promise<LifecycleDefinition | null>;
  fetchProjectAssignments: (projectId: string) => Promise<WorkflowAssignment[]>;
  assignLifecycleToProject: (input: {
    project_id: string;
    lifecycle_id: string;
    role: WorkflowAgentRole;
    enabled?: boolean;
    is_default?: boolean;
  }) => Promise<WorkflowAssignment | null>;
  fetchRunsByTarget: (targetKind: WorkflowTargetKind, targetId: string) => Promise<WorkflowRun[]>;
  startRun: (input: {
    lifecycle_id?: string;
    lifecycle_key?: string;
    target_kind: WorkflowTargetKind;
    target_id: string;
  }) => Promise<WorkflowRun | null>;
  activateStep: (input: {
    run_id: string;
    step_key: string;
    session_binding_id?: string;
  }) => Promise<WorkflowRun | null>;
  completeStep: (input: {
    run_id: string;
    step_key: string;
    summary?: string;
    record_artifacts?: Array<{
      artifact_type: WorkflowRecordArtifactType;
      title: string;
      content: string;
    }>;
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
  enableDefinition: (id: string) => Promise<WorkflowDefinition | null>;
  disableDefinition: (id: string) => Promise<WorkflowDefinition | null>;
  removeDefinition: (id: string) => Promise<boolean>;

  openNewLifecycleDraft: () => void;
  openEditLifecycleDraft: (definitionId: string) => Promise<void>;
  closeLifecycleDraft: () => void;
  updateLifecycleDraft: (patch: Partial<LifecycleEditorDraft>) => void;
  updateLifecycleStep: (stepIndex: number, patch: Partial<LifecycleStepDefinition>) => void;
  addLifecycleStep: () => void;
  removeLifecycleStep: (stepIndex: number) => void;
  updateLifecycleStepAttachments: (
    stepIndex: number,
    attachments: WorkflowAttachmentSpec[],
  ) => void;
  updateLifecycleStepTerminalStates: (
    stepIndex: number,
    states: WorkflowSessionTerminalState[],
  ) => void;
  validateLifecycleDraft: () => Promise<WorkflowValidationResult | null>;
  saveLifecycleDraft: () => Promise<LifecycleDefinition | null>;
  enableLifecycle: (id: string) => Promise<LifecycleDefinition | null>;
  disableLifecycle: (id: string) => Promise<LifecycleDefinition | null>;
  removeLifecycle: (id: string) => Promise<boolean>;
  loadBindingMetadata: () => Promise<BindingKindMetadata[]>;
}

function upsertDefinition(definitions: WorkflowDefinition[], next: WorkflowDefinition): WorkflowDefinition[] {
  const existingIndex = definitions.findIndex((item) => item.id === next.id);
  if (existingIndex >= 0) {
    const updated = [...definitions];
    updated[existingIndex] = next;
    return updated;
  }
  return [next, ...definitions];
}

function upsertLifecycle(
  definitions: LifecycleDefinition[],
  next: LifecycleDefinition,
): LifecycleDefinition[] {
  const existingIndex = definitions.findIndex((item) => item.id === next.id);
  if (existingIndex >= 0) {
    const updated = [...definitions];
    updated[existingIndex] = next;
    return updated;
  }
  return [next, ...definitions];
}

function targetKey(targetKind: WorkflowTargetKind, targetId: string): string {
  return `${targetKind}:${targetId}`;
}

function upsertRun(
  runsByTargetKey: Record<string, WorkflowRun[]>,
  run: WorkflowRun,
): Record<string, WorkflowRun[]> {
  const key = targetKey(run.target_kind, run.target_id);
  const existing = runsByTargetKey[key] ?? [];
  const nextRuns = existing.some((item) => item.id === run.id)
    ? existing.map((item) => (item.id === run.id ? run : item))
    : [run, ...existing];
  nextRuns.sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime());
  return {
    ...runsByTargetKey,
    [key]: nextRuns,
  };
}

export const useWorkflowStore = create<WorkflowState>((set, get) => ({
  templates: [],
  definitions: [],
  lifecycleDefinitions: [],
  assignmentsByProjectId: {},
  runsByTargetKey: {},
  isLoading: false,
  error: null,

  editorDraft: null,
  editorOriginalId: null,
  editorValidation: null,
  editorIsSaving: false,
  editorIsValidating: false,
  editorDirty: false,
  editorIsLoading: false,
  editorError: null,

  lifecycleEditorDraft: null,
  lifecycleEditorOriginalId: null,
  lifecycleEditorValidation: null,
  lifecycleEditorIsSaving: false,
  lifecycleEditorIsValidating: false,
  lifecycleEditorDirty: false,
  lifecycleEditorIsLoading: false,
  lifecycleEditorError: null,

  bindingMetadata: [],
  bindingMetadataLoaded: false,

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

  fetchDefinitions: async (targetKind) => {
    try {
      const definitions = await fetchWorkflowDefinitions(targetKind);
      set((state) => {
        const nextDefinitions = targetKind
          ? [
              ...state.definitions.filter((item) => item.target_kind !== targetKind),
              ...definitions,
            ]
          : definitions;
        return { definitions: nextDefinitions };
      });
      return definitions;
    } catch (error) {
      set({ error: (error as Error).message });
      return [];
    }
  },

  fetchLifecycles: async (targetKind) => {
    try {
      const lifecycleDefinitions = await fetchLifecycleDefinitions(targetKind);
      set((state) => {
        const nextDefinitions = targetKind
          ? [
              ...state.lifecycleDefinitions.filter((item) => item.target_kind !== targetKind),
              ...lifecycleDefinitions,
            ]
          : lifecycleDefinitions;
        return { lifecycleDefinitions: nextDefinitions };
      });
      return lifecycleDefinitions;
    } catch (error) {
      set({ error: (error as Error).message });
      return [];
    }
  },

  bootstrapTemplate: async (builtinKey) => {
    set({ error: null });
    try {
      const lifecycle = await bootstrapWorkflowTemplate(builtinKey);
      const [definitions, lifecycleDefinitions] = await Promise.all([
        fetchWorkflowDefinitions(),
        fetchLifecycleDefinitions(),
      ]);
      set({ definitions, lifecycleDefinitions });
      return lifecycle;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  fetchProjectAssignments: async (projectId) => {
    try {
      const assignments = await fetchProjectWorkflowAssignments(projectId);
      set((state) => ({
        assignmentsByProjectId: {
          ...state.assignmentsByProjectId,
          [projectId]: assignments,
        },
      }));
      return assignments;
    } catch (error) {
      set({ error: (error as Error).message });
      return [];
    }
  },

  assignLifecycleToProject: async (input) => {
    set({ error: null });
    try {
      const assignment = await assignProjectLifecycle(input);
      const refreshedAssignments = await fetchProjectWorkflowAssignments(input.project_id);
      set((state) => ({
        assignmentsByProjectId: {
          ...state.assignmentsByProjectId,
          [input.project_id]: refreshedAssignments,
        },
      }));
      return assignment;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  fetchRunsByTarget: async (targetKind, targetId) => {
    try {
      const runs = await fetchWorkflowRunsByTarget(targetKind, targetId);
      set((state) => ({
        runsByTargetKey: {
          ...state.runsByTargetKey,
          [targetKey(targetKind, targetId)]: runs,
        },
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
      set((state) => ({
        runsByTargetKey: upsertRun(state.runsByTargetKey, run),
      }));
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
      set((state) => ({
        runsByTargetKey: upsertRun(state.runsByTargetKey, run),
      }));
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
      set((state) => ({
        runsByTargetKey: upsertRun(state.runsByTargetKey, run),
      }));
      return run;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  openNewDraft: () => {
    set({
      editorDraft: createEmptyDraft(),
      editorOriginalId: null,
      editorValidation: null,
      editorIsSaving: false,
      editorIsValidating: false,
      editorDirty: false,
      editorIsLoading: false,
      editorError: null,
    });
  },

  openEditDraft: async (definitionId) => {
    set({ editorIsLoading: true, editorError: null });
    try {
      const definition = await getWorkflowDefinition(definitionId);
      set((state) => ({
        definitions: upsertDefinition(state.definitions, definition),
        editorDraft: definitionToDraft(definition),
        editorOriginalId: definition.id,
        editorValidation: null,
        editorIsSaving: false,
        editorIsValidating: false,
        editorDirty: false,
        editorIsLoading: false,
      }));
    } catch (error) {
      set({ editorError: (error as Error).message, editorIsLoading: false });
    }
  },

  closeDraft: () => {
    set({
      editorDraft: null,
      editorOriginalId: null,
      editorValidation: null,
      editorIsSaving: false,
      editorIsValidating: false,
      editorDirty: false,
      editorIsLoading: false,
      editorError: null,
    });
  },

  updateDraft: (patch) => {
    set((state) => {
      if (!state.editorDraft) return state;
      return {
        editorDraft: { ...state.editorDraft, ...patch },
        editorDirty: true,
      };
    });
  },

  updateDraftBinding: (bindingIndex, patch) => {
    set((state) => {
      if (!state.editorDraft) return state;
      const bindings = state.editorDraft.contract.injection.context_bindings.map((binding, index) =>
        index === bindingIndex ? { ...binding, ...patch } : binding,
      );
      return {
        editorDraft: {
          ...state.editorDraft,
          contract: {
            ...state.editorDraft.contract,
            injection: {
              ...state.editorDraft.contract.injection,
              context_bindings: bindings,
            },
          },
        },
        editorDirty: true,
      };
    });
  },

  addDraftBinding: () => {
    set((state) => {
      if (!state.editorDraft) return state;
      const newBinding: WorkflowContextBinding = {
        kind: "document_path",
        locator: "",
        reason: "",
        required: true,
        title: null,
      };
      return {
        editorDraft: {
          ...state.editorDraft,
          contract: {
            ...state.editorDraft.contract,
            injection: {
              ...state.editorDraft.contract.injection,
              context_bindings: [
                ...state.editorDraft.contract.injection.context_bindings,
                newBinding,
              ],
            },
          },
        },
        editorDirty: true,
      };
    });
  },

  removeDraftBinding: (bindingIndex) => {
    set((state) => {
      if (!state.editorDraft) return state;
      return {
        editorDraft: {
          ...state.editorDraft,
          contract: {
            ...state.editorDraft.contract,
            injection: {
              ...state.editorDraft.contract.injection,
              context_bindings: state.editorDraft.contract.injection.context_bindings.filter(
                (_, index) => index !== bindingIndex,
              ),
            },
          },
        },
        editorDirty: true,
      };
    });
  },

  validateDraft: async () => {
    const draft = get().editorDraft;
    if (!draft) return null;
    set({ editorIsValidating: true, editorError: null });
    try {
      const result = await validateWorkflowDefinition({
        key: draft.key,
        name: draft.name,
        description: draft.description,
        target_kind: draft.target_kind,
        recommended_role: draft.recommended_role,
        contract: draft.contract,
        record_policy: draft.record_policy,
      });
      set({ editorValidation: result, editorIsValidating: false });
      return result;
    } catch (error) {
      set({ editorError: (error as Error).message, editorIsValidating: false });
      return null;
    }
  },

  saveDraft: async () => {
    const draft = get().editorDraft;
    const originalId = get().editorOriginalId;
    if (!draft) return null;
    set({ editorIsSaving: true, editorError: null });
    try {
      let definition: WorkflowDefinition;
      if (originalId) {
        definition = await updateWorkflowDefinition(originalId, {
          name: draft.name,
          description: draft.description,
          recommended_role: draft.recommended_role,
          contract: draft.contract,
          record_policy: draft.record_policy,
        });
      } else {
        definition = await createWorkflowDefinition({
          key: draft.key,
          name: draft.name,
          description: draft.description,
          target_kind: draft.target_kind,
          recommended_role: draft.recommended_role,
          contract: draft.contract,
          record_policy: draft.record_policy,
        });
      }
      set((state) => ({
        definitions: upsertDefinition(state.definitions, definition),
        editorDraft: definitionToDraft(definition),
        editorOriginalId: definition.id,
        editorValidation: null,
        editorIsSaving: false,
        editorDirty: false,
      }));
      return definition;
    } catch (error) {
      set({ editorError: (error as Error).message, editorIsSaving: false });
      return null;
    }
  },

  enableDefinition: async (id) => {
    set({ error: null });
    try {
      const definition = await enableWorkflowDefinition(id);
      set((state) => ({
        definitions: upsertDefinition(state.definitions, definition),
      }));
      return definition;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  disableDefinition: async (id) => {
    set({ error: null });
    try {
      const definition = await disableWorkflowDefinition(id);
      set((state) => ({
        definitions: upsertDefinition(state.definitions, definition),
      }));
      return definition;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  removeDefinition: async (id) => {
    set({ error: null });
    try {
      await deleteWorkflowDefinition(id);
      set((state) => ({
        definitions: state.definitions.filter((item) => item.id !== id),
      }));
      return true;
    } catch (error) {
      set({ error: (error as Error).message });
      return false;
    }
  },

  openNewLifecycleDraft: () => {
    set({
      lifecycleEditorDraft: createEmptyLifecycleDraft(),
      lifecycleEditorOriginalId: null,
      lifecycleEditorValidation: null,
      lifecycleEditorIsSaving: false,
      lifecycleEditorIsValidating: false,
      lifecycleEditorDirty: false,
      lifecycleEditorIsLoading: false,
      lifecycleEditorError: null,
    });
  },

  openEditLifecycleDraft: async (definitionId) => {
    set({ lifecycleEditorIsLoading: true, lifecycleEditorError: null });
    try {
      const definition = await getLifecycleDefinition(definitionId);
      set((state) => ({
        lifecycleDefinitions: upsertLifecycle(state.lifecycleDefinitions, definition),
        lifecycleEditorDraft: lifecycleToDraft(definition),
        lifecycleEditorOriginalId: definition.id,
        lifecycleEditorValidation: null,
        lifecycleEditorIsSaving: false,
        lifecycleEditorIsValidating: false,
        lifecycleEditorDirty: false,
        lifecycleEditorIsLoading: false,
      }));
    } catch (error) {
      set({ lifecycleEditorError: (error as Error).message, lifecycleEditorIsLoading: false });
    }
  },

  closeLifecycleDraft: () => {
    set({
      lifecycleEditorDraft: null,
      lifecycleEditorOriginalId: null,
      lifecycleEditorValidation: null,
      lifecycleEditorIsSaving: false,
      lifecycleEditorIsValidating: false,
      lifecycleEditorDirty: false,
      lifecycleEditorIsLoading: false,
      lifecycleEditorError: null,
    });
  },

  updateLifecycleDraft: (patch) => {
    set((state) => {
      if (!state.lifecycleEditorDraft) return state;
      return {
        lifecycleEditorDraft: { ...state.lifecycleEditorDraft, ...patch },
        lifecycleEditorDirty: true,
      };
    });
  },

  updateLifecycleStep: (stepIndex, patch) => {
    set((state) => {
      if (!state.lifecycleEditorDraft) return state;
      return {
        lifecycleEditorDraft: {
          ...state.lifecycleEditorDraft,
          steps: state.lifecycleEditorDraft.steps.map((step, index) =>
            index === stepIndex ? { ...step, ...patch } : step,
          ),
        },
        lifecycleEditorDirty: true,
      };
    });
  },

  addLifecycleStep: () => {
    set((state) => {
      if (!state.lifecycleEditorDraft) return state;
      return {
        lifecycleEditorDraft: {
          ...state.lifecycleEditorDraft,
          steps: [...state.lifecycleEditorDraft.steps, createEmptyLifecycleStep()],
        },
        lifecycleEditorDirty: true,
      };
    });
  },

  removeLifecycleStep: (stepIndex) => {
    set((state) => {
      if (!state.lifecycleEditorDraft) return state;
      return {
        lifecycleEditorDraft: {
          ...state.lifecycleEditorDraft,
          steps: state.lifecycleEditorDraft.steps.filter((_, index) => index !== stepIndex),
        },
        lifecycleEditorDirty: true,
      };
    });
  },

  updateLifecycleStepAttachments: (stepIndex, attachments) => {
    set((state) => {
      if (!state.lifecycleEditorDraft) return state;
      return {
        lifecycleEditorDraft: {
          ...state.lifecycleEditorDraft,
          steps: state.lifecycleEditorDraft.steps.map((step, index) =>
            index === stepIndex ? { ...step, attached_workflows: attachments } : step,
          ),
        },
        lifecycleEditorDirty: true,
      };
    });
  },

  updateLifecycleStepTerminalStates: (stepIndex, states) => {
    set((state) => {
      if (!state.lifecycleEditorDraft) return state;
      return {
        lifecycleEditorDraft: {
          ...state.lifecycleEditorDraft,
          steps: state.lifecycleEditorDraft.steps.map((step, index) =>
            index === stepIndex ? { ...step, session_terminal_states: states } : step,
          ),
        },
        lifecycleEditorDirty: true,
      };
    });
  },

  validateLifecycleDraft: async () => {
    const draft = get().lifecycleEditorDraft;
    if (!draft) return null;
    set({ lifecycleEditorIsValidating: true, lifecycleEditorError: null });
    try {
      const result = await validateLifecycleDefinition({
        key: draft.key,
        name: draft.name,
        description: draft.description,
        target_kind: draft.target_kind,
        recommended_role: draft.recommended_role,
        entry_step_key: draft.entry_step_key,
        steps: draft.steps,
      });
      set({ lifecycleEditorValidation: result, lifecycleEditorIsValidating: false });
      return result;
    } catch (error) {
      set({ lifecycleEditorError: (error as Error).message, lifecycleEditorIsValidating: false });
      return null;
    }
  },

  saveLifecycleDraft: async () => {
    const draft = get().lifecycleEditorDraft;
    const originalId = get().lifecycleEditorOriginalId;
    if (!draft) return null;
    set({ lifecycleEditorIsSaving: true, lifecycleEditorError: null });
    try {
      let definition: LifecycleDefinition;
      if (originalId) {
        definition = await updateLifecycleDefinition(originalId, {
          name: draft.name,
          description: draft.description,
          recommended_role: draft.recommended_role,
          entry_step_key: draft.entry_step_key,
          steps: draft.steps,
        });
      } else {
        definition = await createLifecycleDefinition({
          key: draft.key,
          name: draft.name,
          description: draft.description,
          target_kind: draft.target_kind,
          recommended_role: draft.recommended_role,
          entry_step_key: draft.entry_step_key,
          steps: draft.steps,
        });
      }
      set((state) => ({
        lifecycleDefinitions: upsertLifecycle(state.lifecycleDefinitions, definition),
        lifecycleEditorDraft: lifecycleToDraft(definition),
        lifecycleEditorOriginalId: definition.id,
        lifecycleEditorValidation: null,
        lifecycleEditorIsSaving: false,
        lifecycleEditorDirty: false,
      }));
      return definition;
    } catch (error) {
      set({ lifecycleEditorError: (error as Error).message, lifecycleEditorIsSaving: false });
      return null;
    }
  },

  enableLifecycle: async (id) => {
    set({ error: null });
    try {
      const definition = await enableLifecycleDefinition(id);
      set((state) => ({
        lifecycleDefinitions: upsertLifecycle(state.lifecycleDefinitions, definition),
      }));
      return definition;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  disableLifecycle: async (id) => {
    set({ error: null });
    try {
      const definition = await disableLifecycleDefinition(id);
      set((state) => ({
        lifecycleDefinitions: upsertLifecycle(state.lifecycleDefinitions, definition),
      }));
      return definition;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  removeLifecycle: async (id) => {
    set({ error: null });
    try {
      await deleteLifecycleDefinition(id);
      set((state) => ({
        lifecycleDefinitions: state.lifecycleDefinitions.filter((item) => item.id !== id),
      }));
      return true;
    } catch (error) {
      set({ error: (error as Error).message });
      return false;
    }
  },

  loadBindingMetadata: async () => {
    if (get().bindingMetadataLoaded) {
      return get().bindingMetadata;
    }
    try {
      const metadata = await fetchBindingMetadata();
      set({
        bindingMetadata: metadata,
        bindingMetadataLoaded: true,
      });
      return metadata;
    } catch (error) {
      set({ error: (error as Error).message });
      return [];
    }
  },
}));
