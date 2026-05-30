/**
 * Story / Task / StorySession service 层。
 *
 * 收口 story 相关的 api.client 调用与后端 JSON ↔ 前端类型的 mapper。
 * storyStore 只消费此层导出的函数，不直连 api。
 */

import { api } from "../api/client";
import { requireStringField } from "../api/mappers";
import type {
  AgentBinding,
  ContextContainerDefinition,
  ContextSourceRef,
  ExecutionVfs,
  ResolvedVfsSurface,
  SessionComposition,
  SessionContextSnapshot,
  Story,
  StoryContext,
  StorySessionInfo,
  StoryRunsResponse,
  StoryRunOverviewDto,
  Task,
} from "../types";
import { isThinkingLevel } from "../types";

// ─── 字段读取工具 ────────────────────────────────────────

const readNullableStringField = (raw: Record<string, unknown>, field: string): string | null => {
  const value = raw[field];
  if (value == null) {
    return null;
  }
  if (typeof value === "string") {
    return value;
  }
  throw new Error(`字段 ${field} 必须是字符串或 null`);
};

// ─── 状态/枚举归一化 ─────────────────────────────────────

const normalizeTaskStatus = (value: string): Task["status"] => {
  switch (value) {
    case "pending":
      return "pending";
    case "assigned":
      return "assigned";
    case "running":
      return "running";
    case "awaiting_verification":
      return "awaiting_verification";
    case "completed":
      return "completed";
    case "failed":
      return "failed";
    default:
      throw new Error(`未知 Task 状态: ${value}`);
  }
};

const defaultContext: StoryContext = {
  source_refs: [],
  context_containers: [],
  disabled_container_ids: [],
  session_composition: null,
};

const normalizeStoryPriority = (value: string): Story["priority"] => {
  switch (value) {
    case "p0":
    case "critical":
      return "p0";
    case "p1":
    case "high":
      return "p1";
    case "p2":
    case "medium":
      return "p2";
    case "p3":
    case "low":
      return "p3";
    default:
      throw new Error(`未知 Story priority: ${value}`);
  }
};

const normalizeStoryType = (value: string): Story["story_type"] => {
  switch (value) {
    case "feature":
      return "feature";
    case "bugfix":
    case "bug":
      return "bugfix";
    case "refactor":
      return "refactor";
    case "docs":
    case "documentation":
      return "docs";
    case "test":
      return "test";
    default:
      throw new Error(`未知 Story story_type: ${value}`);
  }
};

// ─── Mapper ──────────────────────────────────────────────

const mapStory = (raw: Record<string, unknown>): Story => {
  let context: StoryContext = defaultContext;
  if (raw.context != null) {
    if (typeof raw.context !== "object") {
      throw new Error("Story context 必须是对象");
    }
    const ctx = raw.context as Record<string, unknown>;
    if (ctx.source_refs != null && !Array.isArray(ctx.source_refs)) {
      throw new Error("Story context.source_refs 必须是数组");
    }
    if (ctx.context_containers != null && !Array.isArray(ctx.context_containers)) {
      throw new Error("Story context.context_containers 必须是数组");
    }
    if (ctx.disabled_container_ids != null && !Array.isArray(ctx.disabled_container_ids)) {
      throw new Error("Story context.disabled_container_ids 必须是数组");
    }
    context = {
      source_refs: ctx.source_refs == null ? [] : (ctx.source_refs as ContextSourceRef[]),
      context_containers:
        ctx.context_containers == null ? [] : (ctx.context_containers as ContextContainerDefinition[]),
      disabled_container_ids:
        ctx.disabled_container_ids == null ? [] : (ctx.disabled_container_ids as string[]),
      session_composition:
        ctx.session_composition == null ? null : (ctx.session_composition as SessionComposition),
    };
  }

  return {
    id: requireStringField(raw, "id"),
    project_id: requireStringField(raw, "project_id"),
    default_workspace_id: raw.default_workspace_id != null ? String(raw.default_workspace_id) : null,
    title: requireStringField(raw, "title"),
    description: raw.description ? String(raw.description) : "",
    status: requireStringField(raw, "status") as Story["status"],
    priority: normalizeStoryPriority(requireStringField(raw, "priority")),
    story_type: normalizeStoryType(requireStringField(raw, "story_type")),
    tags: Array.isArray(raw.tags) ? raw.tags.filter((t): t is string => typeof t === "string") : [],
    task_count: Number.isFinite(Number(raw.task_count ?? 0)) ? Number(raw.task_count ?? 0) : 0,
    context,
    created_at: requireStringField(raw, "created_at"),
    updated_at: requireStringField(raw, "updated_at"),
  };
};

const mapAgentBinding = (raw: unknown): AgentBinding => {
  if (!raw || typeof raw !== "object") {
    throw new Error("Task 缺少 agent_binding");
  }

  const binding = raw as Record<string, unknown>;
  return {
    agent_type: binding.agent_type ? String(binding.agent_type) : null,
    agent_pid: binding.agent_pid ? String(binding.agent_pid) : null,
    preset_name: binding.preset_name ? String(binding.preset_name) : null,
    prompt_template: binding.prompt_template ? String(binding.prompt_template) : null,
    initial_context: binding.initial_context ? String(binding.initial_context) : null,
    thinking_level:
      binding.thinking_level == null
        ? null
        : isThinkingLevel(binding.thinking_level)
          ? binding.thinking_level
          : (() => {
              throw new Error(`未知 thinking_level: ${String(binding.thinking_level)}`);
            })(),
    context_sources: Array.isArray(binding.context_sources)
      ? (binding.context_sources as ContextSourceRef[])
      : (() => {
          throw new Error("agent_binding.context_sources 必须是数组");
        })(),
  };
};

const normalizeArtifactType = (value: string): Task["artifacts"][number]["artifact_type"] => {
  switch (value) {
    case "code_change":
    case "test_result":
    case "log_output":
    case "file":
    case "tool_execution":
      return value;
    default:
      throw new Error(`未知 artifact_type: ${value}`);
  }
};

const mapArtifact = (raw: Record<string, unknown>): Task["artifacts"][number] => {
  return {
    id: requireStringField(raw, "id"),
    artifact_type: normalizeArtifactType(requireStringField(raw, "artifact_type")),
    content: raw.content ?? null,
    created_at: requireStringField(raw, "created_at"),
  };
};

const mapTask = (raw: Record<string, unknown>): Task => {
  return {
    id: requireStringField(raw, "id"),
    project_id: requireStringField(raw, "project_id"),
    story_id: requireStringField(raw, "story_id"),
    workspace_id: raw.workspace_id ? String(raw.workspace_id) : null,
    lifecycle_step_key: raw.lifecycle_step_key ? String(raw.lifecycle_step_key) : null,
    title: requireStringField(raw, "title"),
    description: raw.description ? String(raw.description) : "",
    status: normalizeTaskStatus(requireStringField(raw, "status")),
    agent_binding: mapAgentBinding(raw.agent_binding),
    artifacts:
      raw.artifacts == null
        ? []
        : Array.isArray(raw.artifacts)
          ? raw.artifacts.map((item, index) => {
              if (!item || typeof item !== "object") {
                throw new Error(`Task artifact[${index}] 必须是对象`);
              }
              return mapArtifact(item as Record<string, unknown>);
            })
          : (() => {
              throw new Error("Task artifacts 必须是数组");
            })(),
    created_at: requireStringField(raw, "created_at"),
    updated_at: requireStringField(raw, "updated_at"),
  };
};

/** Story 级会话绑定条目（替代已移除的 SessionBinding） */
export interface StorySessionEntry {
  id: string;
  session_id: string;
  label: string;
  session_title?: string;
  session_updated_at?: number;
}

const mapStorySessionEntry = (raw: Record<string, unknown>): StorySessionEntry => ({
  id: requireStringField(raw, "id"),
  session_id: requireStringField(raw, "session_id"),
  label: requireStringField(raw, "label"),
  session_title: raw.session_title != null ? String(raw.session_title) : undefined,
  session_updated_at: raw.session_updated_at != null ? Number(raw.session_updated_at) : undefined,
});

const requireStorySessionField = (raw: Record<string, unknown>, field: string): string => {
  const value = raw[field];
  if (typeof value === "string" && value.length > 0) {
    return value;
  }
  throw new Error(`StorySessionInfo 缺少必填字段: ${field}`);
};

// ─── 事件 payload → 实体的可映射性判定 ──────────────────

export const canMapStoryFromPayload = (payload: Record<string, unknown>): boolean => {
  return (
    typeof payload.id === "string" &&
    typeof payload.title === "string" &&
    typeof payload.project_id === "string" &&
    typeof payload.status === "string" &&
    payload.task_count !== undefined
  );
};

export const canMapTaskFromPayload = (payload: Record<string, unknown>): boolean => {
  return (
    typeof payload.id === "string" &&
    typeof payload.title === "string" &&
    typeof payload.story_id === "string" &&
    typeof payload.status === "string"
  );
};

export const mapStoryFromPayload = mapStory;
export const mapTaskFromPayload = mapTask;

// ─── Story API ───────────────────────────────────────────

export async function fetchStoriesByProject(projectId: string): Promise<Story[]> {
  const response = await api.get<Record<string, unknown>[]>(`/stories?project_id=${projectId}`);
  return response.map(mapStory);
}

export async function fetchStoryById(storyId: string): Promise<Story> {
  const raw = await api.get<Record<string, unknown>>(`/stories/${storyId}`);
  return mapStory(raw);
}

export interface CreateStoryOptions {
  status?: Story["status"];
  priority?: Story["priority"];
  story_type?: Story["story_type"];
  tags?: string[];
}

export async function createStory(
  projectId: string,
  title: string,
  description?: string,
  options?: CreateStoryOptions,
): Promise<Story> {
  const raw = await api.post<Record<string, unknown>>("/stories", {
    project_id: projectId,
    title,
    description,
    status: options?.status,
    priority: options?.priority,
    story_type: options?.story_type,
    tags: options?.tags,
  });
  return mapStory(raw);
}

export interface UpdateStoryPayload {
  title?: string;
  description?: string;
  default_workspace_id?: string | null;
  status?: Story["status"];
  priority?: Story["priority"];
  story_type?: Story["story_type"];
  tags?: string[];
  context_source_refs?: ContextSourceRef[];
  context_containers?: ContextContainerDefinition[];
  disabled_container_ids?: string[];
  session_composition?: SessionComposition | null;
  clear_session_composition?: boolean;
}

export async function updateStory(storyId: string, payload: UpdateStoryPayload): Promise<Story> {
  const raw = await api.put<Record<string, unknown>>(`/stories/${storyId}`, payload);
  return mapStory(raw);
}

export interface BatchStoryPatch {
  status?: Story["status"];
  priority?: Story["priority"];
  story_type?: Story["story_type"];
}

export function buildBatchStoryRequest(patch: BatchStoryPatch): Record<string, unknown> {
  const requestPayload: Record<string, unknown> = {};
  if (patch.status) requestPayload.status = patch.status;
  if (patch.priority) requestPayload.priority = patch.priority;
  if (patch.story_type) requestPayload.story_type = patch.story_type;
  return requestPayload;
}

export async function patchStory(storyId: string, request: Record<string, unknown>): Promise<Story> {
  const raw = await api.put<Record<string, unknown>>(`/stories/${storyId}`, request);
  return mapStory(raw);
}

export async function deleteStory(storyId: string): Promise<void> {
  await api.delete(`/stories/${storyId}`);
}

// ─── Task API ────────────────────────────────────────────

export interface CreateTaskPayload {
  title: string;
  description?: string;
  workspace_id?: string | null;
  lifecycle_step_key?: string | null;
  agent_binding?: AgentBinding;
}

export async function createTask(storyId: string, payload: CreateTaskPayload): Promise<Task> {
  const raw = await api.post<Record<string, unknown>>(`/stories/${storyId}/tasks`, payload);
  return mapTask(raw);
}

export interface UpdateTaskPayload {
  title?: string;
  description?: string;
  workspace_id?: string | null;
  lifecycle_step_key?: string | null;
  agent_binding?: AgentBinding;
}

export async function updateTask(taskId: string, payload: UpdateTaskPayload): Promise<Task> {
  const requestPayload = {
    ...payload,
    workspace_id: payload.workspace_id,
  };
  const raw = await api.put<Record<string, unknown>>(`/tasks/${taskId}`, requestPayload);
  return mapTask(raw);
}

export async function fetchTask(taskId: string): Promise<Task> {
  const raw = await api.get<Record<string, unknown>>(`/tasks/${taskId}`);
  return mapTask(raw);
}

export async function startTaskExecution(
  taskId: string,
  payload?: { override_prompt?: string; executor_config?: Record<string, unknown> },
): Promise<Task> {
  await api.post<Record<string, unknown>>(`/tasks/${taskId}/start`, payload ?? {});
  return fetchTask(taskId);
}

export async function continueTaskExecution(
  taskId: string,
  payload?: { additional_prompt?: string; executor_config?: Record<string, unknown> },
): Promise<Task> {
  await api.post<Record<string, unknown>>(`/tasks/${taskId}/continue`, payload ?? {});
  return fetchTask(taskId);
}

export async function cancelTaskExecution(taskId: string): Promise<Task> {
  const raw = await api.post<Record<string, unknown>>(`/tasks/${taskId}/cancel`, {});
  return mapTask(raw);
}

export async function deleteTask(taskId: string): Promise<void> {
  await api.delete(`/tasks/${taskId}`);
}

export async function fetchTasks(storyId: string): Promise<Task[]> {
  const response = await api.get<Record<string, unknown>[]>(`/stories/${storyId}/tasks`);
  return response.map(mapTask);
}

export interface TaskSessionPayload {
  task_id: string;
  workspace_id: string | null;
  session_id: string | null;
  task_status: Task["status"];
  agent_binding: AgentBinding;
  session_title: string | null;
  last_activity: number | null;
  vfs: ExecutionVfs | null;
  runtime_surface: ResolvedVfsSurface | null;
  context_snapshot: SessionContextSnapshot | null;
}

export async function fetchTaskSession(taskId: string): Promise<TaskSessionPayload> {
  const raw = await api.get<Record<string, unknown>>(`/tasks/${taskId}/session`);
  return {
    task_id: requireStringField(raw, "task_id"),
    workspace_id: readNullableStringField(raw, "workspace_id"),
    session_id: readNullableStringField(raw, "session_id"),
    task_status: normalizeTaskStatus(requireStringField(raw, "task_status")),
    agent_binding: mapAgentBinding(raw.agent_binding),
    session_title: readNullableStringField(raw, "session_title"),
    last_activity: raw.last_activity == null ? null : Number(raw.last_activity),
    vfs: (raw.vfs as ExecutionVfs) ?? null,
    runtime_surface: (raw.runtime_surface as ResolvedVfsSurface | undefined) ?? null,
    context_snapshot: (raw.context_snapshot as SessionContextSnapshot) ?? null,
  };
}

// ─── Story Runs API (run-oriented) ───────────────────────

export async function fetchStoryRuns(storyId: string): Promise<StoryRunOverviewDto[]> {
  const response = await api.get<StoryRunsResponse>(`/stories/${storyId}/runs`);
  return response.runs;
}

export async function fetchActiveStoryRun(storyId: string): Promise<StoryRunOverviewDto | null> {
  const response = await api.get<StoryRunOverviewDto | null>(`/stories/${storyId}/runs/active`);
  return response;
}

// ─── Story Session 绑定 API ──────────────────────────────

export async function fetchStorySessionInfo(
  storyId: string,
  bindingId: string,
): Promise<StorySessionInfo> {
  const raw = await api.get<Record<string, unknown>>(`/stories/${storyId}/sessions/${bindingId}`);
  return {
    binding_id: requireStorySessionField(raw, "binding_id"),
    session_id: requireStorySessionField(raw, "session_id"),
    session_title: readNullableStringField(raw, "session_title"),
    last_activity: raw.last_activity == null ? null : Number(raw.last_activity),
    vfs: (raw.vfs as ExecutionVfs) ?? null,
    runtime_surface: (raw.runtime_surface as StorySessionInfo["runtime_surface"]) ?? null,
    context_snapshot: (raw.context_snapshot as StorySessionInfo["context_snapshot"]) ?? null,
  };
}

export async function fetchStorySessions(storyId: string): Promise<StorySessionEntry[]> {
  const response = await api.get<Record<string, unknown>[]>(`/stories/${storyId}/sessions`);
  return response.map(mapStorySessionEntry);
}

export interface CreateStorySessionInput {
  session_id?: string;
  title?: string;
  label?: string;
}

export async function createStorySession(
  storyId: string,
  input: CreateStorySessionInput,
): Promise<StorySessionEntry> {
  const raw = await api.post<Record<string, unknown>>(`/stories/${storyId}/sessions`, input);
  return mapStorySessionEntry(raw);
}

export async function unbindStorySession(storyId: string, bindingId: string): Promise<void> {
  await api.delete(`/stories/${storyId}/sessions/${bindingId}`);
}
