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
  SessionChatModel,
  SessionChatSubmitIntent,
  SessionChatViewIntents,
} from "../../session";
import type { AgentRunWorkspaceProjectionState } from "../../workspace-panel/model/useAgentRunWorkspaceState";
import {
  planAgentRunMessageSent,
  planAgentRunSystemEvent,
  planAgentRunTurnEnd,
  planAgentRunWorkspaceModuleOpened,
  resolveAgentRunSubmitCommand,
  type AgentRunControlPlaneEffectPlan,
  type AgentRunWorkspacePanelTarget,
} from "./controlPlaneModel";
import {
  buildDraftSessionCommandState,
  buildRuntimeSessionCommandState,
  isCompleteExecutorConfig,
  projectSessionChatCommandState,
  projectSessionChatMailboxModel,
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
  agentRunWorkspaceState: AgentRunWorkspaceProjectionState;
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
  refreshAgentRunList: (reason: string) => void;
  refreshWorkspaceModuleCatalog: () => void;
  openWorkspacePanel: (target: AgentRunWorkspacePanelTarget) => void;
}

interface UseAgentRunWorkspaceControlPlaneResult {
  runtimeControl: AgentRunWorkspaceView | null;
  deliveryRuntimeSessionId: string | null;
  chatModel: SessionChatModel;
  chatIntents: SessionChatViewIntents;
  refreshAgentRunWorkspaceState: () => Promise<unknown>;
  refreshAgentRunHookRuntime: () => Promise<unknown>;
  handleMessageSent: () => void;
  handleTurnEnd: () => void;
  handleTaskPlanChanged: () => void;
  handleSystemEvent: (eventType: string, event: BackboneEvent) => void;
  handleWorkspaceModuleOpened: () => void;
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

  const runtimeControl = agentRunWorkspaceState.workspace;
  const deliveryRuntimeSessionId = agentRunWorkspaceState.runtime_session_id;

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
    const frameId = runtimeControl?.frame_runtime?.frame_ref.frame_id ?? "pending";
    return `agentrun:${currentRunId}:${currentAgentId}:${frameId}`;
  }, [
    currentAgentId,
    currentRunId,
    draftProjectAgentKey,
    draftProjectId,
    isProjectAgentDraft,
    runtimeControl?.frame_runtime?.frame_ref.frame_id,
  ]);

  const executorHint = draftProjectAgent?.executor.executor
    ?? traceExecutorHint
    ?? null;

  const commandState = useMemo(
    () => isProjectAgentDraft
      ? buildDraftSessionCommandState({
          projectId: draftProjectId,
          agentKey: draftProjectAgentKey,
          agent: draftProjectAgent,
          projectionReady: Boolean(draftProjectId && draftProjectAgentKey && draftProjectAgent),
          explicitExecutorConfigOverride,
        })
      : buildRuntimeSessionCommandState({
          conversation: runtimeControl?.conversation,
          projectionStatus: agentRunWorkspaceState.status,
          projectionError: agentRunWorkspaceState.error,
        }),
    [
      agentRunWorkspaceState.error,
      agentRunWorkspaceState.status,
      draftProjectAgent,
      draftProjectAgentKey,
      draftProjectId,
      explicitExecutorConfigOverride,
      isProjectAgentDraft,
      runtimeControl?.conversation,
    ],
  );

  const conversationMailbox = runtimeControl?.conversation?.mailbox;

  const {
    handleAgentRunCommand,
    handleCancelAgentRun,
    handlePromoteMailboxMessage,
    handleDeleteMailboxMessage,
    handleResumeMailbox,
    handleRecallMailboxMessage,
    handleMoveMailboxMessage,
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
    resolveExecutorConfig: resolveExecutorConfigForConversationCommand,
    isCompleteExecutorConfig,
    onDraftStarted,
  });

  const submitComposer = useCallback(async (intent: SessionChatSubmitIntent) => {
    const resolution = resolveAgentRunSubmitCommand(commandState, intent);
    if (!resolution.ok) {
      throw new Error(resolution.message);
    }
    await handleAgentRunCommand(
      resolution.command,
      intent.sessionId,
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

  const chatModel = useMemo<SessionChatModel>(() => ({
    sessionId: deliveryRuntimeSessionId,
    executorHint,
    agentDefaults: draftProjectAgent?.effective_executor_config
      ?? runtimeControl?.conversation?.model_config.effective_executor_config
      ?? taskExecutorSummary,
    executorStateKey,
    commandState: projectSessionChatCommandState(commandState),
    mailbox: projectSessionChatMailboxModel(commandState, conversationMailbox),
    statusBarRunId: currentRunId,
    statusBarAgentId: currentAgentId,
    injectedInputValue: recalledInput,
  }), [
    commandState,
    conversationMailbox,
    currentAgentId,
    currentRunId,
    deliveryRuntimeSessionId,
    draftProjectAgent?.effective_executor_config,
    executorHint,
    executorStateKey,
    recalledInput,
    runtimeControl?.conversation?.model_config.effective_executor_config,
    taskExecutorSummary,
  ]);

  const chatIntents = useMemo<SessionChatViewIntents>(() => ({
    submitComposer,
    cancelAction,
    setExecutorConfigOverride: setExplicitExecutorConfigOverride,
    promoteMailboxMessage,
    deleteMailboxMessage,
    resumeMailbox,
    recallMailboxMessage,
    moveMailboxMessage,
    injectedInputConsumed: clearRecalledInput,
  }), [
    cancelAction,
    clearRecalledInput,
    deleteMailboxMessage,
    moveMailboxMessage,
    promoteMailboxMessage,
    recallMailboxMessage,
    resumeMailbox,
    setExplicitExecutorConfigOverride,
    submitComposer,
  ]);

  const applyControlPlaneEffectPlan = useCallback((plan: AgentRunControlPlaneEffectPlan) => {
    const openPlan = plan.openWorkspacePanel;
    if (openPlan?.afterWorkspaceRefresh) {
      void (async () => {
        if (plan.refreshWorkspaceState) {
          await refreshAgentRunWorkspaceState().catch(() => {});
        }
        if (plan.refreshWorkspaceModuleCatalog) {
          refreshWorkspaceModuleCatalog();
        }
        openWorkspacePanel(openPlan.target);
      })();
    } else {
      if (plan.refreshWorkspaceState) {
        void refreshAgentRunWorkspaceState().catch(() => {});
      }
      if (plan.refreshWorkspaceModuleCatalog) {
        refreshWorkspaceModuleCatalog();
      }
      if (openPlan) {
        openWorkspacePanel(openPlan.target);
      }
    }

    if (plan.hookRuntimeRefresh) {
      scheduleHookRuntimeRefresh(
        plan.hookRuntimeRefresh.reason,
        plan.hookRuntimeRefresh.immediate,
      );
    }
    if (plan.refreshAgentRunListReason) {
      refreshAgentRunList(plan.refreshAgentRunListReason);
    }
  }, [
    openWorkspacePanel,
    refreshAgentRunList,
    refreshAgentRunWorkspaceState,
    refreshWorkspaceModuleCatalog,
    scheduleHookRuntimeRefresh,
  ]);

  const handleMessageSent = useCallback(() => {
    applyControlPlaneEffectPlan(planAgentRunMessageSent(deliveryRuntimeSessionId));
  }, [applyControlPlaneEffectPlan, deliveryRuntimeSessionId]);

  const refreshStatusBarTasks = useCallback(() => {
    if (currentRunId && currentAgentId) {
      void useTaskPlanStore
        .getState()
        .fetchAgentRunTasks(currentRunId, currentAgentId)
        .catch(() => {});
    }
  }, [currentAgentId, currentRunId]);

  const handleTurnEnd = useCallback(() => {
    applyControlPlaneEffectPlan(planAgentRunTurnEnd());
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
    runtimeControl,
    deliveryRuntimeSessionId,
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
