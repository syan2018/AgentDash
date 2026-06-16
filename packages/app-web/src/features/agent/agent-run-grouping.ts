/**
 * 基于 list entry 的 subject_ref / subject_label 的 AgentRun 分组。
 *
 * 后端已解析好 subject_label（无需前端 storyStore），这里只按 subject 归组。
 * 无 subject 关联的主 Run 归入兜底组。
 */

import type { AgentRunWorkspaceListEntry } from "../../types";

export interface AgentRunGroup {
  /** 分组稳定 key：`${kind}:${id}` 或兜底组常量。 */
  key: string;
  /** subject kind（story / task / ...）或 "ungrouped"。 */
  kind: string;
  /** 展示标签：subject_label，缺失时回退到 kind / 兜底文案。 */
  label: string;
  entries: AgentRunWorkspaceListEntry[];
}

export const UNGROUPED_KEY = "__ungrouped__";

const GROUP_KIND_LABEL: Record<string, string> = {
  story: "Story",
  task: "Task",
  project: "项目",
};

export function groupKindLabel(kind: string): string {
  return GROUP_KIND_LABEL[kind] ?? kind;
}

export function groupAgentRunsBySubject(entries: AgentRunWorkspaceListEntry[]): AgentRunGroup[] {
  const groups = new Map<string, AgentRunGroup>();
  const ungrouped: AgentRunWorkspaceListEntry[] = [];

  for (const entry of entries) {
    const ref = entry.subject_ref;
    if (!ref) {
      ungrouped.push(entry);
      continue;
    }
    const key = `${ref.kind}:${ref.id}`;
    let group = groups.get(key);
    if (!group) {
      group = {
        key,
        kind: ref.kind,
        label: entry.subject_label?.trim() || groupKindLabel(ref.kind),
        entries: [],
      };
      groups.set(key, group);
    }
    group.entries.push(entry);
  }

  const result = [...groups.values()];
  if (ungrouped.length > 0) {
    result.push({ key: UNGROUPED_KEY, kind: "ungrouped", label: "项目会话", entries: ungrouped });
  }
  return result;
}

/** 是否值得以分组形态渲染（存在多组，或唯一组并非兜底组）。 */
export function hasMeaningfulGroups(groups: AgentRunGroup[]): boolean {
  return groups.length > 1 || (groups.length === 1 && groups[0].key !== UNGROUPED_KEY);
}
