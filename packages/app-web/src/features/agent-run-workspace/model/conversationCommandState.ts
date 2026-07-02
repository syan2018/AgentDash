import type {
  ConversationCommandSetView,
  ConversationCommandView,
  ConversationMailboxSnapshotView,
  ConversationModelConfigView,
} from "../../../generated/workflow-contracts";
import type { ConversationEffectiveExecutorConfigView } from "../../../generated/project-agent-contracts";
import type { ProjectAgentSummary } from "../../../types";
import type {
  SessionChatCommandModel,
  SessionChatCommandState,
  SessionChatMailboxModel,
  SessionChatModelConfig,
} from "../../session";
import type { ExecutorConfig } from "../../../services/executor";

export interface LocalDraftStartAction {
  source: "local_draft";
  kind: "draft_start_local";
  command_id: string;
  enabled: boolean;
  unavailable_reason?: string;
  disabled_code?: string;
  shortcut: "enter";
  requires_input: true;
  executor_config_policy: "required";
}

export type AgentRunSessionCommand = ConversationCommandView | LocalDraftStartAction;

export interface AgentRunSessionCommandState {
  mode: "draft" | "runtime";
  executionStatus: string;
  commands: ConversationCommandSetView;
  localDraftAction?: LocalDraftStartAction;
  modelConfig: ConversationModelConfigView;
  helperText?: string;
}

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

export function isLocalDraftStartAction(command: AgentRunSessionCommand): command is LocalDraftStartAction {
  return command.kind === "draft_start_local";
}

export function resolveExecutorConfigForConversationCommand(input: {
  command: AgentRunSessionCommand;
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
}): LocalDraftStartAction {
  const missingDraft = !input.projectId || !input.agentKey || !input.agent;
  const unavailableReason = missingDraft
    ? "当前 Draft 尚未就绪。"
    : input.modelConfig.status === "model_required"
      ? input.modelConfig.message ?? "请选择模型配置后再发送。"
      : input.projectionReady
        ? undefined
        : "当前 Draft 正在加载。";

  return {
    source: "local_draft",
    kind: "draft_start_local",
    command_id: input.modelConfig.status === "resolved" ? "draft:start_local:resolved" : "draft:start_local:model_required",
    enabled: !unavailableReason,
    unavailable_reason: unavailableReason,
    disabled_code: unavailableReason ? (input.modelConfig.status === "model_required" ? "model_required" : "command_unavailable") : undefined,
    requires_input: true,
    executor_config_policy: "required",
    shortcut: "enter",
  };
}

export function buildDraftSessionCommandState(input: {
  projectId: string | null;
  agentKey: string | null;
  agent: ProjectAgentSummary | null;
  projectionReady: boolean;
  explicitExecutorConfigOverride?: ExecutorConfig | null;
}): AgentRunSessionCommandState {
  const modelConfig = modelConfigForDraft({
    agent: input.agent,
    explicitExecutorConfigOverride: input.explicitExecutorConfigOverride,
  });
  const command = draftStartCommand({ ...input, modelConfig });
  return {
    mode: "draft",
    executionStatus: modelConfig.status === "model_required" ? "model_required" : "draft",
    commands: emptyCommandSet(),
    localDraftAction: command,
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
}): AgentRunSessionCommandState {
  if (!input.conversation) {
    const reason = input.projectionStatus !== "ready"
      ? input.projectionError ?? "当前 AgentRun 工作台投影正在刷新。"
      : "当前 AgentRun 尚未返回 conversation snapshot。";
    return {
      mode: "runtime",
      executionStatus: input.projectionStatus !== "ready" ? input.projectionStatus : "delivery_missing",
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

function normalizeExecutorConfigPolicy(value: string): SessionChatCommandModel["executor_config_policy"] {
  if (value === "required" || value === "forbidden") return value;
  return "optional";
}

function normalizeShortcut(value: string | undefined): SessionChatCommandModel["shortcut"] {
  if (value === "enter" || value === "ctrl_enter") return value;
  return undefined;
}

function projectCommand(command: AgentRunSessionCommand): SessionChatCommandModel {
  return {
    command_id: command.command_id,
    kind: command.kind,
    enabled: command.enabled,
    unavailable_reason: command.unavailable_reason,
    disabled_code: command.disabled_code,
    requires_input: command.requires_input,
    executor_config_policy: normalizeExecutorConfigPolicy(command.executor_config_policy),
    shortcut: normalizeShortcut(command.shortcut),
  };
}

function projectModelConfig(modelConfig: ConversationModelConfigView): SessionChatModelConfig {
  return {
    status: modelConfig.status,
    effective_executor_config: modelConfig.effective_executor_config,
    missing_fields: modelConfig.missing_fields,
    message: modelConfig.message,
  };
}

export function conversationCommandByKind(
  commands: ConversationCommandView[],
  kind: ConversationCommandView["kind"],
): ConversationCommandView | undefined {
  return commands.find((command) => command.kind === kind);
}

export function mailboxRowCommand(
  commands: ConversationCommandView[],
  kind: ConversationCommandView["kind"],
): ConversationCommandView | undefined {
  return commands.find((command) => command.kind === kind && command.placement.includes("mailbox_row"));
}

export function projectSessionChatCommandState(
  commandState: AgentRunSessionCommandState,
): SessionChatCommandState {
  const runtimeCommands = commandState.commands.commands.map(projectCommand);
  const commands = commandState.localDraftAction
    ? [projectCommand(commandState.localDraftAction), ...runtimeCommands]
    : runtimeCommands;
  const enter = commandState.localDraftAction?.command_id ?? commandState.commands.keyboard.enter;
  const primaryCommandId =
    enter
    ?? runtimeCommands.find((command) => command.kind === "submit_message" && command.enabled)?.command_id
    ?? runtimeCommands.find((command) => command.kind === "submit_message")?.command_id;
  const cancelCommand = conversationCommandByKind(commandState.commands.commands, "cancel");

  return {
    mode: commandState.mode,
    executionStatus: commandState.executionStatus,
    commands,
    keyboard: {
      enter,
      ctrl_enter: commandState.commands.keyboard.ctrl_enter,
    },
    primaryCommandId,
    cancelCommand: cancelCommand ? projectCommand(cancelCommand) : undefined,
    modelConfig: projectModelConfig(commandState.modelConfig),
    helperText: commandState.helperText,
  };
}

export function projectSessionChatMailboxModel(
  commandState: AgentRunSessionCommandState,
  mailbox: ConversationMailboxSnapshotView | null | undefined,
): SessionChatMailboxModel {
  const promoteCommand = mailboxRowCommand(commandState.commands.commands, "promote_mailbox_message");
  const deleteCommand = mailboxRowCommand(commandState.commands.commands, "delete_mailbox_message");

  return {
    messages: mailbox?.messages ?? [],
    waiting_items: mailbox?.waiting_items ?? [],
    state: mailbox?.state,
    paused: Boolean(mailbox?.paused || mailbox?.state?.paused),
    user_attention: Boolean(mailbox?.user_attention),
    hide_system_steer_messages: Boolean(mailbox?.state?.hide_system_steer_messages),
    can_resume: Boolean(mailbox?.state?.can_resume),
    resumeAction: mailbox?.resume_command ? projectCommand(mailbox.resume_command) : undefined,
    promoteAction: promoteCommand ? projectCommand(promoteCommand) : undefined,
    deleteAction: deleteCommand ? projectCommand(deleteCommand) : undefined,
  };
}
