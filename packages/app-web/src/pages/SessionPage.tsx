/**
 * SessionPage — 会话交互主工作台。
 *
 * 用户认知中 "lifecycle agent = 一个会话"。此页面是用户点击会话后的主视图，
 * 提供 Chat + Workspace Panel 双面板布局、标题编辑、上下文导航等完整交互。
 *
 * 底层数据通过 lifecycle frame 投影驱动（`useSessionRuntimeState`），
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
  WorkspacePanel,
  type WorkspacePanelHandle,
  type WorkspaceRuntimeData,
} from "../features/workspace-panel";
import { useSessionRuntimeState } from "../features/workspace-panel/model/useSessionRuntimeState";
import { sendLifecycleAgentMessageByRuntimeSession } from "../services/lifecycle";
import type { ExecutorConfig } from "../services/executor";
import type { JsonValue } from "../generated/common-contracts";
import { updateSessionTitle } from "../services/session";
import { useLifecycleStore } from "../stores/lifecycleStore";
import { useProjectStore } from "../stores/projectStore";
import { findStoryById, useStoryStore } from "../stores/storyStore";
import { findWorkspaceBinding, useWorkspaceStore } from "../stores/workspaceStore";
import type {
  RuntimeTraceAgentContext,
  SessionNavigationState,
  SessionRuntimeControlView,
  SubjectRunContext,
  ProjectAgentSummary,
  Story,
  StoryNavigationState,
} from "../types";

// ─── SessionPage ────────────────────────────────────────

interface SessionPageProps {
  sessionId?: string;
  draftProjectId?: string;
  draftProjectAgentId?: string;
}

export function SessionPage({
  sessionId: propSessionId,
  draftProjectId,
  draftProjectAgentId,
}: SessionPageProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const selectProject = useProjectStore((state) => state.selectProject);
  const projects = useProjectStore((state) => state.projects);
  const agentsByProjectId = useProjectStore((state) => state.agentsByProjectId);
  const fetchProjectAgents = useProjectStore((state) => state.fetchProjectAgents);
  const createProjectAgentRuntimeSession = useProjectStore((state) => state.createProjectAgentRuntimeSession);
  const fetchAndIngestLifecycleRun = useLifecycleStore((state) => state.fetchAndIngestLifecycleRun);
  const fetchWorkspaces = useWorkspaceStore((state) => state.fetchWorkspaces);
  const workspacesByProjectId = useWorkspaceStore((state) => state.workspacesByProjectId);
  const hookRuntimeRefreshTimerRef = useRef<number | null>(null);

  const [sessionTitleOverride, setSessionTitleOverride] = useState<{
    sessionId: string;
    title: string;
  } | null>(null);
  const [isEditingTitle, setIsEditingTitle] = useState(false);
  const [editingTitleValue, setEditingTitleValue] = useState("");
  const titleInputRef = useRef<HTMLInputElement>(null);

  const [loadedOwnerStory, setLoadedOwnerStory] = useState<{
    story_id: string;
    story: Story | null;
  } | null>(null);
  const [activeCanvasId, setActiveCanvasId] = useState<string | null>(null);

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
  const currentSessionId = propSessionId ?? null;
  const draftProjectAgentKey = !currentSessionId ? draftProjectAgentId?.trim() || null : null;
  const draftProjectIdValue = !currentSessionId ? draftProjectId?.trim() || null : null;
  const isProjectAgentDraft = Boolean(draftProjectIdValue && draftProjectAgentKey);
  const draftProjectAgent: ProjectAgentSummary | null = useMemo(() => {
    if (!draftProjectIdValue || !draftProjectAgentKey) return null;
    return (agentsByProjectId[draftProjectIdValue] ?? [])
      .find((agent) => agent.key === draftProjectAgentKey) ?? null;
  }, [agentsByProjectId, draftProjectAgentKey, draftProjectIdValue]);

  useEffect(() => {
    if (!draftProjectIdValue || currentSessionId) return;
    if (agentsByProjectId[draftProjectIdValue]) return;
    void fetchProjectAgents(draftProjectIdValue);
  }, [agentsByProjectId, currentSessionId, draftProjectIdValue, fetchProjectAgents]);

  useEffect(() => {
    return () => {
      if (hookRuntimeRefreshTimerRef.current) {
        window.clearTimeout(hookRuntimeRefreshTimerRef.current);
        hookRuntimeRefreshTimerRef.current = null;
      }
    };
  }, []);

  const sessionContextSourceKey = currentSessionId ? `session:${currentSessionId}` : null;

  const {
    state: sessionRuntimeState,
    refreshContext: refreshSessionRuntimeContext,
    refreshHookRuntime: refreshSessionRuntimeHook,
  } = useSessionRuntimeState({
    sessionId: currentSessionId,
    sourceKey: sessionContextSourceKey,
  });

  const scheduleHookRuntimeRefresh = useCallback((_reason: string, immediate = false) => {
    if (!currentSessionId) return;
    if (hookRuntimeRefreshTimerRef.current) {
      window.clearTimeout(hookRuntimeRefreshTimerRef.current);
      hookRuntimeRefreshTimerRef.current = null;
    }
    if (immediate) {
      void refreshSessionRuntimeHook();
      return;
    }
    hookRuntimeRefreshTimerRef.current = window.setTimeout(() => {
      hookRuntimeRefreshTimerRef.current = null;
      void refreshSessionRuntimeHook();
    }, 180);
  }, [currentSessionId, refreshSessionRuntimeHook]);

  const activeSessionContext = sessionRuntimeState.context;
  const runtimeControl: SessionRuntimeControlView | null = sessionRuntimeState.control;
  const draftSessionTitle =
    draftProjectAgent?.display_name
    ?? traceAgentContext?.display_name
    ?? "新会话";
  const runtimeSessionTitle = sessionTitleOverride?.sessionId === currentSessionId
    ? sessionTitleOverride.title
    : runtimeControl?.session_meta.title ?? "";
  const sessionTitle = isProjectAgentDraft ? draftSessionTitle : runtimeSessionTitle;
  const activeHookRuntime = sessionRuntimeState.hook_runtime?.runtime_adapter_session_id === currentSessionId
    ? sessionRuntimeState.hook_runtime
    : null;
  const sessionWorkspaceId = activeSessionContext?.workspace_id ?? null;
  const sessionRuntimeSurface = activeSessionContext?.runtime_surface ?? null;
  const sessionContextSnapshot = null;
  const sessionCapabilities = null;
  const taskExecutorSummary = null;

  const runContext: SubjectRunContext | null = activeHookRuntime?.snapshot?.run_context ?? null;
  const sessionLifecycleRun = runtimeControl?.run ?? null;
  const sessionLifecycleRunId = runtimeControl?.run?.run_ref.run_id ?? null;
  const sessionLifecycleAgentId = runtimeControl?.agent?.agent_ref.agent_id ?? null;
  const sessionLifecycleFrameId = runtimeControl?.frame_runtime?.frame_ref.frame_id ?? null;
  const sessionLifecycleDetailTarget = useMemo(() => {
    if (!sessionLifecycleRunId || !sessionLifecycleAgentId) return null;
    return {
      runId: sessionLifecycleRunId,
      agentId: sessionLifecycleAgentId,
      frameId: sessionLifecycleFrameId,
    };
  }, [sessionLifecycleAgentId, sessionLifecycleFrameId, sessionLifecycleRunId]);

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
  const ownerProjectId = sessionLifecycleRun?.project_id
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
  const chatWorkspaceId =
    sessionWorkspaceId
    ?? ownerStory?.default_workspace_id
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

  const handleSessionIdChange = useCallback((id: string) => {
    navigate(`/session/${id}`, { replace: true });
  }, [navigate]);

  const handleMessageSent = useCallback(() => {
    if (!currentSessionId) return;
    scheduleHookRuntimeRefresh("message_sent", true);
  }, [currentSessionId, scheduleHookRuntimeRefresh]);

  const sessionSendReady = useMemo(() => {
    if (isProjectAgentDraft) {
      return Boolean(draftProjectIdValue && draftProjectAgentKey && draftProjectAgent);
    }
    return Boolean(currentSessionId && runtimeControl?.can_send);
  }, [
    currentSessionId,
    draftProjectAgent,
    draftProjectAgentKey,
    draftProjectIdValue,
    isProjectAgentDraft,
    runtimeControl?.can_send,
  ]);

  const sendUnavailableReason = useMemo(() => {
    if (isProjectAgentDraft) {
      if (!draftProjectIdValue || !draftProjectAgentKey) {
        return "Draft 会话缺少 ProjectAgent 参数。";
      }
      if (!draftProjectAgent) {
        return "正在加载 ProjectAgent 配置。";
      }
      return undefined;
    }
    if (!currentSessionId) return "当前没有可发送的 Session。";
    if (sessionRuntimeState.status === "loading" || sessionRuntimeState.status === "refreshing") {
      return "正在解析当前 Session 的 Agent dispatcher…";
    }
    if (sessionRuntimeState.error) return sessionRuntimeState.error;
    if (!sessionSendReady) return runtimeControl?.send_unavailable_reason ?? "当前 Session 不可发送。";
    return undefined;
  }, [
    currentSessionId,
    draftProjectAgent,
    draftProjectAgentKey,
    draftProjectIdValue,
    isProjectAgentDraft,
    runtimeControl?.send_unavailable_reason,
    sessionRuntimeState.error,
    sessionRuntimeState.status,
    sessionSendReady,
  ]);

  const handleAgentSessionSend = useCallback(async (
    sessionId: string | null,
    prompt: string,
    executorConfig?: ExecutorConfig,
  ) => {
    const trimmed = prompt.trim();
    if (!trimmed) {
      throw new Error("请输入要发送的消息。");
    }
    if (isProjectAgentDraft) {
      if (!draftProjectIdValue || !draftProjectAgentKey || !draftProjectAgent) {
        throw new Error(sendUnavailableReason ?? "当前 Draft 尚未就绪。");
      }
      const response = await createProjectAgentRuntimeSession(draftProjectIdValue, draftProjectAgentKey, {
        prompt_blocks: [{ type: "text", text: trimmed }],
        executor_config: executorConfig as unknown as JsonValue | undefined,
      });
      if (!response) {
        throw new Error("创建 ProjectAgent 会话失败。");
      }
      void fetchAndIngestLifecycleRun(response.run_ref.run_id);
      navigate(`/session/${response.runtime_session_id}`, {
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
    if (!sessionId || sessionId !== currentSessionId) {
      throw new Error("当前 Session 尚未就绪，无法发送消息。");
    }
    if (!sessionSendReady) {
      throw new Error(sendUnavailableReason ?? "当前 Session 未连接到 Agent dispatcher。");
    }
    const response = await sendLifecycleAgentMessageByRuntimeSession(sessionId, {
      prompt_blocks: [{ type: "text", text: trimmed }],
      executor_config: executorConfig as unknown as JsonValue | undefined,
    });
    void fetchAndIngestLifecycleRun(response.run_ref.run_id);
    void refreshSessionRuntimeContext().catch(() => {});
    scheduleHookRuntimeRefresh("agent_message_sent", true);
  }, [
    createProjectAgentRuntimeSession,
    currentSessionId,
    draftProjectAgent,
    draftProjectAgentKey,
    draftProjectIdValue,
    fetchAndIngestLifecycleRun,
    isProjectAgentDraft,
    navigate,
    refreshSessionRuntimeContext,
    scheduleHookRuntimeRefresh,
    sendUnavailableReason,
    sessionSendReady,
  ]);

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
          void refreshSessionRuntimeContext();
          scheduleHookRuntimeRefresh(eventType);
        }
        break;
      }
      case "session_meta_updated": {
        const data = extractPlatformEventData(_event);
        const newTitle = typeof data?.title === "string" ? (data.title as string).trim() : "";
        if (newTitle && currentSessionId) {
          setSessionTitleOverride({ sessionId: currentSessionId, title: newTitle });
        }
        break;
      }
      case "canvas_presented": {
        const data = extractPlatformEventData(_event);
        const nextCanvasIdRaw = data?.canvas_id ?? data?.canvasId ?? data?.id;
        const nextCanvasId = typeof nextCanvasIdRaw === "string"
          ? (nextCanvasIdRaw as string).trim()
          : "";
        if (nextCanvasId) {
          setActiveCanvasId(nextCanvasId);
          void refreshSessionRuntimeContext();
          expandWorkspacePanel("canvas", `canvas://${nextCanvasId}`);
        }
        break;
      }
      default:
        break;
    }
  }, [currentSessionId, scheduleHookRuntimeRefresh, refreshSessionRuntimeContext, expandWorkspacePanel]);

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

  const handleCopySessionId = useCallback(async () => {
    if (!currentSessionId) return;
    try { await navigator.clipboard.writeText(currentSessionId); } catch { /* noop */ }
  }, [currentSessionId]);

  const handleOpenRunDetail = useCallback(() => {
    if (!sessionLifecycleDetailTarget) return;
    navigate(`/run/${sessionLifecycleDetailTarget.runId}`, {
      state: {
        agent_id: sessionLifecycleDetailTarget.agentId,
        frame_id: sessionLifecycleDetailTarget.frameId,
        runtime_session_id: currentSessionId,
      },
    });
  }, [currentSessionId, navigate, sessionLifecycleDetailTarget]);

  const handleStartEditTitle = useCallback(() => {
    setEditingTitleValue(sessionTitle);
    setIsEditingTitle(true);
    requestAnimationFrame(() => titleInputRef.current?.select());
  }, [sessionTitle]);

  const handleCommitTitle = useCallback(async () => {
    setIsEditingTitle(false);
    const trimmed = editingTitleValue.trim();
    if (!trimmed || !currentSessionId || trimmed === sessionTitle) return;
    setSessionTitleOverride({ sessionId: currentSessionId, title: trimmed });
    try {
      await updateSessionTitle(currentSessionId, trimmed);
    } catch { /* API 调用失败静默处理 */ }
  }, [currentSessionId, editingTitleValue, sessionTitle]);

  const backButtonLabel = effectiveReturnTarget?.owner_type === "project"
    ? "返回项目"
    : effectiveReturnTarget?.owner_type === "task"
      ? "返回任务"
      : "返回 Story";
  const hasSession = currentSessionId !== null;
  const workspaceRuntimeData: WorkspaceRuntimeData = useMemo(() => ({
    projectId: ownerProjectId,
    sessionId: currentSessionId,
    runtimeSessionId: currentSessionId,
    sessionMeta: runtimeControl?.session_meta ?? null,
    controlAnchor: runtimeControl?.anchor ?? null,
    lifecycleRun: runtimeControl?.run ?? null,
    lifecycleAgent: runtimeControl?.agent ?? null,
    frameRuntime: runtimeControl?.frame_runtime ?? null,
    subjectAssociations: runtimeControl?.subject_associations ?? [],
    runtimeStatus: sessionRuntimeState.status,
    runtimeError: sessionRuntimeState.error,
    extensionRuntime,
    contextSnapshot: sessionContextSnapshot,
    ownerStory,
    ownerProjectName,
    executorSummary: taskExecutorSummary,
    runtimeSurface: sessionRuntimeSurface,
    workspaceBackend,
    hookRuntime: activeHookRuntime,
    sessionCapabilities,
    activeCanvasId,
  }), [
    ownerProjectId,
    currentSessionId,
    runtimeControl,
    sessionRuntimeState.status,
    sessionRuntimeState.error,
    extensionRuntime,
    sessionContextSnapshot,
    ownerStory,
    ownerProjectName,
    taskExecutorSummary,
    sessionRuntimeSurface,
    workspaceBackend,
    activeHookRuntime,
    sessionCapabilities,
    activeCanvasId,
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
            {isProjectAgentDraft ? "DRAFT" : "SESSION"}
          </span>
          {hasSession && isEditingTitle ? (
            <input
              ref={titleInputRef}
              className="min-w-[120px] max-w-[320px] rounded-[6px] border border-primary/40 bg-background px-2 py-0.5 text-sm font-semibold text-foreground outline-none focus:ring-1 focus:ring-primary/40"
              value={editingTitleValue}
              onChange={(e) => setEditingTitleValue(e.target.value)}
              onBlur={() => void handleCommitTitle()}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleCommitTitle();
                if (e.key === "Escape") setIsEditingTitle(false);
              }}
            />
          ) : (
            <h2
              className={`truncate text-sm font-semibold text-foreground ${hasSession ? "cursor-pointer hover:text-primary" : ""}`}
              onClick={hasSession ? handleStartEditTitle : undefined}
              title={hasSession ? "点击编辑标题" : undefined}
            >
              {sessionTitle || "会话"}
            </h2>
          )}
        </div>
        <div className="flex items-center gap-2">
          {effectiveReturnTarget && (
            <button type="button" onClick={handleBackToOwner} className="rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground">
              {backButtonLabel}
            </button>
          )}
          {sessionLifecycleDetailTarget && (
            <button
              type="button"
              onClick={handleOpenRunDetail}
              className="rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
              title="查看当前 Session 的运行详情"
            >
              运行详情
            </button>
          )}
          {hasSession && (
            <>
              <span className="hidden rounded-[8px] border border-border bg-secondary px-2.5 py-1 text-xs font-mono text-muted-foreground lg:inline">
                {currentSessionId.slice(0, 12)}…
              </span>
              <button type="button" onClick={() => void handleCopySessionId()} className="rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground" title="复制 Session ID">
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
              sessionId={currentSessionId}
              workspaceId={chatWorkspaceId}
              onSessionIdChange={handleSessionIdChange}
              onMessageSent={handleMessageSent}
              onTurnEnd={handleTurnEnd}
              onSystemEvent={handleSystemEvent}
              executorHint={executorHint}
              agentDefaults={draftProjectAgent?.executor ?? taskExecutorSummary}
              customSend={sessionSendReady ? handleAgentSessionSend : undefined}
              inputPrefix={ownerBindingBar ?? draftBindingBar}
              sendUnavailableReason={sendUnavailableReason}
              inputPlaceholder={isProjectAgentDraft ? "输入首条消息，Ctrl+Enter 发送…" : undefined}
              idleSendLabel={isProjectAgentDraft ? "开始" : "发送"}
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

export default SessionPage;
