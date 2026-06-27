/**
 * Run-scoped Task plan API.
 *
 * Task plan facts live on LifecycleRun; runtime artifacts and linked runs stay in lifecycle services.
 */

import { api } from "../api/client";
import type {
  CreateRunTaskRequest,
  RunTaskCommandResponse,
  RunTaskPlanResponse,
  TaskPlanStatus,
  UpdateRunTaskRequest,
} from "../types";

export interface RunTaskPlanQuery {
  created_by_agent_id?: string;
  owner_agent_id?: string;
  assigned_agent_id?: string;
  include_archived?: boolean;
}

function buildQuery(query: RunTaskPlanQuery = {}): string {
  const params = new URLSearchParams();
  if (query.created_by_agent_id) params.set("created_by_agent_id", query.created_by_agent_id);
  if (query.owner_agent_id) params.set("owner_agent_id", query.owner_agent_id);
  if (query.assigned_agent_id) params.set("assigned_agent_id", query.assigned_agent_id);
  if (query.include_archived) params.set("include_archived", "true");
  const text = params.toString();
  return text ? `?${text}` : "";
}

export async function fetchRunTasks(
  runId: string,
  query?: RunTaskPlanQuery,
): Promise<RunTaskPlanResponse> {
  return api.get<RunTaskPlanResponse>(
    `/lifecycle-runs/${encodeURIComponent(runId)}/tasks${buildQuery(query)}`,
  );
}

export async function fetchAgentRunTasks(
  runId: string,
  agentId: string,
  query?: RunTaskPlanQuery,
): Promise<RunTaskPlanResponse> {
  return api.get<RunTaskPlanResponse>(
    `/agent-runs/${encodeURIComponent(runId)}/agents/${encodeURIComponent(agentId)}/tasks${buildQuery(query)}`,
  );
}

export async function createAgentRunTask(
  runId: string,
  agentId: string,
  request: CreateRunTaskRequest,
): Promise<RunTaskCommandResponse> {
  return api.post<RunTaskCommandResponse>(
    `/agent-runs/${encodeURIComponent(runId)}/agents/${encodeURIComponent(agentId)}/tasks`,
    request,
  );
}

export async function updateRunTask(
  runId: string,
  taskId: string,
  request: UpdateRunTaskRequest,
): Promise<RunTaskCommandResponse> {
  return api.patch<RunTaskCommandResponse>(
    `/lifecycle-runs/${encodeURIComponent(runId)}/tasks/${encodeURIComponent(taskId)}`,
    request,
  );
}

export async function updateRunTaskStatus(
  runId: string,
  taskId: string,
  status: TaskPlanStatus,
): Promise<RunTaskCommandResponse> {
  return api.patch<RunTaskCommandResponse>(
    `/lifecycle-runs/${encodeURIComponent(runId)}/tasks/${encodeURIComponent(taskId)}/status`,
    { status },
  );
}

export async function archiveRunTask(
  runId: string,
  taskId: string,
): Promise<RunTaskCommandResponse> {
  return api.post<RunTaskCommandResponse>(
    `/lifecycle-runs/${encodeURIComponent(runId)}/tasks/${encodeURIComponent(taskId)}/archive`,
    {},
  );
}
