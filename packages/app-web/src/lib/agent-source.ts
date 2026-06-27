/**
 * AgentSource —— 后端 `source` 字段（标准化枚举 slug）的前端展示映射。
 *
 * 后端 slug 形如 `project_agent` / `workflow_activity`，下划线命名不适合直出给用户；
 * 这里集中维护「来源 → 人类可读短标签」的映射，列表行 / workspace 身份栏等共用。
 */

const SOURCE_LABELS: Record<string, string> = {
  project_agent: "Project",
  routine: "Routine",
  subagent: "Subagent",
  workflow_agent: "Workflow",
};

/**
 * 把后端 `source` slug 映射为展示标签。
 * `unknown` / 空值返回 null（调用方据此决定不渲染标签）。
 */
export function agentSourceLabel(source: string | null | undefined): string | null {
  const slug = source?.trim();
  if (!slug || slug === "unknown") return null;
  return SOURCE_LABELS[slug] ?? slug;
}
