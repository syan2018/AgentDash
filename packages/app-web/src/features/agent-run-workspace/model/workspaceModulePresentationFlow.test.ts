import { describe, expect, it, vi } from "vitest";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { SessionEventEnvelope } from "../../session/model/types";
import { dispatchPlatformSideEffectEvents } from "../../session/ui/SessionChatViewModel";
import { planAgentRunSystemEvent } from "./controlPlaneModel";
import { applyAgentRunControlPlaneEffectPlan } from "./useAgentRunWorkspaceControlPlane";

function eventEnvelope(eventSeq: number, event: BackboneEvent): SessionEventEnvelope {
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

function workspaceModulePresentedEvent(): BackboneEvent {
  return {
    type: "platform",
    payload: {
      kind: "control_plane_projection_changed",
      data: {
        projection: "resource_surface",
        reason: "workspace_module_presented",
        run_id: "run-1",
        agent_id: "agent-1",
        frame_id: "frame-1",
        gate_id: null,
        mailbox_message_id: null,
        delivery_runtime_session_id: "runtime-1",
        workspace_module_presentation: {
          module_id: "canvas:cvs-canvas",
          view_key: "preview",
          renderer_kind: "canvas",
          presentation_uri: "canvas://cvs-canvas",
          title: "临时 Canvas 展示测试",
          payload: { reason: "smoke-test" },
          diagnostics: null,
        },
      },
    },
  };
}

describe("Workspace Module presentation frontend flow", () => {
  it("replays the typed control-plane projection through the page executor and opens its renderer target", () => {
    const openWorkspacePanel = vi.fn();
    const refreshAgentRunWorkspaceState = vi.fn(async () => undefined);
    const refreshWorkspaceModuleCatalog = vi.fn();
    const scheduleHookRuntimeRefresh = vi.fn();
    const refreshAgentRunList = vi.fn();

    const lastSeenSeq = dispatchPlatformSideEffectEvents(
      [
        eventEnvelope(93, {
          type: "platform",
          payload: {
            kind: "session_meta_update",
            data: {
              key: "system_message",
              value: { message: "hydrated message" },
            },
          },
        }),
        eventEnvelope(94, workspaceModulePresentedEvent()),
        eventEnvelope(97, {
          type: "item_completed",
          payload: {
            threadId: "thread-1",
            turnId: "turn-1",
            item: {
              type: "dynamicToolCall",
              id: "tool-1",
              tool: "workspace_module_present",
              status: "completed",
              success: true,
              arguments: {},
              namespace: null,
              durationMs: null,
              contentItems: null,
            },
            completedAtMs: 97,
          },
        }),
      ],
      null,
      97,
      (eventType, event) => {
        applyAgentRunControlPlaneEffectPlan(
          planAgentRunSystemEvent(eventType, event),
          {
            refreshAgentRunWorkspaceState,
            refreshWorkspaceModuleCatalog,
            openWorkspacePanel,
            scheduleHookRuntimeRefresh,
            refreshAgentRunList,
          },
        );
      },
    );

    expect(lastSeenSeq).toBe(97);
    expect(openWorkspacePanel).toHaveBeenCalledTimes(1);
    expect(openWorkspacePanel).toHaveBeenCalledWith({
      typeId: "canvas",
      uri: "canvas://cvs-canvas",
      options: { refreshContent: false },
    });
  });
});
