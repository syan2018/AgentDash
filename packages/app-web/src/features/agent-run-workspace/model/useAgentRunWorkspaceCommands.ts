import { useCallback, useRef } from "react";

import type { JsonValue } from "../../../generated/common-contracts";
import type {
  ConversationModelConfigView,
} from "../../../generated/workflow-contracts";
import type {
  AgentRunForkResponse,
  AgentRunMessageCommandResponse,
  BackendSelectionRequestDto,
  SessionMessageRefDto,
} from "../../../generated/agent-run-interaction-contracts";
import type { ExecutorConfig } from "../../../services/executor";
import { forkAgentRun } from "../../../services/agentRunInteraction";
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
  AgentRunChatSubmitIntent,
  AgentRunConversationCommand,
  AgentRunConversationCommandState,
} from "./conversationCommandState";
import { isLocalDraftStartAction } from "./conversationCommandState";

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
  onAgentRunRedirect: (target: { runId: string; agentId: string }) => void;
  resolveExecutorConfig: ResolveExecutorConfig;
  isCompleteExecutorConfig: IsCompleteExecutorConfig;
  onDraftStarted: (
    response: ProjectAgentRunStartResult,
    initialSubmit: Omit<AgentRunChatSubmitIntent, "command_id">,
  ) => void;
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
  handleForkFromMessageRef: (forkPointRef: SessionMessageRefDto) => Promise<void>;
}

function newClientCommandId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `cmd-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function resolveAgentRunCommandRedirect(
  response: AgentRunMessageCommandResponse,
): { runId: string; agentId: string } | null {
  const redirect = response.fork?.redirect;
  if (!redirect) return null;
  return {
    runId: redirect.run_id,
    agentId: redirect.agent_id,
  };
}

interface ForkAgentRunFromMessageRefInput {
  runId: string;
  agentId: string;
  forkPointRef: SessionMessageRefDto;
  clientCommandId: string;
  forkService: (
    runId: string,
    agentId: string,
    request: { client_command_id: string; fork_point_ref: SessionMessageRefDto },
  ) => Promise<AgentRunForkResponse>;
  fetchAndIngestLifecycleRun: (runId: string) => Promise<unknown>;
  onAgentRunRedirect: (target: { runId: string; agentId: string }) => void;
}

export async function forkAgentRunFromMessageRef({
  runId,
  agentId,
  forkPointRef,
  clientCommandId,
  forkService,
  fetchAndIngestLifecycleRun,
  onAgentRunRedirect,
}: ForkAgentRunFromMessageRefInput): Promise<void> {
  const response = await forkService(runId, agentId, {
    client_command_id: clientCommandId,
    fork_point_ref: forkPointRef,
  });
  void fetchAndIngestLifecycleRun(response.redirect.run_id);
  onAgentRunRedirect({
    runId: response.redirect.run_id,
    agentId: response.redirect.agent_id,
  });
}

function executorConfigToJsonValue(config: ExecutorConfig | undefined): JsonValue | undefined {
  if (!config) return undefined;
  return {
    executor: config.executor,
    provider_id: config.provider_id,
    model_id: config.model_id,
    agent_id: config.agent_id,
    thinking_level: config.thinking_level,
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
    onAgentRunRedirect,
    resolveExecutorConfig,
    isCompleteExecutorConfig,
    onDraftStarted,
  } = options;
  const inFlightCommandRef = useRef<InFlightAgentRunCommand | null>(null);

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
    if (isLocalDraftStartAction(command) && !command.enabled) {
      throw new Error(command.unavailable_reason ?? "当前 AgentRun 不可执行该命令。");
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
      input: {
        prompt: trimmed,
        images: imageAttachments?.map((image) => ({
          name: image.file.name,
          size: image.file.size,
          media_type: image.file.type,
          last_modified: image.file.lastModified,
        })) ?? [],
      },
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
      if (!isLocalDraftStartAction(command)) {
        throw new Error("既有 AgentRun 命令必须通过 AgentRuntimeConnection 执行。");
      }
      if (!draftProjectId || !draftProjectAgentKey || !draftReady) {
        throw new Error(command.unavailable_reason ?? "当前 Draft 尚未就绪。");
      }
      const response = await createProjectAgentRun(draftProjectId, draftProjectAgentKey, {
        client_command_id: resolvedCommand.clientCommandId,
        executor_config: executorConfigToJsonValue(commandExecutorConfig),
        backend_selection: backendSelection,
      });
      void fetchAndIngestLifecycleRun(response.run_ref.run_id);
      onDraftStarted(response, {
        prompt: trimmed,
        executorConfig: commandExecutorConfig,
        backendSelection,
        imageAttachments,
        deliveryIntent,
      });
    } finally {
      inFlightCommandRef.current = null;
    }
  }, [
    chatCommandState.modelConfig,
    createProjectAgentRun,
    draftProjectAgentKey,
    draftProjectId,
    draftReady,
    fetchAndIngestLifecycleRun,
    isCompleteExecutorConfig,
    onDraftStarted,
    resolveExecutorConfig,
  ]);

  const handleCancelAgentRun = useCallback(async () => {
    throw new Error("停止命令必须通过 AgentRuntimeConnection 执行。");
  }, []);

  const handleForkFromMessageRef = useCallback(async (forkPointRef: SessionMessageRefDto) => {
    if (!currentRunId || !currentAgentId) {
      throw new Error("当前 AgentRun 尚未就绪。");
    }
    await forkAgentRunFromMessageRef({
      runId: currentRunId,
      agentId: currentAgentId,
      forkPointRef,
      clientCommandId: newClientCommandId(),
      forkService: forkAgentRun,
      fetchAndIngestLifecycleRun,
      onAgentRunRedirect,
    });
  }, [
    currentAgentId,
    currentRunId,
    fetchAndIngestLifecycleRun,
    onAgentRunRedirect,
  ]);

  return {
    handleAgentRunCommand,
    handleCancelAgentRun,
    handleForkFromMessageRef,
  };
}
