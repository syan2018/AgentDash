import { describe, expect, it, vi } from "vitest";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { AgentRunWorkspaceView } from "../../../generated/workflow-contracts";
import type { SessionEventEnvelope } from "../../session/model/types";
import { dispatchLiveSessionEvents } from "../../session/ui/SessionChatViewModel";
import { planAgentRunLiveEvent } from "./controlPlaneModel";
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

function workspaceModulePresentationRequestedEvent(): BackboneEvent {
  return {
    type: "platform",
    payload: {
      kind: "workspace_module_presentation_requested",
      data: {
        module_id: "canvas:cvs-canvas",
        view_key: "preview",
        renderer_kind: "canvas",
        presentation_uri: "canvas://cvs-canvas",
        title: "临时 Canvas 展示测试",
        payload: { reason: "smoke-test" },
        diagnostics: null,
      },
    },
  };
}

function agentRunWorkspace(
  workspaceModules: AgentRunWorkspaceView["workspace_modules"],
): AgentRunWorkspaceView {
  return {
    run_ref: { run_id: "run-1" },
    agent_ref: { run_id: "run-1", agent_id: "agent-1" },
    project_id: "project-1",
    shell: {
      display_title: "Run",
      title_source: "runtime",
      delivery_status: "ready",
      last_activity_at: "2026-07-17T06:18:29.136Z",
    },
    control_plane: {
      status: "ready",
      ownership: {
        run_created_by_user_id: "user-1",
        agent_created_by_user_id: "user-1",
        current_user_controls_run: true,
      },
    },
    workspace_modules: workspaceModules,
    subject_associations: [],
    children: [],
  };
}

describe("Workspace Module presentation frontend flow", () => {
  it("refreshes the current AgentRun projection before applying a live presentation target", async () => {
    const openWorkspacePanel = vi.fn();
    const refreshAgentRunWorkspaceState = vi.fn(async () => agentRunWorkspace([
      {
        summary: {
          module_id: "canvas:cvs-canvas",
          kind: "canvas",
          title: "临时 Canvas 展示测试",
          description: "",
          source: "cvs-canvas",
          operation_summary: [],
          permission_summary: [],
          status: { kind: "ready" },
        },
        ui_entries: [{
          view_key: "preview",
          renderer_kind: "canvas",
          presentation_uri: "canvas://cvs-canvas",
          title: "临时 Canvas 展示测试",
        }],
        operations: [],
      },
    ]));
    const scheduleHookRuntimeRefresh = vi.fn();
    const refreshAgentRunList = vi.fn();

    const lastSeenSeq = dispatchLiveSessionEvents(
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
        eventEnvelope(94, workspaceModulePresentationRequestedEvent()),
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
      93,
      (event) => {
        const plan = planAgentRunLiveEvent(event);
        applyAgentRunControlPlaneEffectPlan(
          plan.effects,
          {
            refreshAgentRunWorkspaceState,
            openWorkspacePanel,
            scheduleHookRuntimeRefresh,
            refreshAgentRunList,
          },
        );
      },
    );

    expect(lastSeenSeq).toBe(97);
    await vi.waitFor(() => {
      expect(refreshAgentRunWorkspaceState).toHaveBeenCalledTimes(1);
      expect(openWorkspacePanel).toHaveBeenCalledTimes(1);
    });
    expect(openWorkspacePanel).toHaveBeenCalledWith({
      typeId: "canvas",
      uri: "canvas://cvs-canvas",
      options: { refreshContent: false },
    });
  });

  it("does not replay a historical presentation after its Canvas left the current projection", async () => {
    const refreshAgentRunWorkspaceState = vi.fn(async () => agentRunWorkspace([]));
    const openWorkspacePanel = vi.fn();

    applyAgentRunControlPlaneEffectPlan(
      planAgentRunLiveEvent(workspaceModulePresentationRequestedEvent()).effects,
      {
        refreshAgentRunWorkspaceState,
        openWorkspacePanel,
        scheduleHookRuntimeRefresh: vi.fn(),
        refreshAgentRunList: vi.fn(),
      },
    );

    await vi.waitFor(() => {
      expect(refreshAgentRunWorkspaceState).toHaveBeenCalledTimes(1);
    });
    expect(openWorkspacePanel).not.toHaveBeenCalled();
  });

  it("does not execute a historical presentation request as an imperative UI action", () => {
    const onLiveEvent = vi.fn();

    const cursor = dispatchLiveSessionEvents(
      [eventEnvelope(94, workspaceModulePresentationRequestedEvent())],
      null,
      94,
      onLiveEvent,
    );

    expect(cursor).toBe(94);
    expect(onLiveEvent).not.toHaveBeenCalled();
  });

});
