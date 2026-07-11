import { useCallback, useRef } from "react";

import type { JsonValue } from "../../../generated/common-contracts";
import type { UserInput } from "../../../generated/backbone-protocol";
import type {
  AgentRunCommandOnlyRequest,
  ConversationCommandView,
  ConversationModelConfigView,
} from "../../../generated/workflow-contracts";
import type {
  AgentRunCommandPreconditionView,
  BackendSelectionRequestDto,
} from "../../../generated/agent-run-mailbox-contracts";
import type { ExecutorConfig } from "../../../services/executor";
import {
  cancelAgentRun,
  submitAgentRunComposerInput,
} from "../../../services/agentRunMailbox";
import type {
  CreateProjectAgentRunRequest,
  ProjectAgentRunStartResult,
} from "../../../types";
import type { ImageAttachment } from "../../session/ui/composer/useImageAttachments";
import {
  resolveAgentRunClientCommandId,
  type InFlightAgentRunCommand,
} from "./workspaceCommandState";
import type {
  AgentRunConversationCommand,
  AgentRunConversationCommandState,
} from "./conversationCommandState";
import {
  conversationCommandByKind,
  isLocalDraftStartAction,
} from "./conversationCommandState";

interface ResolveExecutorConfigInput {
  command: AgentRunConversationCommand;
  modelConfig: ConversationModelConfigView;
  explicitExecutorConfigOverride?: ExecutorConfig;
}

type ResolveExecutorConfig = (input: ResolveExecutorConfigInput) => ExecutorConfig | undefined;
type IsCompleteExecutorConfig = (config: ExecutorConfig | undefined) => boolean;
type CreateProjectAgentRun = (
  projectId: string,
  agentKey: string,
  payload: CreateProjectAgentRunRequest,
) => Promise<ProjectAgentRunStartResult>;

export interface UseAgentRunWorkspaceCommandsOptions {
  currentRunId: string | null;
  currentAgentId: string | null;
  chatCommandState: AgentRunConversationCommandState;
  draftProjectId: string | null;
  draftProjectAgentKey: string | null;
  draftReady: boolean;
  createProjectAgentRun: CreateProjectAgentRun;
  fetchAndIngestLifecycleRun: (runId: string) => Promise<unknown>;
  refreshWorkspaceState: () => Promise<unknown>;
  scheduleHookRuntimeRefresh: (reason: string, immediate?: boolean) => void;
  resolveExecutorConfig: ResolveExecutorConfig;
  isCompleteExecutorConfig: IsCompleteExecutorConfig;
  onDraftStarted: (response: ProjectAgentRunStartResult) => void;
}

export interface UseAgentRunWorkspaceCommandsResult {
  handleAgentRunCommand: (
    command: AgentRunConversationCommand,
    prompt: string,
    executorConfig?: ExecutorConfig,
    backendSelection?: BackendSelectionRequestDto,
    imageAttachments?: ImageAttachment[],
    deliveryIntent?: string,
  ) => Promise<void>;
  handleCancelAgentRun: () => Promise<void>;
}

class SilentCommandRefreshError extends Error {
  readonly silentCommandRefresh = true;

  constructor() {
    super("AgentRun workspace state refreshed.");
  }
}

function newClientCommandId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `cmd-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function commandPrecondition(command: ConversationCommandView): AgentRunCommandPreconditionView {
  return {
    command_id: command.command_id,
    command_kind: command.kind,
    stale_guard: command.stale_guard,
  };
}

function commandRequest(command: ConversationCommandView): AgentRunCommandOnlyRequest {
  return {
    command: commandPrecondition(command),
    client_command_id: newClientCommandId(),
  };
}

function apiErrorCode(error: unknown): string | null {
  if (!error || typeof error !== "object" || !("errorCode" in error)) return null;
  return typeof error.errorCode === "string" ? error.errorCode : null;
}

function isStaleAgentRunCommandError(error: unknown): boolean {
  return apiErrorCode(error) === "stale_command";
}

function executorConfigToJsonValue(config: ExecutorConfig | undefined): JsonValue | undefined {
  if (!config) return undefined;
  return {
    executor: config.executor,
    provider_id: config.provider_id,
    model_id: config.model_id,
    agent_id: config.agent_id,
    thinking_level: config.thinking_level,
    permission_policy: config.permission_policy,
  };
}

export function useAgentRunWorkspaceCommands(
  options: UseAgentRunWorkspaceCommandsOptions,
): UseAgentRunWorkspaceCommandsResult {
  const {
    currentRunId,
    currentAgentId,
    chatCommandState,
    draftProjectId,
    draftProjectAgentKey,
    draftReady,
    createProjectAgentRun,
    fetchAndIngestLifecycleRun,
    refreshWorkspaceState,
    scheduleHookRuntimeRefresh,
    resolveExecutorConfig,
    isCompleteExecutorConfig,
    onDraftStarted,
  } = options;
  const inFlightCommandRef = useRef<InFlightAgentRunCommand | null>(null);

  const refreshWorkspaceStateSilently = useCallback(() => {
    void refreshWorkspaceState().catch(() => {});
  }, [refreshWorkspaceState]);

  const refreshAfterStaleAgentRunCommandError = useCallback((error: unknown): boolean => {
    if (!isStaleAgentRunCommandError(error)) return false;
    refreshWorkspaceStateSilently();
    return true;
  }, [refreshWorkspaceStateSilently]);

  const handleAgentRunCommand = useCallback(async (
    command: AgentRunConversationCommand,
    prompt: string,
    executorConfig?: ExecutorConfig,
    backendSelection?: BackendSelectionRequestDto,
    imageAttachments?: ImageAttachment[],
    deliveryIntent?: string,
  ) => {
    const trimmed = prompt.trim();
    const hasImages = (imageAttachments?.length ?? 0) > 0;
    if (!trimmed && !hasImages) {
      throw new Error("请输入要发送的消息。");
    }
    if (!command.enabled) {
      throw new Error(command.unavailable_reason ?? "当前 AgentRun 不可执行该命令。");
    }

    const inputBlocks: UserInput[] = [];
    if (trimmed) {
      inputBlocks.push({ type: "text", text: trimmed, text_elements: [] });
    }
    if (imageAttachments) {
      for (const img of imageAttachments) {
        inputBlocks.push({ type: "image", url: img.dataUrl });
      }
    }

    const commandExecutorConfig = resolveExecutorConfig({
      command,
      modelConfig: chatCommandState.modelConfig,
      explicitExecutorConfigOverride: executorConfig,
    });
    if (
      chatCommandState.modelConfig.status === "model_required" &&
      !isCompleteExecutorConfig(commandExecutorConfig)
    ) {
      throw new Error(chatCommandState.modelConfig.message ?? "请选择模型配置后再发送。");
    }
    if (command.executor_config_policy === "required" && !commandExecutorConfig?.executor?.trim()) {
      throw new Error("请选择模型配置后再发送。");
    }

    const commandKey = JSON.stringify({
      command_id: command.command_id,
      kind: command.kind,
      stale_guard: isLocalDraftStartAction(command) ? null : command.stale_guard,
      input: inputBlocks,
      executor_config: commandExecutorConfig ?? null,
      backend_selection: backendSelection ?? null,
    });
    const resolvedCommand = resolveAgentRunClientCommandId(
      inFlightCommandRef.current,
      commandKey,
      newClientCommandId,
    );
    inFlightCommandRef.current = resolvedCommand.inFlightCommand;

    try {
      if (isLocalDraftStartAction(command)) {
        if (!draftProjectId || !draftProjectAgentKey || !draftReady) {
          throw new Error(command.unavailable_reason ?? "当前 Draft 尚未就绪。");
        }
        const response = await createProjectAgentRun(draftProjectId, draftProjectAgentKey, {
          input: inputBlocks,
          client_command_id: resolvedCommand.clientCommandId,
          executor_config: executorConfigToJsonValue(commandExecutorConfig),
          backend_selection: backendSelection,
        });
        void fetchAndIngestLifecycleRun(response.run_ref.run_id);
        onDraftStarted(response);
        return;
      }

      if (!currentRunId || !currentAgentId) {
        throw new Error("当前 AgentRun 尚未就绪，无法执行控制动作。");
      }

      const response = await submitAgentRunComposerInput(currentRunId, currentAgentId, {
        input: inputBlocks,
        client_command_id: resolvedCommand.clientCommandId,
        command: commandPrecondition(command),
        executor_config: executorConfigToJsonValue(commandExecutorConfig),
        backend_selection: backendSelection,
        delivery_intent: deliveryIntent,
      });
      if (response.accepted_refs?.run_ref.run_id) {
        void fetchAndIngestLifecycleRun(response.accepted_refs.run_ref.run_id);
      }
      refreshWorkspaceStateSilently();
      scheduleHookRuntimeRefresh("agent_run_command_submitted", true);
    } catch (error) {
      if (refreshAfterStaleAgentRunCommandError(error)) {
        throw new SilentCommandRefreshError();
      }
      throw error;
    } finally {
      inFlightCommandRef.current = null;
    }
  }, [
    chatCommandState.modelConfig,
    createProjectAgentRun,
    currentAgentId,
    currentRunId,
    draftProjectAgentKey,
    draftProjectId,
    draftReady,
    fetchAndIngestLifecycleRun,
    isCompleteExecutorConfig,
    onDraftStarted,
    refreshAfterStaleAgentRunCommandError,
    refreshWorkspaceStateSilently,
    resolveExecutorConfig,
    scheduleHookRuntimeRefresh,
  ]);

  const handleCancelAgentRun = useCallback(async () => {
    if (!currentRunId || !currentAgentId) {
      throw new Error("当前 AgentRun 尚未就绪。");
    }
    const cancelCommand = conversationCommandByKind(chatCommandState.commands.commands, "cancel");
    if (!cancelCommand?.enabled) {
      throw new Error(cancelCommand?.unavailable_reason ?? "当前 AgentRun 没有可取消的运行。");
    }
    try {
      await cancelAgentRun(currentRunId, currentAgentId, commandRequest(cancelCommand));
    } catch (error) {
      if (refreshAfterStaleAgentRunCommandError(error)) return;
      throw error;
    }
    refreshWorkspaceStateSilently();
    scheduleHookRuntimeRefresh("agent_run_cancelled", true);
  }, [
    chatCommandState.commands.commands,
    currentAgentId,
    currentRunId,
    refreshAfterStaleAgentRunCommandError,
    refreshWorkspaceStateSilently,
    scheduleHookRuntimeRefresh,
  ]);

  return {
    handleAgentRunCommand,
    handleCancelAgentRun,
  };
}
