import { buildApiPath } from "../api/origin";
import type { SessionBindingOwner } from "../types";

function mapSessionBindingOwner(raw: Record<string, unknown>): SessionBindingOwner {
  return {
    id: String(raw.id ?? ""),
    session_id: String(raw.sessionId ?? raw.session_id ?? ""),
    owner_type: String(raw.ownerType ?? raw.owner_type ?? "story") as SessionBindingOwner["owner_type"],
    owner_id: String(raw.ownerId ?? raw.owner_id ?? ""),
    label: String(raw.label ?? ""),
    created_at: String(raw.createdAt ?? raw.created_at ?? new Date().toISOString()),
    owner_title:
      raw.ownerTitle != null || raw.owner_title != null
        ? String(raw.ownerTitle ?? raw.owner_title)
        : null,
    story_id:
      raw.storyId != null || raw.story_id != null
        ? String(raw.storyId ?? raw.story_id)
        : null,
    task_id:
      raw.taskId != null || raw.task_id != null
        ? String(raw.taskId ?? raw.task_id)
        : null,
  };
}

export interface SessionMeta {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
}

export interface FetchSessionsOptions {
  excludeBound?: boolean;
}

export async function fetchSessions(options?: FetchSessionsOptions): Promise<SessionMeta[]> {
  const params = new URLSearchParams();
  if (options?.excludeBound) {
    params.set("exclude_bound", "true");
  }
  const query = params.toString();
  const url = query ? `${buildApiPath("/sessions")}?${query}` : buildApiPath("/sessions");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`获取会话列表失败: HTTP ${res.status}`);
  return res.json();
}

export async function createSession(title?: string): Promise<SessionMeta> {
  const res = await fetch(buildApiPath("/sessions"), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ title }),
  });
  if (!res.ok) throw new Error(`创建会话失败: HTTP ${res.status}`);
  return res.json();
}

export async function fetchSessionMeta(id: string): Promise<SessionMeta> {
  const res = await fetch(buildApiPath(`/sessions/${encodeURIComponent(id)}`));
  if (!res.ok) throw new Error(`获取会话详情失败: HTTP ${res.status}`);
  return res.json();
}

export async function deleteSession(id: string): Promise<void> {
  const res = await fetch(buildApiPath(`/sessions/${encodeURIComponent(id)}`), {
    method: "DELETE",
  });
  if (!res.ok) throw new Error(`删除会话失败: HTTP ${res.status}`);
}

export async function fetchSessionBindings(id: string): Promise<SessionBindingOwner[]> {
  const res = await fetch(buildApiPath(`/sessions/${encodeURIComponent(id)}/bindings`));
  if (!res.ok) throw new Error(`获取会话绑定失败: HTTP ${res.status}`);
  const raw = await res.json() as Record<string, unknown>[];
  return raw.map(mapSessionBindingOwner);
}
