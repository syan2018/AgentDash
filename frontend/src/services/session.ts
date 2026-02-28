import { buildApiPath } from "../api/origin";

export interface SessionMeta {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
}

export async function fetchSessions(): Promise<SessionMeta[]> {
  const res = await fetch(buildApiPath("/sessions"));
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
