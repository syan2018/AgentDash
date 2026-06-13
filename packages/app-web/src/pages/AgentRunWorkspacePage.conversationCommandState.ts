import type {
  ConversationCommandSetView,
  ConversationCommandView,
  ConversationModelConfigView,
  ConversationPendingSnapshotView,
} from "../generated/workflow-contracts";
import type { ProjectAgentSummary } from "../types";
import type { SessionChatCommandState } from "../features/session";
import type { ExecutorConfig } from "../services/executor";
import type { ConversationEffectiveExecutorConfigView } from "../generated/project-agent-contracts";

function emptyCommandSet(): ConversationCommandSetView {
  return {
    commands: [],
    keyboard: {},
  };
}

function optionalTrimmed(value: string | null | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function baseExecutorConfigForDraft(
  agent: ProjectAgentSummary | null,
): ConversationEffectiveExecutorConfigView | undefined {
  if (agent?.effective_executor_config) return agent.effective_executor_config;
  const executor = optionalTrimmed(agent?.executor.executor);
  if (!executor) return undefined;
  return {
    executor,
    provider_id: optionalTrimmed(agent?.executor.provider_id),
    model_id: optionalTrimmed(agent?.executor.model_id),
    agent_id: optionalTrimmed(agent?.executor.agent_id),
    thinking_level: optionalTrimmed(agent?.executor.thinking_level),
    permission_policy: optionalTrimmed(agent?.executor.permission_policy),
    source: "project_agent_preset",
  };
}

function effectiveExecutorConfigForDraft(input: {
  agent: ProjectAgentSummary | null;
  explicitExecutorConfigOverride?: ExecutorConfig | null;
}): ConversationEffectiveExecutorConfigView | undefined {
  const base = baseExecutorConfigForDraft(input.agent);
  const override = input.explicitExecutorConfigOverride;
  if (!override) return base;
  const executor = optionalTrimmed(override.executor) ?? base?.executor;
  if (!executor) return base;
  return {
    executor,
    provider_id: optionalTrimmed(override.provider_id) ?? base?.provider_id,
    model_id: optionalTrimmed(override.model_id) ?? base?.model_id,
    agent_id: optionalTrimmed(override.agent_id) ?? base?.agent_id,
    thinking_level: optionalTrimmed(override.thinking_level) ?? base?.thinking_level,
    permission_policy: optionalTrimmed(override.permission_policy) ?? base?.permission_policy,
    source: "user_override",
  };
}

export function isCompleteExecutorConfig(config: ExecutorConfig | undefined): boolean {
  return Boolean(
    config?.executor?.trim() &&
    config.provider_id?.trim() &&
    config.model_id?.trim(),
  );
}

export function executorConfigFromConversationModel(
  modelConfig: ConversationModelConfigView,
): ExecutorConfig | undefined {
  const effective = modelConfig.effective_executor_config;
  if (!effective?.executor.trim()) return undefined;
  return {
    executor: effective.executor,
    provider_id: effective.provider_id,
    model_id: effective.model_id,
    agent_id: effective.agent_id,
    thinking_level: effective.thinking_level as ExecutorConfig["thinking_level"],
    permission_policy: effective.permission_policy as ExecutorConfig["permission_policy"],
  };
}

export function resolveExecutorConfigForConversationCommand(input: {
  command: ConversationCommandView;
  modelConfig: ConversationModelConfigView;
  explicitExecutorConfigOverride?: ExecutorConfig;
}): ExecutorConfig | undefined {
  const effectiveConfig = executorConfigFromConversationModel(input.modelConfig);
  const overrideConfig = input.explicitExecutorConfigOverride;
  if (input.modelConfig.status === "model_required" && isCompleteExecutorConfig(overrideConfig)) {
    return overrideConfig;
  }
  if (input.command.executor_config_policy === "required") {
    return effectiveConfig ?? overrideConfig;
  }
  return overrideConfig ?? effectiveConfig;
}

function modelConfigForDraft(input: {
  agent: ProjectAgentSummary | null;
  explicitExecutorConfigOverride?: ExecutorConfig | null;
}): ConversationModelConfigView {
  const effective = effectiveExecutorConfigForDraft(input);
  const missingFields: string[] = [];
  if (!effective?.executor?.trim()) missingFields.push("executor");
  if (!effective?.provider_id?.trim()) missingFields.push("provider_id");
  if (!effective?.model_id?.trim()) missingFields.push("model_id");

  if (effective && missingFields.length === 0) {
    return {
      status: "resolved",
      effective_executor_config: effective,
      missing_fields: [],
    };
  }

  return {
    status: "model_required",
    effective_executor_config: effective,
    missing_fields: missingFields,
    message: "该 ProjectAgent 缺少可运行的模型配置，请先选择模型。",
  };
}

function draftStartCommand(input: {
  projectId: string | null;
  agentKey: string | null;
  agent: ProjectAgentSummary | null;
  projectionReady: boolean;
  modelConfig: ConversationModelConfigView;
}): ConversationCommandView {
  const missingDraft = !input.projectId || !input.agentKey || !input.agent;
  const unavailableReason = missingDraft
    ? "当前 Draft 尚未就绪。"
    : input.modelConfig.status === "model_required"
      ? input.modelConfig.message ?? "请选择模型配置后再发送。"
      : input.projectionReady
        ? undefined
        : "当前 Draft 正在加载。";

  return {
    kind: "start_draft",
    command_id: input.modelConfig.status === "resolved" ? "draft:start_draft:resolved" : "draft:start_draft:model_required",
    enabled: !unavailableReason,
    unavailable_reason: unavailableReason,
    disabled_code: unavailableReason ? (input.modelConfig.status === "model_required" ? "model_required" : "command_unavailable") : undefined,
    requires_input: true,
    executor_config_policy: "required",
    placement: ["composer_primary"],
    shortcut: "enter",
    stale_guard: {
      snapshot_id: input.modelConfig.status === "resolved"
        ? `draft:${input.projectId ?? "draft"}:${input.agentKey ?? "draft"}:resolved`
        : `draft:${input.projectId ?? "draft"}:${input.agentKey ?? "draft"}:model_required`,
      run_id: input.projectId ?? "draft",
      agent_id: input.agentKey ?? "draft",
    },
  };
}

export function buildDraftSessionCommandState(input: {
  projectId: string | null;
  agentKey: string | null;
  agent: ProjectAgentSummary | null;
  projectionReady: boolean;
  explicitExecutorConfigOverride?: ExecutorConfig | null;
}): SessionChatCommandState {
  const modelConfig = modelConfigForDraft({
    agent: input.agent,
    explicitExecutorConfigOverride: input.explicitExecutorConfigOverride,
  });
  const command = draftStartCommand({ ...input, modelConfig });
  const commands: ConversationCommandSetView = {
    commands: [command],
    keyboard: command.enabled
      ? {
          enter: command.command_id,
          ctrl_enter: command.command_id,
        }
      : {},
  };
  return {
    mode: "draft",
    executionStatus: modelConfig.status === "model_required" ? "model_required" : "draft",
    commands,
    modelConfig,
    helperText: command.unavailable_reason,
  };
}

export function buildRuntimeSessionCommandState(input: {
  conversation: {
    execution: { status: string; reason?: string };
    commands: ConversationCommandSetView;
    model_config: ConversationModelConfigView;
  } | null | undefined;
  projectionStatus: string;
  projectionError: string | null;
}): SessionChatCommandState {
  if (input.projectionStatus !== "ready") {
    const reason = input.projectionError ?? "当前 AgentRun 工作台投影正在刷新。";
    return {
      mode: "runtime",
      executionStatus: input.projectionStatus,
      commands: emptyCommandSet(),
      modelConfig: {
        status: "model_required",
        missing_fields: [],
        message: reason,
      },
      helperText: reason,
    };
  }

  if (!input.conversation) {
    const reason = "当前 AgentRun 尚未返回 conversation snapshot。";
    return {
      mode: "runtime",
      executionStatus: "delivery_missing",
      commands: emptyCommandSet(),
      modelConfig: {
        status: "model_required",
        missing_fields: [],
        message: reason,
      },
      helperText: reason,
    };
  }

  return {
    mode: "runtime",
    executionStatus: input.conversation.execution.status,
    commands: input.conversation.commands,
    modelConfig: input.conversation.model_config,
    helperText: input.conversation.execution.reason,
  };
}

export function pendingSnapshotFromConversation(
  pending: ConversationPendingSnapshotView | null | undefined,
): ConversationPendingSnapshotView | undefined {
  return pending ?? undefined;
}
