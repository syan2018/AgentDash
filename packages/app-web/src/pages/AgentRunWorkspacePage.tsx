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
import { SessionChatView } from "../features/session";
import { useProjectExtensionRuntime } from "../features/extension-runtime";
import { agentSourceLabel } from "../lib/agent-source";
import { useAgentRunWorkspaceControlPlane } from "../features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane";
import { refreshAgentRunListProjection } from "../features/agent/agent-run-list-projection-store";
import {
  WorkspacePanel,
  type WorkspacePanelHandle,
  type WorkspaceRuntimeData,
} from "../features/workspace-panel";
import { useAgentRunWorkspaceState } from "../features/workspace-panel/model/useAgentRunWorkspaceState";
import { useProjectStore } from "../stores/projectStore";
import { findStoryById, useStoryStore } from "../stores/storyStore";
import { findWorkspaceBinding, useWorkspaceStore } from "../stores/workspaceStore";
import {
  listProjectBackendAccess,
  type ProjectBackendAccess,
} from "../services/backendAccess";
import type { BackendSelectionRequestDto } from "../generated/agent-run-mailbox-contracts";
import { useWorkspaceModuleStore } from "../features/workspace-module";
import type {
  RuntimeTraceAgentContext,
  SessionNavigationState,
  AgentRunWorkspaceView,
  SubjectRunContext,
  ProjectAgentSummary,
  ProjectAgentRunStartResult,
  Story,
  StoryNavigationState,
} from "../types";

// ─── AgentRunWorkspacePage ────────────────────────────────────────

interface AgentRunWorkspacePageProps {
  runId?: string;
  agentId?: string;
  draftProjectId?: string;
  draftProjectAgentId?: string;
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
  const fetchWorkspaces = useWorkspaceStore((state) => state.fetchWorkspaces);
  const workspacesByProjectId = useWorkspaceStore((state) => state.workspacesByProjectId);
  const fetchWorkspaceModules = useWorkspaceModuleStore((state) => state.fetchProject);

  const [loadedOwnerStory, setLoadedOwnerStory] = useState<{
    story_id: string;
    story: Story | null;
  } | null>(null);
  const [backendAccesses, setBackendAccesses] = useState<ProjectBackendAccess[]>([]);
  const [selectedBackendId, setSelectedBackendId] = useState("");
  const workspacePanelRef = useRef<WorkspacePanelHandle>(null);
  const rightPanelRef = useRef<PanelImperativeHandle>(null);

  const expandWorkspacePanel = useCallback((
    typeId?: string,
    uri?: string,
    options?: { refreshContent?: boolean },
  ) => {
    if (typeId) {
      workspacePanelRef.current?.openTab(typeId, uri, options);
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

  const runtimeControl: AgentRunWorkspaceView | null = agentRunWorkspaceState.workspace;
  const deliveryRuntimeSessionId = agentRunWorkspaceState.runtime_session_id;
  const draftWorkspaceTitle =
    draftProjectAgent?.display_name
    ?? traceAgentContext?.display_name
    ?? "新 AgentRun";
  const workspaceTitle = isProjectAgentDraft
    ? draftWorkspaceTitle
    : runtimeControl?.shell.display_title ?? "";

  // ─── 身份 / 从属信息（identity bar）─────────────────────
  const identityAgentSource = agentSourceLabel(runtimeControl?.agent?.source);
  const identitySubject = useMemo(() => {
    const assoc = runtimeControl?.subject_associations?.[0];
    if (!assoc) return null;
    let label = assoc.subject_ref.kind;
    const meta = assoc.metadata;
    if (meta && typeof meta === "object") {
      for (const key of ["label", "title", "name"]) {
        const value = (meta as Record<string, unknown>)[key];
        if (typeof value === "string" && value.trim()) {
          label = value.trim();
          break;
        }
      }
    }
    return { kind: assoc.subject_ref.kind, id: assoc.subject_ref.id, label };
  }, [runtimeControl?.subject_associations]);
  const lineageParent = runtimeControl?.parent ?? null;
  const subagentChildCount = runtimeControl?.children?.length ?? 0;
  const hasIdentityBar =
    !isProjectAgentDraft
    && (identityAgentSource !== null || identitySubject !== null || lineageParent !== null || subagentChildCount > 0);
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
  const refreshWorkspaceModuleCatalog = useCallback(() => {
    if (!ownerProjectId) return;
    void fetchWorkspaceModules(ownerProjectId);
  }, [fetchWorkspaceModules, ownerProjectId]);
  const ownerProject = ownerProjectId
    ? projects.find((project) => project.id === ownerProjectId) ?? null
    : null;
  const ownerProjectName = runContext?.scope === "project"
    ? (ownerProject?.name?.trim() || runContext.project_id)
    : isProjectAgentDraft
      ? (ownerProject?.name?.trim() || "")
    : "";
  const extensionRuntime = useProjectExtensionRuntime(ownerProjectId);
  const refreshAgentRunList = useCallback((reason: string) => {
    refreshAgentRunListProjection(ownerProjectId ?? draftProjectIdValue, reason);
  }, [draftProjectIdValue, ownerProjectId]);

  useEffect(() => {
    if (!ownerProjectId) return;
    void fetchWorkspaces(ownerProjectId);
  }, [fetchWorkspaces, ownerProjectId]);

  useEffect(() => {
    if (!ownerProjectId) {
      setBackendAccesses([]);
      setSelectedBackendId("");
      return;
    }
    let cancelled = false;
    void listProjectBackendAccess(ownerProjectId)
      .then((items) => {
        if (cancelled) return;
        setBackendAccesses(items);
      })
      .catch(() => {
        if (cancelled) return;
        setBackendAccesses([]);
      });
    return () => {
      cancelled = true;
    };
  }, [ownerProjectId]);

  const activeBackendAccesses = useMemo(
    () => backendAccesses.filter((access) => access.status === "active"),
    [backendAccesses],
  );

  useEffect(() => {
    if (!selectedBackendId) return;
    if (activeBackendAccesses.some((access) => access.backend_id === selectedBackendId)) return;
    setSelectedBackendId("");
  }, [activeBackendAccesses, selectedBackendId]);

  const selectedBackendSelection = useMemo<BackendSelectionRequestDto | undefined>(() => {
    const backendId = selectedBackendId.trim();
    if (!backendId) return undefined;
    return { mode: "explicit", backend_id: backendId };
  }, [selectedBackendId]);

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

  const handleDraftAgentRunStarted = useCallback((response: ProjectAgentRunStartResult) => {
    refreshAgentRunListProjection(draftProjectIdValue, "draft_started");
    navigate(`/agent-runs/${encodeURIComponent(response.run_ref.run_id)}/${encodeURIComponent(response.agent_ref.agent_id)}`, {
      replace: true,
      state: {
        trace_agent: {
          display_name: response.agent.display_name,
          executor_hint: response.agent.executor.executor,
        },
      },
    });
  }, [draftProjectIdValue, navigate]);

  const {
    chatModel: controlPlaneChatModel,
    chatIntents: controlPlaneChatIntents,
    handleMessageSent,
    handleTurnEnd,
    handleTaskPlanChanged,
    handleSystemEvent,
    handleWorkspaceModuleOpened,
  } = useAgentRunWorkspaceControlPlane({
    currentRunId,
    currentAgentId,
    draftProjectId: draftProjectIdValue,
    draftProjectAgentKey,
    draftProjectAgent,
    isProjectAgentDraft,
    agentRunWorkspaceState,
    refreshAgentRunWorkspaceState,
    refreshAgentRunHookRuntime,
    traceExecutorHint: traceAgentContext?.executor_hint,
    taskExecutorSummary,
    createProjectAgentRun,
    onDraftStarted: handleDraftAgentRunStarted,
    refreshAgentRunList,
    refreshWorkspaceModuleCatalog,
    openWorkspacePanel: ({ typeId, uri, options }) => {
      expandWorkspacePanel(typeId, uri, options);
    },
  });

  const chatIntents = useMemo(() => ({
    ...controlPlaneChatIntents,
    submitComposer: (intent: Parameters<typeof controlPlaneChatIntents.submitComposer>[0]) =>
      controlPlaneChatIntents.submitComposer({
        ...intent,
        backendSelection: selectedBackendSelection,
      }),
  }), [controlPlaneChatIntents, selectedBackendSelection]);

  const chatModel = useMemo(() => ({
    ...controlPlaneChatModel,
    workspaceId: chatWorkspaceId,
  }), [chatWorkspaceId, controlPlaneChatModel]);

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
  const backendSelectionBar = activeBackendAccesses.length > 0 ? (
    <div className="mb-3 flex flex-wrap items-center gap-2 rounded-[12px] border border-border bg-secondary/20 px-3 py-2 text-xs text-muted-foreground">
      <span className="rounded-[8px] border border-border bg-background px-2 py-0.5 uppercase">
        Backend
      </span>
      <select
        value={selectedBackendId}
        onChange={(event) => setSelectedBackendId(event.target.value)}
        className="min-w-[180px] rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-foreground outline-none transition-colors hover:bg-secondary focus:border-primary"
      >
        <option value="">默认</option>
        {activeBackendAccesses.map((access) => (
          <option key={access.id} value={access.backend_id}>
            {access.backend_id}
          </option>
        ))}
      </select>
    </div>
  ) : null;
  const chatInputPrefix = (
    <>
      {ownerBindingBar ?? draftBindingBar}
      {backendSelectionBar}
    </>
  );

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

      {hasIdentityBar && (
        <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border bg-background/60 px-5 py-1.5 text-[11px] text-muted-foreground">
          {identityAgentSource && (
            <span className="inline-flex items-center gap-1">
              <span className="text-muted-foreground/60">来源</span>
              <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 font-medium text-foreground">
                {identityAgentSource}
              </span>
            </span>
          )}
          {identitySubject && (
            <button
              type="button"
              onClick={() => navigate(`/subject/${encodeURIComponent(identitySubject.kind)}/${encodeURIComponent(identitySubject.id)}`)}
              className="inline-flex items-center gap-1 rounded-[6px] px-1.5 py-0.5 transition-colors hover:bg-secondary hover:text-foreground"
              title="查看所属 subject"
            >
              <span className="text-muted-foreground/60">{identitySubject.kind}</span>
              <span className="font-medium text-foreground">{identitySubject.label}</span>
            </button>
          )}
          {lineageParent && (
            <button
              type="button"
              onClick={() => navigate(`/agent-runs/${encodeURIComponent(lineageParent.run_id)}/${encodeURIComponent(lineageParent.agent_id)}`)}
              className="inline-flex items-center gap-1 rounded-[6px] px-1.5 py-0.5 transition-colors hover:bg-secondary hover:text-foreground"
              title="跳转到父 Run"
            >
              <span aria-hidden>←</span>
              <span className="text-muted-foreground/60">隶属于</span>
              <span className="max-w-[200px] truncate font-medium text-foreground">
                {lineageParent.display_title.trim() || agentSourceLabel(lineageParent.source) || "父 Run"}
              </span>
            </button>
          )}
          {subagentChildCount > 0 && agentRunDetailTarget && (
            <button
              type="button"
              onClick={handleOpenRunDetail}
              className="inline-flex items-center gap-1 rounded-[6px] px-1.5 py-0.5 transition-colors hover:bg-secondary hover:text-foreground"
              title="查看派发的 subagent"
            >
              <span className="font-medium text-foreground">{subagentChildCount}</span>
              <span>个 subagent</span>
            </button>
          )}
        </div>
      )}

      <Group orientation="horizontal" className="flex-1 overflow-hidden">
        <Panel minSize="30%">
          <div className="flex h-full flex-col overflow-hidden">
            <div className="min-h-0 flex-1 overflow-hidden">
              <SessionChatView
                model={chatModel}
                intents={chatIntents}
                onMessageSent={handleMessageSent}
                onTurnEnd={handleTurnEnd}
                onSystemEvent={handleSystemEvent}
                onTaskPlanChanged={handleTaskPlanChanged}
                inputPrefix={chatInputPrefix}
              />
            </div>
          </div>
        </Panel>

        <Separator className="group relative w-1.5 shrink-0 bg-border/30 transition-colors hover:bg-primary/30 active:bg-primary/50 data-[separator]:cursor-col-resize">
          <div className="absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 rounded-[8px] bg-border transition-colors group-hover:bg-primary/50 group-active:bg-primary" />
        </Separator>

        <Panel
          panelRef={rightPanelRef}
          defaultSize="0%"
          minSize="30%"
          maxSize="70%"
          collapsible
          collapsedSize="0%"
          className="border-l border-border"
        >
          <WorkspacePanel
            ref={workspacePanelRef}
            runtimeData={workspaceRuntimeData}
            onWorkspaceModuleOpened={handleWorkspaceModuleOpened}
          />
        </Panel>
      </Group>
    </div>
  );
}

export default AgentRunWorkspacePage;
