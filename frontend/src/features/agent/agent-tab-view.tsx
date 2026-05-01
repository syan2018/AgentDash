/**
 * AgentTabView — Agent-First 主视图
 *
 * 布局：左右双栏
 *   左栏：ProjectAgentView（完整 Agent Hub，含创建/编辑/删除预设）
 *   右栏：
 *     - 未选中会话：展示 ActiveSessionList
 *     - 已选中会话：展示面包屑 + SessionChatView
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ProjectAgentSummary, SessionNavigationState } from "../../types";
import type { BackboneEvent } from "../../generated/backbone-protocol";
import { useProjectStore } from "../../stores/projectStore";
import { useActiveSessionsStore } from "../../stores/activeSessionsStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { SessionChatView } from "../acp-session";
import { ActiveSessionList } from "./active-session-list";
import { ProjectAgentView } from "../project/project-agent-view";

const COMPANION_EVENT_TYPES = new Set([
  "companion_dispatch_registered",
  "companion_result_available",
  "companion_result_returned",
  "companion_review_request",
  "companion_human_request",
  "companion_human_response",
]);

const SESSION_LIST_POLL_INTERVAL = 8_000;

export function AgentTabView() {
  const navigate = useNavigate();
  const {
    currentProjectId,
    projects,
    agentsByProjectId,
    fetchProjectAgents,
    openProjectAgentSession,
    forceNewProjectAgentSession,
    isLoading: projectLoading,
    error: projectError,
  } = useProjectStore();

  const { sessions, isLoading: sessionsLoading, loadForProject, clearForProject } = useActiveSessionsStore();

  // 右栏当前选中的 session，按 project 作用域隔离，避免在 effect 内同步重置 state
  const [selectedSession, setSelectedSession] = useState<{
    projectId: string | null;
    sessionId: string | null;
  }>({
    projectId: null,
    sessionId: null,
  });

  const runsBySessionId = useWorkflowStore((s) => s.runsBySessionId);
  const fetchRunsBySession = useWorkflowStore((s) => s.fetchRunsBySession);
  const lifecycleDefinitions = useWorkflowStore((s) => s.lifecycleDefinitions);
  const fetchLifecycles = useWorkflowStore((s) => s.fetchLifecycles);
  const fetchDefinitions = useWorkflowStore((s) => s.fetchDefinitions);

  const currentProject = projects.find((p) => p.id === currentProjectId);
  const workspaceId = currentProject?.config.default_workspace_id ?? null;
  const agents: ProjectAgentSummary[] = currentProjectId
    ? (agentsByProjectId[currentProjectId] ?? [])
    : [];

  // 通过当前选中的 session（或首个 session）查找活跃 lifecycle run
  const primarySessionId = (selectedSession.projectId === currentProjectId
    ? selectedSession.sessionId
    : null) ?? sessions[0]?.session_id ?? null;
  const sessionRuns = primarySessionId
    ? (runsBySessionId[primarySessionId] ?? [])
    : [];
  const activeRun = sessionRuns.find(
    (r) => r.status === "ready" || r.status === "running" || r.status === "blocked",
  );
  const activeLifecycleName = activeRun
    ? (lifecycleDefinitions.find((l) => l.id === activeRun.lifecycle_id)?.name ?? activeRun.lifecycle_id)
    : null;

  const selectedSessionId = selectedSession.projectId === currentProjectId
    ? selectedSession.sessionId
    : null;
  // 当前选中会话的元数据（用于面包屑显示标题）
  const currentSession = sessions.find((s) => s.session_id === selectedSessionId);
  // 当前选中会话所属 agent 的默认执行器配置（用来 hydrate 右侧配置条）
  const currentAgentDefaults = selectedSessionId
    ? (agents.find((a) => a.session?.session_id === selectedSessionId)?.executor ?? null)
    : null;

  // 项目切换时：立即清空旧数据（防止短暂展示错误项目内容），再异步加载新数据
  const prevProjectIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (!currentProjectId) return;
    if (prevProjectIdRef.current === currentProjectId) return;
    prevProjectIdRef.current = currentProjectId;

    clearForProject(currentProjectId);
    void fetchProjectAgents(currentProjectId);
    void loadForProject(currentProjectId);
    void fetchLifecycles();
    void fetchDefinitions();
  }, [currentProjectId, fetchProjectAgents, loadForProject, clearForProject, fetchLifecycles, fetchDefinitions]);

  // session 关联的 lifecycle runs（session 就绪后加载）
  useEffect(() => {
    if (primarySessionId) void fetchRunsBySession(primarySessionId);
  }, [primarySessionId, fetchRunsBySession]);

  // ─── companion 事件驱动 + 周期轮询：保持 session 列表实时 ──────

  const scheduleSessionRefresh = useCallback(() => {
    if (!currentProjectId) return;
    void loadForProject(currentProjectId);
  }, [currentProjectId, loadForProject]);

  const handleSystemEvent = useCallback(
    (eventType: string, _event: BackboneEvent) => {
      if (COMPANION_EVENT_TYPES.has(eventType)) {
        scheduleSessionRefresh();
      }
    },
    [scheduleSessionRefresh],
  );

  const handleTurnEnd = useCallback(() => {
    scheduleSessionRefresh();
  }, [scheduleSessionRefresh]);

  useEffect(() => {
    if (!currentProjectId || !selectedSessionId) return;
    const timer = window.setInterval(scheduleSessionRefresh, SESSION_LIST_POLL_INTERVAL);
    return () => window.clearInterval(timer);
  }, [currentProjectId, selectedSessionId, scheduleSessionRefresh]);

  // ─── 打开 Agent 会话 → 在右栏展开 ──────────────────

  const handleOpenAgent = useCallback(
    async (agent: ProjectAgentSummary) => {
      if (!currentProjectId) return;
      const result = await openProjectAgentSession(currentProjectId, agent.key);
      if (!result) return;
      await loadForProject(currentProjectId);
      setSelectedSession({
        projectId: currentProjectId,
        sessionId: result.session_id,
      });
    },
    [currentProjectId, openProjectAgentSession, loadForProject],
  );

  const handleForceNewSession = useCallback(
    async (agent: ProjectAgentSummary) => {
      if (!currentProjectId) return;
      const result = await forceNewProjectAgentSession(currentProjectId, agent.key);
      if (!result) return;
      await loadForProject(currentProjectId);
      setSelectedSession({
        projectId: currentProjectId,
        sessionId: result.session_id,
      });
    },
    [currentProjectId, forceNewProjectAgentSession, loadForProject],
  );

  // ─── 无项目时的占位 ───────────────────────────────

  if (!currentProjectId || !currentProject) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-foreground">请选择或创建项目</h2>
          <p className="mt-2 text-sm text-muted-foreground">在左侧面板选择一个项目开始使用</p>
        </div>
      </div>
    );
  }

  // ─── 主布局 ───────────────────────────────────────

  return (
    <div className="flex h-full overflow-hidden">
      {/* ── 左栏：完整 ProjectAgentView ── */}
      <aside className="flex h-full w-[360px] shrink-0 flex-col overflow-y-auto border-r border-border">
        <ProjectAgentView
          project={currentProject}
          agents={agents}
          isLoading={projectLoading}
          error={projectError}
          onOpenAgent={handleOpenAgent}
          onForceNewSession={handleForceNewSession}
        />
      </aside>

      {/* ── 右栏：活跃会话列表 或 SessionChatView ── */}
      <div className="flex flex-1 flex-col overflow-hidden">
        {selectedSessionId === null ? (
          /* 展示活跃会话列表 */
          <ActiveSessionList
            projectId={currentProjectId}
            sessions={sessions}
            isLoading={sessionsLoading}
            selectedSessionId={null}
            onSelectSession={(sessionId) => setSelectedSession({
              projectId: currentProjectId,
              sessionId,
            })}
          />
        ) : (
          /* 展示面包屑 + 聊天视图 */
          <>
            {/* 面包屑导航 */}
            <div className="flex shrink-0 items-center gap-2 border-b border-border bg-background px-4 py-2">
              <button
                type="button"
                onClick={() => setSelectedSession({
                  projectId: currentProjectId,
                  sessionId: null,
                })}
                className="text-sm text-muted-foreground transition-colors hover:text-foreground"
              >
                ← 活跃会话
              </button>
              <span className="text-muted-foreground/40">/</span>
              <span className="truncate text-sm font-medium text-foreground">
                {currentSession?.session_title ?? "会话"}
              </span>
              {activeRun && (
                <span className="shrink-0 rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-[10px] text-primary">
                  {activeLifecycleName}
                  {activeRun.active_node_keys?.[0] ? ` · ${activeRun.active_node_keys?.[0]}` : ""}
                  {activeRun.status === "running" ? " ▶" : activeRun.status === "blocked" ? " ⏸" : ""}
                </span>
              )}
              <button
                type="button"
                onClick={() => {
                  const state: SessionNavigationState = {
                    return_to: { owner_type: "project", project_id: currentProjectId },
                  };
                  navigate(`/session/${selectedSessionId}`, { state });
                }}
                className="ml-auto shrink-0 rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                title="在独立页面全屏打开"
              >
                全屏 ↗
              </button>
            </div>

            {/* 聊天视图 */}
            <div className="flex-1 overflow-hidden">
              <SessionChatView
                sessionId={selectedSessionId}
                workspaceId={workspaceId}
                showStatusBar={false}
                showExecutorSelector
                agentDefaults={currentAgentDefaults}
                onSystemEvent={handleSystemEvent}
                onTurnEnd={handleTurnEnd}
              />
            </div>
          </>
        )}
      </div>
    </div>
  );
}
