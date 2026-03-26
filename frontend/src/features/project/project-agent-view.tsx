import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import type {
  AgentEntity,
  Project,
  ProjectAgentLink,
  ProjectAgentSession,
  ProjectAgentSummary,
  SessionNavigationState,
} from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";

const EMPTY_LINKS: ProjectAgentLink[] = [];

export interface ProjectAgentViewProps {
  project: Project;
  agents: ProjectAgentSummary[];
  isLoading?: boolean;
  error?: string | null;
  onOpenAgent: (agent: ProjectAgentSummary) => void;
  onForceNewSession?: (agent: ProjectAgentSummary) => void;
}

function formatRelativeTime(timestamp: number | null | undefined): string {
  if (timestamp == null) return "无活动";
  const now = Date.now();
  const ts = timestamp < 1e12 ? timestamp * 1000 : timestamp;
  const diffMs = now - ts;
  if (diffMs < 0) return "刚刚";
  const seconds = Math.floor(diffMs / 1000);
  if (seconds < 60) return "刚刚";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  return `${days} 天前`;
}

type ActivityLevel = "active" | "recent" | "idle" | "none";

function getActivityLevel(timestamp: number | null | undefined): ActivityLevel {
  if (timestamp == null) return "none";
  const ts = timestamp < 1e12 ? timestamp * 1000 : timestamp;
  const diffMs = Date.now() - ts;
  if (diffMs < 5 * 60 * 1000) return "active";
  if (diffMs < 60 * 60 * 1000) return "recent";
  return "idle";
}

const activityDotClass: Record<ActivityLevel, string> = {
  active: "bg-emerald-500",
  recent: "bg-amber-400",
  idle: "bg-muted-foreground/30",
  none: "bg-muted-foreground/15",
};

function SessionHistoryPanel({
  projectId,
  agentKey,
  agentDisplayName,
  executorHint,
}: {
  projectId: string;
  agentKey: string;
  agentDisplayName: string;
  executorHint: string;
}) {
  const navigate = useNavigate();
  const { fetchProjectAgentSessions } = useProjectStore();
  const [sessions, setSessions] = useState<ProjectAgentSession[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [expanded, setExpanded] = useState(false);

  const loadHistory = useCallback(async () => {
    if (isLoading) return;
    setIsLoading(true);
    try {
      const result = await fetchProjectAgentSessions(projectId, agentKey);
      setSessions(result);
    } finally {
      setIsLoading(false);
    }
  }, [fetchProjectAgentSessions, projectId, agentKey, isLoading]);

  const toggleExpanded = () => {
    const next = !expanded;
    setExpanded(next);
    if (next && sessions.length === 0) void loadHistory();
  };

  const handleNavigate = (sessionId: string) => {
    const state: SessionNavigationState = {
      return_to: { owner_type: "project", project_id: projectId },
      project_agent: {
        agent_key: agentKey,
        display_name: agentDisplayName,
        executor_hint: executorHint,
      },
    };
    navigate(`/session/${sessionId}`, { state });
  };

  return (
    <div>
      <button type="button" onClick={toggleExpanded} className="text-[11px] text-muted-foreground transition-colors hover:text-foreground">
        {expanded ? "收起历史" : "查看历史会话"}
      </button>
      {expanded && (
        <div className="mt-2 max-h-36 space-y-1 overflow-y-auto">
          {isLoading && sessions.length === 0 && <p className="text-[11px] text-muted-foreground">加载中...</p>}
          {!isLoading && sessions.length === 0 && <p className="text-[11px] text-muted-foreground">暂无历史会话</p>}
          {sessions.map((s) => (
            <button
              key={s.binding_id}
              type="button"
              onClick={() => handleNavigate(s.session_id)}
              className="flex w-full items-center justify-between rounded-[8px] border border-border bg-secondary/30 px-2.5 py-1.5 text-left transition-colors hover:bg-secondary"
            >
              <span className="truncate text-xs text-foreground">{s.session_title ?? "无标题会话"}</span>
              <span className="ml-2 shrink-0 text-[10px] text-muted-foreground">{formatRelativeTime(s.last_activity)}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// ─── Agent 创建/链接对话框 ───

function CreateAgentDialog({
  open,
  projectId,
  onClose,
}: {
  open: boolean;
  projectId: string;
  onClose: () => void;
}) {
  const { createAgent, createProjectAgentLink, fetchProjectAgents, fetchAgents } = useProjectStore();
  const lifecycles = useWorkflowStore((s) => s.lifecycleDefinitions);
  const definitions = useWorkflowStore((s) => s.definitions);

  const [name, setName] = useState("");
  const [agentType, setAgentType] = useState("PI_AGENT");
  const [selectedLifecycleKey, setSelectedLifecycleKey] = useState("");
  const [selectedWorkflowKey, setSelectedWorkflowKey] = useState("");
  const [bindMode, setBindMode] = useState<"lifecycle" | "workflow" | "none">("none");
  const [isSaving, setIsSaving] = useState(false);

  if (!open) return null;

  const handleSave = async () => {
    if (!name.trim()) return;
    setIsSaving(true);
    try {
      const agent = await createAgent({ name: name.trim(), agent_type: agentType.trim() });
      if (!agent) return;

      const linkPayload: Parameters<typeof createProjectAgentLink>[1] = {
        agent_id: agent.id,
      };
      if (bindMode === "lifecycle" && selectedLifecycleKey) {
        linkPayload.default_lifecycle_key = selectedLifecycleKey;
      } else if (bindMode === "workflow" && selectedWorkflowKey) {
        linkPayload.default_workflow_key = selectedWorkflowKey;
      }
      await createProjectAgentLink(projectId, linkPayload);
      await fetchProjectAgents(projectId);
      await fetchAgents();
      onClose();
    } finally {
      setIsSaving(false);
    }
  };

  const activeLifecycles = lifecycles.filter((l) => l.status === "active");
  const activeWorkflows = definitions.filter((w) => w.status === "active");

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={onClose}>
      <div className="w-[480px] rounded-[14px] border border-border bg-background p-6 shadow-xl" onClick={(e) => e.stopPropagation()}>
        <h3 className="text-base font-semibold text-foreground">新建 Agent 并关联到项目</h3>

        <div className="mt-4 space-y-3">
          <div>
            <label className="text-xs font-medium text-muted-foreground">名称</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="如：code-reviewer"
              className="mt-1 w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary"
            />
          </div>

          <div>
            <label className="text-xs font-medium text-muted-foreground">执行器类型</label>
            <select
              value={agentType}
              onChange={(e) => setAgentType(e.target.value)}
              className="mt-1 w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary"
            >
              <option value="PI_AGENT">PI_AGENT</option>
              <option value="claude-code">Claude Code</option>
              <option value="codex-cli">Codex CLI</option>
            </select>
          </div>

          <div>
            <label className="text-xs font-medium text-muted-foreground">默认工作流绑定</label>
            <div className="mt-1 flex gap-1 rounded-[8px] border border-border bg-secondary/35 p-0.5">
              {(["none", "lifecycle", "workflow"] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => setBindMode(mode)}
                  className={`flex-1 rounded-[6px] px-2 py-1 text-xs transition-colors ${
                    bindMode === mode
                      ? "bg-background font-medium text-foreground shadow-sm"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {mode === "none" ? "无" : mode === "lifecycle" ? "Lifecycle" : "Workflow"}
                </button>
              ))}
            </div>

            {bindMode === "lifecycle" && (
              <select
                value={selectedLifecycleKey}
                onChange={(e) => setSelectedLifecycleKey(e.target.value)}
                className="mt-2 w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary"
              >
                <option value="">选择 Lifecycle…</option>
                {activeLifecycles.map((l) => (
                  <option key={l.key} value={l.key}>{l.name} ({l.key})</option>
                ))}
              </select>
            )}

            {bindMode === "workflow" && (
              <select
                value={selectedWorkflowKey}
                onChange={(e) => setSelectedWorkflowKey(e.target.value)}
                className="mt-2 w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary"
              >
                <option value="">选择 Workflow…</option>
                {activeWorkflows.map((w) => (
                  <option key={w.key} value={w.key}>{w.name} ({w.key})</option>
                ))}
              </select>
            )}
          </div>
        </div>

        <div className="mt-5 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-[8px] border border-border px-3.5 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-secondary"
          >
            取消
          </button>
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={!name.trim() || isSaving}
            className="rounded-[8px] border border-primary bg-primary px-3.5 py-1.5 text-sm text-primary-foreground transition-colors hover:opacity-95 disabled:opacity-50"
          >
            {isSaving ? "创建中…" : "创建"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── 关联已有 Agent 对话框 ───

function LinkExistingAgentDialog({
  open,
  projectId,
  excludeAgentIds,
  onClose,
}: {
  open: boolean;
  projectId: string;
  excludeAgentIds: Set<string>;
  onClose: () => void;
}) {
  const { agents, fetchAgents, createProjectAgentLink, fetchProjectAgents } = useProjectStore();
  const lifecycles = useWorkflowStore((s) => s.lifecycleDefinitions);
  const definitions = useWorkflowStore((s) => s.definitions);

  const [selectedAgentId, setSelectedAgentId] = useState("");
  const [selectedLifecycleKey, setSelectedLifecycleKey] = useState("");
  const [selectedWorkflowKey, setSelectedWorkflowKey] = useState("");
  const [bindMode, setBindMode] = useState<"lifecycle" | "workflow" | "none">("none");
  const [isSaving, setIsSaving] = useState(false);
  const loaded = useRef(false);

  useEffect(() => {
    if (open && !loaded.current) {
      loaded.current = true;
      void fetchAgents();
    }
  }, [open, fetchAgents]);

  if (!open) return null;

  const available = agents.filter((a) => !excludeAgentIds.has(a.id));
  const activeLifecycles = lifecycles.filter((l) => l.status === "active");
  const activeWorkflows = definitions.filter((w) => w.status === "active");

  const handleSave = async () => {
    if (!selectedAgentId) return;
    setIsSaving(true);
    try {
      const payload: Parameters<typeof createProjectAgentLink>[1] = {
        agent_id: selectedAgentId,
      };
      if (bindMode === "lifecycle" && selectedLifecycleKey) {
        payload.default_lifecycle_key = selectedLifecycleKey;
      } else if (bindMode === "workflow" && selectedWorkflowKey) {
        payload.default_workflow_key = selectedWorkflowKey;
      }
      await createProjectAgentLink(projectId, payload);
      await fetchProjectAgents(projectId);
      onClose();
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={onClose}>
      <div className="w-[440px] rounded-[14px] border border-border bg-background p-6 shadow-xl" onClick={(e) => e.stopPropagation()}>
        <h3 className="text-base font-semibold text-foreground">关联已有 Agent</h3>

        <div className="mt-4 space-y-3">
          <div>
            <label className="text-xs font-medium text-muted-foreground">选择 Agent</label>
            <select
              value={selectedAgentId}
              onChange={(e) => setSelectedAgentId(e.target.value)}
              className="mt-1 w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary"
            >
              <option value="">选择…</option>
              {available.map((a) => (
                <option key={a.id} value={a.id}>{a.name} ({a.agent_type})</option>
              ))}
            </select>
            {available.length === 0 && <p className="mt-1 text-xs text-muted-foreground">没有可关联的 Agent，请先新建</p>}
          </div>

          <div>
            <label className="text-xs font-medium text-muted-foreground">默认工作流</label>
            <div className="mt-1 flex gap-1 rounded-[8px] border border-border bg-secondary/35 p-0.5">
              {(["none", "lifecycle", "workflow"] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => setBindMode(mode)}
                  className={`flex-1 rounded-[6px] px-2 py-1 text-xs transition-colors ${
                    bindMode === mode ? "bg-background font-medium text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {mode === "none" ? "无" : mode === "lifecycle" ? "Lifecycle" : "Workflow"}
                </button>
              ))}
            </div>
            {bindMode === "lifecycle" && (
              <select value={selectedLifecycleKey} onChange={(e) => setSelectedLifecycleKey(e.target.value)} className="mt-2 w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary">
                <option value="">选择 Lifecycle…</option>
                {activeLifecycles.map((l) => <option key={l.key} value={l.key}>{l.name}</option>)}
              </select>
            )}
            {bindMode === "workflow" && (
              <select value={selectedWorkflowKey} onChange={(e) => setSelectedWorkflowKey(e.target.value)} className="mt-2 w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary">
                <option value="">选择 Workflow…</option>
                {activeWorkflows.map((w) => <option key={w.key} value={w.key}>{w.name}</option>)}
              </select>
            )}
          </div>
        </div>

        <div className="mt-5 flex justify-end gap-2">
          <button type="button" onClick={onClose} className="rounded-[8px] border border-border px-3.5 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-secondary">取消</button>
          <button type="button" onClick={() => void handleSave()} disabled={!selectedAgentId || isSaving} className="rounded-[8px] border border-primary bg-primary px-3.5 py-1.5 text-sm text-primary-foreground transition-colors hover:opacity-95 disabled:opacity-50">
            {isSaving ? "关联中…" : "关联"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── 主视图 ───

export function ProjectAgentView({
  project,
  agents,
  isLoading = false,
  error = null,
  onOpenAgent,
  onForceNewSession,
}: ProjectAgentViewProps) {
  const { deleteProjectAgentLink, fetchProjectAgents } = useProjectStore();
  const agentLinks = useProjectStore((s) => s.agentLinksByProjectId[project.id]) ?? EMPTY_LINKS;
  const fetchLinks = useProjectStore((s) => s.fetchProjectAgentLinks);

  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [isLinkOpen, setIsLinkOpen] = useState(false);

  useEffect(() => {
    void fetchLinks(project.id);
  }, [fetchLinks, project.id]);

  const linkedAgentIds = useMemo(
    () => new Set(agentLinks.map((l) => l.agent_id)),
    [agentLinks],
  );

  const findLinkForAgent = (agent: ProjectAgentSummary): ProjectAgentLink | undefined => {
    return agentLinks.find((l) => l.agent_id === agent.key);
  };

  const handleUnlink = async (agentId: string) => {
    await deleteProjectAgentLink(project.id, agentId);
    await fetchProjectAgents(project.id);
  };

  const sortedAgents = useMemo(() => {
    return [...agents].sort((a, b) => {
      const aTime = a.session?.last_activity ?? 0;
      const bTime = b.session?.last_activity ?? 0;
      return bTime - aTime;
    });
  }, [agents]);

  const activeCount = agents.filter((a) => a.session != null).length;

  if (isLoading && agents.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  return (
    <>
      <div className="flex h-full flex-col overflow-hidden">
        <header className="flex h-14 shrink-0 items-center justify-between gap-3 border-b border-border bg-background px-4">
          <div className="flex min-w-0 items-center gap-2">
            <span className="shrink-0 inline-flex rounded-[8px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              AGENT
            </span>
            <div className="min-w-0">
              <h2 className="truncate text-sm font-semibold tracking-tight text-foreground">Agent Hub</h2>
              <p className="truncate text-[11px] text-muted-foreground">
                {agents.length} 个 Agent
                {activeCount > 0 && ` · ${activeCount} 个活跃会话`}
              </p>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            <button
              type="button"
              onClick={() => setIsLinkOpen(true)}
              className="h-8 rounded-[8px] border border-border bg-background px-2.5 text-xs text-foreground transition-colors hover:bg-secondary"
            >
              + 关联已有
            </button>
            <button
              type="button"
              onClick={() => setIsCreateOpen(true)}
              className="h-8 rounded-[8px] border border-primary bg-primary px-2.5 text-xs text-primary-foreground transition-colors hover:opacity-95"
            >
              + 新建 Agent
            </button>
          </div>
        </header>

        {error && (
          <div className="shrink-0 border-b border-destructive/30 bg-destructive/10 px-6 py-2.5 text-sm text-destructive">
            Agent 列表加载异常：{error}
          </div>
        )}

        <div className="flex-1 overflow-y-auto p-4 pt-3">
          <div className="flex flex-col gap-3">
            {sortedAgents.map((agent) => {
              const activity = getActivityLevel(agent.session?.last_activity);
              const link = findLinkForAgent(agent);

              return (
                <article
                  key={agent.key}
                  className="flex flex-col rounded-[14px] border border-border bg-background/75 p-4"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex items-center gap-2.5">
                      <span
                        className={`mt-1 h-2.5 w-2.5 shrink-0 rounded-full ${activityDotClass[activity]}`}
                        title={formatRelativeTime(agent.session?.last_activity)}
                      />
                      <div>
                        <p className="text-lg font-semibold text-foreground">{agent.display_name}</p>
                        <p className="mt-0.5 text-sm leading-6 text-muted-foreground">{agent.description}</p>
                      </div>
                    </div>
                    <div className="flex shrink-0 items-center gap-1.5">
                      <span className="rounded-full border border-border bg-secondary px-2.5 py-1 text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
                        {agent.executor.executor}
                      </span>
                      <button
                        type="button"
                        onClick={() => void handleUnlink(agent.key)}
                        className="rounded-[6px] border border-destructive/30 px-2 py-0.5 text-[10px] text-destructive transition-colors hover:bg-destructive/10"
                        title="解除关联"
                      >
                        解除
                      </button>
                    </div>
                  </div>

                  <div className="mt-3 flex flex-wrap gap-1.5">
                    {link?.default_lifecycle_key && (
                      <span className="rounded-full border border-primary/30 bg-primary/10 px-2.5 py-0.5 text-[11px] text-primary">
                        Lifecycle: {link.default_lifecycle_key}
                      </span>
                    )}
                    {link?.is_default_for_story && (
                      <span className="rounded-full border border-border bg-secondary/60 px-2.5 py-0.5 text-[11px] text-muted-foreground">
                        Story 默认
                      </span>
                    )}
                    {link?.is_default_for_task && (
                      <span className="rounded-full border border-border bg-secondary/60 px-2.5 py-0.5 text-[11px] text-muted-foreground">
                        Task 默认
                      </span>
                    )}
                  </div>

                  <div className="mt-auto pt-4">
                    {agent.session && (
                      <div className="mb-2 flex items-center justify-between text-[11px] text-muted-foreground">
                        <span className="truncate">{agent.session.session_title ?? "会话进行中"}</span>
                        <span className="ml-2 shrink-0">{formatRelativeTime(agent.session.last_activity)}</span>
                      </div>
                    )}
                    <div className="mb-2.5">
                      <SessionHistoryPanel
                        projectId={project.id}
                        agentKey={agent.key}
                        agentDisplayName={agent.display_name}
                        executorHint={agent.executor.executor}
                      />
                    </div>
                    <div className="flex gap-2">
                      {(!onForceNewSession || !agent.session) && (
                        <button
                          type="button"
                          onClick={() => onOpenAgent(agent)}
                          className="flex-1 rounded-[10px] border border-primary bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-95"
                        >
                          {agent.session ? "继续对话" : "打开 Agent 会话"}
                        </button>
                      )}
                      {agent.session && onForceNewSession && (
                        <button
                          type="button"
                          onClick={() => onForceNewSession(agent)}
                          className="flex-1 rounded-[10px] border border-border bg-background px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                        >
                          新对话
                        </button>
                      )}
                    </div>
                  </div>
                </article>
              );
            })}

            {agents.length === 0 && (
              <p className="mt-6 text-center text-sm text-muted-foreground">暂无 Agent，点击右上角新建或关联已有</p>
            )}
          </div>
        </div>
      </div>

      <CreateAgentDialog
        open={isCreateOpen}
        projectId={project.id}
        onClose={() => setIsCreateOpen(false)}
      />

      <LinkExistingAgentDialog
        open={isLinkOpen}
        projectId={project.id}
        excludeAgentIds={linkedAgentIds}
        onClose={() => setIsLinkOpen(false)}
      />
    </>
  );
}
