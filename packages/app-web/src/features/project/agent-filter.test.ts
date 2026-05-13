import { describe, expect, it } from "vitest";
import type { ProjectAgentSummary } from "../../types";

import { filterAgents } from "./agent-filter";

function makeAgent(overrides: Partial<ProjectAgentSummary> = {}): ProjectAgentSummary {
  return {
    key: "agent-1",
    display_name: "Backend Coder",
    description: "Rust 后端开发助手",
    executor: {
      executor: "claude_code",
      model_id: "claude-opus-4",
      provider_id: null,
      agent_id: null,
      thinking_level: null,
      permission_policy: null,
    },
    preset_name: "backend",
    source: "preset",
    session: null,
    ...overrides,
  };
}

describe("filterAgents", () => {
  const agents: ProjectAgentSummary[] = [
    makeAgent({ key: "a", display_name: "Backend Coder", description: "Rust 后端", preset_name: "backend-coder" }),
    makeAgent({ key: "b", display_name: "Frontend UI", description: "React 前端组件", preset_name: "frontend" }),
    makeAgent({
      key: "c",
      display_name: "Reviewer",
      description: "代码审查",
      preset_name: "reviewer",
      executor: {
        executor: "iflow",
        model_id: "qwen-max",
        provider_id: null,
        agent_id: null,
        thinking_level: null,
        permission_policy: null,
      },
    }),
  ];

  it("空关键词返回全部", () => {
    expect(filterAgents(agents, "")).toEqual(agents);
    expect(filterAgents(agents, "   ")).toEqual(agents);
  });

  it("按 display_name 子串匹配（不区分大小写）", () => {
    const result = filterAgents(agents, "backend");
    expect(result.map((a) => a.key)).toEqual(["a"]);
    const result2 = filterAgents(agents, "FRONT");
    expect(result2.map((a) => a.key)).toEqual(["b"]);
  });

  it("按 description 中文匹配", () => {
    const result = filterAgents(agents, "审查");
    expect(result.map((a) => a.key)).toEqual(["c"]);
  });

  it("按 executor 名称匹配", () => {
    const result = filterAgents(agents, "iflow");
    expect(result.map((a) => a.key)).toEqual(["c"]);
  });

  it("按 model_id 匹配", () => {
    const result = filterAgents(agents, "qwen");
    expect(result.map((a) => a.key)).toEqual(["c"]);
  });

  it("按 preset_name 匹配", () => {
    const result = filterAgents(agents, "frontend");
    expect(result.map((a) => a.key).sort()).toEqual(["b"]);
  });

  it("无匹配返回空数组", () => {
    expect(filterAgents(agents, "不存在的关键词xyz")).toEqual([]);
  });
});
