import { create } from "zustand";

import type {
  BindingKindMetadata,
  WorkflowAgentRole,
  WorkflowAssignment,
  WorkflowContextBinding,
  WorkflowDefinition,

  WorkflowPhaseDefinition,
  WorkflowRecordArtifactType,
  WorkflowRecordPolicy,
  WorkflowTargetKind,
  WorkflowTemplate,
  WorkflowRun,
  WorkflowValidationResult,
} from "../types";
import {
  activateWorkflowPhase,
  assignProjectWorkflow,
  bootstrapWorkflowTemplate,
  completeWorkflowPhase,
  createWorkflowDefinition,
  deleteWorkflowDefinition,
  disableWorkflowDefinition,
  enableWorkflowDefinition,
  fetchBindingMetadata,
  fetchProjectWorkflowAssignments,
  fetchWorkflowDefinitions,
  fetchWorkflowTemplates,
  fetchWorkflowRunsByTarget,
  getWorkflowDefinition,
  startWorkflowRun,
  updateWorkflowDefinition,
  validateWorkflowDefinition,
} from "../services/workflow";

// ─── Editor Draft ──────────────────────────────────────

export interface WorkflowEditorDraft {
  id: string | null;
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_role: WorkflowAgentRole | null;
  phases: WorkflowPhaseDefinition[];
  record_policy: WorkflowRecordPolicy;
}

export function createEmptyDraft(): WorkflowEditorDraft {
  return {
    id: null,
    key: "",
    name: "",
    description: "",
    target_kind: "task",
    recommended_role: "task_execution_worker",
    phases: [],
    record_policy: {
      emit_summary: true,
      emit_journal_update: true,
      emit_archive_suggestion: true,
    },
  };
}

export function createEmptyPhase(index: number): WorkflowPhaseDefinition {
  return {
    key: `phase_${index + 1}`,
    title: "",
    description: "",
    agent_instructions: [],
    context_bindings: [],
    requires_session: false,
    completion_mode: "manual",
    default_artifact_type: null,
    default_artifact_title: null,
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
    phases: definition.phases.map((p) => ({ ...p })),
    record_policy: { ...definition.record_policy },
  };
}

// ─── Store Interface ───────────────────────────────────

interface WorkflowState {
  templates: WorkflowTemplate[];
  definitions: WorkflowDefinition[];
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

  bindingMetadata: BindingKindMetadata[];
  bindingMetadataLoaded: boolean;

  fetchTemplates: () => Promise<WorkflowTemplate[]>;
  fetchDefinitions: (targetKind?: WorkflowTargetKind) => Promise<WorkflowDefinition[]>;
  bootstrapTemplate: (builtinKey: string) => Promise<WorkflowDefinition | null>;
  fetchProjectAssignments: (projectId: string) => Promise<WorkflowAssignment[]>;
  assignWorkflowToProject: (input: {
    project_id: string;
    workflow_id: string;
    role: WorkflowAgentRole;
    enabled?: boolean;
    is_default?: boolean;
  }) => Promise<WorkflowAssignment | null>;
  fetchRunsByTarget: (targetKind: WorkflowTargetKind, targetId: string) => Promise<WorkflowRun[]>;
  startRun: (input: {
    workflow_id?: string;
    workflow_key?: string;
    target_kind: WorkflowTargetKind;
    target_id: string;
  }) => Promise<WorkflowRun | null>;
  activatePhase: (input: {
    run_id: string;
    phase_key: string;
    session_binding_id?: string;
  }) => Promise<WorkflowRun | null>;
  completePhase: (input: {
    run_id: string;
    phase_key: string;
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
  updateDraftPhase: (phaseIndex: number, patch: Partial<WorkflowPhaseDefinition>) => void;
  addDraftPhase: () => void;
  removeDraftPhase: (phaseIndex: number) => void;
  moveDraftPhase: (fromIndex: number, toIndex: number) => void;
  updateDraftPhaseBinding: (phaseIndex: number, bindingIndex: number, patch: Partial<WorkflowContextBinding>) => void;
  addDraftPhaseBinding: (phaseIndex: number) => void;
  removeDraftPhaseBinding: (phaseIndex: number, bindingIndex: number) => void;
  validateDraft: () => Promise<WorkflowValidationResult | null>;
  saveDraft: () => Promise<WorkflowDefinition | null>;
  enableDefinition: (id: string) => Promise<WorkflowDefinition | null>;
  disableDefinition: (id: string) => Promise<WorkflowDefinition | null>;
  removeDefinition: (id: string) => Promise<boolean>;
  loadBindingMetadata: () => Promise<BindingKindMetadata[]>;
}

// ─── Helpers ───────────────────────────────────────────

function upsertDefinition(definitions: WorkflowDefinition[], next: WorkflowDefinition): WorkflowDefinition[] {
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

function replacePhase(phases: WorkflowPhaseDefinition[], index: number, patch: Partial<WorkflowPhaseDefinition>): WorkflowPhaseDefinition[] {
  return phases.map((phase, i) => (i === index ? { ...phase, ...patch } : phase));
}

// ─── Store ─────────────────────────────────────────────

export const useWorkflowStore = create<WorkflowState>((set, get) => ({
  templates: [],
  definitions: [],
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

  bindingMetadata: [],
  bindingMetadataLoaded: false,

  // ─── Existing: Templates & Definitions ───────────────

  fetchTemplates: async () => {
    set({ isLoading: true, error: null });
    try {
      const templates = await fetchWorkflowTemplates();
      set({ templates, isLoading: false });
      return templates;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return [];
    }
  },

  fetchDefinitions: async (targetKind) => {
    set({ isLoading: true, error: null });
    try {
      const definitions = await fetchWorkflowDefinitions(targetKind);
      set((state) => {
        const nextDefinitions = targetKind
          ? [
              ...state.definitions.filter((item) => item.target_kind !== targetKind),
              ...definitions,
            ]
          : definitions;
        return { definitions: nextDefinitions, isLoading: false };
      });
      return definitions;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return [];
    }
  },

  bootstrapTemplate: async (builtinKey) => {
    set({ isLoading: true, error: null });
    try {
      const definition = await bootstrapWorkflowTemplate(builtinKey);
      set((state) => ({
        definitions: upsertDefinition(state.definitions, definition),
        isLoading: false,
      }));
      return definition;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return null;
    }
  },

  // ─── Existing: Assignments ───────────────────────────

  fetchProjectAssignments: async (projectId) => {
    set({ isLoading: true, error: null });
    try {
      const assignments = await fetchProjectWorkflowAssignments(projectId);
      set((state) => ({
        assignmentsByProjectId: {
          ...state.assignmentsByProjectId,
          [projectId]: assignments,
        },
        isLoading: false,
      }));
      return assignments;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return [];
    }
  },

  assignWorkflowToProject: async (input) => {
    set({ isLoading: true, error: null });
    try {
      const assignment = await assignProjectWorkflow(input);
      const refreshedAssignments = await fetchProjectWorkflowAssignments(input.project_id);
      set((state) => ({
        assignmentsByProjectId: {
          ...state.assignmentsByProjectId,
          [input.project_id]: refreshedAssignments,
        },
        isLoading: false,
      }));
      return assignment;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return null;
    }
  },

  // ─── Existing: Runs ──────────────────────────────────

  fetchRunsByTarget: async (targetKind, targetId) => {
    set({ isLoading: true, error: null });
    try {
      const runs = await fetchWorkflowRunsByTarget(targetKind, targetId);
      set((state) => ({
        runsByTargetKey: {
          ...state.runsByTargetKey,
          [targetKey(targetKind, targetId)]: runs,
        },
        isLoading: false,
      }));
      return runs;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return [];
    }
  },

  startRun: async (input) => {
    set({ isLoading: true, error: null });
    try {
      const run = await startWorkflowRun(input);
      set((state) => ({
        runsByTargetKey: upsertRun(state.runsByTargetKey, run),
        isLoading: false,
      }));
      return run;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return null;
    }
  },

  activatePhase: async (input) => {
    set({ isLoading: true, error: null });
    try {
      const run = await activateWorkflowPhase(input);
      set((state) => ({
        runsByTargetKey: upsertRun(state.runsByTargetKey, run),
        isLoading: false,
      }));
      return run;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return null;
    }
  },

  completePhase: async (input) => {
    set({ isLoading: true, error: null });
    try {
      const run = await completeWorkflowPhase(input);
      set((state) => ({
        runsByTargetKey: upsertRun(state.runsByTargetKey, run),
        isLoading: false,
      }));
      return run;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return null;
    }
  },

  // ─── Editor: Draft Lifecycle ─────────────────────────

  openNewDraft: () => {
    set({
      editorDraft: createEmptyDraft(),
      editorOriginalId: null,
      editorValidation: null,
      editorIsSaving: false,
      editorIsValidating: false,
      editorDirty: false,
    });
  },

  openEditDraft: async (definitionId) => {
    set({ isLoading: true, error: null });
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
        isLoading: false,
      }));
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
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
    });
  },

  // ─── Editor: Draft Mutations ─────────────────────────

  updateDraft: (patch) => {
    set((state) => {
      if (!state.editorDraft) return state;
      return {
        editorDraft: { ...state.editorDraft, ...patch },
        editorDirty: true,
      };
    });
  },

  updateDraftPhase: (phaseIndex, patch) => {
    set((state) => {
      if (!state.editorDraft) return state;
      return {
        editorDraft: {
          ...state.editorDraft,
          phases: replacePhase(state.editorDraft.phases, phaseIndex, patch),
        },
        editorDirty: true,
      };
    });
  },

  addDraftPhase: () => {
    set((state) => {
      if (!state.editorDraft) return state;
      const nextPhases = [...state.editorDraft.phases, createEmptyPhase(state.editorDraft.phases.length)];
      return {
        editorDraft: { ...state.editorDraft, phases: nextPhases },
        editorDirty: true,
      };
    });
  },

  removeDraftPhase: (phaseIndex) => {
    set((state) => {
      if (!state.editorDraft) return state;
      return {
        editorDraft: {
          ...state.editorDraft,
          phases: state.editorDraft.phases.filter((_, i) => i !== phaseIndex),
        },
        editorDirty: true,
      };
    });
  },

  moveDraftPhase: (fromIndex, toIndex) => {
    set((state) => {
      if (!state.editorDraft) return state;
      const phases = [...state.editorDraft.phases];
      const [moved] = phases.splice(fromIndex, 1);
      if (!moved) return state;
      phases.splice(toIndex, 0, moved);
      return {
        editorDraft: { ...state.editorDraft, phases },
        editorDirty: true,
      };
    });
  },

  updateDraftPhaseBinding: (phaseIndex, bindingIndex, patch) => {
    set((state) => {
      if (!state.editorDraft) return state;
      const phase = state.editorDraft.phases[phaseIndex];
      if (!phase) return state;
      const bindings = phase.context_bindings.map((b, i) =>
        i === bindingIndex ? { ...b, ...patch } : b,
      );
      return {
        editorDraft: {
          ...state.editorDraft,
          phases: replacePhase(state.editorDraft.phases, phaseIndex, { context_bindings: bindings }),
        },
        editorDirty: true,
      };
    });
  },

  addDraftPhaseBinding: (phaseIndex) => {
    set((state) => {
      if (!state.editorDraft) return state;
      const phase = state.editorDraft.phases[phaseIndex];
      if (!phase) return state;
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
          phases: replacePhase(state.editorDraft.phases, phaseIndex, {
            context_bindings: [...phase.context_bindings, newBinding],
          }),
        },
        editorDirty: true,
      };
    });
  },

  removeDraftPhaseBinding: (phaseIndex, bindingIndex) => {
    set((state) => {
      if (!state.editorDraft) return state;
      const phase = state.editorDraft.phases[phaseIndex];
      if (!phase) return state;
      return {
        editorDraft: {
          ...state.editorDraft,
          phases: replacePhase(state.editorDraft.phases, phaseIndex, {
            context_bindings: phase.context_bindings.filter((_, i) => i !== bindingIndex),
          }),
        },
        editorDirty: true,
      };
    });
  },

  // ─── Editor: Validate & Save ─────────────────────────

  validateDraft: async () => {
    const draft = get().editorDraft;
    if (!draft) return null;
    set({ editorIsValidating: true, error: null });
    try {
      const result = await validateWorkflowDefinition({
        key: draft.key,
        name: draft.name,
        description: draft.description,
        target_kind: draft.target_kind,
        recommended_role: draft.recommended_role,
        phases: draft.phases,
        record_policy: draft.record_policy,
      });
      set({ editorValidation: result, editorIsValidating: false });
      return result;
    } catch (error) {
      set({ error: (error as Error).message, editorIsValidating: false });
      return null;
    }
  },

  saveDraft: async () => {
    const draft = get().editorDraft;
    const originalId = get().editorOriginalId;
    if (!draft) return null;
    set({ editorIsSaving: true, error: null });
    try {
      let definition: WorkflowDefinition;
      if (originalId) {
        definition = await updateWorkflowDefinition(originalId, {
          name: draft.name,
          description: draft.description,
          recommended_role: draft.recommended_role,
          phases: draft.phases,
          record_policy: draft.record_policy,
        });
      } else {
        definition = await createWorkflowDefinition({
          key: draft.key,
          name: draft.name,
          description: draft.description,
          target_kind: draft.target_kind,
          recommended_role: draft.recommended_role,
          phases: draft.phases,
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
      set({ error: (error as Error).message, editorIsSaving: false });
      return null;
    }
  },

  // ─── Editor: Enable/Disable/Delete ───────────────────

  enableDefinition: async (id) => {
    set({ isLoading: true, error: null });
    try {
      const definition = await enableWorkflowDefinition(id);
      set((state) => ({
        definitions: upsertDefinition(state.definitions, definition),
        isLoading: false,
      }));
      return definition;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return null;
    }
  },

  disableDefinition: async (id) => {
    set({ isLoading: true, error: null });
    try {
      const definition = await disableWorkflowDefinition(id);
      set((state) => ({
        definitions: upsertDefinition(state.definitions, definition),
        isLoading: false,
      }));
      return definition;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return null;
    }
  },

  removeDefinition: async (id) => {
    set({ isLoading: true, error: null });
    try {
      await deleteWorkflowDefinition(id);
      set((state) => ({
        definitions: state.definitions.filter((d) => d.id !== id),
        isLoading: false,
      }));
      return true;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return false;
    }
  },

  // ─── Binding Metadata ────────────────────────────────

  loadBindingMetadata: async () => {
    if (get().bindingMetadataLoaded) return get().bindingMetadata;
    try {
      const metadata = await fetchBindingMetadata();
      set({ bindingMetadata: metadata, bindingMetadataLoaded: true });
      return metadata;
    } catch (error) {
      set({ error: (error as Error).message });
      return [];
    }
  },
}));
