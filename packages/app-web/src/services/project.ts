/**
 * Project service 层。
 *
 * 收口 project / project-agent / grant 相关的 api.client 调用，
 * 并将后端 JSON ↔ 前端类型的 mapper（ProjectAgentSummary / ProjectAgentLaunch）
 * 集中于此。projectStore 只消费此层导出的函数，不直连 api。
 */

import { api } from "../api/client";
import { requireStringField } from "../api/mappers";
import type {
  ContextContainerDefinition,
  Project,
  ProjectAgent,
  ProjectAgentLaunchResult,
  ProjectAgentSessionStartResult,
  ProjectAgentSummary,
  CreateProjectAgentSessionRequest,
  ProjectConfig,
  ProjectRole,
  ProjectSubjectGrant,
} from "../types";
import { isThinkingLevel } from "../types";

// ─── Mapper ──────────────────────────────────────────────

function mapProjectAgentSummary(raw: Record<string, unknown>): ProjectAgentSummary {
  const rawExecutor =
    raw.executor && typeof raw.executor === "object" ? (raw.executor as Record<string, unknown>) : {};
  const thinkingLevel = isThinkingLevel(rawExecutor.thinking_level) ? rawExecutor.thinking_level : null;

  return {
    key: String(raw.key ?? ""),
    display_name: String(raw.display_name ?? "未命名 Agent"),
    description: String(raw.description ?? ""),
    executor: {
      executor: String(rawExecutor.executor ?? ""),
      provider_id: rawExecutor.provider_id != null ? String(rawExecutor.provider_id) : null,
      model_id: rawExecutor.model_id != null ? String(rawExecutor.model_id) : null,
      agent_id: rawExecutor.agent_id != null ? String(rawExecutor.agent_id) : null,
      thinking_level: thinkingLevel,
      permission_policy:
        rawExecutor.permission_policy != null ? String(rawExecutor.permission_policy) : null,
    },
    preset_name: raw.preset_name != null ? String(raw.preset_name) : null,
    source: String(raw.source ?? ""),
  };
}

function requireRecordField(raw: Record<string, unknown>, field: string): Record<string, unknown> {
  const value = raw[field];
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  throw new Error(`字段 ${field} 必须是对象`);
}

function mapProjectAgentLaunchResult(raw: Record<string, unknown>): ProjectAgentLaunchResult {
  const rawAgent =
    raw.agent && typeof raw.agent === "object" ? (raw.agent as Record<string, unknown>) : {};
  const runRef = requireRecordField(raw, "run_ref");
  const agentRef = requireRecordField(raw, "agent_ref");
  const frameRef = requireRecordField(raw, "frame_ref");
  const runtimeRef = raw.delivery_runtime_ref == null ? null : requireRecordField(raw, "delivery_runtime_ref");
  const assignmentRef = raw.assignment_ref == null ? null : requireRecordField(raw, "assignment_ref");
  const subjectRef = raw.subject_ref == null ? null : requireRecordField(raw, "subject_ref");

  return {
    created: Boolean(raw.created),
    run_ref: { run_id: requireStringField(runRef, "run_id") },
    agent_ref: {
      run_id: requireStringField(agentRef, "run_id"),
      agent_id: requireStringField(agentRef, "agent_id"),
    },
    frame_ref: {
      agent_id: requireStringField(frameRef, "agent_id"),
      frame_id: requireStringField(frameRef, "frame_id"),
      revision: typeof frameRef.revision === "number" ? frameRef.revision : undefined,
    },
    delivery_runtime_ref: runtimeRef
      ? { runtime_session_id: requireStringField(runtimeRef, "runtime_session_id") }
      : undefined,
    assignment_ref: assignmentRef
      ? {
          assignment_id: requireStringField(assignmentRef, "assignment_id"),
          run_id: assignmentRef.run_id != null ? String(assignmentRef.run_id) : undefined,
          agent_id: assignmentRef.agent_id != null ? String(assignmentRef.agent_id) : undefined,
          frame_id: assignmentRef.frame_id != null ? String(assignmentRef.frame_id) : undefined,
        }
      : undefined,
    subject_ref: subjectRef
      ? {
          kind: requireStringField(subjectRef, "kind"),
          id: requireStringField(subjectRef, "id"),
        }
      : undefined,
    agent: mapProjectAgentSummary(rawAgent),
  };
}

// ─── Project API ─────────────────────────────────────────

export async function fetchProjects(): Promise<Project[]> {
  return api.get<Project[]>("/projects");
}

export async function createProject(
  name: string,
  description: string,
  config?: Partial<ProjectConfig>,
): Promise<Project> {
  return api.post<Project>("/projects", {
    name,
    description,
    config: config ?? {
      agent_presets: [],
      context_containers: [],
    },
  });
}

export interface UpdateProjectPayload {
  name?: string;
  description?: string;
  config?: ProjectConfig;
  context_containers?: ContextContainerDefinition[];
  visibility?: Project["visibility"];
  is_template?: boolean;
}

export async function updateProject(id: string, payload: UpdateProjectPayload): Promise<Project> {
  return api.put<Project>(`/projects/${id}`, payload);
}

export async function updateProjectConfig(
  id: string,
  config: Partial<ProjectConfig>,
): Promise<Project> {
  return api.put<Project>(`/projects/${id}`, { config });
}

export async function cloneProject(
  projectId: string,
  payload?: { name?: string; description?: string },
): Promise<Project> {
  return api.post<Project>(`/projects/${projectId}/clone`, payload ?? {});
}

export async function deleteProject(id: string): Promise<void> {
  await api.delete(`/projects/${id}`);
}

// ─── Project Agent 配置 API ──────────────────────────────

export interface CreateProjectAgentPayload {
  name: string;
  agent_type: string;
  config?: Record<string, unknown>;
  default_lifecycle_key?: string;
  is_default_for_story?: boolean;
  is_default_for_task?: boolean;
}

export interface UpdateProjectAgentPayload {
  name?: string;
  agent_type?: string;
  config?: Record<string, unknown>;
  default_lifecycle_key?: string;
  is_default_for_story?: boolean;
  is_default_for_task?: boolean;
  knowledge_enabled?: boolean;
}

export async function fetchProjectAgentConfigs(projectId: string): Promise<ProjectAgent[]> {
  return api.get<ProjectAgent[]>(`/projects/${projectId}/agents`);
}

export async function createProjectAgent(
  projectId: string,
  payload: CreateProjectAgentPayload,
): Promise<ProjectAgent> {
  return api.post<ProjectAgent>(`/projects/${projectId}/agents`, payload);
}

export async function updateProjectAgent(
  projectId: string,
  agentId: string,
  payload: UpdateProjectAgentPayload,
): Promise<ProjectAgent> {
  return api.put<ProjectAgent>(`/projects/${projectId}/agents/${agentId}`, payload);
}

export async function deleteProjectAgent(projectId: string, agentId: string): Promise<void> {
  await api.delete(`/projects/${projectId}/agents/${agentId}`);
}

// ─── Project Agent Summary / Session API ─────────────────

export async function fetchProjectAgents(projectId: string): Promise<ProjectAgentSummary[]> {
  const response = await api.get<Record<string, unknown>[]>(`/projects/${projectId}/agents/summary`);
  return response.map(mapProjectAgentSummary);
}

export async function launchProjectAgent(
  projectId: string,
  agentKey: string,
): Promise<ProjectAgentLaunchResult> {
  const response = await api.post<Record<string, unknown>>(
    `/projects/${projectId}/agents/${encodeURIComponent(agentKey)}/launch`,
    {},
  );
  return mapProjectAgentLaunchResult(response);
}

export async function createProjectAgentRuntimeSession(
  projectId: string,
  agentKey: string,
  payload: CreateProjectAgentSessionRequest,
): Promise<ProjectAgentSessionStartResult> {
  return api.post<ProjectAgentSessionStartResult>(
    `/projects/${projectId}/agents/${encodeURIComponent(agentKey)}/sessions`,
    payload,
  );
}

// ─── Grant API ───────────────────────────────────────────

export async function fetchProjectGrants(projectId: string): Promise<ProjectSubjectGrant[]> {
  return api.get<ProjectSubjectGrant[]>(`/projects/${projectId}/grants`);
}

export async function grantProjectUser(
  projectId: string,
  userId: string,
  role: ProjectRole,
): Promise<ProjectSubjectGrant> {
  return api.put<ProjectSubjectGrant>(
    `/projects/${projectId}/grants/users/${encodeURIComponent(userId)}`,
    { role },
  );
}

export async function revokeProjectUser(projectId: string, userId: string): Promise<void> {
  await api.delete(`/projects/${projectId}/grants/users/${encodeURIComponent(userId)}`);
}

export async function grantProjectGroup(
  projectId: string,
  groupId: string,
  role: ProjectRole,
): Promise<ProjectSubjectGrant> {
  return api.put<ProjectSubjectGrant>(
    `/projects/${projectId}/grants/groups/${encodeURIComponent(groupId)}`,
    { role },
  );
}

export async function revokeProjectGroup(projectId: string, groupId: string): Promise<void> {
  await api.delete(`/projects/${projectId}/grants/groups/${encodeURIComponent(groupId)}`);
}
