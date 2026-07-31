import { describe, expect, it } from "vitest";
import { parseContextFrame } from "./contextFrame";

describe("parseContextFrame", () => {
  it("严格解析当前 ContextFrame 核心字段并保留 fragment 顺序", () => {
    const frame = parseContextFrame({
      id: "ctx-1",
      kind: "identity",
      delivery_status: "applied_before_prompt",
      rendered_text: "",
      created_at_ms: 123,
      sections: [
        {
          kind: "context_fragments",
          fragments: [
            {
              slot: "identity",
              label: "first",
              source: "connector",
              content: "first content",
            },
            {
              slot: "identity",
              label: "second",
              source: "product",
              content: "second content",
            },
          ],
        },
      ],
    });

    expect(frame).not.toBeNull();
    expect(frame?.rendered_text).toBe("");
    const section = frame?.sections[0];
    expect(section?.kind).toBe("context_fragments");
    if (section?.kind === "context_fragments") {
      expect(section.fragments.map((fragment) => fragment.label)).toEqual([
        "first",
        "second",
      ]);
    }
  });

  it("直接消费 generated ContextFrame 的 number 时间坐标", () => {
    const frame = parseContextFrame({
      id: "ctx-bigint",
      kind: "environment",
      delivery_status: "applied_before_prompt",
      rendered_text: "workspace",
      created_at_ms: 0,
      sections: [],
    });

    expect(frame?.created_at_ms).toBe(0);
  });

  it("解析 capability delta 的当前 section 集合", () => {
    const frame = parseContextFrame({
      id: "ctx-delta",
      kind: "capability_state_delta",
      delivery_status: "applied_to_compacted_context",
      rendered_text: "Capability updated",
      created_at_ms: 123,
      sections: [
        {
          kind: "mcp_server_delta",
          added_mcp_servers: ["code-analyzer"],
          removed_mcp_servers: [],
          changed_mcp_servers: [],
        },
        {
          kind: "companion_agent_roster_delta",
          added_agents: [{
            agent_key: "reviewer",
            executor: "PI_AGENT",
            display_name: "Review Agent",
          }],
          removed_agent_keys: [],
          changed_agents: [],
          effective_agents: [],
        },
      ],
    });

    expect(frame?.delivery_status).toBe("applied_to_compacted_context");
    expect(frame?.sections.map((section) => section.kind)).toEqual([
      "mcp_server_delta",
      "companion_agent_roster_delta",
    ]);
  });

  it("拒绝旧 delivery status 与未知 section", () => {
    expect(parseContextFrame({
      id: "ctx-old-status",
      kind: "identity",
      delivery_status: "prepared_for_connector",
      rendered_text: "",
      created_at_ms: 123,
      sections: [],
    })).toBeNull();

    expect(parseContextFrame({
      id: "ctx-unknown-section",
      kind: "assignment_context",
      delivery_status: "applied_before_prompt",
      rendered_text: "",
      created_at_ms: 123,
      sections: [{ kind: "future_section" }],
    })).toBeNull();
  });

  it("拒绝缺失 required 字段或包含无效成员的 wire", () => {
    expect(parseContextFrame({
      id: "ctx-missing-sections",
      kind: "identity",
      delivery_status: "applied_before_prompt",
      rendered_text: "",
      created_at_ms: 123,
    })).toBeNull();

    expect(parseContextFrame({
      id: "ctx-missing-delta-array",
      kind: "capability_state_delta",
      delivery_status: "applied_before_prompt",
      rendered_text: "",
      created_at_ms: 123,
      sections: [{
        kind: "capability_key_delta",
        added_capabilities: [],
        removed_capabilities: [],
      }],
    })).toBeNull();

    expect(parseContextFrame({
      id: "ctx-invalid-fragment",
      kind: "identity",
      delivery_status: "applied_before_prompt",
      rendered_text: "",
      created_at_ms: 123,
      sections: [{
        kind: "context_fragments",
        fragments: [{
          slot: "identity",
          source: "product",
          content: "identity",
        }],
      }],
    })).toBeNull();
  });
});
