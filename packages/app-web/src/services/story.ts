/**
 * Story / Task service 层。
 *
 * 收口 story 相关的 api.client 调用，响应类型直接受 generated contracts 约束。
 * storyStore 只消费此层导出的函数，不直连 api。
 */

import { api } from "../api/client";
import type {
  TaskDispatchPreference,
  ContextContainerDefinition,
  ContextSourceRef,
  SessionComposition,
  Story,
  Task,
} from "../types";

// ─── Generated contract payload guards ───────────────────

const storyStatusValues = [
  "created",
  "context_ready",
  "decomposed",
  "executing",
  "completed",
  "failed",
  "cancelled",
] satisfies Array<Story["status"]>;
const storyPriorityValues = ["p0", "p1", "p2", "p3"] satisfies Array<Story["priority"]>;
const storyTypeValues = ["feature", "bugfix", "refactor", "docs", "test", "other"] satisfies Array<Story["story_type"]>;
const taskStatusValues = [
  "pending",
  "assigned",
  "running",
  "awaiting_verification",
  "completed",
  "failed",
  "cancelled",
] satisfies Array<Task["status"]>;

const storyStatuses = new Set<string>(storyStatusValues);
const storyPriorities = new Set<string>(storyPriorityValues);
const storyTypes = new Set<string>(storyTypeValues);
const taskStatuses = new Set<string>(taskStatusValues);

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null;

const isStringOrNull = (value: unknown): boolean =>
  typeof value === "string" || value === null;

const hasString = (payload: Record<string, unknown>, key: string): boolean =>
  typeof payload[key] === "string";

const hasStringOrNull = (payload: Record<string, unknown>, key: string): boolean =>
  isStringOrNull(payload[key]);

// ─── 事件 payload → 实体的可映射性判定 ──────────────────

export const canMapStoryFromPayload = (payload: Record<string, unknown>): payload is Story => {
  return (
    hasString(payload, "id") &&
    hasString(payload, "project_id") &&
    hasStringOrNull(payload, "default_workspace_id") &&
    hasString(payload, "title") &&
    hasString(payload, "description") &&
    typeof payload.status === "string" &&
    storyStatuses.has(payload.status) &&
    typeof payload.priority === "string" &&
    storyPriorities.has(payload.priority) &&
    typeof payload.story_type === "string" &&
    storyTypes.has(payload.story_type) &&
    Array.isArray(payload.tags) &&
    typeof payload.task_count === "number" &&
    isRecord(payload.context) &&
    hasString(payload, "created_at") &&
    hasString(payload, "updated_at")
  );
};

export const canMapTaskFromPayload = (payload: Record<string, unknown>): payload is Task => {
  return (
    hasString(payload, "id") &&
    hasString(payload, "project_id") &&
    hasString(payload, "story_id") &&
    hasStringOrNull(payload, "workspace_id") &&
    hasString(payload, "title") &&
    hasString(payload, "description") &&
    typeof payload.status === "string" &&
    taskStatuses.has(payload.status) &&
    isRecord(payload.dispatch_preference) &&
    Array.isArray(payload.artifacts) &&
    hasString(payload, "created_at") &&
    hasString(payload, "updated_at")
  );
};

export const mapStoryFromPayload = (payload: Story): Story => payload;
export const mapTaskFromPayload = (payload: Task): Task => payload;

// ─── Story API ───────────────────────────────────────────

export async function fetchStoriesByProject(projectId: string): Promise<Story[]> {
  return await api.get<Story[]>(`/stories?project_id=${projectId}`);
}

export async function fetchStoryById(storyId: string): Promise<Story> {
  return await api.get<Story>(`/stories/${storyId}`);
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
  return await api.post<Story>("/stories", {
    project_id: projectId,
    title,
    description,
    status: options?.status,
    priority: options?.priority,
    story_type: options?.story_type,
    tags: options?.tags,
  });
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
  return await api.put<Story>(`/stories/${storyId}`, payload);
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
  return await api.put<Story>(`/stories/${storyId}`, request);
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
  return await api.post<Task>(`/stories/${storyId}/tasks`, payload);
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
  return await api.put<Task>(`/tasks/${taskId}`, requestPayload);
}

export async function fetchTask(taskId: string): Promise<Task> {
  return await api.get<Task>(`/tasks/${taskId}`);
}

export async function deleteTask(taskId: string): Promise<void> {
  await api.delete(`/tasks/${taskId}`);
}

export async function fetchTasks(storyId: string): Promise<Task[]> {
  return await api.get<Task[]>(`/stories/${storyId}/tasks`);
}
