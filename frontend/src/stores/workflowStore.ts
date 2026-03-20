import { create } from "zustand";

import type {
  WorkflowAgentRole,
  WorkflowAssignment,
  WorkflowDefinition,
  WorkflowRecordArtifactType,
  WorkflowRun,
  WorkflowTargetKind,
} from "../types";
import {
  activateWorkflowPhase,
  assignProjectWorkflow,
  bootstrapTrellisWorkflow,
  completeWorkflowPhase,
  fetchProjectWorkflowAssignments,
  fetchWorkflowDefinitions,
  fetchWorkflowRunsByTarget,
  startWorkflowRun,
} from "../services/workflow";

interface WorkflowState {
  definitions: WorkflowDefinition[];
  assignmentsByProjectId: Record<string, WorkflowAssignment[]>;
  runsByTargetKey: Record<string, WorkflowRun[]>;
  isLoading: boolean;
  error: string | null;

  fetchDefinitions: (targetKind?: WorkflowTargetKind) => Promise<WorkflowDefinition[]>;
  bootstrapTrellis: (targetKind: WorkflowTargetKind) => Promise<WorkflowDefinition | null>;
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

export const useWorkflowStore = create<WorkflowState>((set) => ({
  definitions: [],
  assignmentsByProjectId: {},
  runsByTargetKey: {},
  isLoading: false,
  error: null,

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
        return {
          definitions: nextDefinitions,
          isLoading: false,
        };
      });
      return definitions;
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
      return [];
    }
  },

  bootstrapTrellis: async (targetKind) => {
    set({ isLoading: true, error: null });
    try {
      const definition = await bootstrapTrellisWorkflow(targetKind);
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
}));
