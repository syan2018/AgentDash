import { useCallback, useEffect, useMemo, useState } from "react";

import type { ExecutorConfig } from "../../../services/executor";
import type { BackboneEvent } from "../../../generated/backbone-protocol";
import { subscribeProjectEvents } from "../../../stores/eventStore";
import { useLifecycleStore } from "../../../stores/lifecycleStore";
import type {
  AgentRunWorkspaceView,
  CreateProjectAgentRunRequest,
  ProjectAgentRunStartResult,
  ProjectAgentSummary,
} from "../../../types";
import type { TaskSessionExecutorSummary } from "../../../types/context";
import {
  connectAgentRunTerminalFeed,
  projectAgentRunTerminalChanges,
  projectAgentRunTerminalSnapshot,
} from "../../agent-run-product-projections";
import type {
  AgentRunWorkspaceState,
} from "../../workspace-panel/model/useAgentRunWorkspaceState";
import { isWorkspaceModulePresentationCurrent } from "../../workspace-module/model/presentation";
import {
  planAgentRunLiveEvent,
  planAgentRunProjectEvent,
  resolveAgentRunSubmitCommand,
  type AgentRunControlPlaneEffectPlan,
  type AgentRunWorkspacePanelTarget,
} from "./controlPlaneModel";
import {
  type AgentRunChatModel,
  type AgentRunChatSubmitIntent,
  type AgentRunChatViewIntents,
  buildAgentRunConversationCommandState,
  buildDraftConversationCommandState,
  conversationCommandByKind,
  isCompleteExecutorConfig,
  projectAgentRunChatCommandState,
  resolveExecutorConfigForConversationCommand,
} from "./conversationCommandState";
import { useAgentRunWorkspaceCommands } from "./useAgentRunWorkspaceCommands";

export interface UseAgentRunWorkspaceControlPlaneOptions {
  currentRunId: string | null;
  currentAgentId: string | null;
  draftProjectId: string | null;
  draftProjectAgentKey: string | null;
  draftProjectAgent: ProjectAgentSummary | null;
  isProjectAgentDraft: boolean;
  agentRunWorkspaceState: AgentRunWorkspaceState;
  refreshAgentRunWorkspaceState: () => Promise<AgentRunWorkspaceView | null>;
  traceExecutorHint?: string | null;
  taskExecutorSummary?: TaskSessionExecutorSummary | null;
  createProjectAgentRun: (
    projectId: string,
    agentKey: string,
    payload: CreateProjectAgentRunRequest,
  ) => Promise<ProjectAgentRunStartResult>;
  onDraftStarted: (
    response: ProjectAgentRunStartResult,
    initialSubmit: Omit<AgentRunChatSubmitIntent, "command_id">,
  ) => void;
  onAgentRunRedirect: (target: { runId: string; agentId: string }) => void;
  refreshAgentRunList: (reason: string) => void;
  openWorkspacePanel: (target: AgentRunWorkspacePanelTarget) => void;
}

interface UseAgentRunWorkspaceControlPlaneResult {
  workspaceControl: AgentRunWorkspaceView | null;
  chatModel: AgentRunChatModel;
  chatIntents: AgentRunChatViewIntents;
  handleAgentRunLiveEvent: (event: BackboneEvent) => void;
}

export interface AgentRunControlPlaneEffectExecutor {
  refreshAgentRunWorkspaceState: () => Promise<AgentRunWorkspaceView | null>;
  openWorkspacePanel: (target: AgentRunWorkspacePanelTarget) => void;
  refreshAgentRunList: (reason: string) => void;
  workspacePanelOpened?: () => void;
  workspacePanelOpenFailed?: (error: Error) => void;
}

export function applyAgentRunControlPlaneEffectPlan(
  plan: AgentRunControlPlaneEffectPlan,
  executor: AgentRunControlPlaneEffectExecutor,
): void {
  const openPlan = plan.openWorkspacePanel;
  if (openPlan?.afterWorkspaceRefresh) {
    void (async () => {
      const workspace = plan.refreshWorkspaceState
        ? await executor.refreshAgentRunWorkspaceState().catch((error: unknown) => {
            executor.workspacePanelOpenFailed?.(
              error instanceof Error ? error : new Error("Workspace 刷新失败"),
            );
            return null;
          })
        : null;
      if (
        !workspace
        || !isWorkspaceModulePresentationCurrent(
          openPlan.presentation,
          workspace.workspace_modules,
        )
      ) {
        if (workspace) {
          executor.workspacePanelOpenFailed?.(
            new Error("Workspace presentation currentness fence 尚未生效"),
          );
        }
        return;
      }
      try {
        executor.openWorkspacePanel(openPlan.target);
        executor.workspacePanelOpened?.();
      } catch (error) {
        executor.workspacePanelOpenFailed?.(
          error instanceof Error ? error : new Error("Workspace 面板打开失败"),
        );
      }
    })();
  } else {
    if (plan.refreshWorkspaceState) {
      void executor.refreshAgentRunWorkspaceState().catch(() => {});
    }
    if (openPlan) {
      executor.openWorkspacePanel(openPlan.target);
      executor.workspacePanelOpened?.();
    }
  }

  if (plan.refreshAgentRunListReason) {
    executor.refreshAgentRunList(plan.refreshAgentRunListReason);
  }
}

export function useAgentRunWorkspaceControlPlane({
  currentRunId,
  currentAgentId,
  draftProjectId,
  draftProjectAgentKey,
  draftProjectAgent,
  isProjectAgentDraft,
  agentRunWorkspaceState,
  refreshAgentRunWorkspaceState,
  traceExecutorHint,
  taskExecutorSummary = null,
  createProjectAgentRun,
  onDraftStarted,
  onAgentRunRedirect,
  refreshAgentRunList,
  openWorkspacePanel,
}: UseAgentRunWorkspaceControlPlaneOptions): UseAgentRunWorkspaceControlPlaneResult {
  const fetchAndIngestLifecycleRun = useLifecycleStore((state) => state.fetchAndIngestLifecycleRun);
  const [explicitExecutorConfigOverrideState, setExplicitExecutorConfigOverrideState] = useState<{
    scopeKey: string | null;
    config: ExecutorConfig | null;
  }>({ scopeKey: null, config: null });

  const workspaceControl = agentRunWorkspaceState.workspace;

  const executorOverrideScopeKey = isProjectAgentDraft
    ? `draft:${draftProjectId ?? ""}:${draftProjectAgentKey ?? ""}`
    : currentRunId && currentAgentId
      ? `agentrun:${currentRunId}:${currentAgentId}`
      : null;
  const explicitExecutorConfigOverride =
    explicitExecutorConfigOverrideState.scopeKey === executorOverrideScopeKey
      ? explicitExecutorConfigOverrideState.config
      : null;

  const setExplicitExecutorConfigOverride = useCallback((config: ExecutorConfig | null) => {
    setExplicitExecutorConfigOverrideState({
      scopeKey: executorOverrideScopeKey,
      config,
    });
  }, [executorOverrideScopeKey]);

  const executorStateKey = useMemo(() => {
    if (isProjectAgentDraft) {
      return draftProjectId && draftProjectAgentKey
        ? `draft:${draftProjectId}:${draftProjectAgentKey}`
        : null;
    }
    if (!currentRunId || !currentAgentId) return null;
    const frameId = workspaceControl?.frame_runtime?.frame_ref.frame_id ?? "pending";
    return `agentrun:${currentRunId}:${currentAgentId}:${frameId}`;
  }, [
    currentAgentId,
    currentRunId,
    draftProjectAgentKey,
    draftProjectId,
    isProjectAgentDraft,
    workspaceControl?.frame_runtime?.frame_ref.frame_id,
  ]);

  const executorHint = draftProjectAgent?.executor.executor
    ?? traceExecutorHint
    ?? null;

  const commandState = useMemo(
    () => isProjectAgentDraft
      ? buildDraftConversationCommandState({
          projectId: draftProjectId,
          agentKey: draftProjectAgentKey,
          agent: draftProjectAgent,
          workspaceStateReady: Boolean(draftProjectId && draftProjectAgentKey && draftProjectAgent),
          explicitExecutorConfigOverride,
        })
      : buildAgentRunConversationCommandState({
          conversation: workspaceControl?.conversation,
          workspaceStateStatus: agentRunWorkspaceState.status,
          workspaceStateError: agentRunWorkspaceState.error,
        }),
    [
      agentRunWorkspaceState.error,
      agentRunWorkspaceState.status,
      draftProjectAgent,
      draftProjectAgentKey,
      draftProjectId,
      explicitExecutorConfigOverride,
      isProjectAgentDraft,
      workspaceControl?.conversation,
    ],
  );

  const {
    handleAgentRunCommand,
    handleCancelAgentRun,
    handleForkFromMessageRef,
  } = useAgentRunWorkspaceCommands({
    currentRunId,
    currentAgentId,
    chatCommandState: commandState,
    draftProjectId,
    draftProjectAgentKey,
    draftReady: Boolean(draftProjectId && draftProjectAgentKey && draftProjectAgent),
    createProjectAgentRun,
    fetchAndIngestLifecycleRun,
    refreshWorkspaceState: refreshAgentRunWorkspaceState,
    onAgentRunRedirect,
    resolveExecutorConfig: resolveExecutorConfigForConversationCommand,
    isCompleteExecutorConfig,
    onDraftStarted,
  });

  const submitComposer = useCallback(async (intent: AgentRunChatSubmitIntent) => {
    const resolution = resolveAgentRunSubmitCommand(commandState, intent);
    if (!resolution.ok) {
      throw new Error(resolution.message);
    }
    await handleAgentRunCommand(
      resolution.command,
      intent.prompt,
      intent.executorConfig,
      intent.backendSelection,
      intent.imageAttachments,
      intent.deliveryIntent,
    );
  }, [commandState, handleAgentRunCommand]);

  const cancelAction = useCallback(async () => {
    await handleCancelAgentRun();
  }, [handleCancelAgentRun]);

  const chatModel = useMemo<AgentRunChatModel>(() => ({
    executorHint,
    agentDefaults: draftProjectAgent?.effective_executor_config
      ?? workspaceControl?.conversation?.model_config.effective_executor_config
      ?? taskExecutorSummary,
    executorStateKey,
    commandState: projectAgentRunChatCommandState(commandState),
    compactContextCommand: conversationCommandByKind(commandState.commands.commands, "compact_context"),
    waitingItems: workspaceControl?.conversation?.waiting_items ?? [],
    statusBarRunId: currentRunId,
    statusBarAgentId: currentAgentId,
  }), [
    commandState,
    currentAgentId,
    currentRunId,
    draftProjectAgent?.effective_executor_config,
    executorHint,
    executorStateKey,
    workspaceControl?.conversation?.waiting_items,
    workspaceControl?.conversation?.model_config.effective_executor_config,
    taskExecutorSummary,
  ]);

  const chatIntents = useMemo<AgentRunChatViewIntents>(() => ({
    submitComposer,
    cancelAction,
    setExecutorConfigOverride: setExplicitExecutorConfigOverride,
    forkFromMessageRef: handleForkFromMessageRef,
  }), [
    cancelAction,
    handleForkFromMessageRef,
    setExplicitExecutorConfigOverride,
    submitComposer,
  ]);

  const applyControlPlaneEffectPlan = useCallback((
    plan: AgentRunControlPlaneEffectPlan,
    workspacePanelOpened?: () => void,
    workspacePanelOpenFailed?: (error: Error) => void,
  ) => {
    applyAgentRunControlPlaneEffectPlan(plan, {
      refreshAgentRunWorkspaceState,
      openWorkspacePanel,
      refreshAgentRunList,
      workspacePanelOpened,
      workspacePanelOpenFailed,
    });
  }, [
    openWorkspacePanel,
    refreshAgentRunList,
    refreshAgentRunWorkspaceState,
  ]);

  const handleAgentRunLiveEvent = useCallback((event: BackboneEvent) => {
    applyControlPlaneEffectPlan(planAgentRunLiveEvent(event).effects);
  }, [applyControlPlaneEffectPlan]);

  useEffect(() => {
    if (!currentRunId || !currentAgentId) return;
    return subscribeProjectEvents((event) => {
      applyControlPlaneEffectPlan(
        planAgentRunProjectEvent(event, {
          runId: currentRunId,
          agentId: currentAgentId,
        }),
      );
    });
  }, [
    applyControlPlaneEffectPlan,
    currentAgentId,
    currentRunId,
  ]);

  useEffect(() => {
    if (!currentRunId || !currentAgentId) return;
    const target = { runId: currentRunId, agentId: currentAgentId };
    const terminalFeed = connectAgentRunTerminalFeed(target, {
      onSnapshot: projectAgentRunTerminalSnapshot,
      onChanges: projectAgentRunTerminalChanges,
    });
    return () => {
      terminalFeed.close();
    };
  }, [
    currentAgentId,
    currentRunId,
  ]);

  return {
    workspaceControl,
    chatModel,
    chatIntents,
    handleAgentRunLiveEvent,
  };
}
