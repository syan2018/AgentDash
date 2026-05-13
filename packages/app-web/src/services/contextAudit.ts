/**
 * Session Context Audit 客户端
 *
 * 对应后端 `GET /sessions/{id}/context/audit`，返回 Bundle / Fragment 的审计时间线。
 * 数据由 `InMemoryContextAuditBus` 按 session 环形缓冲提供，前端用于 Context Inspector
 * 面板渲染。
 */

import { api } from "../api/client";

export type FragmentScopeTag =
  | "runtime_agent"
  | "title_gen"
  | "summarizer"
  | "bridge_replay"
  | "audit";

export const FRAGMENT_SCOPE_TAGS: FragmentScopeTag[] = [
  "runtime_agent",
  "title_gen",
  "summarizer",
  "bridge_replay",
  "audit",
];

export interface ContextAuditEvent {
  event_id: string;
  bundle_id: string;
  /** Session 外部 ID（SessionHub 分配的 `sess-<ms>-<short>`）。 */
  session_id: string;
  /** Bundle 内部追踪 UUID（可能是占位值）。 */
  bundle_session_uuid: string;
  at_ms: number;
  /** trigger 标签：`session_bootstrap` / `composer_rebuild` / `hook:<TriggerName>` / ... */
  trigger: string;
  slot: string;
  label: string;
  source: string;
  order: number;
  scope: FragmentScopeTag[];
  content_preview: string;
  content_hash: number;
  full_content_available: boolean;
}

export interface ContextAuditQueryParams {
  since_ms?: number;
  scope?: FragmentScopeTag;
  slot?: string;
  source_prefix?: string;
}

export async function fetchContextAudit(
  sessionId: string,
  params?: ContextAuditQueryParams,
): Promise<ContextAuditEvent[]> {
  const search = new URLSearchParams();
  if (params?.since_ms != null) search.set("since_ms", String(params.since_ms));
  if (params?.scope) search.set("scope", params.scope);
  if (params?.slot) search.set("slot", params.slot);
  if (params?.source_prefix) search.set("source_prefix", params.source_prefix);
  const suffix = search.toString() ? `?${search.toString()}` : "";
  return api.get<ContextAuditEvent[]>(
    `/sessions/${encodeURIComponent(sessionId)}/context/audit${suffix}`,
  );
}
