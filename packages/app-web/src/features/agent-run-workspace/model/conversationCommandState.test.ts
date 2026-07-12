import { describe, expect, it } from "vitest";

import type { RuntimeSnapshot } from "../../../generated/agent-runtime-contracts";
import type { ConversationModelConfigView } from "../../../generated/workflow-contracts";
import type { ProjectAgentSummary } from "../../../types";
import {
  buildAgentRunConversationCommandState,
  buildDraftConversationCommandState,
  projectAgentRunChatCommandState,
} from "./conversationCommandState";

function resolvedModelConfig(): ConversationModelConfigView {
  return {
    status: "resolved",
    missing_fields: [],
    effective_executor_config: {
      executor: "CODEX",
      provider_id: "openai",
      model_id: "gpt-test",
      source: "frame_execution_profile",
    },
  };
}

function runtimeSnapshot(activeTurnId: string | null = "turn-1"): RuntimeSnapshot {
  return {
    thread_id: "thread-1",
    revision: 4n,
    status: "active",
    active_turn_id: activeTurnId,
    binding_id: "binding-1",
    binding_epoch: 0n,
    profile_digest: "sha256:profile",
    bound_profile: {
      reference_class: "managed_thread",
      input: { modalities: ["text"] },
      instruction: { channels: ["system"], configuration_boundary: "thread_start" },
      tools: { channels: ["direct_callback"], configuration_boundary: "turn_start", cancellation: true },
      workspace: { capabilities: ["read"], mechanism: "host_adapted_exact" },
      interactions: { kinds: [], durable_correlation: true },
      lifecycle: ["turn_start", "turn_steer", "turn_interrupt"],
      hooks: { points: [], configuration_boundary: "thread_start" },
      context: { capabilities: ["read"], fidelity: "platform_exact", activation_idempotent: true },
      telemetry_config: ["deltas"],
    },
    active_checkpoint_id: null,
    context_revision: 1n,
    settings_revision: 1n,
    tool_set_revision: 1n,
    pending_interactions: [],
    command_availability: {
      [activeTurnId ? "turn_steer" : "turn_start"]: { status: "available" },
      turn_interrupt: { status: "available" },
      context_compact: { status: "available" },
    },
    transcript: [],
    transcript_fidelity: "platform_exact",
  };
}

describe("AgentRun conversation command state", () => {
  it("projects Runtime snapshot as the only runtime command authority", () => {
    const commandState = buildAgentRunConversationCommandState({
      modelConfig: resolvedModelConfig(),
      workspaceStateStatus: "ready",
      workspaceStateError: null,
      runtimeSnapshot: runtimeSnapshot(),
    });

    const model = projectAgentRunChatCommandState(commandState);

    expect(model.mode).toBe("runtime");
    expect(model.executionStatus).toBe("running_active");
    expect(model.keyboard).toEqual({
      enter: "runtime:turn_steer",
      ctrl_enter: undefined,
    });
    expect(model.primaryCommandId).toBe("runtime:turn_steer");
    expect(model.commands.find((command) => command.command_id === "runtime:turn_steer")).toMatchObject({
      delivery_intent: "steer",
    });
    expect(model.cancelCommand).toMatchObject({
      command_id: "runtime:turn_interrupt",
      kind: "cancel",
      enabled: true,
      executor_config_policy: "forbidden",
    });
    expect(model.modelConfig.status).toBe("resolved");
  });

  it("keeps product projection errors visible without fabricating model state", () => {
    const modelConfig = resolvedModelConfig();
    const commandState = buildAgentRunConversationCommandState({
      modelConfig,
      workspaceStateStatus: "error",
      workspaceStateError: "工作台状态加载失败",
      runtimeSnapshot: runtimeSnapshot(),
    });

    const model = projectAgentRunChatCommandState(commandState);

    expect(model.executionStatus).toBe("error");
    expect(model.commands).toEqual([]);
    expect(model.modelConfig).toMatchObject(modelConfig);
    expect(model.helperText).toBe("工作台状态加载失败");
  });

  it("uses canonical command availability for every Runtime control", () => {
    const snapshot = runtimeSnapshot();
    snapshot.command_availability = {
      turn_steer: {
        status: "unavailable",
        unmet: [{ kind: "lifecycle", capability: "turn_steer" }],
        reason: "bound profile does not support steer",
      },
      turn_interrupt: { status: "available" },
      context_compact: {
        status: "unavailable",
        unmet: [{ kind: "context", capability: "prepare_compaction", minimum_fidelity: "driver_exact" }],
        reason: "exact compaction is unavailable",
      },
    };
    const state = buildAgentRunConversationCommandState({
      modelConfig: resolvedModelConfig(),
      workspaceStateStatus: "ready",
      workspaceStateError: null,
      runtimeSnapshot: snapshot,
    });

    expect(state.commands.commands.find((item) => item.kind === "submit_message")).toMatchObject({
      enabled: false,
      unavailable_reason: "bound profile does not support steer",
    });
    expect(state.commands.commands.find((item) => item.kind === "cancel")?.enabled).toBe(true);
    expect(state.commands.commands.find((item) => item.kind === "compact_context")).toMatchObject({
      enabled: false,
      unavailable_reason: "exact compaction is unavailable",
    });
  });

  it("does not expose commands while the product projection is refreshing", () => {
    const commandState = buildAgentRunConversationCommandState({
      modelConfig: resolvedModelConfig(),
      workspaceStateStatus: "refreshing",
      workspaceStateError: null,
      runtimeSnapshot: runtimeSnapshot(),
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
        executor: "PI_AGENT",
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

    const ready = buildDraftConversationCommandState({
      projectId: "project-1",
      agentKey: "agent-key",
      agent,
      workspaceStateReady: true,
      explicitExecutorConfigOverride: {
        executor: "PI_AGENT",
        provider_id: "openai",
        model_id: "gpt-test",
      },
    });
    const model = projectAgentRunChatCommandState(ready);

    expect(ready.executionStatus).toBe("draft");
    expect(model.primaryCommandId).toBe("draft:start_local:resolved");
    expect(model.modelConfig.effective_executor_config).toMatchObject({
      executor: "PI_AGENT",
      provider_id: "openai",
      model_id: "gpt-test",
      source: "user_override",
    });
  });

});
