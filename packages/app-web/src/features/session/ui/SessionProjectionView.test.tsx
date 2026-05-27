import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { SessionProjectionViewResponse } from "../../../generated/session-contracts";
import { SessionProjectionViewPanel } from "./SessionProjectionView";

describe("SessionProjectionViewPanel", () => {
  it("渲染模型投影版本、压缩范围和 synthetic segment", () => {
    const markup = renderToStaticMarkup(
      <SessionProjectionViewPanel projection={sampleProjection()} />,
    );

    expect(markup).toContain("CONTEXT");
    expect(markup).toContain("v2");
    expect(markup).toContain("head #42");
    expect(markup).toContain("summary_chunk");
    expect(markup).toContain("#1-#30");
    expect(markup).toContain("synthetic");
    expect(markup).toContain("压缩后的历史摘要");
    expect(markup).toContain("System / Developer");
    expect(markup).toContain("工具调用");
  });
});

function sampleProjection(): SessionProjectionViewResponse {
  return {
    session_id: "sess-1",
    projection_kind: "model_context",
    projection_version: 2,
    head_event_seq: 42,
    active_compaction_id: "compaction-1",
    token_estimate: 128,
    message_count: 2,
    context_usage: {
      categories: [
        {
          kind: "system_developer",
          label: "System / Developer",
          token_estimate: 0,
          source: "not_loaded",
          deferred: true,
        },
        {
          kind: "messages",
          label: "Messages",
          token_estimate: 32,
          source: "local_estimate",
          deferred: false,
        },
        {
          kind: "compaction_summary",
          label: "Compaction Summary",
          token_estimate: 96,
          source: "projected",
          deferred: false,
        },
      ],
      messages: {
        user_message_tokens: 32,
        assistant_message_tokens: 0,
        tool_call_tokens: 0,
        tool_result_tokens: 0,
        attachment_tokens: 0,
      },
      top_tools: [],
      top_attachments: [],
    },
    segments: [
      {
        id: "segment-1",
        sort_order: 0,
        segment_type: "summary_chunk",
        role: "compaction_summary",
        origin: "projection",
        synthetic: true,
        projection_kind: "compaction_summary",
        message_ref: {
          turn_id: "_projection:segment-1",
          entry_index: 0,
        },
        source_range: {
          start_event_seq: 1,
          end_event_seq: 30,
        },
        projection_segment_id: "segment-1",
        preview: "压缩后的历史摘要",
        token_estimate: 96,
        tool_names: [],
        provenance: {
          compaction_id: "compaction-1",
          projection_version: 2,
          segment_type: "summary_chunk",
          strategy: "summary_prefix",
          trigger: "auto",
          phase: "pre_provider",
        },
      },
      {
        id: "original_event:1",
        sort_order: 1,
        segment_type: "original_event",
        role: "user",
        origin: "event",
        synthetic: false,
        projection_kind: "model_context",
        message_ref: {
          turn_id: "turn-9",
          entry_index: 0,
        },
        source_event_seq: 31,
        preview: "继续推进下一步",
        token_estimate: 32,
        tool_names: [],
        provenance: {},
      },
    ],
  };
}
