import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import {
  parseContextFrame,
  type ContextFrame,
  type ToolSchemaDeltaSection,
} from "../model/contextFrame";
import { ContextFrameCard } from "./ContextFrameCard";
import { ToolSchemaDeltaBody } from "./contextFrame/SectionRenderers";

describe("ContextFrameCard", () => {
  it("默认折叠时仅渲染当前协议 header", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleCapabilityFrame())} />,
    );

    expect(markup).toContain("上下文已更新");
    expect(markup).toContain("CAPABILITY");
    expect(markup).not.toContain("Tool Schema");
  });

  it("展开后严格按 sections 原顺序渲染", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard
        frame={readFrame(sampleCapabilityFrame())}
        defaultExpanded
      />,
    );

    const fragments = markup.indexOf("Context Fragments");
    const mcp = markup.indexOf("MCP Servers");
    const tools = markup.indexOf("Tool Schema");
    expect(fragments).toBeGreaterThanOrEqual(0);
    expect(fragments).toBeLessThan(mcp);
    expect(mcp).toBeLessThan(tools);
    expect(markup).toContain("accepted_fragment");
    expect(markup).toContain("mcp_code_analyzer_scan_repo");
    expect(markup).toContain("Agent 实际原文");
    expect(markup).toContain("调试信息");
  });

  it("ToolSchema renderer 展示 structured parameters", () => {
    const section: ToolSchemaDeltaSection = {
      kind: "tool_schema_delta",
      added_tools: [{
        name: "scan_repo",
        description: "扫描仓库",
        parameters_schema: {
          type: "object",
          properties: { root: { type: "string" } },
          required: ["root"],
        },
      }],
      removed_tools: [],
      changed_tools: [],
    };

    const markup = renderToStaticMarkup(
      <ToolSchemaDeltaBody section={section} />,
    );
    expect(markup).toContain("scan_repo");
    expect(markup).toContain("1 params");
  });

  it("rebuild 单帧展示上下文已重建与 compaction 摘要", () => {
    const frame = readFrame({
      id: "compaction-1",
      kind: "compaction_summary",
      delivery_status: "applied_to_compacted_context",
      rendered_text: "压缩后的完整摘要",
      created_at_ms: 100,
      sections: [{
        kind: "compaction_summary",
        title: "摘要",
        summary: "压缩后的完整摘要",
        tokens_before: 1_000,
        messages_compacted: 4,
      }],
    });

    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={frame} defaultExpanded />,
    );
    expect(markup).toContain("上下文已重建");
    expect(markup).toContain("4 messages");
    expect(markup).toContain("压缩后的完整摘要");
  });
});

function readFrame(value: Record<string, unknown>): ContextFrame {
  const frame = parseContextFrame(value);
  if (!frame) throw new Error("invalid ContextFrame fixture");
  return frame;
}

function sampleCapabilityFrame(): Record<string, unknown> {
  return {
    id: "capability-1",
    kind: "capability_state_delta",
    delivery_status: "applied_before_prompt",
    rendered_text: "## Capability State Delta",
    created_at_ms: 1,
    sections: [
      {
        kind: "context_fragments",
        fragments: [{
          slot: "guidance",
          label: "accepted_fragment",
          source: "test",
          content: "accepted content",
        }],
      },
      {
        kind: "mcp_server_delta",
        added_mcp_servers: ["code-analyzer"],
        removed_mcp_servers: [],
        changed_mcp_servers: [],
      },
      {
        kind: "tool_schema_delta",
        added_tools: [{
          name: "mcp_code_analyzer_scan_repo",
          description: "扫描仓库",
          parameters_schema: {
            type: "object",
            properties: { root: { type: "string" } },
          },
          capability_key: "mcp:code-analyzer",
          source: "mcp:code-analyzer",
          tool_path: "mcp:code-analyzer::scan_repo",
        }],
        removed_tools: [],
        changed_tools: [],
      },
    ],
  };
}
