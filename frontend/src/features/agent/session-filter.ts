/**
 * Session 列表过滤工具
 *
 * 提供两个纯函数：
 * - filterSessionsByKeyword：按关键词模糊匹配 title / agent / owner
 * - filterSessionsByStatus：按状态枚举过滤（含组合 group：进行中 / 空闲 / 已结束）
 *
 * 抽出为独立模块以便单元测试。
 */

import type { ProjectSessionEntry } from "../../types";

export type SessionStatusFilter = "all" | "running" | "idle" | "ended";

/**
 * 把 ProjectSessionEntry 的具体 execution_status 归并到筛选 tab 的语义组：
 * - running → "running"
 * - idle    → "idle"
 * - completed / failed / interrupted → "ended"（统称"已结束"）
 */
export function statusGroupOf(
  status: ProjectSessionEntry["execution_status"],
): Exclude<SessionStatusFilter, "all"> {
  switch (status) {
    case "running":
      return "running";
    case "idle":
      return "idle";
    case "completed":
    case "failed":
    case "interrupted":
      return "ended";
  }
}

/**
 * 按关键词过滤。
 *
 * - 空关键词或全空白返回原数组
 * - 不区分大小写、子串匹配
 * - 匹配字段：session_title、agent_display_name、agent_key、owner_title、story_title
 */
export function filterSessionsByKeyword(
  sessions: ProjectSessionEntry[],
  keyword: string,
): ProjectSessionEntry[] {
  const trimmed = keyword.trim().toLowerCase();
  if (!trimmed) return sessions;

  return sessions.filter((s) => {
    const haystacks: Array<string | null | undefined> = [
      s.session_title,
      s.agent_display_name,
      s.agent_key,
      s.owner_title,
      s.story_title,
    ];
    return haystacks.some(
      (h) => typeof h === "string" && h.toLowerCase().includes(trimmed),
    );
  });
}

/**
 * 按状态 tab 过滤。"all" 直接返回原数组。
 */
export function filterSessionsByStatus(
  sessions: ProjectSessionEntry[],
  filter: SessionStatusFilter,
): ProjectSessionEntry[] {
  if (filter === "all") return sessions;
  return sessions.filter((s) => statusGroupOf(s.execution_status) === filter);
}

/**
 * 组合应用两个过滤器（关键词 + 状态）。便于 UI 层一次调用。
 */
export function applySessionFilters(
  sessions: ProjectSessionEntry[],
  keyword: string,
  status: SessionStatusFilter,
): ProjectSessionEntry[] {
  return filterSessionsByStatus(filterSessionsByKeyword(sessions, keyword), status);
}
