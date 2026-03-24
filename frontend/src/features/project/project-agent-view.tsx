import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import type {
  AgentPreset,
  Project,
  ProjectAgentSession,
  ProjectAgentSummary,
  ProjectConfig,
  SessionNavigationState,
} from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { SinglePresetDialog } from "./agent-preset-editor";

export interface ProjectAgentViewProps {
  project: Project;
  agents: ProjectAgentSummary[];
  isLoading?: boolean;
  error?: string | null;
  onOpenAgent: (agent: ProjectAgentSummary) => void;
  onForceNewSession?: (agent: ProjectAgentSummary) => void;
}

function formatWritebackMode(mode: ProjectAgentSummary["writeback_mode"]): string {
  return mode === "confirm_before_write" ? "确认后写回" : "只读";
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

function CardActionMenu({
  items,
}: {
  items: Array<{ key: string; label: string; onSelect: () => void; danger?: boolean }>;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const handlePointerDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="inline-flex h-7 w-7 items-center justify-center rounded-[8px] border border-border bg-secondary/60 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        title="操作"
      >
        ⋯
      </button>
      {open && (
        <div className="absolute right-0 top-9 z-[60] min-w-[9rem] rounded-[10px] border border-border bg-background p-1 shadow-xl">
          {items.map((item) => (
            <button
              key={item.key}
              type="button"
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              className={`w-full rounded-[7px] px-2.5 py-1.5 text-left text-xs transition-colors ${
                item.danger
                  ? "text-destructive hover:bg-destructive/10"
                  : "text-foreground hover:bg-secondary"
              }`}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

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
    if (next && sessions.length === 0) {
      void loadHistory();
    }
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
      <button
        type="button"
        onClick={toggleExpanded}
        className="text-[11px] text-muted-foreground transition-colors hover:text-foreground"
      >
        {expanded ? "收起历史" : "查看历史会话"}
      </button>
      {expanded && (
        <div className="mt-2 max-h-36 space-y-1 overflow-y-auto">
          {isLoading && sessions.length === 0 && (
            <p className="text-[11px] text-muted-foreground">加载中...</p>
          )}
          {!isLoading && sessions.length === 0 && (
            <p className="text-[11px] text-muted-foreground">暂无历史会话</p>
          )}
          {sessions.map((s) => (
            <button
              key={s.binding_id}
              type="button"
              onClick={() => handleNavigate(s.session_id)}
              className="flex w-full items-center justify-between rounded-[8px] border border-border bg-secondary/30 px-2.5 py-1.5 text-left transition-colors hover:bg-secondary"
            >
              <span className="truncate text-xs text-foreground">
                {s.session_title ?? "无标题会话"}
              </span>
              <span className="ml-2 shrink-0 text-[10px] text-muted-foreground">
                {formatRelativeTime(s.last_activity)}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function ProjectAgentView({
  project,
  agents,
  isLoading = false,
  error = null,
  onOpenAgent,
  onForceNewSession,
}: ProjectAgentViewProps) {
  const { updateProjectConfig, fetchProjectAgents } = useProjectStore();
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [editingPreset, setEditingPreset] = useState<AgentPreset | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  const sortedAgents = useMemo(() => {
    return [...agents].sort((a, b) => {
      const aTime = a.session?.last_activity ?? 0;
      const bTime = b.session?.last_activity ?? 0;
      return bTime - aTime;
    });
  }, [agents]);

  const existingPresetNames = useMemo(
    () => (project.config.agent_presets ?? []).map((p) => p.name),
    [project.config.agent_presets],
  );

  const savePresets = async (nextPresets: ProjectConfig["agent_presets"]) => {
    setIsSaving(true);
    try {
      await updateProjectConfig(project.id, {
        default_agent_type: project.config.default_agent_type ?? null,
        default_workspace_id: project.config.default_workspace_id ?? null,
        agent_presets: nextPresets,
        context_containers: project.config.context_containers ?? [],
        mount_policy: project.config.mount_policy ?? { include_local_workspace: true, local_workspace_capabilities: [] },
      });
      await fetchProjectAgents(project.id);
    } finally {
      setIsSaving(false);
    }
  };

  const handleCreatePreset = async (preset: AgentPreset) => {
    await savePresets([...(project.config.agent_presets ?? []), preset]);
    setIsCreateOpen(false);
  };

  const handleEditPreset = async (updated: AgentPreset) => {
    const original = editingPreset;
    if (!original) return;
    const next = (project.config.agent_presets ?? []).map((p) =>
      p.name === original.name ? updated : p,
    );
    await savePresets(next);
    setEditingPreset(null);
  };

  const handleDeletePreset = async (presetName: string) => {
    const next = (project.config.agent_presets ?? []).filter((p) => p.name !== presetName);
    await savePresets(next);
  };

  const findPresetForAgent = (agent: ProjectAgentSummary): AgentPreset | undefined => {
    if (!agent.preset_name) return undefined;
    return (project.config.agent_presets ?? []).find((p) => p.name === agent.preset_name);
  };

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
      {/* ── Header：对齐 StoryListView 的 h-14 固定栏 ── */}
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-6">
        <div className="flex items-center gap-2.5">
          <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            AGENT
          </span>
          <div>
            <h2 className="text-sm font-semibold tracking-tight text-foreground">Agent Hub</h2>
            <p className="text-xs text-muted-foreground">
              {agents.length} 个 Agent
              {activeCount > 0 && `  ·  ${activeCount} 个活跃会话`}
            </p>
          </div>
        </div>
        <button
          type="button"
          onClick={() => setIsCreateOpen(true)}
          className="h-9 rounded-[10px] border border-primary bg-primary px-3.5 text-sm text-primary-foreground transition-colors hover:opacity-95"
        >
          + 新建预设
        </button>
      </header>

      {error && (
        <div className="shrink-0 border-b border-destructive/30 bg-destructive/10 px-6 py-2.5 text-sm text-destructive">
          Agent 列表加载异常：{error}
        </div>
      )}

      {/* ── 卡片列表 ── */}
      <div className="flex-1 overflow-y-auto p-4 pt-3">
        <div className="flex flex-col gap-3">
        {sortedAgents.map((agent) => {
          const activity = getActivityLevel(agent.session?.last_activity);
          const preset = findPresetForAgent(agent);
          const isPreset = agent.key !== "default" && preset != null;

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
                  {isPreset && (
                    <CardActionMenu
                      items={[
                        { key: "edit", label: "编辑预设", onSelect: () => setEditingPreset(preset) },
                        { key: "delete", label: "删除预设", danger: true, onSelect: () => void handleDeletePreset(preset.name) },
                      ]}
                    />
                  )}
                </div>
              </div>

              <div className="mt-3 flex flex-wrap gap-1.5">
                <span className="rounded-full border border-border bg-secondary/60 px-2.5 py-0.5 text-[11px] text-muted-foreground">
                  {formatWritebackMode(agent.writeback_mode)}
                </span>
                {agent.preset_name && (
                  <span className="rounded-full border border-border bg-secondary/60 px-2.5 py-0.5 text-[11px] text-muted-foreground">
                    预设: {agent.preset_name}
                  </span>
                )}
                {agent.executor.variant && (
                  <span className="rounded-full border border-border bg-secondary/60 px-2.5 py-0.5 text-[11px] text-muted-foreground">
                    variant: {agent.executor.variant}
                  </span>
                )}
              </div>

              <div className="mt-4">
                <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground/70">
                  共享资料
                </p>
                {agent.shared_context_mounts.length > 0 ? (
                  <div className="mt-1.5 flex flex-wrap gap-1.5">
                    {agent.shared_context_mounts.map((mount) => (
                      <span
                        key={`${agent.key}-${mount.mount_id}`}
                        className="rounded-[10px] border border-border bg-secondary/40 px-2.5 py-1 text-xs text-foreground/85"
                      >
                        {mount.display_name}
                        <span className="ml-1 font-mono text-[10px] text-muted-foreground">
                          /{mount.mount_id}
                        </span>
                      </span>
                    ))}
                  </div>
                ) : (
                  <p className="mt-1.5 text-xs text-muted-foreground">无共享资料容器</p>
                )}
              </div>

              <div className="mt-auto pt-4">
                {agent.session && (
                  <div className="mb-2 flex items-center justify-between text-[11px] text-muted-foreground">
                    <span className="truncate">
                      {agent.session.session_title ?? "会话进行中"}
                    </span>
                    <span className="ml-2 shrink-0">
                      {formatRelativeTime(agent.session.last_activity)}
                    </span>
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
                  {/* 有活跃 session 且外部提供了新对话入口时，隐藏"继续对话"——右栏已展示该 session */}
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
                      title="新建一个全新的会话"
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
          <p className="mt-6 text-center text-sm text-muted-foreground">暂无 Agent 配置，点击右上角新建预设</p>
        )}
      </div>
    </div>

    <SinglePresetDialog
      key={isCreateOpen ? "create" : "closed"}
      open={isCreateOpen}
      existingNames={existingPresetNames}
      onSave={handleCreatePreset}
      onClose={() => setIsCreateOpen(false)}
      isSaving={isSaving}
    />

    {editingPreset && (
      <SinglePresetDialog
        key={`edit-${editingPreset.name}`}
        open
        initialPreset={editingPreset}
        existingNames={existingPresetNames}
        onSave={handleEditPreset}
        onClose={() => setEditingPreset(null)}
        isSaving={isSaving}
      />
      )}
    </div>
    </>
  );
}
