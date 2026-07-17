import { describe, expect, it, vi } from "vitest";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { SessionEventEnvelope } from "../../session/model/types";
import { dispatchLiveSessionEvents } from "../../session/ui/SessionChatViewModel";
import { planAgentRunLiveEvent } from "./controlPlaneModel";

function eventEnvelope(
  eventSeq: number,
  event: BackboneEvent,
): SessionEventEnvelope {
  const sessionId = "agentrun:run-1:agent-1";
  return {
    session_id: sessionId,
    event_seq: eventSeq,
    occurred_at_ms: eventSeq,
    committed_at_ms: eventSeq,
    session_update_type: event.type,
    notification: {
      event,
      sessionId,
      source: {
        connectorId: "managed-runtime",
        connectorType: "managed_runtime",
        executorId: "binding-1",
      },
      trace: {
        turnId: "turn-1",
        entryIndex: null,
      },
      observedAt: "2026-07-17T06:18:29.136Z",
    },
  };
}

describe("Managed Agent thread-name projection flow", () => {
  it("dispatches only live standard thread-name updates to the page effect planner", () => {
    const onLiveEvent = vi.fn();
    const historicalNameUpdate: BackboneEvent = {
      type: "thread_name_updated",
      payload: {
        threadId: "native-thread-1",
        threadName: "历史标题",
      },
    };
    const liveNameUpdate: BackboneEvent = {
      type: "thread_name_updated",
      payload: {
        threadId: "native-thread-1",
        threadName: "实时标题",
      },
    };

    const hydratedCursor = dispatchLiveSessionEvents(
      [eventEnvelope(31, historicalNameUpdate)],
      null,
      31,
      onLiveEvent,
    );
    const liveCursor = dispatchLiveSessionEvents(
      [
        eventEnvelope(31, historicalNameUpdate),
        eventEnvelope(32, liveNameUpdate),
      ],
      hydratedCursor,
      31,
      onLiveEvent,
    );

    expect(hydratedCursor).toBe(31);
    expect(liveCursor).toBe(32);
    expect(onLiveEvent).toHaveBeenCalledTimes(1);
    expect(onLiveEvent).toHaveBeenCalledWith(liveNameUpdate);
    expect(
      planAgentRunLiveEvent(onLiveEvent.mock.calls[0][0]),
    ).toEqual({
      effects: {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "thread_name_updated",
      },
      refreshTaskPlan: false,
    });
  });
});
