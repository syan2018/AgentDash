import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import { parseContextFrame, type ContextFrame } from "../model/contextFrame";
import { ContextFrameStream } from "./ContextFrameStream";

describe("ContextFrameStream", () => {
  it("严格保留后端 Vec 顺序", () => {
    const frames = [
      readFrame(sampleFrame("system-guidelines", "system_guidelines", "guidelines_marker")),
      readFrame(sampleFrame("identity", "identity", "identity_marker")),
    ];

    const markup = renderToStaticMarkup(
      <ContextFrameStream frames={frames} defaultExpanded />,
    );

    expect(markup).toContain("GUIDELINES / IDENTITY");
    expect(markup.indexOf("guidelines_marker")).toBeLessThan(
      markup.indexOf("identity_marker"),
    );
  });

  it("批次任一帧为 rebuild 状态时展示上下文已重建", () => {
    const normal = readFrame(sampleFrame("identity", "identity", "identity_marker"));
    const rebuiltValue = sampleFrame("assignment", "assignment_context", "assignment_marker");
    rebuiltValue.delivery_status = "applied_to_compacted_context";

    const markup = renderToStaticMarkup(
      <ContextFrameStream
        frames={[normal, readFrame(rebuiltValue)]}
        defaultExpanded
      />,
    );

    expect(markup).toContain("上下文已重建");
    expect(markup).not.toContain("上下文已更新");
  });
});

function readFrame(value: Record<string, unknown>): ContextFrame {
  const frame = parseContextFrame(value);
  if (!frame) throw new Error("invalid context frame test fixture");
  return frame;
}

function sampleFrame(
  id: string,
  kind: ContextFrame["kind"],
  marker: string,
): Record<string, unknown> {
  return {
    id,
    kind,
    delivery_status: "applied_before_prompt",
    rendered_text: marker,
    created_at_ms: 1,
    sections: [{
      kind: "context_fragments",
      fragments: [{
        slot: kind,
        label: marker,
        source: "test",
        content: marker,
      }],
    }],
  };
}
