import { describe, expect, it } from "vitest";

import type {
  AgentRunOwnershipView,
  ConversationCommandPlacement,
  ConversationCommandView,
  ConversationModelConfigView,
} from "../../../generated/workflow-contracts";
import type {
  ConversationCommandKind,
  ConversationCommandStaleGuardView,
} from "../../../generated/agent-run-interaction-contracts";
import type { ProjectAgentSummary } from "../../../types";
import {
  buildAgentRunConversationCommandState,
  buildDraftConversationCommandState,
  projectAgentRunChatCommandState,
} from "./conversationCommandState";

const ownership: AgentRunOwnershipView = {
  run_created_by_user_id: "owner-user",
  agent_created_by_user_id: "owner-user",
  current_user_controls_run: true,
};

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
  unavailable_reason?: string;
  disabled_code?: string;
  shortcut?: string;
  requires_input?: boolean;
  executor_config_policy?: string;
  placement?: ConversationCommandPlacement[];
}): ConversationCommandView {
  return {
    kind: input.kind,
    command_id: input.command_id,
    enabled: input.enabled ?? true,
    unavailable_reason: input.unavailable_reason,
    disabled_code: input.disabled_code,
    shortcut: input.shortcut,
    requires_input: input.requires_input ?? input.kind === "submit_message",
    executor_config_policy: input.executor_config_policy ?? "optional",
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

describe("AgentRun conversation command state", () => {
  it("projects runtime keyboard, primary command, cancel command, and helper text", () => {
    const submit = command({
      kind: "submit_message",
      command_id: "cmd-submit",
      shortcut: "enter",
      placement: ["composer_primary"],
    });
    const cancel = command({
      kind: "cancel",
      command_id: "cmd-cancel",
      enabled: false,
      unavailable_reason: "当前没有运行中的 turn。",
      disabled_code: "not_running",
      requires_input: false,
      executor_config_policy: "forbidden",
      placement: ["header"],
    });
    const commandState = buildAgentRunConversationCommandState({
      conversation: {
        execution: {
          status: "running_active",
          reason: "正在运行",
        },
        commands: {
          ownership,
          keyboard: {
            enter: "cmd-submit",
            ctrl_enter: "cmd-submit-steer",
          },
          commands: [cancel, submit],
        },
        model_config: resolvedModelConfig(),
      },
      workspaceStateStatus: "ready",
      workspaceStateError: null,
    });

    const model = projectAgentRunChatCommandState(commandState);

    expect(model.mode).toBe("runtime");
    expect(model.executionStatus).toBe("running_active");
    expect(model.keyboard).toEqual({
      enter: "cmd-submit",
      ctrl_enter: "cmd-submit-steer",
    });
    expect(model.primaryCommandId).toBe("cmd-submit");
    expect(model.cancelCommand).toEqual({
      command_id: "cmd-cancel",
      kind: "cancel",
      enabled: false,
      unavailable_reason: "当前没有运行中的 turn。",
      disabled_code: "not_running",
      requires_input: false,
      executor_config_policy: "forbidden",
      shortcut: undefined,
    });
    expect(model.modelConfig.status).toBe("resolved");
    expect(model.helperText).toBe("正在运行");
  });

  it("keeps workspace state loading and error states visible when conversation snapshot is missing", () => {
    const commandState = buildAgentRunConversationCommandState({
      conversation: null,
      workspaceStateStatus: "error",
      workspaceStateError: "工作台状态加载失败",
    });

    const model = projectAgentRunChatCommandState(commandState);

    expect(model.executionStatus).toBe("error");
    expect(model.commands).toEqual([]);
    expect(model.modelConfig).toEqual({
      status: "model_required",
      missing_fields: [],
      message: "工作台状态加载失败",
    });
    expect(model.helperText).toBe("工作台状态加载失败");
  });

  it("does not expose stale running commands while workspace state is refreshing", () => {
    const submit = command({
      kind: "submit_message",
      command_id: "cmd-submit",
      shortcut: "enter",
      placement: ["composer_primary"],
    });
    const commandState = buildAgentRunConversationCommandState({
      conversation: {
        execution: {
          status: "running_active",
          active_turn_id: "turn-1",
          reason: "当前 AgentRun 正在执行中。",
        },
        commands: {
          ownership,
          keyboard: {
            enter: "cmd-submit",
          },
          commands: [submit],
        },
        model_config: resolvedModelConfig(),
      },
      workspaceStateStatus: "refreshing",
      workspaceStateError: null,
    });

    const model = projectAgentRunChatCommandState(commandState);

    expect(model.executionStatus).toBe("refreshing");
    expect(model.commands).toEqual([]);
    expect(model.helperText).toBe("当前 AgentRun 工作台状态正在刷新。");
  });

  it("uses draft model policy as the local draft command authority", () => {
    const agent: ProjectAgentSummary = {
      key: "agent-key",
      display_name: "Draft Agent",
      description: "Draft agent",
      source: "project_agent",
      executor: {
        executor: "CODEX",
        provider_id: null,
        model_id: null,
      },
    };

    const missingModel = buildDraftConversationCommandState({
      projectId: "project-1",
      agentKey: "agent-key",
      agent,
      workspaceStateReady: true,
    });
    expect(missingModel.executionStatus).toBe("model_required");
    expect(missingModel.localDraftAction?.enabled).toBe(false);
    expect(missingModel.localDraftAction?.disabled_code).toBe("model_required");

    const ready = buildDraftConversationCommandState({
      projectId: "project-1",
      agentKey: "agent-key",
      agent,
      workspaceStateReady: true,
      explicitExecutorConfigOverride: {
        executor: "CODEX",
        provider_id: "openai",
        model_id: "gpt-test",
      },
    });
    const model = projectAgentRunChatCommandState(ready);

    expect(ready.executionStatus).toBe("draft");
    expect(ready.localDraftAction?.enabled).toBe(true);
    expect(model.mode).toBe("draft");
    expect(model.keyboard.enter).toBe("draft:start_local:resolved");
    expect(model.primaryCommandId).toBe("draft:start_local:resolved");
    expect(model.modelConfig.effective_executor_config).toEqual({
      executor: "CODEX",
      provider_id: "openai",
      model_id: "gpt-test",
      agent_id: undefined,
      thinking_level: undefined,
      source: "user_override",
    });
  });

});
