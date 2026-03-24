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
import { useProjectStore } from "../../stores/projectStore";
import { useActiveSessionsStore } from "../../stores/activeSessionsStore";
import { SessionChatView } from "../acp-session";
import { ActiveSessionList } from "./active-session-list";
import { ProjectAgentView } from "../project/project-agent-view";

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

  // 右栏当前选中的 session ID
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);

  const currentProject = projects.find((p) => p.id === currentProjectId);
  const workspaceId = currentProject?.config.default_workspace_id ?? null;
  const agents: ProjectAgentSummary[] = currentProjectId
    ? (agentsByProjectId[currentProjectId] ?? [])
    : [];

  // 当前选中会话的元数据（用于面包屑显示标题）
  const currentSession = sessions.find((s) => s.session_id === selectedSessionId);

  // 项目切换时：立即清空旧数据（防止短暂展示错误项目内容），再异步加载新数据
  const prevProjectIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (!currentProjectId) return;
    if (prevProjectIdRef.current === currentProjectId) return;
    prevProjectIdRef.current = currentProjectId;

    setSelectedSessionId(null);
    clearForProject(currentProjectId);           // 立即清空 + 标记目标项目
    void fetchProjectAgents(currentProjectId);
    void loadForProject(currentProjectId);       // 异步加载，竞态保护在 store 内
  }, [currentProjectId, fetchProjectAgents, loadForProject, clearForProject]);

  // TODO: 等待后端补充 session_status_changed SSE 事件后，此处接入实时状态更新

  // ─── 打开 Agent 会话 → 在右栏展开 ──────────────────

  const handleOpenAgent = useCallback(
    async (agent: ProjectAgentSummary) => {
      if (!currentProjectId) return;
      const result = await openProjectAgentSession(currentProjectId, agent.key);
      if (!result) return;
      await loadForProject(currentProjectId);
      setSelectedSessionId(result.session_id);
    },
    [currentProjectId, openProjectAgentSession, loadForProject],
  );

  const handleForceNewSession = useCallback(
    async (agent: ProjectAgentSummary) => {
      if (!currentProjectId) return;
      const result = await forceNewProjectAgentSession(currentProjectId, agent.key);
      if (!result) return;
      await loadForProject(currentProjectId);
      setSelectedSessionId(result.session_id);
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
      <aside className="flex h-full w-[480px] shrink-0 flex-col overflow-y-auto border-r border-border">
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
            sessions={sessions}
            isLoading={sessionsLoading}
            selectedSessionId={null}
            onSelectSession={setSelectedSessionId}
          />
        ) : (
          /* 展示面包屑 + 聊天视图 */
          <>
            {/* 面包屑导航 */}
            <div className="flex shrink-0 items-center gap-2 border-b border-border bg-background px-4 py-2">
              <button
                type="button"
                onClick={() => setSelectedSessionId(null)}
                className="text-sm text-muted-foreground transition-colors hover:text-foreground"
              >
                ← 活跃会话
              </button>
              <span className="text-muted-foreground/40">/</span>
              <span className="truncate text-sm font-medium text-foreground">
                {currentSession?.session_title ?? "会话"}
              </span>
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
              />
            </div>
          </>
        )}
      </div>
    </div>
  );
}
