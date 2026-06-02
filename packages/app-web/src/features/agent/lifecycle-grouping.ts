/**
 * 基于 lifecycle subject_associations 的会话分组。
 *
 * 将扁平的 session entries 按 subject 关联组织为 Story → Task 层级。
 * 分组标签使用 story/task 的真实标题（从 storyStore 解析），
 * 绝不向用户暴露 GUID。
 */

import type { LifecycleRunView, LifecycleAgentView } from "../../types";
import type { SessionExecutionStatusValue } from "../../services/session";
import { findStoryById, useStoryStore } from "../../stores/storyStore";

export interface SessionEntry {
  run: LifecycleRunView;
  agent: LifecycleAgentView;
  sessionTitle: string | null;
  deliveryRuntimeSessionId: string | null;
  executionStatus: SessionExecutionStatusValue;
}

export type SessionGroupKind = "story" | "task" | "project";

export interface SessionGroup {
  kind: SessionGroupKind;
  subjectId: string;
  label: string;
  entries: SessionEntry[];
  children: SessionGroup[];
}

/**
 * 从 storyStore 解析 subject 的可读标签。
 * Story/Task 都有 title 字段，优先使用；找不到则返回 null（不显示 GUID）。
 */
function resolveSubjectLabel(kind: string, id: string): string | null {
  const state = useStoryStore.getState();

  if (kind === "story") {
    const story = findStoryById(state.storiesByProjectId, id);
    return story?.title ?? null;
  }

  if (kind === "task") {
    for (const tasks of Object.values(state.tasksByStoryId)) {
      const task = tasks.find((t) => t.id === id);
      if (task) return task.title;
    }
    return null;
  }

  return null;
}

/**
 * 将扁平 entries 按 subject_associations 分组为层级结构。
 *
 * 分组策略：
 * - subject_kind=story → Story 组（标签为 story title）
 * - subject_kind=task → Task 组（标签为 task title）
 * - 无 subject → project 独立组
 */
export function groupSessionsBySubject(entries: SessionEntry[]): SessionGroup[] {
  const storyGroups = new Map<string, SessionGroup>();
  const taskGroups = new Map<string, SessionGroup>();
  const projectEntries: SessionEntry[] = [];

  for (const entry of entries) {
    const subjects = entry.run.subject_associations;
    if (subjects.length === 0) {
      projectEntries.push(entry);
      continue;
    }

    let placed = false;
    for (const sa of subjects) {
      const { kind, id } = sa.subject_ref;
      if (kind === "story") {
        let group = storyGroups.get(id);
        if (!group) {
          const title = resolveSubjectLabel("story", id);
          group = {
            kind: "story",
            subjectId: id,
            label: title || "Story",
            entries: [],
            children: [],
          };
          storyGroups.set(id, group);
        }
        group.entries.push(entry);
        placed = true;
        break;
      }
      if (kind === "task") {
        let group = taskGroups.get(id);
        if (!group) {
          const title = resolveSubjectLabel("task", id);
          group = {
            kind: "task",
            subjectId: id,
            label: title || "任务",
            entries: [],
            children: [],
          };
          taskGroups.set(id, group);
        }
        group.entries.push(entry);
        placed = true;
        break;
      }
    }
    if (!placed) {
      projectEntries.push(entry);
    }
  }

  const result: SessionGroup[] = [];
  for (const group of storyGroups.values()) {
    result.push(group);
  }
  for (const group of taskGroups.values()) {
    result.push(group);
  }
  if (projectEntries.length > 0) {
    result.push({
      kind: "project",
      subjectId: "__project__",
      label: "项目级会话",
      entries: projectEntries,
      children: [],
    });
  }

  return result;
}
