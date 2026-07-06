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
import type {
  ConversationCommandKind,
  ConversationCommandStaleGuardView,
} from "../../../generated/agent-run-mailbox-contracts";
import type { ProjectAgentSummary } from "../../../types";
import {
  type AgentRunChatSubmitIntent,
  buildAgentRunConversationCommandState,
  buildDraftConversationCommandState,
} from "./conversationCommandState";
import {
  planAgentRunMessageSent,
  planAgentRunSystemEvent,
  planAgentRunTurnEnd,
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
  workspace_module_presentation?: {
    module_id: string;
    view_key: string;
    renderer_kind: string;
    presentation_uri: string;
    title: string;
    payload: null;
    diagnostics: null;
  } | null;
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
        workspace_module_presentation: null,
        ...data,
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

  it("plans message, turn-end, and manual workspace-module refresh effects", () => {
    expect(planAgentRunMessageSent()).toEqual({
      refreshWorkspaceState: true,
      hookRuntimeRefresh: { reason: "message_sent", immediate: true },
      refreshAgentRunListReason: "message_sent",
    });
    expect(planAgentRunTurnEnd()).toEqual({
      refreshWorkspaceState: true,
      hookRuntimeRefresh: { reason: "turn_end", immediate: true },
      refreshAgentRunListReason: "turn_end",
    });
    expect(planAgentRunWorkspaceModuleOpened()).toEqual({
      refreshWorkspaceState: true,
      refreshWorkspaceModuleCatalog: true,
      hookRuntimeRefresh: { reason: "workspace_module_user_opened" },
    });
  });

  it("plans workspace and list refresh from typed control-plane projection changes", () => {
    const plan = planAgentRunSystemEvent(
      "control_plane_projection_changed",
      controlPlaneProjectionEvent({
        projection: "mailbox",
        reason: "mailbox_state_changed",
      }),
    );

    expect(plan).toEqual({
      refreshWorkspaceState: true,
      refreshAgentRunListReason: "control_plane:mailbox:mailbox_state_changed",
    });
  });

  it("plans resource surface and hook refresh from typed capability projection changes", () => {
    const plan = planAgentRunSystemEvent(
      "control_plane_projection_changed",
      controlPlaneProjectionEvent({
        projection: "resource_surface",
        reason: "capability_state_changed",
      }),
    );

    expect(plan).toEqual({
      refreshWorkspaceState: true,
      refreshWorkspaceModuleCatalog: true,
      hookRuntimeRefresh: {
        reason: "control_plane:resource_surface:capability_state_changed",
      },
    });
  });

  it("opens Canvas presentation from typed projection payload after refreshing runtime surface", () => {
    const plan = planAgentRunSystemEvent(
      "control_plane_projection_changed",
      controlPlaneProjectionEvent({
        projection: "resource_surface",
        reason: "capability_state_changed",
        workspace_module_presentation: {
          module_id: "canvas:canvas-1",
          view_key: "preview",
          renderer_kind: "canvas",
          presentation_uri: "canvas://canvas-1",
          title: "Canvas Preview",
          payload: null,
          diagnostics: null,
        },
      }),
    );

    expect(plan).toEqual({
      refreshWorkspaceState: true,
      refreshWorkspaceModuleCatalog: true,
      hookRuntimeRefresh: {
        reason: "control_plane:resource_surface:capability_state_changed",
      },
      openWorkspacePanel: {
        afterWorkspaceRefresh: true,
        target: {
          typeId: "canvas",
          uri: "canvas://canvas-1",
          options: { refreshContent: false },
        },
      },
    });
  });

  it("does not synthesize Canvas presentation URI from view_key", () => {
    const plan = planAgentRunSystemEvent(
      "control_plane_projection_changed",
      controlPlaneProjectionEvent({
        projection: "resource_surface",
        reason: "capability_state_changed",
        workspace_module_presentation: {
          module_id: "canvas:canvas-1",
          view_key: "canvas-1",
          renderer_kind: "canvas",
          presentation_uri: "",
          title: "Canvas Preview",
          payload: null,
          diagnostics: null,
        },
      }),
    );

    expect(plan).toEqual({
      refreshWorkspaceState: true,
      refreshWorkspaceModuleCatalog: true,
      hookRuntimeRefresh: {
        reason: "control_plane:resource_surface:capability_state_changed",
      },
    });
  });
});
