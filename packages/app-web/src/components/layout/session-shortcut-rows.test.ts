import { describe, expect, it } from "vitest";
import type { ProjectSessionEntry } from "../../types";
import { buildSessionShortcutRows } from "./session-shortcut-rows";

function makeSession(overrides: Partial<ProjectSessionEntry> = {}): ProjectSessionEntry {
  return {
    session_id: "s-1",
    session_title: "Session",
    last_activity: 0,
    execution_status: "idle",
    owner_id: "p-1",
    owner_title: "Project",
    story_id: null,
    story_title: null,
    agent_key: null,
    agent_display_name: null,
    parent_session_id: null,
    parent_relation_kind: null,
    ...overrides,
  };
}

describe("buildSessionShortcutRows", () => {
  it("按最近活跃时间排序根 session", () => {
    const rows = buildSessionShortcutRows([
      makeSession({ session_id: "old", last_activity: 1 }),
      makeSession({ session_id: "new", last_activity: 3 }),
      makeSession({ session_id: "mid", last_activity: 2 }),
    ]);

    expect(rows.map((row) => row.session.session_id)).toEqual(["new", "mid", "old"]);
    expect(rows.every((row) => row.depth === 0)).toBe(true);
  });

  it("把 relation child 紧跟在父 session 后并保留 relation kind", () => {
    const rows = buildSessionShortcutRows([
      makeSession({ session_id: "other", last_activity: 20 }),
      makeSession({ session_id: "parent", last_activity: 10 }),
      makeSession({
        session_id: "child",
        last_activity: 30,
        parent_session_id: "parent",
        parent_relation_kind: "fork",
      }),
    ]);

    expect(rows.map((row) => row.session.session_id)).toEqual([
      "parent",
      "child",
      "other",
    ]);
    expect(rows[1]).toMatchObject({ depth: 1, parentRelationKind: "fork" });
  });

  it("父 session 缺失时将 relation child 当作根行展示", () => {
    const rows = buildSessionShortcutRows([
      makeSession({
        session_id: "orphan-child",
        parent_session_id: "missing",
      }),
    ]);

    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ depth: 0, parentRelationKind: null });
  });

  it("异常循环数据不会导致无限递归", () => {
    const rows = buildSessionShortcutRows([
      makeSession({ session_id: "a", parent_session_id: "b" }),
      makeSession({ session_id: "b", parent_session_id: "a" }),
    ]);

    expect(rows.map((row) => row.session.session_id).sort()).toEqual(["a", "b"]);
  });
});
