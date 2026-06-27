import { api } from "../api/client";
import type {
  CreateRoutineRequest,
  RegenerateTokenResponse,
  RoutineCreationResponse,
  RoutineExecutionResponse,
  RoutineResponse,
  UpdateRoutineRequest,
} from "../generated/routine-contracts";

export type CreateRoutinePayload = CreateRoutineRequest;
export type UpdateRoutinePayload = UpdateRoutineRequest;

export async function fetchProjectRoutines(projectId: string): Promise<RoutineResponse[]> {
  return api.get<RoutineResponse[]>(`/projects/${projectId}/routines`);
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
): Promise<RoutineResponse> {
  return api.put<RoutineResponse>(`/routines/${routineId}`, payload);
}

export async function deleteRoutine(routineId: string): Promise<void> {
  await api.delete(`/routines/${routineId}`);
}

export async function setRoutineEnabled(
  routineId: string,
  enabled: boolean,
): Promise<RoutineResponse> {
  return api.patch<RoutineResponse>(`/routines/${routineId}/enable`, { enabled });
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
): Promise<RoutineExecutionResponse[]> {
  return api.get<RoutineExecutionResponse[]>(
    `/routines/${routineId}/executions?limit=${limit}&offset=${offset}`,
  );
}
