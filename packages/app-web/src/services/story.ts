/**
 * Story / Task service 层。
 *
 * 收口 story 相关的 api.client 调用与后端 JSON ↔ 前端类型的 mapper。
 * storyStore 只消费此层导出的函数，不直连 api。
 */

import { api } from "../api/client";
import { requireStringField } from "../api/mappers";
import type {
  TaskDispatchPreference,
  ContextContainerDefinition,
  ContextSourceRef,
  SessionComposition,
  Story,
  StoryContext,
  Task,
} from "../types";
import { isThinkingLevel } from "../types";

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
    case "cancelled":
      return "cancelled";
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

const mapDispatchPreference = (raw: unknown): TaskDispatchPreference => {
  if (!raw || typeof raw !== "object") {
    throw new Error("Task 缺少 dispatch_preference");
  }

  const pref = raw as Record<string, unknown>;
  return {
    agent_type: pref.agent_type ? String(pref.agent_type) : null,
    agent_pid: pref.agent_pid ? String(pref.agent_pid) : null,
    preset_name: pref.preset_name ? String(pref.preset_name) : null,
    prompt_template: pref.prompt_template ? String(pref.prompt_template) : null,
    initial_context: pref.initial_context ? String(pref.initial_context) : null,
    thinking_level:
      pref.thinking_level == null
        ? null
        : isThinkingLevel(pref.thinking_level)
          ? pref.thinking_level
          : (() => {
              throw new Error(`未知 thinking_level: ${String(pref.thinking_level)}`);
            })(),
    context_sources: Array.isArray(pref.context_sources)
      ? (pref.context_sources as ContextSourceRef[])
      : (() => {
          throw new Error("dispatch_preference.context_sources 必须是数组");
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
    title: requireStringField(raw, "title"),
    description: raw.description ? String(raw.description) : "",
    status: normalizeTaskStatus(requireStringField(raw, "status")),
    dispatch_preference: mapDispatchPreference(raw.dispatch_preference),
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
  dispatch_preference?: TaskDispatchPreference;
}

export async function createTask(storyId: string, payload: CreateTaskPayload): Promise<Task> {
  const raw = await api.post<Record<string, unknown>>(`/stories/${storyId}/tasks`, payload);
  return mapTask(raw);
}

export interface UpdateTaskPayload {
  title?: string;
  description?: string;
  workspace_id?: string | null;
  dispatch_preference?: TaskDispatchPreference;
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

export async function deleteTask(taskId: string): Promise<void> {
  await api.delete(`/tasks/${taskId}`);
}

export async function fetchTasks(storyId: string): Promise<Task[]> {
  const response = await api.get<Record<string, unknown>[]>(`/stories/${storyId}/tasks`);
  return response.map(mapTask);
}
