/**
 * Session 分组工具
 *
 * 将扁平的 ProjectSessionEntry 列表按以下层级组织：
 *
 *   Root（story / task-orphan / project）
 *     └── Child Task（仅 story root 下挂属于该 story 的 task session）
 *         └── Companion（由 parent_session_id 表达的父子会话，与 Story→Task 嵌套独立）
 *
 * 规则：
 * - owner_type === "story" 的 session → Story root，task session 按 story_id 挂到下方
 * - owner_type === "task" 的 session：
 *   - 若有匹配的 Story root → 作为该 Story 的 child
 *   - 若 story_id 为 null 或指向不存在的 Story root → 降级为独立 root（孤儿）
 * - owner_type === "project" 的 session → 独立 root
 * - parent_session_id 不为 null 的 session → 作为所指 session 的 companion 挂在其下；
 *   若父 session 不在列表中，则作为独立 root 降级展示
 *
 * Companion 与 Story→Task 嵌套共存：先按 parent_session_id 抽出 companions，
 * 剩余 session 再按 owner_type/story_id 做 Story→Task 分组。
 */

import type { ProjectSessionEntry } from "../../types";

// ─── 数据结构 ─────────────────────────────────────────────────────────────

export type SessionGroupNodeKind = "story" | "task" | "orphan" | "project";

/**
 * 分组后的一个节点。
 *
 * - kind=story：root，children 可能含 task 节点
 * - kind=task：某个 Story root 下的 child
 * - kind=orphan：owner_type=task 但找不到 Story root，降级为独立 root
 * - kind=project：owner_type=project 的独立 root
 *
 * companions 总是指向当前 session 的 parent_session_id-子会话。
 */
export interface SessionGroupNode {
  kind: SessionGroupNodeKind;
  session: ProjectSessionEntry;
  /** 仅 kind=story 时才会有内容（对应 task session） */
  children: SessionGroupNode[];
  /** 与 parent_session_id 语义一致的 companion 子会话 */
  companions: ProjectSessionEntry[];
}

// ─── 核心分组函数 ─────────────────────────────────────────────────────────

/**
 * 将扁平 sessions 分组为 Story→Task 树。
 *
 * 稳定性：输入顺序决定输出顺序（对 root、对 task children、对 companions 均保持输入顺序）。
 */
export function groupSessionsByStory(sessions: ProjectSessionEntry[]): SessionGroupNode[] {
  if (sessions.length === 0) return [];

  // Pass 1：按 parent_session_id 把 companions 归到父 session 名下
  const companionsByParent = new Map<string, ProjectSessionEntry[]>();
  const sessionIds = new Set(sessions.map((s) => s.session_id));
  const nonCompanions: ProjectSessionEntry[] = [];
  const orphanCompanions: ProjectSessionEntry[] = [];

  for (const s of sessions) {
    if (s.parent_session_id && sessionIds.has(s.parent_session_id)) {
      const arr = companionsByParent.get(s.parent_session_id) ?? [];
      arr.push(s);
      companionsByParent.set(s.parent_session_id, arr);
    } else if (s.parent_session_id) {
      // 父 session 不在当前列表里，降级为独立节点
      orphanCompanions.push(s);
    } else {
      nonCompanions.push(s);
    }
  }

  const companionsOf = (sessionId: string) => companionsByParent.get(sessionId) ?? [];

  // Pass 2：在 nonCompanions 中拆出 Story root 与 Task
  const storyRoots: SessionGroupNode[] = [];
  const storyIdToNode = new Map<string, SessionGroupNode>();
  const taskSessions: ProjectSessionEntry[] = [];
  const projectSessions: ProjectSessionEntry[] = [];

  for (const s of nonCompanions) {
    if (s.owner_type === "story") {
      const node: SessionGroupNode = {
        kind: "story",
        session: s,
        children: [],
        companions: companionsOf(s.session_id),
      };
      storyRoots.push(node);
      storyIdToNode.set(s.owner_id, node);
    } else if (s.owner_type === "task") {
      taskSessions.push(s);
    } else {
      projectSessions.push(s);
    }
  }

  // Pass 3：把 task session 挂到对应 Story root 下；找不到归属的降级为 orphan root
  const orphanRoots: SessionGroupNode[] = [];
  for (const t of taskSessions) {
    const storyNode = t.story_id ? storyIdToNode.get(t.story_id) : undefined;
    if (storyNode) {
      storyNode.children.push({
        kind: "task",
        session: t,
        children: [],
        companions: companionsOf(t.session_id),
      });
    } else {
      orphanRoots.push({
        kind: "orphan",
        session: t,
        children: [],
        companions: companionsOf(t.session_id),
      });
    }
  }

  // Pass 4：project session 独立 root
  const projectRoots: SessionGroupNode[] = projectSessions.map((s) => ({
    kind: "project",
    session: s,
    children: [],
    companions: companionsOf(s.session_id),
  }));

  // Pass 5：孤儿 companion 降级独立 root（kind 按 owner_type 归类）
  const orphanCompanionRoots: SessionGroupNode[] = orphanCompanions.map((s) => ({
    kind:
      s.owner_type === "story"
        ? "story"
        : s.owner_type === "task"
          ? "orphan"
          : "project",
    session: s,
    children: [],
    companions: companionsOf(s.session_id),
  }));

  // 合并输出：保持 session 输入相对顺序（按类型简单拼接即可，单类型内部保持输入顺序）
  return [...storyRoots, ...orphanRoots, ...projectRoots, ...orphanCompanionRoots];
}

// ─── 折叠状态持久化 ──────────────────────────────────────────────────────

/** localStorage key 规则：折叠记一条，展开即移除 */
export function storyFoldStorageKey(projectId: string, storyId: string): string {
  return `agent-page:story-collapsed:${projectId}:${storyId}`;
}

/**
 * 读取某个 Story 是否已折叠。默认（无记录）返回 false（即展开）。
 *
 * 与 localStorage 交互失败（privacy mode / quota）时做静默降级。
 */
export function readStoryCollapsed(projectId: string, storyId: string): boolean {
  try {
    return localStorage.getItem(storyFoldStorageKey(projectId, storyId)) === "1";
  } catch {
    return false;
  }
}

/** 写入折叠状态：collapsed=true 记 "1"，false 删除记录。*/
export function writeStoryCollapsed(
  projectId: string,
  storyId: string,
  collapsed: boolean,
): void {
  const key = storyFoldStorageKey(projectId, storyId);
  try {
    if (collapsed) {
      localStorage.setItem(key, "1");
    } else {
      localStorage.removeItem(key);
    }
  } catch {
    // 忽略持久化失败
  }
}
