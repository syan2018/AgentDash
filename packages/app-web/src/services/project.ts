/**
 * Project service 层。
 *
 * 收口 project / project-agent / grant 相关的 api.client 调用，
 * 并将后端 JSON ↔ 前端类型的 mapper（ProjectAgentSummary）
 * 集中于此。projectStore 只消费此层导出的函数，不直连 api。
 */

import { api } from "../api/client";
import type {
  ContextContainerDefinition,
  Project,
  ProjectAgent,
  ProjectAgentRunStartResult,
  ProjectAgentSummary,
  CreateProjectAgentRunRequest,
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
    },
    effective_executor_config:
      raw.effective_executor_config && typeof raw.effective_executor_config === "object"
        ? raw.effective_executor_config as ProjectAgentSummary["effective_executor_config"]
        : undefined,
    preset_name: raw.preset_name != null ? String(raw.preset_name) : null,
    source: String(raw.source ?? ""),
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
}

export interface UpdateProjectAgentPayload {
  name?: string;
  agent_type?: string;
  config?: Record<string, unknown>;
  default_lifecycle_key?: string;
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

// ─── Project Agent Summary / AgentRun API ─────────────────

export async function fetchProjectAgents(projectId: string): Promise<ProjectAgentSummary[]> {
  const response = await api.get<Record<string, unknown>[]>(`/projects/${projectId}/agents/summary`);
  return response.map(mapProjectAgentSummary);
}

export async function createProjectAgentRun(
  projectId: string,
  agentKey: string,
  payload: CreateProjectAgentRunRequest,
): Promise<ProjectAgentRunStartResult> {
  return api.post<ProjectAgentRunStartResult>(
    `/projects/${projectId}/agents/${encodeURIComponent(agentKey)}/agent-runs`,
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
