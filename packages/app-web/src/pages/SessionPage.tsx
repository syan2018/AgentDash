import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Group, Panel, Separator, type PanelImperativeHandle } from "react-resizable-panels";
import type { BackboneEvent } from "../generated/backbone-protocol";
import { SessionChatView } from "../features/session";
import { extractPlatformEventData } from "../features/session/model/platformEvent";
import { useProjectExtensionRuntime } from "../features/extension-runtime";
import { LifecycleSessionView } from "../features/workflow/lifecycle-session-view";
import {
  WorkspacePanel,
  type WorkspacePanelHandle,
  type WorkspaceRuntimeData,
} from "../features/workspace-panel";
import { useSessionRuntimeState } from "../features/workspace-panel/model/useSessionRuntimeState";
import { fetchSessionMeta } from "../services/session";
import { useProjectStore } from "../stores/projectStore";
import { useSessionHistoryStore } from "../stores/sessionHistoryStore";
import { findStoryById, useStoryStore } from "../stores/storyStore";
import { useWorkflowStore } from "../stores/workflowStore";
import { findWorkspaceBinding, useWorkspaceStore } from "../stores/workspaceStore";
import type {
  ProjectSessionAgentContext,
  SessionNavigationState,
  SessionRunContext,
  Story,
  StoryNavigationState,
} from "../types";

// ─── SessionPage ────────────────────────────────────────

interface SessionPageProps {
  sessionId?: string;
}

export function SessionPage({ sessionId: propSessionId }: SessionPageProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const selectProject = useProjectStore((state) => state.selectProject);
  const projects = useProjectStore((state) => state.projects);
  const fetchWorkspaces = useWorkspaceStore((state) => state.fetchWorkspaces);
  const workspacesByProjectId = useWorkspaceStore((state) => state.workspacesByProjectId);
  const { createNew, setActiveSessionId, reload: reloadSessions, updateTitle, patchSessionLocally } = useSessionHistoryStore();
  const runsBySessionId = useWorkflowStore((state) => state.runsBySessionId);
  const fetchRunsBySession = useWorkflowStore((state) => state.fetchRunsBySession);
  const hookRuntimeRefreshTimerRef = useRef<number | null>(null);

  const [sessionTitle, setSessionTitle] = useState<string>("");
  const [isEditingTitle, setIsEditingTitle] = useState(false);
  const [editingTitleValue, setEditingTitleValue] = useState("");
  const titleInputRef = useRef<HTMLInputElement>(null);

  const [loadedOwnerStory, setLoadedOwnerStory] = useState<{
    story_id: string;
    story: Story | null;
  } | null>(null);
  const [activeCanvasId, setActiveCanvasId] = useState<string | null>(null);
  const [sessionViewMode, setSessionViewMode] = useState<"chat" | "lifecycle">("chat");

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
  const taskContextFromRoute = routeState?.task_context ?? null;
  const projectAgentContext = (routeState?.project_agent ?? null) as ProjectSessionAgentContext | null;
  const returnTarget = routeState?.return_to ?? null;
  const currentSessionId = propSessionId ?? null;

  // ─── session ID 同步 ──────────────────────────────────

  useEffect(() => {
    setActiveSessionId(propSessionId ?? null);
  }, [propSessionId, setActiveSessionId]);

  useEffect(() => {
    if (!currentSessionId) return;
    void fetchRunsBySession(currentSessionId);
  }, [currentSessionId, fetchRunsBySession]);

  useEffect(() => {
    if (!currentSessionId) return;
    const timer = window.setInterval(() => {
      void fetchRunsBySession(currentSessionId);
    }, 5000);
    return () => window.clearInterval(timer);
  }, [currentSessionId, fetchRunsBySession]);

  // ─── 加载初始标题 ──────────────────────────────────────

  useEffect(() => {
    if (!propSessionId) return;
    let cancelled = false;
    void (async () => {
      try {
        const meta = await fetchSessionMeta(propSessionId);
        if (!cancelled) setSessionTitle(meta.title);
      } catch { /* 加载失败保留空标题 */ }
    })();
    return () => {
      cancelled = true;
      setSessionTitle("");
    };
  }, [propSessionId]);

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
  const activeHookRuntime = sessionRuntimeState.hook_runtime?.session_id === currentSessionId
    ? sessionRuntimeState.hook_runtime
    : null;
  const taskAgentBinding = taskContextFromRoute?.agent_binding
    ?? activeSessionContext?.agent_binding
    ?? null;
  const sessionWorkspaceId = activeSessionContext?.workspace_id ?? null;
  const sessionRuntimeSurface = activeSessionContext?.runtime_surface ?? null;
  const sessionContextSnapshot = activeSessionContext?.context_snapshot ?? null;
  const sessionCapabilities = activeSessionContext?.session_capabilities ?? null;
  const taskExecutorSummary = sessionContextSnapshot?.executor ?? null;

  const runContext: SessionRunContext | null = activeHookRuntime?.snapshot?.run_context ?? null;

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
  const ownerProjectId = runContext?.project_id ?? ownerStory?.project_id ?? null;
  const ownerProject = ownerProjectId
    ? projects.find((project) => project.id === ownerProjectId) ?? null
    : null;
  const ownerProjectName = runContext?.scope === "project"
    ? (ownerProject?.name?.trim() || runContext.project_id)
    : "";
  const extensionRuntime = useProjectExtensionRuntime(ownerProjectId);
  useEffect(() => {
    if (!ownerProjectId) return;
    void fetchWorkspaces(ownerProjectId);
  }, [fetchWorkspaces, ownerProjectId]);

  const effectiveReturnTarget = useMemo(() => {
    if (returnTarget) return returnTarget;
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
  }, [returnTarget, runContext]);

  // ─── 页面级回调 ───────────────────────────────────────

  const executorHint = taskAgentBinding?.agent_type
    ?? projectAgentContext?.executor_hint
    ?? taskExecutorSummary?.executor
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

  const handleCreateSession = useCallback(async (title: string) => {
    if (!ownerProjectId) {
      throw new Error("创建会话需要先选择 Project");
    }
    const meta = await createNew(ownerProjectId, title);
    return meta.id;
  }, [createNew, ownerProjectId]);

  const handleSessionIdChange = useCallback((id: string) => {
    setActiveSessionId(id);
    navigate(`/session/${id}`, { replace: true });
  }, [navigate, setActiveSessionId]);

  const handleMessageSent = useCallback(() => {
    void reloadSessions();
    if (!currentSessionId) return;
    scheduleHookRuntimeRefresh("message_sent", true);
  }, [currentSessionId, reloadSessions, scheduleHookRuntimeRefresh]);

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
        const newTitleSource = typeof data?.title_source === "string" ? data.title_source as string : undefined;
        if (newTitle && currentSessionId) {
          setSessionTitle(newTitle);
          patchSessionLocally(currentSessionId, {
            title: newTitle,
            title_source:
              newTitleSource === "auto" || newTitleSource === "source" || newTitleSource === "user"
                ? newTitleSource
                : undefined,
          });
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
  }, [scheduleHookRuntimeRefresh, currentSessionId, patchSessionLocally, refreshSessionRuntimeContext, expandWorkspacePanel]);

  const handleNewSession = useCallback(() => {
    setActiveSessionId(null);
    navigate("/session", { replace: true });
  }, [navigate, setActiveSessionId]);

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

  const handleStartEditTitle = useCallback(() => {
    setEditingTitleValue(sessionTitle);
    setIsEditingTitle(true);
    requestAnimationFrame(() => titleInputRef.current?.select());
  }, [sessionTitle]);

  const handleCommitTitle = useCallback(async () => {
    setIsEditingTitle(false);
    const trimmed = editingTitleValue.trim();
    if (!trimmed || !currentSessionId || trimmed === sessionTitle) return;
    setSessionTitle(trimmed);
    try {
      await updateTitle(currentSessionId, trimmed);
    } catch { /* store 内已记录 error */ }
  }, [currentSessionId, editingTitleValue, sessionTitle, updateTitle]);

  const backButtonLabel = effectiveReturnTarget?.owner_type === "project"
    ? "返回项目"
    : effectiveReturnTarget?.owner_type === "task"
      ? "返回任务"
      : "返回 Story";
  const hasSession = currentSessionId !== null;
  const lifecycleRuns = useMemo(
    () => (currentSessionId ? runsBySessionId[currentSessionId] ?? [] : []),
    [currentSessionId, runsBySessionId],
  );
  const workspaceRuntimeData: WorkspaceRuntimeData = useMemo(() => ({
    projectId: ownerProjectId,
    sessionId: currentSessionId,
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
    workflowRuns: lifecycleRuns,
    activeCanvasId,
  }), [
    ownerProjectId,
    currentSessionId,
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
    lifecycleRuns,
    activeCanvasId,
  ]);
  const hasLifecycleGraph = useMemo(
    () =>
      lifecycleRuns.some((run) =>
        (run.activity_state?.attempts.some((attempt) =>
          attempt.executor_run?.kind === "agent_session"
            ? Boolean(attempt.executor_run.session_id)
            : false,
        ) ?? false)
        || (run.active_node_keys?.length ?? 0) > 0
      ),
    [lifecycleRuns],
  );
  const canShowLifecycleView = hasSession && hasLifecycleGraph;
  const showLifecycleView = canShowLifecycleView && sessionViewMode === "lifecycle";

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
      {runContext.scope === "project" && sessionContextSnapshot?.owner_context.owner_level === "project" && sessionContextSnapshot.owner_context.agent_display_name && (
        <span className="rounded-[8px] border border-border bg-background px-2 py-0.5 text-[11px] text-foreground/80">
          Agent · {sessionContextSnapshot.owner_context.agent_display_name}
        </span>
      )}
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

  // ─── 路由 state 驱动自动展开右栏 ───────────────────────
  // 通过 requestAnimationFrame 延迟到 paint 后调用，避免 effect 内同步 setState
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
      {/* 页面 Header */}
      <header className="flex shrink-0 items-center justify-between border-b border-border bg-background px-5 py-3.5">
        <div className="flex min-w-0 items-center gap-2.5">
          <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            RUNTIME TRACE
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
          {canShowLifecycleView && (
            <div className="hidden items-center gap-1 rounded-[8px] border border-border bg-secondary/40 p-0.5 md:flex">
              <button
                type="button"
                onClick={() => setSessionViewMode("chat")}
                className={`rounded-[8px] px-2.5 py-1 text-xs transition-colors ${
                  sessionViewMode === "chat"
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                聊天
              </button>
              <button
                type="button"
                onClick={() => setSessionViewMode("lifecycle")}
                className={`rounded-[8px] px-2.5 py-1 text-xs transition-colors ${
                  sessionViewMode === "lifecycle"
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                Lifecycle
              </button>
            </div>
          )}
          <button type="button" onClick={handleNewSession} className="rounded-[8px] border border-border bg-secondary px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-secondary/80">
            新会话
          </button>
          {/* 工作空间面板展开/收起 */}
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

      {/* 中栏 + 右栏：可拖拽双面板 */}
      <Group orientation="horizontal" className="flex-1 overflow-hidden">
        {/* 中栏：聊天 / Lifecycle 视图 */}
        <Panel minSize="30%">
          <div className="h-full overflow-hidden">
            {showLifecycleView && currentSessionId ? (
              <LifecycleSessionView sessionId={currentSessionId} />
            ) : (
              <SessionChatView
                sessionId={currentSessionId}
                workspaceId={chatWorkspaceId}
                onCreateSession={handleCreateSession}
                onSessionIdChange={handleSessionIdChange}
                onMessageSent={handleMessageSent}
                onTurnEnd={handleTurnEnd}
                onSystemEvent={handleSystemEvent}
                executorHint={executorHint}
                agentDefaults={taskExecutorSummary}
                inputPrefix={ownerBindingBar}
              />
            )}
          </div>
        </Panel>

        {/* 拖拽手柄 */}
        <Separator className="group relative w-1.5 shrink-0 bg-border/30 transition-colors hover:bg-primary/30 active:bg-primary/50 data-[separator]:cursor-col-resize">
          <div className="absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 rounded-[8px] bg-border transition-colors group-hover:bg-primary/50 group-active:bg-primary" />
        </Separator>

        {/* 右栏：工作空间面板（默认折叠） */}
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
