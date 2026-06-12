import type {
  ConversationCommandSetView,
  ConversationCommandView,
  ConversationModelConfigView,
  ConversationPendingSnapshotView,
} from "../generated/workflow-contracts";
import type { ProjectAgentSummary } from "../types";
import type { SessionChatCommandState } from "../features/session";

function unavailableCommand(
  kind: ConversationCommandView["kind"],
  commandId: string,
  reason: string,
): ConversationCommandView {
  return {
    kind,
    command_id: commandId,
    enabled: false,
    unavailable_reason: reason,
    disabled_code: "command_unavailable",
    requires_input: true,
    executor_config_policy: "required",
    placement: ["composer_primary"],
    stale_guard: {
      run_id: commandId,
      agent_id: commandId,
    },
  };
}

function readonlyCommandSet(reason: string): ConversationCommandSetView {
  return {
    commands: [unavailableCommand("send_next", "readonly", reason)],
    keyboard: {},
  };
}

function modelConfigForDraft(agent: ProjectAgentSummary | null): ConversationModelConfigView {
  const effective = agent?.effective_executor_config;
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
}): SessionChatCommandState {
  const modelConfig = modelConfigForDraft(input.agent);
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
      commands: readonlyCommandSet(reason),
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
      commands: readonlyCommandSet(reason),
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
