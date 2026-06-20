import { describe, expect, it } from "vitest";
import { parseContextFrame } from "./contextFrame";

describe("parseContextFrame", () => {
  it("允许 rendered_text 为空但保留结构化 sections", () => {
    const frame = parseContextFrame({
      id: "ctx-1",
      kind: "identity",
      source: "runtime",
      delivery_status: "delivered",
      delivery_channel: "system",
      message_role: "system",
      rendered_text: "",
      created_at_ms: 123,
      sections: [
        {
          kind: "identity",
          title: "Identity",
          summary: "当前身份",
          base_prompt: "base",
          mode: "append",
          effective_prompt: "effective",
        },
      ],
    });

    expect(frame).not.toBeNull();
    expect(frame?.rendered_text).toBe("");
    expect(frame?.sections).toHaveLength(1);
  });

  it("解析后端新增的 guidelines 与 companion section", () => {
    const frame = parseContextFrame({
      id: "ctx-2",
      kind: "capability_state_snapshot",
      source: "runtime_context_update",
      delivery_status: "queued_for_transform_context",
      delivery_channel: "turn_start",
      message_role: "user",
      rendered_text: "## Companion Agent Roster Delta",
      created_at_ms: 123,
      sections: [
        {
          kind: "companion_agent_roster_delta",
          added_agents: [
            {
              agent_key: "reviewer",
              executor: "PI_AGENT",
              display_name: "Review Agent",
              context_usage_kind: "agents",
            },
          ],
          removed_agent_keys: ["legacy-reviewer"],
          changed_agents: [],
          effective_agents: [
            {
              agent_key: "reviewer",
              executor: "PI_AGENT",
              display_name: "Review Agent",
            },
          ],
        },
        {
          kind: "user_preferences",
          title: "User Preferences",
          summary: "用户级偏好设置。",
          items: ["使用中文"],
        },
        {
          kind: "project_guidelines",
          title: "Project Guidelines",
          summary: "工作区中发现的项目级指引文件。",
          entries: [
            {
              path: "AGENTS.md",
              content: "项目约定",
            },
          ],
        },
      ],
    });

    expect(frame?.sections.map((section) => section.kind)).toEqual([
      "companion_agent_roster_delta",
      "user_preferences",
      "project_guidelines",
    ]);
  });

  it("保留未知 section 以便诊断协议漂移", () => {
    const frame = parseContextFrame({
      id: "ctx-3",
      kind: "assignment_context",
      source: "runtime_context_update",
      delivery_status: "queued_for_transform_context",
      delivery_channel: "turn_start",
      message_role: "user",
      rendered_text: "",
      created_at_ms: 123,
      sections: [
        {
          kind: "future_section",
          payload: { value: 1 },
        },
      ],
    });

    expect(frame?.sections).toEqual([
      {
        kind: "unknown_section",
        original_kind: "future_section",
        raw: {
          kind: "future_section",
          payload: { value: 1 },
        },
      },
    ]);
  });
});
