import { api } from "../api/client";
import type {
  RegenerateTokenResponse,
  Routine,
  RoutineCreationResponse,
  RoutineExecution,
} from "../types";

export interface CreateRoutinePayload {
  name: string;
  prompt_template: string;
  project_agent_id: string;
  trigger_config: Record<string, unknown>;
  dispatch_strategy?: Record<string, unknown>;
}

export type UpdateRoutinePayload = Record<string, unknown>;

export async function fetchProjectRoutines(projectId: string): Promise<Routine[]> {
  return api.get<Routine[]>(`/projects/${projectId}/routines`);
}

export async function createRoutine(
  projectId: string,
  payload: CreateRoutinePayload,
): Promise<RoutineCreationResponse> {
  return api.post<RoutineCreationResponse>(`/projects/${projectId}/routines`, payload);
}

export async function updateRoutine(
  routineId: string,
  payload: UpdateRoutinePayload,
): Promise<Routine> {
  return api.put<Routine>(`/routines/${routineId}`, payload);
}

export async function deleteRoutine(routineId: string): Promise<void> {
  await api.delete(`/routines/${routineId}`);
}

export async function setRoutineEnabled(
  routineId: string,
  enabled: boolean,
): Promise<Routine> {
  return api.patch<Routine>(`/routines/${routineId}/enable`, { enabled });
}

export async function regenerateRoutineToken(
  routineId: string,
): Promise<RegenerateTokenResponse> {
  return api.post<RegenerateTokenResponse>(`/routines/${routineId}/regenerate-token`, {});
}

export async function fetchRoutineExecutions(
  routineId: string,
  limit = 20,
  offset = 0,
): Promise<RoutineExecution[]> {
  return api.get<RoutineExecution[]>(
    `/routines/${routineId}/executions?limit=${limit}&offset=${offset}`,
  );
}
