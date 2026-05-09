import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { AggregatedContextFrameGroup } from "../model/types";
import { SessionEntry } from "./SessionEntry";

describe("SessionEntry ContextFrame 聚合", () => {
  it("把多个 context_frame 渲染为一张批量更新卡片", () => {
    const group: AggregatedContextFrameGroup = {
      type: "aggregated_context_frames",
      id: "ctx-1",
      groupKey: "context-frame-ctx-1",
      entries: [
        contextFrameEntry("ctx-1", "capability_state_update"),
        contextFrameEntry("ctx-2", "mission_context"),
      ],
    };

    const html = renderToStaticMarkup(<SessionEntry item={group} />);

    expect(html).toContain("Agent 上下文批量更新");
    // header 汇总 "N 帧 · 最后阶段 X"，覆盖原 "N 个 frame"
    expect(html).toContain("2 帧");
    expect(html).toContain("最后阶段 apply");
    expect(html).not.toContain("已注入动态上下文");
  });
});

function contextFrameEntry(id: string, kind: string): AggregatedContextFrameGroup["entries"][number] {
  return {
    id,
    sessionId: "session-1",
    timestamp: 1,
    eventSeq: id === "ctx-1" ? 1 : 2,
    event: {
      type: "platform",
      payload: {
        kind: "session_meta_update",
        data: {
          key: "context_frame",
          value: {
            id,
            kind,
            source: "runtime_context_update",
            phase_node: "apply",
            apply_mode: "live",
            delivery_status: "queued_for_transform_context",
            delivery_channel: "turn_start",
            message_role: "user",
            rendered_text: "## Capability Update",
            created_at_ms: 1,
            sections: [
              {
                kind: "capability_delta",
                added_capabilities: [],
                removed_capabilities: [],
                effective_capabilities: ["workflow_management"],
                blocked_tool_paths: [],
                unblocked_tool_paths: [],
                whitelisted_tool_paths: [],
                removed_whitelist_paths: [],
                added_mcp_servers: [],
                removed_mcp_servers: [],
                changed_mcp_servers: [],
                vfs_mounts_added: [],
                vfs_mounts_removed: [],
              },
            ],
          },
        },
      },
    },
  };
}
