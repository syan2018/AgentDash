import { describe, expect, it } from "vitest";
import type { ProjectSessionEntry } from "../../types";

import {
  applySessionFilters,
  filterSessionsByKeyword,
  filterSessionsByStatus,
  statusGroupOf,
} from "./session-filter";

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

describe("statusGroupOf", () => {
  it("running → running", () => {
    expect(statusGroupOf("running")).toBe("running");
  });
  it("idle → idle", () => {
    expect(statusGroupOf("idle")).toBe("idle");
  });
  it("completed / failed / interrupted → ended", () => {
    expect(statusGroupOf("completed")).toBe("ended");
    expect(statusGroupOf("failed")).toBe("ended");
    expect(statusGroupOf("interrupted")).toBe("ended");
  });
});

describe("filterSessionsByKeyword", () => {
  const sessions: ProjectSessionEntry[] = [
    makeSession({ session_id: "s1", session_title: "审查 PR", agent_display_name: "Claude" }),
    makeSession({ session_id: "s2", session_title: "实现登录", agent_display_name: "Codex" }),
    makeSession({ session_id: "s3", session_title: "Hello World", owner_title: "示例任务" }),
  ];

  it("空关键词返回原数组", () => {
    expect(filterSessionsByKeyword(sessions, "")).toBe(sessions);
    expect(filterSessionsByKeyword(sessions, "   ")).toBe(sessions);
  });

  it("按 session_title 匹配", () => {
    expect(filterSessionsByKeyword(sessions, "登录").map((s) => s.session_id)).toEqual(["s2"]);
  });

  it("不区分大小写", () => {
    expect(filterSessionsByKeyword(sessions, "claude").map((s) => s.session_id)).toEqual(["s1"]);
    expect(filterSessionsByKeyword(sessions, "HELLO").map((s) => s.session_id)).toEqual(["s3"]);
  });

  it("按 owner_title 匹配", () => {
    expect(filterSessionsByKeyword(sessions, "示例").map((s) => s.session_id)).toEqual(["s3"]);
  });

  it("无匹配返回空数组", () => {
    expect(filterSessionsByKeyword(sessions, "不存在的关键词")).toEqual([]);
  });
});

describe("filterSessionsByStatus", () => {
  const sessions: ProjectSessionEntry[] = [
    makeSession({ session_id: "r", execution_status: "running" }),
    makeSession({ session_id: "i", execution_status: "idle" }),
    makeSession({ session_id: "c", execution_status: "completed" }),
    makeSession({ session_id: "f", execution_status: "failed" }),
    makeSession({ session_id: "x", execution_status: "interrupted" }),
  ];

  it("all 返回全部", () => {
    expect(filterSessionsByStatus(sessions, "all")).toBe(sessions);
  });

  it("running 只保留 running", () => {
    expect(filterSessionsByStatus(sessions, "running").map((s) => s.session_id)).toEqual(["r"]);
  });

  it("idle 只保留 idle", () => {
    expect(filterSessionsByStatus(sessions, "idle").map((s) => s.session_id)).toEqual(["i"]);
  });

  it("ended 包含 completed / failed / interrupted", () => {
    expect(filterSessionsByStatus(sessions, "ended").map((s) => s.session_id)).toEqual([
      "c",
      "f",
      "x",
    ]);
  });
});

describe("applySessionFilters", () => {
  const sessions: ProjectSessionEntry[] = [
    makeSession({
      session_id: "a",
      session_title: "前端重构",
      execution_status: "running",
    }),
    makeSession({
      session_id: "b",
      session_title: "前端测试",
      execution_status: "idle",
    }),
    makeSession({
      session_id: "c",
      session_title: "后端日志",
      execution_status: "running",
    }),
  ];

  it("关键词 + 状态叠加", () => {
    expect(
      applySessionFilters(sessions, "前端", "running").map((s) => s.session_id),
    ).toEqual(["a"]);
  });

  it("空关键词只过滤状态", () => {
    expect(applySessionFilters(sessions, "", "running").map((s) => s.session_id)).toEqual([
      "a",
      "c",
    ]);
  });

  it("all 状态只过滤关键词", () => {
    expect(applySessionFilters(sessions, "前端", "all").map((s) => s.session_id)).toEqual([
      "a",
      "b",
    ]);
  });
});
