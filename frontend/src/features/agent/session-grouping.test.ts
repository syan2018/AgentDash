import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { ProjectSessionEntry } from "../../types";

import {
  groupSessionsByStory,
  readStoryCollapsed,
  storyFoldStorageKey,
  writeStoryCollapsed,
} from "./session-grouping";

// 测试环境（node）默认没有 localStorage —— 手动装一个内存实现即可。
// 我们既想测 readStoryCollapsed 与 writeStoryCollapsed 的契约，也想断言
// 底层 localStorage 的实际写入，因此装一个真正可读写的 Storage 兼容对象。
if (typeof globalThis.localStorage === "undefined") {
  const store = new Map<string, string>();
  const mock: Storage = {
    get length() {
      return store.size;
    },
    clear: () => store.clear(),
    getItem: (key: string) => (store.has(key) ? (store.get(key) as string) : null),
    key: (index: number) => Array.from(store.keys())[index] ?? null,
    removeItem: (key: string) => {
      store.delete(key);
    },
    setItem: (key: string, value: string) => {
      store.set(key, String(value));
    },
  };
  Object.defineProperty(globalThis, "localStorage", { value: mock, writable: true });
}

function makeSession(overrides: Partial<ProjectSessionEntry> = {}): ProjectSessionEntry {
  return {
    session_id: "s-1",
    session_title: "Session",
    last_activity: 0,
    execution_status: "idle",
    owner_type: "project",
    owner_id: "p-1",
    owner_title: "Project",
    story_id: null,
    story_title: null,
    agent_key: null,
    agent_display_name: null,
    parent_session_id: null,
    ...overrides,
  };
}

describe("groupSessionsByStory", () => {
  it("空输入返回空数组", () => {
    expect(groupSessionsByStory([])).toEqual([]);
  });

  it("只有 project session 时全部作为独立 root", () => {
    const sessions = [
      makeSession({ session_id: "p1" }),
      makeSession({ session_id: "p2" }),
    ];
    const roots = groupSessionsByStory(sessions);
    expect(roots).toHaveLength(2);
    expect(roots.every((r) => r.kind === "project")).toBe(true);
    expect(roots.every((r) => r.children.length === 0)).toBe(true);
  });

  it("Story + Task 组合：Task 正确挂到对应 Story 下", () => {
    const sessions = [
      makeSession({
        session_id: "story-a",
        owner_type: "story",
        owner_id: "S-A",
        owner_title: "Story A",
      }),
      makeSession({
        session_id: "task-a1",
        owner_type: "task",
        owner_id: "T-A1",
        story_id: "S-A",
        story_title: "Story A",
        owner_title: "Task A1",
      }),
      makeSession({
        session_id: "task-a2",
        owner_type: "task",
        owner_id: "T-A2",
        story_id: "S-A",
        story_title: "Story A",
        owner_title: "Task A2",
      }),
    ];
    const roots = groupSessionsByStory(sessions);
    expect(roots).toHaveLength(1);
    expect(roots[0].kind).toBe("story");
    expect(roots[0].session.session_id).toBe("story-a");
    expect(roots[0].children.map((c) => c.session.session_id)).toEqual(["task-a1", "task-a2"]);
    expect(roots[0].children.every((c) => c.kind === "task")).toBe(true);
  });

  it("孤儿 Task（story_id 指向不存在的 Story）降级为独立 root", () => {
    const sessions = [
      makeSession({
        session_id: "task-orphan",
        owner_type: "task",
        owner_id: "T-X",
        story_id: "S-NOT-EXIST",
        owner_title: "Orphan Task",
      }),
    ];
    const roots = groupSessionsByStory(sessions);
    expect(roots).toHaveLength(1);
    expect(roots[0].kind).toBe("orphan");
    expect(roots[0].session.session_id).toBe("task-orphan");
  });

  it("Task 的 story_id 为 null 时降级为独立 root", () => {
    const sessions = [
      makeSession({
        session_id: "task-null-story",
        owner_type: "task",
        owner_id: "T-Y",
        story_id: null,
        owner_title: "Task without story",
      }),
    ];
    const roots = groupSessionsByStory(sessions);
    expect(roots).toHaveLength(1);
    expect(roots[0].kind).toBe("orphan");
  });

  it("Companion session 挂到父 session 的 companions 字段下", () => {
    const sessions = [
      makeSession({
        session_id: "parent",
        owner_type: "story",
        owner_id: "S-A",
      }),
      makeSession({
        session_id: "companion-1",
        owner_type: "story",
        owner_id: "S-A",
        parent_session_id: "parent",
      }),
    ];
    const roots = groupSessionsByStory(sessions);
    expect(roots).toHaveLength(1);
    expect(roots[0].session.session_id).toBe("parent");
    expect(roots[0].companions.map((c) => c.session_id)).toEqual(["companion-1"]);
  });

  it("Companion 的父不在列表中 → 降级为独立 root", () => {
    const sessions = [
      makeSession({
        session_id: "orphan-companion",
        owner_type: "task",
        owner_id: "T-1",
        story_id: "S-NOT-EXIST",
        parent_session_id: "p-missing",
      }),
    ];
    const roots = groupSessionsByStory(sessions);
    expect(roots).toHaveLength(1);
    expect(roots[0].session.session_id).toBe("orphan-companion");
  });

  it("混合：project + story + task + companion + orphan 全部正确分组", () => {
    const sessions = [
      makeSession({ session_id: "proj-1", owner_type: "project", owner_id: "P" }),
      makeSession({ session_id: "story-a", owner_type: "story", owner_id: "S-A" }),
      makeSession({
        session_id: "task-a1",
        owner_type: "task",
        owner_id: "T-1",
        story_id: "S-A",
      }),
      makeSession({
        session_id: "task-a1-companion",
        owner_type: "task",
        owner_id: "T-1",
        story_id: "S-A",
        parent_session_id: "task-a1",
      }),
      makeSession({
        session_id: "orphan-task",
        owner_type: "task",
        owner_id: "T-X",
        story_id: "S-NOT-EXIST",
      }),
    ];
    const roots = groupSessionsByStory(sessions);
    expect(roots.map((r) => `${r.kind}:${r.session.session_id}`)).toEqual([
      "story:story-a",
      "orphan:orphan-task",
      "project:proj-1",
    ]);
    const storyRoot = roots[0];
    expect(storyRoot.children).toHaveLength(1);
    expect(storyRoot.children[0].session.session_id).toBe("task-a1");
    expect(storyRoot.children[0].companions.map((c) => c.session_id)).toEqual([
      "task-a1-companion",
    ]);
  });
});

describe("story fold localStorage", () => {
  beforeEach(() => {
    localStorage.clear();
  });
  afterEach(() => {
    localStorage.clear();
  });

  it("storyFoldStorageKey 包含 project/story id", () => {
    expect(storyFoldStorageKey("proj-1", "story-a")).toBe(
      "agent-page:story-collapsed:proj-1:story-a",
    );
  });

  it("默认未折叠（无记录 → false）", () => {
    expect(readStoryCollapsed("p", "s")).toBe(false);
  });

  it("写入折叠后读取为 true", () => {
    writeStoryCollapsed("p", "s", true);
    expect(readStoryCollapsed("p", "s")).toBe(true);
    expect(localStorage.getItem(storyFoldStorageKey("p", "s"))).toBe("1");
  });

  it("写入展开（false）会清除记录", () => {
    writeStoryCollapsed("p", "s", true);
    writeStoryCollapsed("p", "s", false);
    expect(readStoryCollapsed("p", "s")).toBe(false);
    expect(localStorage.getItem(storyFoldStorageKey("p", "s"))).toBeNull();
  });

  it("不同 project/story 的折叠状态互不干扰", () => {
    writeStoryCollapsed("p1", "s1", true);
    writeStoryCollapsed("p2", "s1", false);
    expect(readStoryCollapsed("p1", "s1")).toBe(true);
    expect(readStoryCollapsed("p2", "s1")).toBe(false);
    expect(readStoryCollapsed("p1", "s2")).toBe(false);
  });
});
