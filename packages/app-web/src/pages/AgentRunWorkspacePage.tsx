/**
 * AgentRunWorkspacePage — AgentRun 交互工作台。
 *
 * 用户认知中 "AgentRun = 一个可继续交互的工作台"。此页面是用户点击 AgentRun 后的主视图，
 * 提供 Chat + Workspace Panel 双面板布局、标题编辑、上下文导航等完整交互。
 *
 * 底层数据通过 AgentRun workspace 投影驱动（`useAgentRunWorkspaceState`），
 * 不直接暴露 lifecycle 技术概念给用户。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Group, Panel, Separator, type PanelImperativeHandle } from "react-resizable-panels";
import type { BackboneEvent } from "../generated/backbone-protocol";
import { SessionChatView } from "../features/session";
import { extractPlatformEventData } from "../features/session/model/platformEvent";
import { useProjectExtensionRuntime } from "../features/extension-runtime";
import {
  executorSourceFromExecutionProfile,
  resolveAgentRunClientCommandId,
  type InFlightAgentRunCommand,
} from "../features/agent-run-workspace/model/workspaceCommandState";
import {
  WorkspacePanel,
  type WorkspacePanelHandle,
  type WorkspaceRuntimeData,
} from "../features/workspace-panel";
import { useAgentRunWorkspaceState } from "../features/workspace-panel/model/useAgentRunWorkspaceState";
import {
  cancelAgentRun,
  deleteAgentRunPendingMessage,
  enqueueAgentRunPendingMessage,
  promoteAgentRunPendingMessage,
  sendAgentRunMessage,
  steerAgentRun,
} from "../services/lifecycle";
import type { ExecutorConfig } from "../services/executor";
import type { JsonValue } from "../generated/common-contracts";
import type { UserInput } from "../generated/backbone-protocol";
import { useLifecycleStore } from "../stores/lifecycleStore";
import { useProjectStore } from "../stores/projectStore";
import { findStoryById, useStoryStore } from "../stores/storyStore";
import { findWorkspaceBinding, useWorkspaceStore } from "../stores/workspaceStore";
import { workspaceModulePresentedTabTarget } from "./AgentRunWorkspacePage.workspaceModulePresentation";
import type {
  RuntimeTraceAgentContext,
  SessionNavigationState,
  AgentRunWorkspaceView,
  SubjectRunContext,
  ProjectAgentSummary,
  Story,
  StoryNavigationState,
} from "../types";
import type {
  SessionChatControlState,
  SessionChatPrimaryActionKind,
} from "../features/session";
import type { ImageAttachment } from "../features/session/ui/composer/useImageAttachments";

// ─── AgentRunWorkspacePage ────────────────────────────────────────

interface AgentRunWorkspacePageProps {
  runId?: string;
  agentId?: string;
  draftProjectId?: string;
  draftProjectAgentId?: string;
}

function readonlyChatControlState(reason: string): SessionChatControlState {
  return {
    mode: "runtime",
    controlPlaneStatus: "unavailable",
    primaryAction: {
      kind: "none",
      enabled: false,
      label: "发送",
      placeholder: "当前 AgentRun 只能查看 runtime trace。",
      unavailableReason: reason,
    },
    cancelAction: {
      enabled: false,
      label: "取消",
      unavailableReason: "当前 AgentRun 没有正在执行的 turn。",
    },
    helperText: reason,
  };
}

function newClientCommandId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `cmd-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function AgentRunWorkspacePage({
  runId: propRunId,
  agentId: propAgentId,
  draftProjectId,
  draftProjectAgentId,
}: AgentRunWorkspacePageProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const selectProject = useProjectStore((state) => state.selectProject);
  const projects = useProjectStore((state) => state.projects);
  const agentsByProjectId = useProjectStore((state) => state.agentsByProjectId);
  const fetchProjectAgents = useProjectStore((state) => state.fetchProjectAgents);
  const createProjectAgentRun = useProjectStore((state) => state.createProjectAgentRun);
  const fetchAndIngestLifecycleRun = useLifecycleStore((state) => state.fetchAndIngestLifecycleRun);
  const fetchWorkspaces = useWorkspaceStore((state) => state.fetchWorkspaces);
  const workspacesByProjectId = useWorkspaceStore((state) => state.workspacesByProjectId);
  const hookRuntimeRefreshTimerRef = useRef<number | null>(null);
  const inFlightCommandRef = useRef<InFlightAgentRunCommand | null>(null);

  const [loadedOwnerStory, setLoadedOwnerStory] = useState<{
    story_id: string;
    story: Story | null;
  } | null>(null);
  const workspacePanelRef = useRef<WorkspacePanelHandle>(null);
  const rightPanelRef = useRef<PanelImperativeHandle>(null);

  const expandWorkspacePanel = useCallback((typeId?: string, uri?: string) => {
    if (typeId) {
      workspacePanelRef.current?.openTab(typeId, uri);
    }
    rightPanelRef.current?.expand();
  }, []);

  const toggleWorkspacePanel = useCallback(() => {
    const panel = rightPanelRef.current;
    if (!panel) return;
    if (panel.isCollapsed()) {
      panel.expand();
    } else {
      panel.collapse();
    }
  }, []);

  const routeState = useMemo(
    () => (location.state as SessionNavigationState | null) ?? null,
    [location.state],
  );
  const traceAgentContext = (routeState?.trace_agent ?? null) as RuntimeTraceAgentContext | null;
  const currentRunId = propRunId?.trim() || null;
  const currentAgentId = propAgentId?.trim() || null;
  const draftProjectAgentKey = !currentRunId ? draftProjectAgentId?.trim() || null : null;
  const draftProjectIdValue = !currentRunId ? draftProjectId?.trim() || null : null;
  const isProjectAgentDraft = Boolean(draftProjectIdValue && draftProjectAgentKey);
  const draftProjectAgent: ProjectAgentSummary | null = useMemo(() => {
    if (!draftProjectIdValue || !draftProjectAgentKey) return null;
    return (agentsByProjectId[draftProjectIdValue] ?? [])
      .find((agent) => agent.key === draftProjectAgentKey) ?? null;
  }, [agentsByProjectId, draftProjectAgentKey, draftProjectIdValue]);

  useEffect(() => {
    if (!draftProjectIdValue || currentRunId) return;
    if (agentsByProjectId[draftProjectIdValue]) return;
    void fetchProjectAgents(draftProjectIdValue);
  }, [agentsByProjectId, currentRunId, draftProjectIdValue, fetchProjectAgents]);

  useEffect(() => {
    return () => {
      if (hookRuntimeRefreshTimerRef.current) {
        window.clearTimeout(hookRuntimeRefreshTimerRef.current);
        hookRuntimeRefreshTimerRef.current = null;
      }
    };
  }, []);

  const agentRunSourceKey = currentRunId && currentAgentId
    ? `agentrun:${currentRunId}:${currentAgentId}`
    : null;

  const {
    state: agentRunWorkspaceState,
    refreshWorkspaceState: refreshAgentRunWorkspaceState,
    refreshHookRuntime: refreshAgentRunHookRuntime,
  } = useAgentRunWorkspaceState({
    runId: currentRunId,
    agentId: currentAgentId,
    sourceKey: agentRunSourceKey,
  });

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

  const runtimeControl: AgentRunWorkspaceView | null = agentRunWorkspaceState.workspace;
  const deliveryRuntimeSessionId = agentRunWorkspaceState.runtime_session_id;
  const draftWorkspaceTitle =
    draftProjectAgent?.display_name
    ?? traceAgentContext?.display_name
    ?? "新 AgentRun";
  const workspaceTitle = isProjectAgentDraft
    ? draftWorkspaceTitle
    : runtimeControl?.shell.display_title ?? "";
  const activeHookRuntime = agentRunWorkspaceState.hook_runtime?.runtime_adapter_session_id === deliveryRuntimeSessionId
    ? agentRunWorkspaceState.hook_runtime
    : null;
  const deliveryRuntimeSurface = agentRunWorkspaceState.runtime_surface;
  const sessionContextSnapshot = null;
  const sessionCapabilities = null;
  const taskExecutorSummary = null;

  const runContext: SubjectRunContext | null = activeHookRuntime?.snapshot?.run_context ?? null;
  const agentRunDetailRunId = runtimeControl?.run_ref.run_id ?? currentRunId;
  const agentRunDetailAgentId = runtimeControl?.agent_ref.agent_id ?? currentAgentId;
  const agentRunDetailFrameId = runtimeControl?.frame_runtime?.frame_ref.frame_id ?? null;
  const agentRunDetailTarget = useMemo(() => {
    if (!agentRunDetailRunId || !agentRunDetailAgentId) return null;
    return {
      runId: agentRunDetailRunId,
      agentId: agentRunDetailAgentId,
      frameId: agentRunDetailFrameId,
    };
  }, [agentRunDetailAgentId, agentRunDetailFrameId, agentRunDetailRunId]);

  const fetchStoryById = useStoryStore((s) => s.fetchStoryById);
  const storiesByProjectId = useStoryStore((s) => s.storiesByProjectId);
  const ownerStoryId = runContext?.story_id ?? null;

  useEffect(() => {
    const cached = ownerStoryId ? findStoryById(storiesByProjectId, ownerStoryId) : null;
    if (!ownerStoryId || cached) return;
    let cancelled = false;
    void (async () => {
      const result = await fetchStoryById(ownerStoryId);
      if (!cancelled) {
        setLoadedOwnerStory({
          story_id: ownerStoryId,
          story: result,
        });
      }
    })();
    return () => { cancelled = true; };
  }, [ownerStoryId, storiesByProjectId, fetchStoryById]);

  const ownerStory = useMemo(() => {
    if (!ownerStoryId) return null;
    const cached = findStoryById(storiesByProjectId, ownerStoryId);
    if (cached) return cached;
    if (loadedOwnerStory?.story_id === ownerStoryId) {
      return loadedOwnerStory.story;
    }
    return null;
  }, [loadedOwnerStory, ownerStoryId, storiesByProjectId]);
  const ownerProjectId = runtimeControl?.project_id
    ?? runContext?.project_id
    ?? ownerStory?.project_id
    ?? draftProjectIdValue
    ?? null;
  const ownerProject = ownerProjectId
    ? projects.find((project) => project.id === ownerProjectId) ?? null
    : null;
  const ownerProjectName = runContext?.scope === "project"
    ? (ownerProject?.name?.trim() || runContext.project_id)
    : isProjectAgentDraft
      ? (ownerProject?.name?.trim() || "")
    : "";
  const extensionRuntime = useProjectExtensionRuntime(ownerProjectId);

  useEffect(() => {
    if (!ownerProjectId) return;
    void fetchWorkspaces(ownerProjectId);
  }, [fetchWorkspaces, ownerProjectId]);

  const effectiveReturnTarget = useMemo(() => {
    if (isProjectAgentDraft && draftProjectIdValue) {
      return { owner_type: "project" as const, project_id: draftProjectIdValue };
    }
    if (!runContext) return null;
    if (runContext.scope === "project") {
      return { owner_type: "project" as const, project_id: runContext.project_id };
    }
    if (runContext.scope === "story" && runContext.story_id) {
      return { owner_type: "story" as const, story_id: runContext.story_id };
    }
    if (runContext.scope === "task" && runContext.story_id && runContext.task_id) {
      return { owner_type: "task" as const, story_id: runContext.story_id, task_id: runContext.task_id };
    }
    return null;
  }, [draftProjectIdValue, isProjectAgentDraft, runContext]);

  // ─── 页面级回调 ───────────────────────────────────────

  const executorHint = draftProjectAgent?.executor.executor
    ?? traceAgentContext?.executor_hint
    ?? null;
  const frameExecutorDefaults = useMemo(
    () => executorSourceFromExecutionProfile(runtimeControl?.frame_runtime?.execution_profile),
    [runtimeControl?.frame_runtime?.execution_profile],
  );
  const executorStateKey = useMemo(() => {
    if (isProjectAgentDraft) {
      return draftProjectIdValue && draftProjectAgentKey
        ? `draft:${draftProjectIdValue}:${draftProjectAgentKey}`
        : null;
    }
    if (!currentRunId || !currentAgentId) return null;
    const frameId = runtimeControl?.frame_runtime?.frame_ref.frame_id ?? "pending";
    return `agentrun:${currentRunId}:${currentAgentId}:${frameId}`;
  }, [
    currentAgentId,
    currentRunId,
    draftProjectAgentKey,
    draftProjectIdValue,
    isProjectAgentDraft,
    runtimeControl?.frame_runtime?.frame_ref.frame_id,
  ]);
  const chatWorkspaceId =
    ownerStory?.default_workspace_id
    ?? ownerProject?.config.default_workspace_id
    ?? null;
  const workspaceBackend = useMemo(() => {
    const ownerProjectWorkspaces = ownerProjectId ? workspacesByProjectId[ownerProjectId] ?? [] : [];
    const selectedWorkspace = chatWorkspaceId
      ? ownerProjectWorkspaces.find((workspace) => workspace.id === chatWorkspaceId) ?? null
      : ownerProjectWorkspaces[0] ?? null;
    if (!selectedWorkspace) return null;
    const binding = findWorkspaceBinding(selectedWorkspace);
    if (!binding) return null;
    return {
      backend_id: binding.backend_id,
      label: selectedWorkspace.name || binding.backend_id,
      online: binding.status !== "offline" && binding.status !== "error",
    };
  }, [chatWorkspaceId, ownerProjectId, workspacesByProjectId]);

  const handleMessageSent = useCallback(() => {
    if (!deliveryRuntimeSessionId) return;
    scheduleHookRuntimeRefresh("message_sent", true);
  }, [deliveryRuntimeSessionId, scheduleHookRuntimeRefresh]);

  const chatControlState = useMemo<SessionChatControlState>(() => {
    if (isProjectAgentDraft) {
      const enabled = Boolean(draftProjectIdValue && draftProjectAgentKey && draftProjectAgent);
      const unavailableReason = !draftProjectIdValue || !draftProjectAgentKey
        ? "Draft AgentRun 缺少 ProjectAgent 参数。"
        : !draftProjectAgent
          ? "正在加载 ProjectAgent 配置。"
          : undefined;
      return {
        mode: "draft",
        controlPlaneStatus: "draft",
        primaryAction: {
          kind: "start_draft",
          enabled,
          label: "开始",
          placeholder: "输入首条消息，Ctrl+Enter 开始…",
          unavailableReason,
        },
        cancelAction: {
          enabled: false,
          label: "取消",
          unavailableReason: "Draft AgentRun 尚未启动。",
        },
        helperText: enabled ? undefined : unavailableReason,
      };
    }

    if (!currentRunId || !currentAgentId) {
      return readonlyChatControlState("当前没有可控制的 AgentRun。");
    }
    if (
      agentRunWorkspaceState.status === "loading" ||
      (agentRunWorkspaceState.status === "refreshing" && !runtimeControl)
    ) {
      return readonlyChatControlState("正在解析当前 AgentRun 的工作台状态。");
    }
    if (agentRunWorkspaceState.error) {
      return readonlyChatControlState(agentRunWorkspaceState.error);
    }
    if (!runtimeControl) {
      return readonlyChatControlState("当前 AgentRun 工作台状态尚未返回。");
    }

    const actions = runtimeControl.actions;
    const isRunning = runtimeControl.control_plane.status === "running";

    // Running 态：主动作=enqueue（排队），辅助动作=steer
    if (isRunning && actions.enqueue.enabled) {
      const steerSecondary: SessionChatControlState["secondaryAction"] = actions.steer.enabled
        ? {
            kind: "steer" as const,
            enabled: true,
            label: "Steer",
            placeholder: "Ctrl+Enter 立即注入 steer 指令…",
            unavailableReason: undefined,
          }
        : undefined;

      return {
        mode: "runtime",
        controlPlaneStatus: runtimeControl.control_plane.status,
        primaryAction: {
          kind: "enqueue" as const,
          enabled: true,
          label: "排队",
          placeholder: steerSecondary
            ? "Enter 排队，Ctrl+Enter steer，@ 引用文件…"
            : "Enter 排队发送，@ 引用文件…",
          unavailableReason: undefined,
        },
        secondaryAction: steerSecondary,
        cancelAction: {
          enabled: actions.cancel.enabled,
          label: "取消",
          unavailableReason: actions.cancel.unavailable_reason,
        },
        helperText: runtimeControl.control_plane.reason ?? undefined,
      };
    }

    // Idle 态或非 running 态
    const primary = actions.send_next.enabled
      ? {
          kind: "send_next" as const,
          enabled: true,
          label: "发送",
          placeholder: "继续对话，@ 引用文件，Ctrl+Enter 发送…",
          unavailableReason: undefined,
        }
      : {
          kind: "none" as const,
          enabled: false,
          label: "发送",
          placeholder: isRunning
            ? "当前 AgentRun 正在执行，等待可用或取消。"
            : "当前 AgentRun 只能查看 runtime trace。",
          unavailableReason: isRunning
            ? actions.steer.unavailable_reason ?? runtimeControl.control_plane.reason
            : actions.send_next.unavailable_reason ?? runtimeControl.control_plane.reason,
        };

    return {
      mode: "runtime",
      controlPlaneStatus: runtimeControl.control_plane.status,
      primaryAction: primary,
      cancelAction: {
        enabled: actions.cancel.enabled,
        label: "取消",
        unavailableReason: actions.cancel.unavailable_reason,
      },
      helperText: primary.enabled
        ? runtimeControl.control_plane.reason ?? undefined
        : primary.unavailableReason,
    };
  }, [
    currentAgentId,
    currentRunId,
    draftProjectAgent,
    draftProjectAgentKey,
    draftProjectIdValue,
    isProjectAgentDraft,
    runtimeControl,
    agentRunWorkspaceState.error,
    agentRunWorkspaceState.status,
  ]);

  const handleAgentRunPrimaryAction = useCallback(async (
    action: SessionChatPrimaryActionKind,
    sessionId: string | null,
    prompt: string,
    executorConfig?: ExecutorConfig,
    imageAttachments?: ImageAttachment[],
  ) => {
    const trimmed = prompt.trim();
    const hasImages = (imageAttachments?.length ?? 0) > 0;
    if (!trimmed && !hasImages) {
      throw new Error("请输入要发送的消息。");
    }

    const inputBlocks: UserInput[] = [];
    if (trimmed) {
      inputBlocks.push({ type: "text" as const, text: trimmed, text_elements: [] });
    }
    if (imageAttachments) {
      for (const img of imageAttachments) {
        inputBlocks.push({ type: "image" as const, url: img.dataUrl });
      }
    }
    if (!executorConfig?.executor?.trim()) {
      throw new Error("请选择模型配置后再发送。");
    }
    const commandKey = JSON.stringify({
      action,
      input: inputBlocks,
      executor_config: executorConfig ?? null,
    });
    const resolvedCommand = resolveAgentRunClientCommandId(
      inFlightCommandRef.current,
      commandKey,
      newClientCommandId,
    );
    const clientCommandId = resolvedCommand.clientCommandId;
    inFlightCommandRef.current = resolvedCommand.inFlightCommand;

    if (action === "start_draft") {
      if (!draftProjectIdValue || !draftProjectAgentKey || !draftProjectAgent) {
        throw new Error(chatControlState.primaryAction.unavailableReason ?? "当前 Draft 尚未就绪。");
      }
      const response = await createProjectAgentRun(draftProjectIdValue, draftProjectAgentKey, {
        input: inputBlocks,
        client_command_id: clientCommandId,
        executor_config: executorConfig as unknown as JsonValue | undefined,
      });
      if (!response) {
        throw new Error("创建 ProjectAgent AgentRun 失败。");
      }
      void fetchAndIngestLifecycleRun(response.run_ref.run_id);
      inFlightCommandRef.current = null;
      navigate(`/agent-runs/${encodeURIComponent(response.run_ref.run_id)}/${encodeURIComponent(response.agent_ref.agent_id)}`, {
        replace: true,
        state: {
          trace_agent: {
            display_name: response.agent.display_name,
            executor_hint: response.agent.executor.executor,
          },
        },
      });
      return;
    }
    if (!currentRunId || !currentAgentId || !sessionId || sessionId !== deliveryRuntimeSessionId) {
      throw new Error("当前 AgentRun 尚未就绪，无法执行控制动作。");
    }

    // enqueue 和 steer 在 running 态可同时可用（主/辅动作），需分别校验
    const { primaryAction: csPrimary, secondaryAction: csSecondary } = chatControlState;
    const isPrimaryMatch = csPrimary.enabled && action === csPrimary.kind;
    const isSecondaryMatch = csSecondary?.enabled && action === csSecondary.kind;
    if (!isPrimaryMatch && !isSecondaryMatch) {
      throw new Error(csPrimary.unavailableReason ?? "当前 AgentRun 不可执行该控制动作。");
    }

    if (action === "send_next") {
      const response = await sendAgentRunMessage(currentRunId, currentAgentId, {
        input: inputBlocks,
        client_command_id: clientCommandId,
        executor_config: executorConfig as unknown as JsonValue | undefined,
      });
      void fetchAndIngestLifecycleRun(response.accepted_refs.run_ref.run_id);
      inFlightCommandRef.current = null;
      void refreshAgentRunWorkspaceState().catch(() => {});
      scheduleHookRuntimeRefresh("agent_message_sent", true);
      return;
    }
    if (action === "steer") {
      await steerAgentRun(currentRunId, currentAgentId, {
        input: inputBlocks,
        client_command_id: clientCommandId,
        expected_runtime_session_id: sessionId,
        expected_turn_id: runtimeControl?.delivery_trace_meta?.last_turn_id,
      });
      inFlightCommandRef.current = null;
      void refreshAgentRunWorkspaceState().catch(() => {});
      scheduleHookRuntimeRefresh("agent_message_steered", true);
      return;
    }
    if (action === "enqueue") {
      await enqueueAgentRunPendingMessage(currentRunId, currentAgentId, {
        input: inputBlocks,
        client_command_id: clientCommandId,
        executor_config: executorConfig as unknown as JsonValue | undefined,
      });
      inFlightCommandRef.current = null;
      void refreshAgentRunWorkspaceState().catch(() => {});
      scheduleHookRuntimeRefresh("pending_message_enqueued", true);
      return;
    }
    throw new Error(csPrimary.unavailableReason ?? "当前 AgentRun 不可执行该控制动作。");
  }, [
    chatControlState,
    createProjectAgentRun,
    currentAgentId,
    currentRunId,
    deliveryRuntimeSessionId,
    draftProjectAgent,
    draftProjectAgentKey,
    draftProjectIdValue,
    fetchAndIngestLifecycleRun,
    navigate,
    refreshAgentRunWorkspaceState,
    runtimeControl?.delivery_trace_meta?.last_turn_id,
    scheduleHookRuntimeRefresh,
  ]);

  const handleCancelAgentRun = useCallback(async () => {
    if (!currentRunId || !currentAgentId) {
      throw new Error("当前 AgentRun 尚未就绪。");
    }
    await cancelAgentRun(currentRunId, currentAgentId);
    void refreshAgentRunWorkspaceState().catch(() => {});
    scheduleHookRuntimeRefresh("agent_run_cancelled", true);
  }, [currentAgentId, currentRunId, refreshAgentRunWorkspaceState, scheduleHookRuntimeRefresh]);

  const handlePromotePending = useCallback(async (messageId: string) => {
    if (!currentRunId || !currentAgentId) return;
    await promoteAgentRunPendingMessage(currentRunId, currentAgentId, messageId);
    void refreshAgentRunWorkspaceState().catch(() => {});
    scheduleHookRuntimeRefresh("pending_message_promoted", true);
  }, [currentAgentId, currentRunId, refreshAgentRunWorkspaceState, scheduleHookRuntimeRefresh]);

  const handleDeletePending = useCallback(async (messageId: string) => {
    if (!currentRunId || !currentAgentId) return;
    await deleteAgentRunPendingMessage(currentRunId, currentAgentId, messageId);
    void refreshAgentRunWorkspaceState().catch(() => {});
    scheduleHookRuntimeRefresh("pending_message_deleted", true);
  }, [currentAgentId, currentRunId, refreshAgentRunWorkspaceState, scheduleHookRuntimeRefresh]);

  const handleTurnEnd = useCallback(() => {
    scheduleHookRuntimeRefresh("turn_end", true);
  }, [scheduleHookRuntimeRefresh]);

  const handleSystemEvent = useCallback((eventType: string, _event: BackboneEvent) => {
    switch (eventType) {
      case "hook_event":
      case "hook_action_resolved":
      case "companion_dispatch_registered":
      case "companion_result_available":
      case "companion_result_returned":
        scheduleHookRuntimeRefresh(eventType);
        break;
      case "context_frame": {
        const frameData = extractPlatformEventData(_event);
        if (frameData?.kind === "capability_state_update") {
          void refreshAgentRunWorkspaceState();
          scheduleHookRuntimeRefresh(eventType);
        }
        break;
      }
      case "session_meta_updated": {
        void refreshAgentRunWorkspaceState();
        break;
      }
      case "workspace_module_presented": {
        // workspace_module_present 推送：按 renderer_kind 决定 workspace tab typeId/uri。
        // - canvas → typeId "canvas"，presentation_uri=canvas://{mount_id}。
        // - extension webview/panel → typeId = view_key，presentation_uri 为后端生成的 tab URI。
        const data = extractPlatformEventData(_event);
        const target = workspaceModulePresentedTabTarget(data);
        if (target) {
          if (target.refreshRuntime) {
            void refreshAgentRunWorkspaceState();
          }
          expandWorkspacePanel(target.typeId, target.uri);
        }
        break;
      }
      case "workspace_module_present_failed": {
        // 后端已产出可操作诊断（无可展示目标）；展示层无需打开 tab，事件本身在 feed 可见。
        break;
      }
      default:
        break;
    }
  }, [scheduleHookRuntimeRefresh, refreshAgentRunWorkspaceState, expandWorkspacePanel]);

  const handleBackToOwner = useCallback(() => {
    if (!effectiveReturnTarget) return;
    if (effectiveReturnTarget.owner_type === "project") {
      selectProject(effectiveReturnTarget.project_id);
      navigate("/");
      return;
    }
    if (effectiveReturnTarget.owner_type === "task") {
      const state: StoryNavigationState = { open_task_id: effectiveReturnTarget.task_id };
      navigate(`/story/${effectiveReturnTarget.story_id}`, { state });
      return;
    }
    navigate(`/story/${effectiveReturnTarget.story_id}`);
  }, [effectiveReturnTarget, navigate, selectProject]);

  const handleCopyRuntimeSessionId = useCallback(async () => {
    if (!deliveryRuntimeSessionId) return;
    try { await navigator.clipboard.writeText(deliveryRuntimeSessionId); } catch { /* noop */ }
  }, [deliveryRuntimeSessionId]);

  const handleOpenRunDetail = useCallback(() => {
    if (!agentRunDetailTarget) return;
    navigate(`/run/${agentRunDetailTarget.runId}`, {
      state: {
        agent_id: agentRunDetailTarget.agentId,
        frame_id: agentRunDetailTarget.frameId,
        runtime_session_id: deliveryRuntimeSessionId,
      },
    });
  }, [agentRunDetailTarget, deliveryRuntimeSessionId, navigate]);

  const backButtonLabel = effectiveReturnTarget?.owner_type === "project"
    ? "返回项目"
    : effectiveReturnTarget?.owner_type === "task"
      ? "返回任务"
      : "返回 Story";
  const hasDeliveryRuntime = deliveryRuntimeSessionId !== null;
  const workspaceRuntimeData: WorkspaceRuntimeData = useMemo(() => ({
    projectId: ownerProjectId,
    sessionId: deliveryRuntimeSessionId,
    runtimeSessionId: deliveryRuntimeSessionId,
    sessionMeta: runtimeControl?.delivery_trace_meta
      ? {
          id: runtimeControl.delivery_trace_meta.runtime_session_ref.runtime_session_id,
          title: runtimeControl.delivery_trace_meta.trace_title,
          title_source: runtimeControl.delivery_trace_meta.trace_title_source,
          created_at: runtimeControl.delivery_trace_meta.updated_at,
          updated_at: runtimeControl.delivery_trace_meta.updated_at,
          last_event_seq: runtimeControl.delivery_trace_meta.last_event_seq,
          last_turn_id: runtimeControl.delivery_trace_meta.last_turn_id,
          last_delivery_status: runtimeControl.delivery_trace_meta.delivery_status,
        }
      : null,
    controlAnchor: null,
    lifecycleRun: null,
    lifecycleAgent: runtimeControl?.agent ?? null,
    frameRuntime: runtimeControl?.frame_runtime ?? null,
    subjectAssociations: runtimeControl?.subject_associations ?? [],
    runtimeStatus: agentRunWorkspaceState.status,
    runtimeError: agentRunWorkspaceState.error ?? agentRunWorkspaceState.runtime_surface_error,
    extensionRuntime,
    contextSnapshot: sessionContextSnapshot,
    ownerStory,
    ownerProjectName,
    executorSummary: taskExecutorSummary,
    runtimeSurface: deliveryRuntimeSurface,
    workspaceBackend,
    hookRuntime: activeHookRuntime,
    sessionCapabilities,
  }), [
    ownerProjectId,
    deliveryRuntimeSessionId,
    runtimeControl,
    agentRunWorkspaceState.status,
    agentRunWorkspaceState.error,
    agentRunWorkspaceState.runtime_surface_error,
    extensionRuntime,
    sessionContextSnapshot,
    ownerStory,
    ownerProjectName,
    taskExecutorSummary,
    deliveryRuntimeSurface,
    workspaceBackend,
    activeHookRuntime,
    sessionCapabilities,
  ]);

  // ─── owner 信息条（作为 inputPrefix 传入 ChatView）

  const runContextDisplayName = useMemo(() => {
    if (!runContext) return "";
    if (runContext.scope === "task") return runContext.task_title?.trim() || runContext.task_id || "";
    if (runContext.scope === "story") return runContext.story_title?.trim() || runContext.story_id || "";
    return ownerProject?.name?.trim() || runContext.project_id;
  }, [runContext, ownerProject]);

  const ownerBindingBar = runContext ? (
    <div className="mb-3 flex flex-wrap items-center gap-2 rounded-[12px] border border-border bg-secondary/20 px-3 py-2 text-xs text-muted-foreground">
      <span className="rounded-[8px] border border-border bg-background px-2 py-0.5 uppercase">
        {runContext.scope}
      </span>
      <span>
        已绑定：{runContextDisplayName}
      </span>
      {effectiveReturnTarget && (
        <button
          type="button"
          onClick={handleBackToOwner}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] transition-colors hover:bg-secondary hover:text-foreground"
        >
          打开关联
          {runContext.scope === "project"
            ? "项目"
            : runContext.scope === "task"
              ? "任务"
              : "Story"}
        </button>
      )}
    </div>
  ) : null;
  const draftBindingBar = isProjectAgentDraft ? (
    <div className="mb-3 flex flex-wrap items-center gap-2 rounded-[12px] border border-border bg-secondary/20 px-3 py-2 text-xs text-muted-foreground">
      <span className="rounded-[8px] border border-border bg-background px-2 py-0.5 uppercase">
        Draft
      </span>
      <span className="min-w-0 truncate">
        {draftProjectAgent?.display_name ?? traceAgentContext?.display_name ?? "ProjectAgent"}
      </span>
      <span className="rounded-[8px] border border-border bg-background px-2 py-0.5">
        待发送
      </span>
    </div>
  ) : null;

  // ─── 路由 state 驱动自动展开右栏 ───────────────────────
  useEffect(() => {
    if (!routeState?.open_workspace_panel) return;
    const raf = requestAnimationFrame(() => {
      rightPanelRef.current?.expand();
    });
    return () => cancelAnimationFrame(raf);
  }, [routeState]);

  // ─── 渲染 ────────────────────────────────────────────

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="flex shrink-0 items-center justify-between border-b border-border bg-background px-5 py-3.5">
        <div className="flex min-w-0 items-center gap-2.5">
          <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            {isProjectAgentDraft ? "DRAFT" : "AGENT RUN"}
          </span>
          <h2 className="truncate text-sm font-semibold text-foreground">
            {workspaceTitle || "AgentRun"}
          </h2>
        </div>
        <div className="flex items-center gap-2">
          {effectiveReturnTarget && (
            <button type="button" onClick={handleBackToOwner} className="rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground">
              {backButtonLabel}
            </button>
          )}
          {agentRunDetailTarget && (
            <button
              type="button"
              onClick={handleOpenRunDetail}
              className="rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
              title="查看当前 AgentRun 的运行详情"
            >
              运行详情
            </button>
          )}
          {hasDeliveryRuntime && (
            <>
              <span className="hidden rounded-[8px] border border-border bg-secondary px-2.5 py-1 text-xs font-mono text-muted-foreground lg:inline">
                {deliveryRuntimeSessionId.slice(0, 12)}…
              </span>
              <button type="button" onClick={() => void handleCopyRuntimeSessionId()} className="rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground" title="复制 RuntimeSession ID">
                复制
              </button>
            </>
          )}
          <button
            type="button"
            onClick={toggleWorkspacePanel}
            className="rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            title="展开/收起工作空间面板"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <rect width="18" height="18" x="3" y="3" rx="2" />
              <path d="M15 3v18" />
            </svg>
          </button>
        </div>
      </header>

      <Group orientation="horizontal" className="flex-1 overflow-hidden">
        <Panel minSize="30%">
          <div className="h-full overflow-hidden">
            <SessionChatView
              sessionId={deliveryRuntimeSessionId}
              workspaceId={chatWorkspaceId}
              onMessageSent={handleMessageSent}
              onTurnEnd={handleTurnEnd}
              onSystemEvent={handleSystemEvent}
              executorHint={executorHint}
              agentDefaults={frameExecutorDefaults ?? draftProjectAgent?.executor ?? taskExecutorSummary}
              executorStateKey={executorStateKey}
              controlState={chatControlState}
              onPrimaryAction={handleAgentRunPrimaryAction}
              onCancelAction={handleCancelAgentRun}
              pendingMessages={runtimeControl?.pending_messages}
              onPromotePending={(id) => { void handlePromotePending(id); }}
              onDeletePending={(id) => { void handleDeletePending(id); }}
              inputPrefix={ownerBindingBar ?? draftBindingBar}
            />
          </div>
        </Panel>

        <Separator className="group relative w-1.5 shrink-0 bg-border/30 transition-colors hover:bg-primary/30 active:bg-primary/50 data-[separator]:cursor-col-resize">
          <div className="absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 rounded-[8px] bg-border transition-colors group-hover:bg-primary/50 group-active:bg-primary" />
        </Separator>

        <Panel
          panelRef={rightPanelRef}
          defaultSize="0%"
          minSize="20%"
          maxSize="60%"
          collapsible
          collapsedSize="0%"
          className="border-l border-border"
        >
          <WorkspacePanel
            ref={workspacePanelRef}
            runtimeData={workspaceRuntimeData}
          />
        </Panel>
      </Group>
    </div>
  );
}

export default AgentRunWorkspacePage;
