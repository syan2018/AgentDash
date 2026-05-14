/**
 * Agent 列表过滤工具
 *
 * 提供纯函数 filterAgents，用于按搜索关键词过滤 Agent。
 * 抽出为独立模块以便单元测试。
 */

import type { ProjectAgentSummary } from "../../types";

/**
 * 按关键词过滤 Agent 列表。
 *
 * - 空关键词或全空白返回原数组
 * - 不区分大小写、子串匹配
 * - 匹配字段：display_name、description、preset_name、executor.executor、executor.model_id
 */
export function filterAgents(
  agents: ProjectAgentSummary[],
  keyword: string,
): ProjectAgentSummary[] {
  const trimmed = keyword.trim().toLowerCase();
  if (!trimmed) return agents;

  return agents.filter((agent) => {
    const haystacks: Array<string | null | undefined> = [
      agent.display_name,
      agent.description,
      agent.preset_name,
      agent.executor.executor,
      agent.executor.model_id,
    ];
    return haystacks.some(
      (s) => typeof s === "string" && s.toLowerCase().includes(trimmed),
    );
  });
}
