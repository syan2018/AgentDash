import { describe, expect, it } from "vitest";

import type {
  AgentRunOwnershipView,
  ConversationCommandView,
  ConversationModelConfigView,
} from "../../../generated/workflow-contracts";
import type { ProjectAgentSummary } from "../../../types";
import { agentRuntimeTestFixtures } from "../../agent-run-runtime/model/agentRuntimeTestFixtures";
import { applyAgentRuntimeControlToChatCommandState } from "../../session/ui/SessionChatViewModel";
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

function command(
  kind: ConversationCommandView["kind"],
  commandId: string,
): ConversationCommandView {
  return {
    kind,
    command_id: commandId,
    runtime_command: kind === "cancel"
      ? "interrupt"
      : kind === "compact_context"
        ? "request_compaction"
        : "submit_input",
    shortcut: kind === "submit_message" ? "enter" : undefined,
    requires_input: kind === "submit_message",
    executor_config_policy: kind === "cancel" ? "forbidden" : "optional",
    placement: kind === "cancel" ? ["header"] : ["composer_primary"],
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
  it("Workspace 只提供静态 Runtime command binding", () => {
    const state = buildAgentRunConversationCommandState({
      conversation: {
        commands: {
          ownership,
          keyboard: { enter: "cmd-submit" },
          commands: [
            command("submit_message", "cmd-submit"),
            command("cancel", "cmd-cancel"),
          ],
        },
        model_config: resolvedModelConfig(),
      },
      workspaceStateStatus: "ready",
      workspaceStateError: null,
    });
    const model = projectAgentRunChatCommandState(state);

    expect(model.executionStatus).toBe("runtime");
    expect(model.commands).toEqual(expect.arrayContaining([
      expect.objectContaining({
        command_id: "cmd-submit",
        runtimeCommand: "submit_input",
        enabled: false,
      }),
    ]));
    expect(model.cancelCommand).toEqual(expect.objectContaining({
      runtimeCommand: "interrupt",
      enabled: false,
    }));
  });

  it("AgentRuntimeView 覆盖执行事实和命令可用性", () => {
    const state = projectAgentRunChatCommandState(
      buildAgentRunConversationCommandState({
        conversation: {
          commands: {
            ownership,
            keyboard: { enter: "cmd-submit" },
            commands: [
              command("submit_message", "cmd-submit"),
              command("cancel", "cmd-cancel"),
            ],
          },
          model_config: resolvedModelConfig(),
        },
        workspaceStateStatus: "ready",
        workspaceStateError: null,
      }),
    );

    const projected = applyAgentRuntimeControlToChatCommandState(
      state,
      agentRuntimeTestFixtures.snapshots.started,
    );

    expect(projected.executionStatus).toBe("running_active");
    expect(projected.activeTurnId).toBe("turn-compaction");
    expect(projected.cancelCommand).toEqual(expect.objectContaining({
      enabled: false,
      unavailable_reason: "turn_not_cancellable",
    }));
    expect(projected.commands.find(
      (item) => item.command_id === "cmd-submit",
    )).toEqual(expect.objectContaining({
      enabled: true,
      unavailable_reason: undefined,
    }));
  });

  it("Workspace refresh/error 不覆盖 Runtime 运行态与停止能力", () => {
    const productError = projectAgentRunChatCommandState(
      buildAgentRunConversationCommandState({
        conversation: null,
        workspaceStateStatus: "error",
        workspaceStateError: "Workspace refresh failed",
      }),
    );

    const projected = applyAgentRuntimeControlToChatCommandState(
      productError,
      agentRuntimeTestFixtures.snapshots.started,
    );

    expect(projected.executionStatus).toBe("running_active");
    expect(projected.cancelCommand?.enabled).toBe(false);
  });

  it("Workspace 加载失败时保留 Product shell 错误", () => {
    const model = projectAgentRunChatCommandState(
      buildAgentRunConversationCommandState({
        conversation: null,
        workspaceStateStatus: "error",
        workspaceStateError: "工作台状态加载失败",
      }),
    );

    expect(model.executionStatus).toBe("error");
    expect(model.commands).toEqual([]);
    expect(model.helperText).toBe("工作台状态加载失败");
  });

  it("Draft 命令继续由本地模型配置事实控制", () => {
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

    expect(missingModel.localDraftAction?.enabled).toBe(false);
    expect(projectAgentRunChatCommandState(ready).keyboard.enter)
      .toBe("draft:start_local:resolved");
  });
});
