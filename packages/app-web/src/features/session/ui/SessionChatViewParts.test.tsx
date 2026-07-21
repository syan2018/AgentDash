import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { TurnSegment } from "../model/useSessionFeed";
import { SessionChatStream } from "./SessionChatViewParts";

describe("SessionChatStream turn headers", () => {
  it("does not render a completed turn header for an unscoped trailing segment", () => {
    const turnSegments: TurnSegment[] = [
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

  it("renders the authoritative error for a failed turn without output", () => {
    const turnSegments: TurnSegment[] = [
      {
        turnId: "turn-failed",
        status: "failed",
        errorMessage: "provider rejected reasoning effort",
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

    expect(html).toContain("执行失败");
    expect(html).toContain("provider rejected reasoning effort");
  });
});
