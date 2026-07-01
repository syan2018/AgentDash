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
          fragments: [
            {
              slot: "identity",
              label: "identity_system_prompt",
              source: "connector",
              content: "## System Prompt\nbase",
            },
          ],
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

  it("解析 MCP server delta 与 project MCP ToolSchema PromptText", () => {
    const frame = parseContextFrame({
      id: "ctx-mcp-tool-schema",
      kind: "capability_state_delta",
      source: "runtime_context_update",
      phase_node: "bootstrap",
      apply_mode: "initial",
      delivery_status: "queued_for_transform_context",
      delivery_channel: "turn_start",
      message_role: "user",
      rendered_text:
        "## Tool Schema Delta\n\n### `mcp_code_analyzer_scan_repo`\n\ncapability: `mcp:code-analyzer`；source: `mcp:code-analyzer`；path: `mcp:code-analyzer::scan_repo`",
      created_at_ms: 123,
      sections: [
        {
          kind: "mcp_server_delta",
          added_mcp_servers: ["code-analyzer"],
          removed_mcp_servers: [],
          changed_mcp_servers: [],
        },
        {
          kind: "tool_schema_delta",
          added_tools: [
            {
              name: "mcp_code_analyzer_scan_repo",
              description: "扫描仓库结构",
              parameters_schema: {
                type: "object",
                properties: {
                  root: { type: "string", description: "扫描根目录" },
                },
                required: ["root"],
              },
              capability_key: "mcp:code-analyzer",
              source: "mcp:code-analyzer",
              tool_path: "mcp:code-analyzer::scan_repo",
              context_usage_kind: "mcp_tools",
            },
          ],
        },
      ],
    });

    expect(frame?.rendered_text).toContain("mcp_code_analyzer_scan_repo");
    expect(frame?.rendered_text).toContain("mcp:code-analyzer::scan_repo");
    expect(frame?.sections.map((section) => section.kind)).toEqual([
      "mcp_server_delta",
      "tool_schema_delta",
    ]);
    const toolSchema = frame?.sections[1];
    expect(toolSchema?.kind).toBe("tool_schema_delta");
    if (toolSchema?.kind === "tool_schema_delta") {
      expect(toolSchema.added_tools[0]?.source).toBe("mcp:code-analyzer");
      expect(toolSchema.added_tools[0]?.tool_path).toBe("mcp:code-analyzer::scan_repo");
    }
  });
});
