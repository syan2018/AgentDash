import { describe, expect, it } from "vitest";

import type {
  ControlPlaneProjection,
  ControlPlaneProjectionChangeReason,
  ControlPlaneProjectionChanged,
} from "../../../generated/backbone-protocol";
import type {
  ConversationModelConfigView,
} from "../../../generated/workflow-contracts";
import type { RuntimeSnapshot } from "../../../generated/agent-runtime-contracts";
import type { ProjectAgentSummary } from "../../../types";
import {
  type AgentRunChatSubmitIntent,
  buildAgentRunConversationCommandState,
  buildDraftConversationCommandState,
} from "./conversationCommandState";
import {
  planAgentRunMessageSent,
  planAgentRunControlPlaneProjectionChanged,
  planAgentRunTurnEnded,
  planAgentRunWorkspaceModuleOpened,
  resolveAgentRunSubmitCommand,
} from "./controlPlaneModel";

function runtimeSnapshot(): RuntimeSnapshot {
  return {
    thread_id: "thread-1",
    revision: 1n,
    status: "active",
    active_turn_id: null,
    binding_id: "binding-1",
    profile_digest: "sha256:profile",
    bound_profile: {
      reference_class: "managed_thread",
      input: { modalities: ["text"] },
      instruction: { channels: ["system"], configuration_boundary: "thread_start" },
      tools: { channels: [], configuration_boundary: "turn_start", cancellation: true },
      workspace: { capabilities: [], mechanism: "host_adapted_exact" },
      interactions: { kinds: [], durable_correlation: true },
      lifecycle: ["turn_start"],
      hooks: { points: [], configuration_boundary: "thread_start" },
      context: { capabilities: ["read"], fidelity: "platform_exact", activation_idempotent: true },
      telemetry_config: [],
    },
    active_checkpoint_id: null,
    context_revision: 1n,
    settings_revision: 1n,
    tool_set_revision: 1n,
    pending_interactions: [],
    command_availability: {
      turn_start: { status: "unavailable", unmet: [], reason: "runtime unavailable" },
    },
    transcript: [],
    transcript_fidelity: "platform_exact",
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
}): ControlPlaneProjectionChanged {
  return {
    run_id: "run-1",
    agent_id: "agent-1",
    frame_id: null,
    gate_id: null,
    mailbox_message_id: null,
    workspace_module_presentation: null,
    ...data,
  };
}

describe("AgentRun control-plane model", () => {
  it("resolves submit intent against Runtime snapshot commands", () => {
    const commandState = buildAgentRunConversationCommandState({
      modelConfig: resolvedModelConfig(),
      workspaceStateStatus: "ready",
      workspaceStateError: null,
      runtimeSnapshot: runtimeSnapshot(),
    });

    const result = resolveAgentRunSubmitCommand(commandState, submitIntent("runtime:turn_start"));

    if (!result.ok) throw new Error(result.message);
    expect(result.command).toBe(commandState.commands.commands[0]);
    expect(result.command.enabled).toBe(false);
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
      modelConfig: resolvedModelConfig(),
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
      refreshWorkspaceModuleCatalog: true,
      hookRuntimeRefresh: { reason: "workspace_module_user_opened" },
    });
  });

  it("plans workspace and list refresh from typed control-plane projection changes", () => {
    const plan = planAgentRunControlPlaneProjectionChanged(
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
    const plan = planAgentRunControlPlaneProjectionChanged(
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
    const plan = planAgentRunControlPlaneProjectionChanged(
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
    const plan = planAgentRunControlPlaneProjectionChanged(
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
