import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { TurnSegment } from "../model/useSessionFeed";
import type { SessionDisplayEntry } from "../model/types";
import { SessionChatStream } from "./SessionChatViewParts";
import { getTurnSectionKey } from "./turnSectionIdentity";

function completedMessage(id: string): SessionDisplayEntry {
  return {
    id,
    sessionId: "session-1",
    timestamp: 1,
    eventSeq: 1,
    turnId: "turn-1",
    event: {
      type: "item_completed",
      payload: {
        threadId: "session-1",
        turnId: "turn-1",
        completedAtMs: 1,
        item: {
          type: "agentMessage",
          id,
          text: id,
        },
      },
    },
  };
}

describe("SessionChatStream turn headers", () => {
  it("keeps split presentation sections of one canonical turn uniquely identified", () => {
    const firstItem = completedMessage("item:first");
    const secondItem = completedMessage("item:second");
    const first: TurnSegment = {
      turnId: "turn-1",
      status: "active",
      items: [firstItem],
      finalOutput: firstItem,
    };
    const second: TurnSegment = {
      turnId: "turn-1",
      status: "active",
      items: [secondItem],
      finalOutput: secondItem,
    };

    expect(getTurnSectionKey(first)).toBe("turn-section:turn-1:item:first");
    expect(getTurnSectionKey(second)).toBe("turn-section:turn-1:item:second");
  });

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
