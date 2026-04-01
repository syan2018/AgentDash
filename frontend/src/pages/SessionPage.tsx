import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { SessionChatView } from "../features/acp-session";
import { extractAgentDashMetaFromUpdate, isRecord } from "../features/acp-session/model/agentdashMeta";
import { CanvasSessionPanel } from "../features/canvas-panel";
import { hasStoryContextInfo, ProjectSessionContextPanel, StorySessionContextPanel } from "../features/session-context";
import { fetchSessionBindings, fetchSessionContext, fetchSessionHookRuntime } from "../services/session";
import { useProjectStore } from "../stores/projectStore";
import { useSessionHistoryStore } from "../stores/sessionHistoryStore";
import { findStoryById, useStoryStore } from "../stores/storyStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import type {
  AgentBinding,
  ExecutionAddressSpace,
  HookSessionRuntimeInfo,
  ProjectSessionAgentContext,
  SessionBindingOwner,
  SessionContextSnapshot,
  SessionNavigationState,
  Story,
  StoryNavigationState,
} from "../types";

const EMPTY_SESSION_BINDINGS: SessionBindingOwner[] = [];
const EMPTY_WORKSPACES: [] = [];

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
  const { createNew, setActiveSessionId, reload: reloadSessions } = useSessionHistoryStore();
  const hookRuntimeRefreshTimerRef = useRef<number | null>(null);

  const [loadedSessionBindings, setLoadedSessionBindings] = useState<SessionBindingOwner[]>([]);
  const [loadedHookRuntime, setLoadedHookRuntime] = useState<HookSessionRuntimeInfo | null>(null);
  const [loadedSessionContext, setLoadedSessionContext] = useState<{
    source_key: string;
    workspace_id: string | null;
    task_agent_binding: AgentBinding | null;
    address_space: ExecutionAddressSpace | null;
    context_snapshot: SessionContextSnapshot | null;
  } | null>(null);
  const [loadedOwnerStory, setLoadedOwnerStory] = useState<{
    story_id: string;
    story: Story | null;
  } | null>(null);
  const [isContextPanelOpen, setIsContextPanelOpen] = useState(false);
  const [activeCanvasId, setActiveCanvasId] = useState<string | null>(null);
  const [isCanvasPanelOpen, setIsCanvasPanelOpen] = useState(false);

  const routeState = useMemo(
    () => (location.state as SessionNavigationState | null) ?? null,
    [location.state],
  );
  const taskContextFromRoute = routeState?.task_context ?? null;
  const projectAgentContext = (routeState?.project_agent ?? null) as ProjectSessionAgentContext | null;
  const returnTarget = routeState?.return_to ?? null;
  const currentSessionId = propSessionId ?? null;

  const refreshHookRuntime = useCallback(async (sessionId: string) => {
    try {
      const runtime = await fetchSessionHookRuntime(sessionId);
      setLoadedHookRuntime(runtime);
    } catch {
      setLoadedHookRuntime(null);
    }
  }, []);

  const scheduleHookRuntimeRefresh = useCallback((_reason: string, immediate = false) => {
    if (!currentSessionId) return;
    if (hookRuntimeRefreshTimerRef.current) {
      window.clearTimeout(hookRuntimeRefreshTimerRef.current);
      hookRuntimeRefreshTimerRef.current = null;
    }
    if (immediate) {
      void refreshHookRuntime(currentSessionId);
      return;
    }
    hookRuntimeRefreshTimerRef.current = window.setTimeout(() => {
      hookRuntimeRefreshTimerRef.current = null;
      void refreshHookRuntime(currentSessionId);
    }, 180);
  }, [currentSessionId, refreshHookRuntime]);

  // ─── session ID 同步 ──────────────────────────────────

  useEffect(() => {
    setActiveSessionId(propSessionId ?? null);
  }, [propSessionId, setActiveSessionId]);

  // ─── session bindings（用于 owner 展示） ──────────────

  useEffect(() => {
    if (!currentSessionId) return;
    let cancelled = false;
    void (async () => {
      try {
        const bindings = await fetchSessionBindings(currentSessionId);
        if (!cancelled) setLoadedSessionBindings(bindings);
      } catch {
        if (!cancelled) setLoadedSessionBindings([]);
      }
    })();
    return () => { cancelled = true; };
  }, [currentSessionId]);

  useEffect(() => {
    return () => {
      if (hookRuntimeRefreshTimerRef.current) {
        window.clearTimeout(hookRuntimeRefreshTimerRef.current);
        hookRuntimeRefreshTimerRef.current = null;
      }
    };
  }, []);

  useEffect(() => {
    if (!currentSessionId) return;
    let cancelled = false;
    void (async () => {
      try {
        const runtime = await fetchSessionHookRuntime(currentSessionId);
        if (!cancelled) setLoadedHookRuntime(runtime);
      } catch {
        if (!cancelled) setLoadedHookRuntime(null);
      }
    })();
    return () => {
      cancelled = true;
      if (hookRuntimeRefreshTimerRef.current) {
        window.clearTimeout(hookRuntimeRefreshTimerRef.current);
        hookRuntimeRefreshTimerRef.current = null;
      }
    };
  }, [currentSessionId]);

  const sessionBindings = currentSessionId ? loadedSessionBindings : EMPTY_SESSION_BINDINGS;
  const activeHookRuntime = loadedHookRuntime?.session_id === currentSessionId
    ? loadedHookRuntime
    : null;

  const sessionOwnerBinding = useMemo(() => {
    if (sessionBindings.length === 0) return null;
    return (
      sessionBindings.find((b) => b.owner_type === "project")
      ?? sessionBindings.find((b) => b.owner_type === "story")
      ?? sessionBindings.find((b) => b.owner_type === "task")
      ?? sessionBindings[0]
      ?? null
    );
  }, [sessionBindings]);

  const sessionContextSourceKey = useMemo(() => {
    if (!sessionOwnerBinding) return null;
    if (sessionOwnerBinding.owner_type === "task" && sessionOwnerBinding.task_id) {
      return `task:${sessionOwnerBinding.task_id}`;
    }
    if (
      sessionOwnerBinding.owner_type === "story"
      && sessionOwnerBinding.story_id
      && sessionOwnerBinding.id
    ) {
      return `story:${sessionOwnerBinding.story_id}:${sessionOwnerBinding.id}`;
    }
    if (
      sessionOwnerBinding.owner_type === "project"
      && sessionOwnerBinding.project_id
      && sessionOwnerBinding.id
    ) {
      return `project:${sessionOwnerBinding.project_id}:${sessionOwnerBinding.id}`;
    }
    return null;
  }, [sessionOwnerBinding]);

  useEffect(() => {
    if (!sessionOwnerBinding || !sessionContextSourceKey || !currentSessionId) return;
    let cancelled = false;
    void (async () => {
      const ctx = await fetchSessionContext(currentSessionId);
      if (cancelled) return;
      setLoadedSessionContext({
        source_key: sessionContextSourceKey,
        workspace_id: ctx?.workspace_id ?? null,
        task_agent_binding: ctx?.agent_binding ?? null,
        address_space: ctx?.address_space ?? null,
        context_snapshot: ctx?.context_snapshot ?? null,
      });
    })();
    return () => {
      cancelled = true;
    };
  }, [sessionOwnerBinding, sessionContextSourceKey, currentSessionId]);

  const activeSessionContext = loadedSessionContext?.source_key === sessionContextSourceKey
    ? loadedSessionContext
    : null;
  const taskAgentBinding = taskContextFromRoute?.agent_binding
    ?? activeSessionContext?.task_agent_binding
    ?? null;
  const sessionWorkspaceId = activeSessionContext?.workspace_id ?? null;
  const sessionAddressSpace = activeSessionContext?.address_space ?? null;
  const sessionContextSnapshot = activeSessionContext?.context_snapshot ?? null;
  const taskExecutorSummary = sessionContextSnapshot?.executor ?? null;

  const fetchStoryById = useStoryStore((s) => s.fetchStoryById);
  const storiesByProjectId = useStoryStore((s) => s.storiesByProjectId);
  const ownerStoryId = sessionOwnerBinding?.story_id ?? null;
  const ownerProjectName = sessionOwnerBinding?.owner_type === "project"
    ? sessionOwnerBinding.owner_title?.trim() || sessionOwnerBinding.owner_id
    : "";

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
  const ownerProjectId = sessionOwnerBinding?.project_id ?? ownerStory?.project_id ?? null;
  const ownerProject = ownerProjectId
    ? projects.find((project) => project.id === ownerProjectId) ?? null
    : null;
  const projectWorkspaces = useWorkspaceStore((s) =>
    ownerProjectId ? s.workspacesByProjectId[ownerProjectId] : undefined,
  );
  const ownerProjectWorkspaces = projectWorkspaces ?? EMPTY_WORKSPACES;

  useEffect(() => {
    if (!ownerProjectId) return;
    void fetchWorkspaces(ownerProjectId);
  }, [fetchWorkspaces, ownerProjectId]);

  const effectiveReturnTarget = useMemo(() => {
    if (returnTarget) return returnTarget;
    if (sessionOwnerBinding?.owner_type === "project") {
      return {
        owner_type: "project" as const,
        project_id: sessionOwnerBinding.project_id ?? sessionOwnerBinding.owner_id,
      };
    }
    if (!sessionOwnerBinding?.story_id) return null;
    if (sessionOwnerBinding.owner_type === "story") {
      return { owner_type: "story" as const, story_id: sessionOwnerBinding.story_id };
    }
    if (!sessionOwnerBinding.task_id) return null;
    return { owner_type: "task" as const, story_id: sessionOwnerBinding.story_id, task_id: sessionOwnerBinding.task_id };
  }, [returnTarget, sessionOwnerBinding]);

  // ─── 页面级回调 ───────────────────────────────────────

  const executorHint = taskAgentBinding?.agent_type
    ?? projectAgentContext?.executor_hint
    ?? taskExecutorSummary?.executor
    ?? null;
  const chatWorkspaceId =
    sessionWorkspaceId
    ?? ownerStory?.default_workspace_id
    ?? ownerProject?.config.default_workspace_id
    ?? ownerProjectWorkspaces[0]?.id
    ?? null;

  const handleCreateSession = useCallback(async (title: string) => {
    const meta = await createNew(title);
    return meta.id;
  }, [createNew]);

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

  const handleSystemEvent = useCallback((eventType: string, update: SessionUpdate) => {
    switch (eventType) {
      case "hook_event":
      case "hook_action_resolved":
      case "companion_dispatch_registered":
      case "companion_result_available":
      case "companion_result_returned":
        scheduleHookRuntimeRefresh(eventType);
        break;
      case "canvas_presented": {
        const event = extractAgentDashMetaFromUpdate(update)?.event;
        const data = isRecord(event?.data) ? event.data : null;
        const nextCanvasIdRaw = data?.canvas_id ?? data?.canvasId ?? data?.id;
        const nextCanvasId = typeof nextCanvasIdRaw === "string"
          ? nextCanvasIdRaw.trim()
          : "";
        if (nextCanvasId) {
          setActiveCanvasId(nextCanvasId);
          setIsCanvasPanelOpen(true);
        }
        break;
      }
      default:
        break;
    }
  }, [scheduleHookRuntimeRefresh]);

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

  const backButtonLabel = effectiveReturnTarget?.owner_type === "project"
    ? "返回项目"
    : effectiveReturnTarget?.owner_type === "task"
      ? "返回任务"
      : "返回 Story";
  const hasSession = currentSessionId !== null;

  // ─── owner binding 信息条（作为 inputPrefix 传入 ChatView）

  const ownerBindingBar = sessionOwnerBinding ? (
    <div className="mb-3 flex flex-wrap items-center gap-2 rounded-[12px] border border-border bg-secondary/20 px-3 py-2 text-xs text-muted-foreground">
      <span className="rounded-full border border-border bg-background px-2 py-0.5 uppercase">
        {sessionOwnerBinding.owner_type}
      </span>
      <span>
        已绑定：{sessionOwnerBinding.owner_title?.trim() || sessionOwnerBinding.owner_id}
      </span>
      {sessionOwnerBinding.owner_type === "project" && sessionContextSnapshot?.owner_context.owner_level === "project" && sessionContextSnapshot.owner_context.agent_display_name && (
        <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[11px] text-foreground/80">
          Agent · {sessionContextSnapshot.owner_context.agent_display_name}
        </span>
      )}
      {(sessionOwnerBinding.project_id || sessionOwnerBinding.story_id) && (
        <button
          type="button"
          onClick={handleBackToOwner}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] transition-colors hover:bg-secondary hover:text-foreground"
        >
          打开关联
          {sessionOwnerBinding.owner_type === "project"
            ? "项目"
            : sessionOwnerBinding.owner_type === "task"
              ? "任务"
              : "Story"}
        </button>
      )}
    </div>
  ) : null;

  // ─── 渲染 ────────────────────────────────────────────

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* 页面 Header */}
      <header className="flex shrink-0 items-center justify-between border-b border-border bg-background px-5 py-3.5">
        <div className="flex min-w-0 items-center gap-2.5">
          <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            CHAT
          </span>
          <h2 className="text-sm font-semibold text-foreground">会话</h2>
        </div>
        <div className="flex items-center gap-2">
          {effectiveReturnTarget && (
            <button type="button" onClick={handleBackToOwner} className="rounded-[10px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground">
              {backButtonLabel}
            </button>
          )}
          {hasSession && (
            <>
              <span className="hidden rounded-full border border-border bg-secondary px-2.5 py-1 text-xs font-mono text-muted-foreground lg:inline">
                {currentSessionId.slice(0, 12)}…
              </span>
              <button type="button" onClick={() => void handleCopySessionId()} className="rounded-[10px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground" title="复制 Session ID">
                复制
              </button>
            </>
          )}
          <button type="button" onClick={handleNewSession} className="rounded-[10px] border border-border bg-secondary px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-secondary/80">
            新会话
          </button>
          {activeCanvasId && !isCanvasPanelOpen && (
            <button
              type="button"
              onClick={() => setIsCanvasPanelOpen(true)}
              className="rounded-[10px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            >
              打开 Canvas
            </button>
          )}
        </div>
      </header>

      {sessionContextSnapshot?.owner_context.owner_level === "project" && (
        <ProjectSessionContextPanel
          projectId={sessionOwnerBinding?.project_id ?? ""}
          projectName={ownerProjectName}
          contextSnapshot={sessionContextSnapshot}
          addressSpace={sessionAddressSpace}
          hookRuntime={activeHookRuntime}
          ownerType={sessionOwnerBinding?.owner_type}
          ownerId={sessionOwnerBinding?.owner_id}
          isOpen={isContextPanelOpen}
          onToggle={() => setIsContextPanelOpen((value) => !value)}
        />
      )}

      {sessionContextSnapshot?.owner_context.owner_level !== "project" && ownerStory && (
        hasStoryContextInfo(ownerStory)
        || sessionContextSnapshot != null
        || (sessionAddressSpace && sessionAddressSpace.mounts.length > 0)
      ) && (
        <StorySessionContextPanel
          story={ownerStory}
          contextSnapshot={sessionContextSnapshot}
          executorSummary={taskExecutorSummary}
          addressSpace={sessionAddressSpace}
          hookRuntime={activeHookRuntime}
          ownerType={sessionOwnerBinding?.owner_type}
          ownerId={sessionOwnerBinding?.owner_id}
          isOpen={isContextPanelOpen}
          onToggle={() => setIsContextPanelOpen((value) => !value)}
        />
      )}

      {/* 复用的聊天视图 + Canvas 侧栏 */}
      <div className="flex flex-1 overflow-hidden">
        <div className="min-w-0 flex-1 overflow-hidden">
          <SessionChatView
            sessionId={currentSessionId}
            workspaceId={chatWorkspaceId}
            onCreateSession={handleCreateSession}
            onSessionIdChange={handleSessionIdChange}
            onMessageSent={handleMessageSent}
            onTurnEnd={handleTurnEnd}
            onSystemEvent={handleSystemEvent}
            executorHint={executorHint}
            inputPrefix={ownerBindingBar}
          />
        </div>
        {isCanvasPanelOpen && activeCanvasId && (
          <div className="w-[55vw] max-w-[1100px] min-w-[680px] shrink-0 border-l border-border">
            <CanvasSessionPanel
              canvasId={activeCanvasId}
              sessionId={currentSessionId}
              onClose={() => setIsCanvasPanelOpen(false)}
            />
          </div>
        )}
      </div>
    </div>
  );
}

export default SessionPage;
