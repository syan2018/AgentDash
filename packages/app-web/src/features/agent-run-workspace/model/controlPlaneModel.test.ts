import { describe, expect, it, vi } from "vitest";

import type {
  AgentRunOwnershipView,
  ConversationCommandKind,
  ConversationCommandPlacement,
  ConversationCommandView,
  ConversationModelConfigView,
} from "../../../generated/workflow-contracts";
import type { ProjectEventStreamEnvelope } from "../../../generated/project-contracts";
import type { ProjectAgentSummary } from "../../../types";
import {
  type AgentRunChatSubmitIntent,
  buildAgentRunConversationCommandState,
  buildDraftConversationCommandState,
} from "./conversationCommandState";
import {
  planAgentRunLiveEvent,
  planAgentRunProjectEvent,
  planAgentRunTurnEnded,
  planAgentRunTurnStarted,
  resolveAgentRunSubmitCommand,
} from "./controlPlaneModel";
import { applyAgentRunControlPlaneEffectPlan } from "./useAgentRunWorkspaceControlPlane";

function command(input: {
  kind: ConversationCommandKind;
  command_id: string;
  placement?: ConversationCommandPlacement[];
}): ConversationCommandView {
  const runtimeCommand = input.kind === "cancel"
    ? "interrupt"
    : input.kind === "compact_context"
      ? "request_compaction"
      : "submit_input";
  return {
    kind: input.kind,
    command_id: input.command_id,
    runtime_command: runtimeCommand,
    requires_input: input.kind === "submit_message",
    executor_config_policy: "optional",
    placement: input.placement ?? ["composer_primary"],
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

  it("keeps Runtime turn boundaries out of Workspace refresh", () => {
    expect(planAgentRunTurnStarted()).toEqual({});
    expect(planAgentRunTurnEnded()).toEqual({});
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
    });
  });

  it("keeps terminal display metadata out of control-plane refresh", () => {
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

    expect(plan).toEqual({ effects: {} });
  });

  it("does not project Runtime execution through Workspace live effects", () => {
    expect(planAgentRunLiveEvent({
      type: "turn_started",
      payload: {
        threadId: "native-thread-1",
        turn: {
          id: "turn-1",
          items: [],
          itemsView: "full",
          status: "inProgress",
          error: null,
        },
      },
    }).effects).toEqual({});

    expect(planAgentRunLiveEvent({
      type: "turn_completed",
      payload: {
        threadId: "native-thread-1",
        turn: {
          id: "turn-1",
          items: [],
          itemsView: "full",
          status: "completed",
          error: null,
        },
      },
    }).effects).toEqual({});
  });

  it("keeps Agent-native ContextFrame changes inside the canonical feed lane", () => {
    const plan = planAgentRunLiveEvent({
      type: "platform",
      payload: {
        kind: "context_frame_changed",
        data: {
          frame: {
            id: "frame-capability-1",
            kind: "capability_state_delta",
            source: "runtime_context_update",
            delivery_status: "applied_before_prompt",
            delivery_channel: "connector_context",
            message_role: "system",
            delivery_metadata: {
              delivery_phase: "turn_runtime",
              delivery_order: 1,
              cache_policy: "turn_ephemeral",
              model_channel: "system",
              agent_consumption: {
                target: "dash-agent",
                mode: "system_append",
                reason: "materialized_surface",
              },
              frontend_label: "Capability",
              connector_profile: {
                profile_id: "dash-agent",
                declared_consumption_modes: ["system_append"],
              },
            },
            rendered_text: "Capability updated",
            sections: [],
            created_at_ms: 1n,
          },
        },
      },
    });

    expect(plan).toEqual({ effects: {} });
  });

  it("refreshes the Task owner after a successful completed task_write", () => {
    const plan = planAgentRunLiveEvent({
      type: "item_completed",
      payload: {
        threadId: "native-thread-1",
        turnId: "turn-1",
        completedAtMs: 10,
        item: {
          type: "dynamicToolCall",
          id: "task-write-1",
          tool: "task_write",
          arguments: { operation: "update_status" },
          status: "completed",
          success: true,
        },
      },
    });

    expect(plan).toEqual({ effects: { refreshTaskPlan: true } });
  });

  it("does not refresh Task owner for failed or unrelated completed tools", () => {
    for (const item of [
      {
        type: "dynamicToolCall" as const,
        id: "task-write-failed",
        tool: "task_write",
        arguments: {},
        status: "failed" as const,
        success: false,
      },
      {
        type: "dynamicToolCall" as const,
        id: "task-read-completed",
        tool: "task_read",
        arguments: {},
        status: "completed" as const,
        success: true,
      },
    ]) {
      expect(planAgentRunLiveEvent({
        type: "item_completed",
        payload: {
          threadId: "native-thread-1",
          turnId: "turn-1",
          completedAtMs: 10,
          item,
        },
      })).toEqual({ effects: {} });
    }
  });

  it("executes the Task owner refresh effect exactly once", () => {
    const refreshTaskPlan = vi.fn().mockResolvedValue(null);

    applyAgentRunControlPlaneEffectPlan(
      { refreshTaskPlan: true },
      {
        refreshAgentRunWorkspaceState: vi.fn().mockResolvedValue(null),
        openWorkspacePanel: vi.fn(),
        refreshAgentRunList: vi.fn(),
        refreshTaskPlan,
      },
    );

    expect(refreshTaskPlan).toHaveBeenCalledTimes(1);
  });

  it("opens the exact Workspace Module from the canonical presentation event", () => {
    const plan = planAgentRunLiveEvent({
      type: "platform",
      payload: {
        kind: "workspace_module_presentation_requested",
        data: {
          module_id: "canvas:cvs-live",
          view_key: "default",
          renderer_kind: "canvas",
          presentation_uri: "canvas://cvs-live",
          title: "Live Canvas",
        },
      },
    });

    expect(plan.effects).toEqual({
      refreshWorkspaceState: true,
      openWorkspacePanel: {
        afterWorkspaceRefresh: true,
        presentation: {
          module_id: "canvas:cvs-live",
          view_key: "default",
          renderer_kind: "canvas",
          presentation_uri: "canvas://cvs-live",
          title: "Live Canvas",
        },
        target: {
          typeId: "canvas",
          uri: "canvas://cvs-live",
          options: { refreshContent: false },
        },
      },
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
      refreshAgentRunListReason: "control_plane:agent_run_list:title_changed",
    });
    expect(
      planAgentRunProjectEvent(event, {
        runId: "run-1",
        agentId: "another-agent",
      }),
    ).toEqual({});
  });

});
