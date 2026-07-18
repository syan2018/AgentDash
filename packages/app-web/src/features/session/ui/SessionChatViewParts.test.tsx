import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { AgentRunRuntimeTurnSegment } from "../../agent-run-runtime";
import { SessionChatStream } from "./SessionChatViewParts";

describe("SessionChatStream turn headers", () => {
  it("does not render a completed turn header for an unscoped trailing segment", () => {
    const turnSegments: AgentRunRuntimeTurnSegment[] = [
      {
        turnId: "turn-1",
        status: "completed",
        durationMs: 55_000,
        items: [],
        finalOutput: null,
      },
      {
        turnId: null,
        status: "completed",
        items: [],
        finalOutput: null,
      },
    ];

    const html = renderToStaticMarkup(
      <SessionChatStream
        containerRef={{ current: null }}
        displayItems={[]}
        turnSegments={turnSegments}
        hasRuntimeStreamTarget
        isLoading={false}
        streamingEntryId={null}
        streamPrefixContent={<span />}
        onScroll={() => undefined}
      />,
    );

    expect(html).toContain("已处理 55s");
    expect(html.match(/已处理/g)).toHaveLength(1);
  });
});
