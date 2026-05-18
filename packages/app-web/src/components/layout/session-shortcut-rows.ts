import type { ProjectSessionEntry } from "../../types";

export interface SessionShortcutRow {
  session: ProjectSessionEntry;
  depth: number;
  isCompanion: boolean;
}

function sessionActivity(session: ProjectSessionEntry): number {
  return session.last_activity ?? 0;
}

function sortByActivityDesc(
  sessions: ProjectSessionEntry[],
  latestBySessionId: Map<string, number>,
): ProjectSessionEntry[] {
  return [...sessions].sort((a, b) => {
    const latestDiff =
      (latestBySessionId.get(b.session_id) ?? sessionActivity(b)) -
      (latestBySessionId.get(a.session_id) ?? sessionActivity(a));
    if (latestDiff !== 0) return latestDiff;
    return sessionActivity(b) - sessionActivity(a);
  });
}

export function buildSessionShortcutRows(
  sessions: ProjectSessionEntry[],
): SessionShortcutRow[] {
  if (sessions.length === 0) return [];

  const sessionIds = new Set(sessions.map((session) => session.session_id));
  const byParentId = new Map<string, ProjectSessionEntry[]>();
  const roots: ProjectSessionEntry[] = [];

  for (const session of sessions) {
    const parentId = session.parent_session_id;
    if (parentId && sessionIds.has(parentId)) {
      const children = byParentId.get(parentId) ?? [];
      children.push(session);
      byParentId.set(parentId, children);
    } else {
      roots.push(session);
    }
  }

  const latestBySessionId = new Map<string, number>();
  const computeLatestActivity = (
    session: ProjectSessionEntry,
    visiting: Set<string>,
  ): number => {
    const cached = latestBySessionId.get(session.session_id);
    if (cached != null) return cached;
    if (visiting.has(session.session_id)) return sessionActivity(session);

    visiting.add(session.session_id);
    let latest = sessionActivity(session);
    for (const child of byParentId.get(session.session_id) ?? []) {
      latest = Math.max(latest, computeLatestActivity(child, visiting));
    }
    visiting.delete(session.session_id);
    latestBySessionId.set(session.session_id, latest);
    return latest;
  };

  for (const session of sessions) {
    computeLatestActivity(session, new Set<string>());
  }

  const rows: SessionShortcutRow[] = [];
  const visited = new Set<string>();
  const appendTree = (
    session: ProjectSessionEntry,
    depth: number,
    isCompanion: boolean,
  ) => {
    if (visited.has(session.session_id)) return;
    visited.add(session.session_id);
    rows.push({ session, depth, isCompanion });

    const children = sortByActivityDesc(
      byParentId.get(session.session_id) ?? [],
      latestBySessionId,
    );
    for (const child of children) {
      appendTree(child, depth + 1, true);
    }
  };

  for (const root of sortByActivityDesc(roots, latestBySessionId)) {
    appendTree(root, 0, false);
  }

  // 防御异常数据：如果 parent_session_id 形成环，仍然把未访问的 session 展示出来。
  const unvisited = sessions.filter((session) => !visited.has(session.session_id));
  for (const session of sortByActivityDesc(unvisited, latestBySessionId)) {
    appendTree(session, 0, false);
  }

  return rows;
}
