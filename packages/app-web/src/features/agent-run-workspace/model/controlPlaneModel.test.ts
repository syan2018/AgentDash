import { describe, expect, it } from "vitest";

import type {
  BackboneEvent,
  ControlPlaneProjection,
  ControlPlaneProjectionChangeReason,
} from "../../../generated/backbone-protocol";
import type {
  AgentRunOwnershipView,
  ConversationCommandPlacement,
  ConversationCommandView,
  ConversationModelConfigView,
} from "../../../generated/workflow-contracts";
import type { ProjectEventStreamEnvelope } from "../../../generated/project-contracts";
import type {
  ConversationCommandKind,
  ConversationCommandStaleGuardView,
} from "../../../generated/agent-run-mailbox-contracts";
import type { ProjectAgentSummary } from "../../../types";
import { managedRuntimeTestFixtures } from "../../agent-run-runtime/model/managedRuntimeTestFixtures";
import {
  type AgentRunChatSubmitIntent,
  buildAgentRunConversationCommandState,
  buildDraftConversationCommandState,
} from "./conversationCommandState";
import {
  planAgentRunLiveEvent,
  planAgentRunMessageSent,
  planAgentRunProjectEvent,
  planAgentRunRuntimeChanges,
  planAgentRunTurnEnded,
  planAgentRunWorkspaceModuleOpened,
  resolveAgentRunSubmitCommand,
} from "./controlPlaneModel";

function staleGuard(commandId: string): ConversationCommandStaleGuardView {
  return {
    snapshot_id: "snapshot-1",
    run_id: "run-1",
    agent_id: "agent-1",
    active_turn_id: commandId === "cancel" ? "turn-1" : undefined,
  };
}

function command(input: {
  kind: ConversationCommandKind;
  command_id: string;
  enabled?: boolean;
  placement?: ConversationCommandPlacement[];
}): ConversationCommandView {
  return {
    kind: input.kind,
    command_id: input.command_id,
    enabled: input.enabled ?? true,
    requires_input: input.kind === "submit_message",
    executor_config_policy: "optional",
    placement: input.placement ?? ["composer_primary"],
    stale_guard: staleGuard(input.kind),
  };
}

function resolvedModelConfig(): ConversationModelConfigView {
  return {
    status: "resolved",
    missing_fields: [],
    effective_executor_config: {
      executor: "CODEX",
      provider_id: "openai",
      model_id: "gpt-test",
      source: "project_agent_preset",
    },
  };
}

function submitIntent(commandId: string): AgentRunChatSubmitIntent {
  return {
    command_id: commandId,
    prompt: "继续",
  };
}

function controlPlaneProjectionEvent(data: {
  projection: ControlPlaneProjection;
  reason: ControlPlaneProjectionChangeReason;
}): BackboneEvent {
  return {
    type: "platform",
    payload: {
      kind: "control_plane_projection_changed",
      data: {
        run_id: "run-1",
        agent_id: "agent-1",
        frame_id: null,
        gate_id: null,
        mailbox_message_id: null,
        delivery_runtime_session_id: null,
        ...data,
      },
    },
  };
}

function workspaceModulePresentationRequest(
  presentationUri = "canvas://canvas-1",
): BackboneEvent {
  return {
    type: "platform",
    payload: {
      kind: "workspace_module_presentation_requested",
      data: {
        module_id: "canvas:canvas-1",
        view_key: "preview",
        renderer_kind: "canvas",
        presentation_uri: presentationUri,
        title: "Canvas Preview",
        payload: null,
        diagnostics: null,
      },
    },
  };
}

const ownership: AgentRunOwnershipView = {
  run_created_by_user_id: "owner-user",
  agent_created_by_user_id: "owner-user",
  current_user_controls_run: true,
};

describe("AgentRun control-plane model", () => {
  it("resolves submit intent against runtime conversation commands", () => {
    const submit = command({
      kind: "submit_message",
      command_id: "cmd-submit",
    });
    const commandState = buildAgentRunConversationCommandState({
      conversation: {
        execution: { status: "ready" },
        commands: {
          ownership,
          keyboard: { enter: "cmd-submit" },
          commands: [submit],
        },
        model_config: resolvedModelConfig(),
      },
      workspaceStateStatus: "ready",
      workspaceStateError: null,
    });

    const result = resolveAgentRunSubmitCommand(commandState, submitIntent("cmd-submit"));

    if (!result.ok) throw new Error(result.message);
    expect(result.command).toBe(submit);
  });

  it("resolves submit intent against local draft command", () => {
    const agent: ProjectAgentSummary = {
      key: "agent-key",
      display_name: "Draft Agent",
      description: "Draft agent",
      source: "project_agent",
      executor: {
        executor: "CODEX",
        provider_id: "openai",
        model_id: "gpt-test",
      },
    };
    const commandState = buildDraftConversationCommandState({
      projectId: "project-1",
      agentKey: "agent-key",
      agent,
      workspaceStateReady: true,
    });
    const draftCommand = commandState.localDraftAction;
    if (!draftCommand) throw new Error("draft command missing");

    const result = resolveAgentRunSubmitCommand(
      commandState,
      submitIntent(draftCommand.command_id),
    );

    if (!result.ok) throw new Error(result.message);
    expect(result.command).toBe(draftCommand);
  });

  it("rejects submit intent when command id came from a stale snapshot", () => {
    const commandState = buildAgentRunConversationCommandState({
      conversation: {
        execution: { status: "ready" },
        commands: {
          ownership,
          keyboard: {},
          commands: [],
        },
        model_config: resolvedModelConfig(),
      },
      workspaceStateStatus: "ready",
      workspaceStateError: null,
    });

    expect(resolveAgentRunSubmitCommand(
      commandState,
      submitIntent("cmd-stale"),
    )).toEqual({
      ok: false,
      message: "当前 AgentRun 命令已刷新，请重试。",
    });
  });

  it("plans message and manual workspace-module refresh effects", () => {
    expect(planAgentRunMessageSent()).toEqual({
      refreshWorkspaceState: true,
      hookRuntimeRefresh: { reason: "message_sent", immediate: true },
      refreshAgentRunListReason: "message_sent",
    });
    expect(planAgentRunTurnEnded()).toEqual({
      refreshWorkspaceState: true,
      refreshAgentRunListReason: "turn_ended",
    });
    expect(planAgentRunWorkspaceModuleOpened()).toEqual({
      refreshWorkspaceState: true,
      hookRuntimeRefresh: { reason: "workspace_module_user_opened" },
    });
  });

  it("plans workspace and list refresh from typed control-plane projection changes", () => {
    const plan = planAgentRunLiveEvent(
      controlPlaneProjectionEvent({
        projection: "mailbox",
        reason: "mailbox_state_changed",
      }),
    );

    expect(plan).toEqual({
      effects: {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "control_plane:mailbox:mailbox_state_changed",
      },
      refreshTaskPlan: false,
    });
  });

  it("refreshes workspace and list after a standard thread name update", () => {
    const plan = planAgentRunLiveEvent({
      type: "thread_name_updated",
      payload: {
        threadId: "native-thread-1",
        threadName: "修复登录态刷新",
      },
    });

    expect(plan).toEqual({
      effects: {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "thread_name_updated",
      },
      refreshTaskPlan: false,
    });
  });

  it("uses the same refresh plan when the standard thread name is cleared", () => {
    const plan = planAgentRunLiveEvent({
      type: "thread_name_updated",
      payload: {
        threadId: "native-thread-1",
      },
    });

    expect(plan).toEqual({
      effects: {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "thread_name_updated",
      },
      refreshTaskPlan: false,
    });
  });

  it("plans resource surface and hook refresh from typed capability projection changes", () => {
    const plan = planAgentRunLiveEvent(
      controlPlaneProjectionEvent({
        projection: "resource_surface",
        reason: "capability_state_changed",
      }),
    );

    expect(plan).toEqual({
      effects: {
        refreshWorkspaceState: true,
        hookRuntimeRefresh: {
          reason: "control_plane:resource_surface:capability_state_changed",
        },
      },
      refreshTaskPlan: false,
    });
  });

  it("opens a typed Workspace Module presentation request without projection semantics", () => {
    const plan = planAgentRunLiveEvent(workspaceModulePresentationRequest());

    expect(plan).toEqual({
      effects: {
        refreshWorkspaceState: true,
        openWorkspacePanel: {
          afterWorkspaceRefresh: true,
          presentation: {
            module_id: "canvas:canvas-1",
            view_key: "preview",
            renderer_kind: "canvas",
            presentation_uri: "canvas://canvas-1",
            title: "Canvas Preview",
            payload: null,
            diagnostics: null,
          },
          target: {
            typeId: "canvas",
            uri: "canvas://canvas-1",
            options: { refreshContent: false },
          },
        },
      },
      refreshTaskPlan: false,
    });
  });

  it("does not synthesize Canvas presentation URI from view_key", () => {
    const plan = planAgentRunLiveEvent(workspaceModulePresentationRequest(""));

    expect(plan).toEqual({
      effects: {},
      refreshTaskPlan: false,
    });
  });

  it("coalesces turn terminal workspace and task refresh behind one live-event plan", () => {
    const plan = planAgentRunLiveEvent({
      type: "platform",
      payload: {
        kind: "session_meta_update",
        data: {
          key: "turn_terminal",
          value: {
            terminal_type: "turn_failed",
          },
        },
      },
    });

    expect(plan).toEqual({
      effects: {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "turn_ended",
      },
      refreshTaskPlan: true,
    });
  });

  it("refreshes product projections from committed Runtime snapshot changes", () => {
    expect(
      planAgentRunRuntimeChanges(
        managedRuntimeTestFixtures.changePage.changes,
      ),
    ).toEqual({
      effects: {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "managed_runtime_projection_changed",
      },
      refreshTaskPlan: true,
    });
  });

  it("maps a Runtime surface change to workspace and hook projection refresh", () => {
    expect(
      planAgentRunRuntimeChanges([
        {
          thread_id: "runtime-thread",
          sequence: 12n,
          revision: 8n,
          delta: {
            kind: "source_projection_changed",
            source_change_sequence: 12n,
            source_projection_revision: 8n,
            observation_digest: "sha256:observation",
            section: "surface",
            section_digest: "sha256:surface",
            delta: {
              kind: "surface_changed",
              applied_surface_revision: 7n,
            },
          },
        },
      ]),
    ).toEqual({
      effects: {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "managed_runtime_projection_changed",
        hookRuntimeRefresh: {
          reason: "managed_runtime_surface_changed",
        },
      },
      refreshTaskPlan: false,
    });
  });

  it("refreshes the exact AgentRun workspace from the typed title invalidation", () => {
    const event: ProjectEventStreamEnvelope = {
      type: "ControlPlaneProjectionChanged",
      data: {
        project_id: "project-1",
        change: {
          projection: "agent_run_list",
          reason: "title_changed",
          run_id: "run-1",
          agent_id: "agent-1",
          frame_id: null,
          gate_id: null,
          mailbox_message_id: null,
          delivery_runtime_session_id: null,
        },
      },
    };

    expect(
      planAgentRunProjectEvent(event, {
        runId: "run-1",
        agentId: "agent-1",
      }),
    ).toEqual({
      refreshWorkspaceState: true,
      refreshAgentRunListReason: "title_changed",
    });
    expect(
      planAgentRunProjectEvent(event, {
        runId: "run-1",
        agentId: "another-agent",
      }),
    ).toEqual({});
  });
});
