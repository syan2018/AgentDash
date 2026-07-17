import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { ExecutorConfig } from "../../../services/executor";
import { useLifecycleStore } from "../../../stores/lifecycleStore";
import { useTaskPlanStore } from "../../../stores/taskPlanStore";
import type {
  AgentRunWorkspaceView,
  CreateProjectAgentRunRequest,
  ProjectAgentRunStartResult,
  ProjectAgentSummary,
} from "../../../types";
import type { TaskSessionExecutorSummary } from "../../../types/context";
import type {
  AgentRunWorkspaceState,
} from "../../workspace-panel/model/useAgentRunWorkspaceState";
import {
  planAgentRunMessageSent,
  planAgentRunSystemEvent,
  planAgentRunTurnEnded,
  planAgentRunWorkspaceModuleOpened,
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
  projectAgentRunChatMailboxModel,
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
  refreshAgentRunWorkspaceState: () => Promise<unknown>;
  refreshAgentRunHookRuntime: () => Promise<unknown>;
  traceExecutorHint?: string | null;
  taskExecutorSummary?: TaskSessionExecutorSummary | null;
  createProjectAgentRun: (
    projectId: string,
    agentKey: string,
    payload: CreateProjectAgentRunRequest,
  ) => Promise<ProjectAgentRunStartResult>;
  onDraftStarted: (response: ProjectAgentRunStartResult) => void;
  onAgentRunRedirect: (target: { runId: string; agentId: string }) => void;
  refreshAgentRunList: (reason: string) => void;
  refreshWorkspaceModuleCatalog: () => void;
  openWorkspacePanel: (target: AgentRunWorkspacePanelTarget) => void;
}

interface UseAgentRunWorkspaceControlPlaneResult {
  workspaceControl: AgentRunWorkspaceView | null;
  chatModel: AgentRunChatModel;
  chatIntents: AgentRunChatViewIntents;
  refreshAgentRunWorkspaceState: () => Promise<unknown>;
  refreshAgentRunHookRuntime: () => Promise<unknown>;
  handleMessageSent: () => void;
  handleTurnEnd: () => void;
  handleTaskPlanChanged: () => void;
  handleSystemEvent: (eventType: string, event: BackboneEvent) => void;
  handleWorkspaceModuleOpened: () => void;
}

export interface AgentRunControlPlaneEffectExecutor {
  refreshAgentRunWorkspaceState: () => Promise<unknown>;
  refreshWorkspaceModuleCatalog: () => void;
  openWorkspacePanel: (target: AgentRunWorkspacePanelTarget) => void;
  scheduleHookRuntimeRefresh: (reason: string, immediate?: boolean) => void;
  refreshAgentRunList: (reason: string) => void;
}

export function applyAgentRunControlPlaneEffectPlan(
  plan: AgentRunControlPlaneEffectPlan,
  executor: AgentRunControlPlaneEffectExecutor,
): void {
  const openPlan = plan.openWorkspacePanel;
  if (openPlan?.afterWorkspaceRefresh) {
    void (async () => {
      if (plan.refreshWorkspaceState) {
        await executor.refreshAgentRunWorkspaceState().catch(() => {});
      }
      if (plan.refreshWorkspaceModuleCatalog) {
        executor.refreshWorkspaceModuleCatalog();
      }
      executor.openWorkspacePanel(openPlan.target);
    })();
  } else {
    if (plan.refreshWorkspaceState) {
      void executor.refreshAgentRunWorkspaceState().catch(() => {});
    }
    if (plan.refreshWorkspaceModuleCatalog) {
      executor.refreshWorkspaceModuleCatalog();
    }
    if (openPlan) {
      executor.openWorkspacePanel(openPlan.target);
    }
  }

  if (plan.hookRuntimeRefresh) {
    executor.scheduleHookRuntimeRefresh(
      plan.hookRuntimeRefresh.reason,
      plan.hookRuntimeRefresh.immediate,
    );
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
  refreshAgentRunHookRuntime,
  traceExecutorHint,
  taskExecutorSummary = null,
  createProjectAgentRun,
  onDraftStarted,
  onAgentRunRedirect,
  refreshAgentRunList,
  refreshWorkspaceModuleCatalog,
  openWorkspacePanel,
}: UseAgentRunWorkspaceControlPlaneOptions): UseAgentRunWorkspaceControlPlaneResult {
  const fetchAndIngestLifecycleRun = useLifecycleStore((state) => state.fetchAndIngestLifecycleRun);
  const hookRuntimeRefreshTimerRef = useRef<number | null>(null);
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

  useEffect(() => {
    return () => {
      if (hookRuntimeRefreshTimerRef.current) {
        window.clearTimeout(hookRuntimeRefreshTimerRef.current);
        hookRuntimeRefreshTimerRef.current = null;
      }
    };
  }, []);

  const scheduleHookRuntimeRefresh = useCallback((_reason: string, immediate = false) => {
    if (!currentRunId || !currentAgentId) return;
    if (hookRuntimeRefreshTimerRef.current) {
      window.clearTimeout(hookRuntimeRefreshTimerRef.current);
      hookRuntimeRefreshTimerRef.current = null;
    }
    if (immediate) {
      void refreshAgentRunHookRuntime();
      return;
    }
    hookRuntimeRefreshTimerRef.current = window.setTimeout(() => {
      hookRuntimeRefreshTimerRef.current = null;
      void refreshAgentRunHookRuntime();
    }, 180);
  }, [currentAgentId, currentRunId, refreshAgentRunHookRuntime]);

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

  const conversationMailbox = workspaceControl?.conversation?.mailbox;

  const {
    handleAgentRunCommand,
    handleCancelAgentRun,
    handlePromoteMailboxMessage,
    handleDeleteMailboxMessage,
    handleResumeMailbox,
    handleRecallMailboxMessage,
    handleMoveMailboxMessage,
    handleForkFromMessageRef,
    recalledInput,
    clearRecalledInput,
  } = useAgentRunWorkspaceCommands({
    currentRunId,
    currentAgentId,
    chatCommandState: commandState,
    conversationMailbox,
    draftProjectId,
    draftProjectAgentKey,
    draftReady: Boolean(draftProjectId && draftProjectAgentKey && draftProjectAgent),
    createProjectAgentRun,
    fetchAndIngestLifecycleRun,
    refreshWorkspaceState: refreshAgentRunWorkspaceState,
    scheduleHookRuntimeRefresh,
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
    refreshAgentRunList("command_submitted");
  }, [commandState, handleAgentRunCommand, refreshAgentRunList]);

  const cancelAction = useCallback(async () => {
    await handleCancelAgentRun();
    refreshAgentRunList("agent_run_cancelled");
  }, [handleCancelAgentRun, refreshAgentRunList]);

  const promoteMailboxMessage = useCallback((messageId: string) => {
    void (async () => {
      await handlePromoteMailboxMessage(messageId);
      refreshAgentRunList("mailbox_message_promoted");
    })();
  }, [handlePromoteMailboxMessage, refreshAgentRunList]);

  const deleteMailboxMessage = useCallback((messageId: string) => {
    void (async () => {
      await handleDeleteMailboxMessage(messageId);
      refreshAgentRunList("mailbox_message_deleted");
    })();
  }, [handleDeleteMailboxMessage, refreshAgentRunList]);

  const resumeMailbox = useCallback(() => {
    void (async () => {
      await handleResumeMailbox();
      refreshAgentRunList("mailbox_resumed");
    })();
  }, [handleResumeMailbox, refreshAgentRunList]);

  const recallMailboxMessage = useCallback((messageId: string) => {
    void (async () => {
      await handleRecallMailboxMessage(messageId);
      refreshAgentRunList("mailbox_message_recalled");
    })();
  }, [handleRecallMailboxMessage, refreshAgentRunList]);

  const moveMailboxMessage = useCallback((messageId: string, afterMessageId: string | null) => {
    void (async () => {
      await handleMoveMailboxMessage(messageId, afterMessageId);
      refreshAgentRunList("mailbox_message_moved");
    })();
  }, [handleMoveMailboxMessage, refreshAgentRunList]);

  const chatModel = useMemo<AgentRunChatModel>(() => ({
    executorHint,
    agentDefaults: draftProjectAgent?.effective_executor_config
      ?? workspaceControl?.conversation?.model_config.effective_executor_config
      ?? taskExecutorSummary,
    executorStateKey,
    commandState: projectAgentRunChatCommandState(commandState),
    compactContextCommand: conversationCommandByKind(commandState.commands.commands, "compact_context"),
    mailbox: projectAgentRunChatMailboxModel(commandState, conversationMailbox),
    statusBarRunId: currentRunId,
    statusBarAgentId: currentAgentId,
    injectedInputValue: recalledInput,
  }), [
    commandState,
    conversationMailbox,
    currentAgentId,
    currentRunId,
    draftProjectAgent?.effective_executor_config,
    executorHint,
    executorStateKey,
    recalledInput,
    workspaceControl?.conversation?.model_config.effective_executor_config,
    taskExecutorSummary,
  ]);

  const chatIntents = useMemo<AgentRunChatViewIntents>(() => ({
    submitComposer,
    cancelAction,
    setExecutorConfigOverride: setExplicitExecutorConfigOverride,
    promoteMailboxMessage,
    deleteMailboxMessage,
    resumeMailbox,
    recallMailboxMessage,
    moveMailboxMessage,
    forkFromMessageRef: handleForkFromMessageRef,
    injectedInputConsumed: clearRecalledInput,
  }), [
    cancelAction,
    clearRecalledInput,
    deleteMailboxMessage,
    handleForkFromMessageRef,
    moveMailboxMessage,
    promoteMailboxMessage,
    recallMailboxMessage,
    resumeMailbox,
    setExplicitExecutorConfigOverride,
    submitComposer,
  ]);

  const applyControlPlaneEffectPlan = useCallback((plan: AgentRunControlPlaneEffectPlan) => {
    applyAgentRunControlPlaneEffectPlan(plan, {
      refreshAgentRunWorkspaceState,
      refreshWorkspaceModuleCatalog,
      openWorkspacePanel,
      scheduleHookRuntimeRefresh,
      refreshAgentRunList,
    });
  }, [
    openWorkspacePanel,
    refreshAgentRunList,
    refreshAgentRunWorkspaceState,
    refreshWorkspaceModuleCatalog,
    scheduleHookRuntimeRefresh,
  ]);

  const handleMessageSent = useCallback(() => {
    applyControlPlaneEffectPlan(planAgentRunMessageSent());
  }, [applyControlPlaneEffectPlan]);

  const refreshStatusBarTasks = useCallback(() => {
    if (currentRunId && currentAgentId) {
      void useTaskPlanStore
        .getState()
        .fetchAgentRunTasks(currentRunId, currentAgentId)
        .catch(() => {});
    }
  }, [currentAgentId, currentRunId]);

  const handleTurnEnd = useCallback(() => {
    applyControlPlaneEffectPlan(planAgentRunTurnEnded());
    refreshStatusBarTasks();
  }, [applyControlPlaneEffectPlan, refreshStatusBarTasks]);

  const handleTaskPlanChanged = useCallback(() => {
    refreshStatusBarTasks();
  }, [refreshStatusBarTasks]);

  const handleSystemEvent = useCallback((eventType: string, event: BackboneEvent) => {
    applyControlPlaneEffectPlan(planAgentRunSystemEvent(eventType, event));
  }, [applyControlPlaneEffectPlan]);

  const handleWorkspaceModuleOpened = useCallback(() => {
    applyControlPlaneEffectPlan(planAgentRunWorkspaceModuleOpened());
  }, [applyControlPlaneEffectPlan]);

  return {
    workspaceControl,
    chatModel,
    chatIntents,
    refreshAgentRunWorkspaceState,
    refreshAgentRunHookRuntime,
    handleMessageSent,
    handleTurnEnd,
    handleTaskPlanChanged,
    handleSystemEvent,
    handleWorkspaceModuleOpened,
  };
}
