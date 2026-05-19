import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import type {
  AgentPreset,
  CapabilityDirective,
  CapabilityKey,
  Project,
  ProjectAgent,
  ProjectAgentSession,
  ProjectAgentSummary,
  SessionNavigationState,
} from "../../types";
import { CAPABILITY_OPTIONS, THINKING_LEVEL_OPTIONS } from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { useCurrentUserStore } from "../../stores/currentUserStore";
import {
  PresetFormFields,
  useAgentTypeOptions,
  presetToForm,
  formToPreset,
  SinglePresetDialog,
} from "./agent-preset-editor";
import type { PresetFormState } from "./agent-preset-editor";
import { filterAgents } from "./agent-filter";
import { Notice, type NoticeData } from "../assets-panel/_shared/Notice";
import { CardMenu, StatusDot, type StatusDotTone } from "@agentdash/ui";
import { PublishLibraryAssetDialog } from "../assets-panel/publish/PublishLibraryAssetDialog";

const EMPTY_PROJECT_AGENTS: ProjectAgent[] = [];

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

const activityDotTone: Record<ActivityLevel, StatusDotTone> = {
  active: "success",
  recent: "warning",
  idle: "muted",
  none: "muted",
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
  siblingAgents,
  onClose,
}: {
  open: boolean;
  projectId: string;
  siblingAgents: Array<{ name: string; display_name: string }>;
  onClose: () => void;
}) {
  const { createProjectAgent, fetchProjectAgents } = useProjectStore();
  const { agentTypeOptions, isDiscoveryLoading } = useAgentTypeOptions();
  const lifecycles = useWorkflowStore((s) => s.lifecycleDefinitions);
  const definitions = useWorkflowStore((s) => s.definitions);

  const [form, setForm] = useState<PresetFormState>(() => presetToForm({ name: "", agent_type: "PI_AGENT", config: {} }));
  const [selectedLifecycleKey, setSelectedLifecycleKey] = useState("");
  const [selectedWorkflowKey, setSelectedWorkflowKey] = useState("");
  const [bindMode, setBindMode] = useState<"lifecycle" | "workflow" | "none">("none");
  const [isSaving, setIsSaving] = useState(false);

  const patchForm = (patch: Partial<PresetFormState>) => setForm((prev) => ({ ...prev, ...patch }));

  if (!open) return null;

  const handleSave = async () => {
    if (!form.name.trim() || !form.agent_type.trim()) return;
    setIsSaving(true);
    try {
      const preset = formToPreset(form);
      const projectAgentPayload: Parameters<typeof createProjectAgent>[1] = {
        name: preset.name,
        agent_type: preset.agent_type,
        config: preset.config,
      };
      if (bindMode === "lifecycle" && selectedLifecycleKey) {
        projectAgentPayload.default_lifecycle_key = selectedLifecycleKey;
      } else if (bindMode === "workflow" && selectedWorkflowKey) {
        projectAgentPayload.default_workflow_key = selectedWorkflowKey;
      }
      await createProjectAgent(projectId, projectAgentPayload);
      await fetchProjectAgents(projectId);
      onClose();
    } finally {
      setIsSaving(false);
    }
  };

  // status 字段自 migration 0013 起已废弃，后端不再维护；直接透传全部定义。
  const activeLifecycles = lifecycles;
  const activeWorkflows = definitions;

  return (
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/18 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-2xl rounded-[12px] border border-border bg-background shadow-2xl">
          <div className="border-b border-border px-5 py-4">
            <span className="agentdash-panel-header-tag">Agent</span>
            <h3 className="text-base font-semibold text-foreground">新建 Agent 并关联到项目</h3>
            <p className="mt-1 text-xs text-muted-foreground">
              配置 Agent 的模型、工具集和权限策略
            </p>
          </div>

          <div className="max-h-[70vh] space-y-3 overflow-y-auto p-5">
            <PresetFormFields
              form={form}
              patchForm={patchForm}
              agentTypeOptions={agentTypeOptions}
              isDiscoveryLoading={isDiscoveryLoading}
              siblingAgents={siblingAgents}
              projectId={projectId}
            />

            {/* 工作流绑定 */}
            <div className="border-t border-border/50 pt-3">
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

          <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
            <button
              type="button"
              onClick={onClose}
              className="agentdash-button-secondary"
            >
              取消
            </button>
            <button
              type="button"
              onClick={() => void handleSave()}
              disabled={!form.name.trim() || !form.agent_type.trim() || isSaving}
              className="agentdash-button-primary disabled:opacity-50"
            >
              {isSaving ? "创建中…" : "创建"}
            </button>
          </div>
        </div>
      </div>
    </>
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
  const { deleteProjectAgent, fetchProjectAgents, updateProjectAgent } = useProjectStore();
  const projectAgentConfigs = useProjectStore((s) => s.projectAgentConfigsByProjectId[project.id]) ?? EMPTY_PROJECT_AGENTS;
  const fetchProjectAgentConfigs = useProjectStore((s) => s.fetchProjectAgentConfigs);
  const currentUserId = useCurrentUserStore((s) => s.currentUser?.user_id ?? null);

  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [editingAgent, setEditingAgent] = useState<{ agentId: string; preset: AgentPreset } | null>(null);
  const [publishTarget, setPublishTarget] = useState<{
    projectAgentId: string;
    key: string;
    displayName: string;
    description: string;
  } | null>(null);
  const [notice, setNotice] = useState<NoticeData | null>(null);
  const [isEditSaving, setIsEditSaving] = useState(false);
  const [searchKeyword, setSearchKeyword] = useState("");
  const [expandedAgentKeys, setExpandedAgentKeys] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    void fetchProjectAgentConfigs(project.id);
  }, [fetchProjectAgentConfigs, project.id]);

  const findProjectAgentConfig = (agent: ProjectAgentSummary): ProjectAgent | undefined => {
    return projectAgentConfigs.find((item) => item.id === agent.key);
  };

  const handleUnlink = async (agentId: string) => {
    await deleteProjectAgent(project.id, agentId);
    await fetchProjectAgents(project.id);
  };

  const handleOpenEditConfig = (agent: ProjectAgentSummary) => {
    const projectAgentConfig = findProjectAgentConfig(agent);
    const config = projectAgentConfig?.config ?? {};
    setEditingAgent({
      agentId: agent.key,
      preset: {
        name: agent.preset_name ?? agent.display_name,
        agent_type: agent.executor.executor,
        config,
      },
    });
  };

  const handleSaveEditConfig = async (preset: AgentPreset) => {
    if (!editingAgent) return;
    setIsEditSaving(true);
    try {
      await updateProjectAgent(project.id, editingAgent.agentId, {
        name: preset.name,
        agent_type: preset.agent_type,
        config: preset.config,
      });
      await fetchProjectAgents(project.id);
      setEditingAgent(null);
    } finally {
      setIsEditSaving(false);
    }
  };

  const handleToggleLinkDefault = async (
    agentId: string,
    field: "is_default_for_story" | "is_default_for_task",
    current: boolean,
  ) => {
    await updateProjectAgent(project.id, agentId, { [field]: !current });
    await fetchProjectAgentConfigs(project.id);
  };

  const sortedAgents = useMemo(() => {
    return [...agents].sort((a, b) => {
      const aTime = a.session?.last_activity ?? 0;
      const bTime = b.session?.last_activity ?? 0;
      return bTime - aTime;
    });
  }, [agents]);

  const visibleAgents = useMemo(
    () => filterAgents(sortedAgents, searchKeyword),
    [sortedAgents, searchKeyword],
  );

  const activeCount = agents.filter((a) => a.session != null).length;

  const toggleExpand = useCallback((agentKey: string) => {
    setExpandedAgentKeys((prev) => {
      const next = new Set(prev);
      if (next.has(agentKey)) next.delete(agentKey);
      else next.add(agentKey);
      return next;
    });
  }, []);

  const handleQuickNewSession = useCallback(
    (agent: ProjectAgentSummary) => {
      // 复用展开态“新对话/打开会话”按钮的 handler：
      // 若该 agent 已有活跃会话且父组件提供了强制新开的入口，则强制新建；
      // 否则走常规 onOpenAgent（若无会话则新建，若有会话则继续）。
      if (agent.session && onForceNewSession) {
        onForceNewSession(agent);
      } else {
        onOpenAgent(agent);
      }
    },
    [onForceNewSession, onOpenAgent],
  );

  if (isLoading && agents.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-[8px] border-2 border-primary border-t-transparent" />
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
              onClick={() => setIsCreateOpen(true)}
              className="h-8 rounded-[8px] border border-primary bg-primary px-2.5 text-xs text-primary-foreground transition-colors hover:opacity-95"
            >
              + 新建 Agent
            </button>
          </div>
        </header>

        {/* ── 搜索框 ── */}
        <div className="shrink-0 border-b border-border bg-background px-3 py-2">
          <div className="relative">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
              className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground/70"
            >
              <circle cx="11" cy="11" r="8" />
              <path d="m21 21-4.3-4.3" />
            </svg>
            <input
              type="text"
              value={searchKeyword}
              onChange={(e) => setSearchKeyword(e.target.value)}
              placeholder="搜索 agent…"
              className="h-8 w-full rounded-[8px] border border-border bg-secondary/35 pl-8 pr-7 text-xs text-foreground placeholder:text-muted-foreground/70 outline-none transition-colors focus:border-primary focus:bg-background"
            />
            {searchKeyword && (
              <button
                type="button"
                onClick={() => setSearchKeyword("")}
                className="absolute right-1.5 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-[8px] text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground"
                title="清除"
                aria-label="清除搜索"
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="12"
                  height="12"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2.2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden
                >
                  <path d="M18 6 6 18" />
                  <path d="m6 6 12 12" />
                </svg>
              </button>
            )}
          </div>
        </div>

        {error && (
          <div className="shrink-0 border-b border-destructive/30 bg-destructive/10 px-6 py-2.5 text-sm text-destructive">
            Agent 列表加载异常：{error}
          </div>
        )}

        <Notice notice={notice} onDismiss={() => setNotice(null)} />

        <div className="flex-1 overflow-y-auto px-3 py-3">
          <div className="flex flex-col gap-2">
            {visibleAgents.map((agent) => {
              const activity = getActivityLevel(agent.session?.last_activity);
              const projectAgentConfig = findProjectAgentConfig(agent);
              const config = projectAgentConfig?.config ?? {};
              const rawDirectives = Array.isArray(config.capability_directives)
                ? (config.capability_directives as CapabilityDirective[])
                : [];
              const toolClusters: CapabilityKey[] = rawDirectives
                .filter((d): d is { add: CapabilityKey } => "add" in d)
                .map((d) => d.add);
              const allowedCompanions = Array.isArray(config.allowed_companions)
                ? (config.allowed_companions as string[])
                : [];
              const isCompanionTarget = projectAgentConfigs.some(
                (otherAgent) =>
                  otherAgent.id !== agent.key &&
                  Array.isArray(otherAgent.config?.allowed_companions) &&
                  (otherAgent.config.allowed_companions as string[]).includes(
                    agent.preset_name ?? agent.display_name,
                  ),
              );
              const isExpanded = expandedAgentKeys.has(agent.key);
              const thinkingActive =
                agent.executor.thinking_level && agent.executor.thinking_level !== "off";

              return (
                <div
                  key={agent.key}
                  role="button"
                  tabIndex={0}
                  aria-expanded={isExpanded}
                  aria-controls={`agent-detail-${agent.key}`}
                  onClick={() => toggleExpand(agent.key)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      toggleExpand(agent.key);
                    }
                  }}
                  className={`group cursor-pointer rounded-md border border-border bg-background p-3 transition-all hover:shadow-sm ${
                    isExpanded ? "bg-secondary/30" : ""
                  }`}
                >
                  {/* ── 卡片头部：状态点 + 名称 + 操作按钮组 ── */}
                  <div className="flex items-start gap-2">
                    <StatusDot
                      tone={activityDotTone[activity]}
                      size="md"
                      className="mt-1.5 shrink-0"
                      title={formatRelativeTime(agent.session?.last_activity)}
                    />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-medium text-foreground" title={agent.display_name}>
                        {agent.display_name}
                      </div>
                      {/* 描述：默认 1 行截断 */}
                      {agent.description && !isExpanded && (
                        <p
                          className="mt-0.5 truncate text-[11px] text-muted-foreground"
                          title={agent.description}
                        >
                          {agent.description}
                        </p>
                      )}
                    </div>

                    {/* 操作按钮区：新建会话(常驻) + 菜单 */}
                    <div
                      className="flex shrink-0 items-center gap-1"
                      onClick={(e) => e.stopPropagation()}
                      onKeyDown={(e) => e.stopPropagation()}
                    >
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleQuickNewSession(agent);
                        }}
                        aria-label="新建会话"
                        title="新建会话"
                        className="inline-flex h-7 w-7 items-center justify-center rounded-[8px] bg-secondary/50 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                      >
                        <svg
                          xmlns="http://www.w3.org/2000/svg"
                          width="14"
                          height="14"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="2"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          aria-hidden
                        >
                          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h9" />
                          <path d="M16 4h6" />
                          <path d="M19 1v6" />
                        </svg>
                      </button>
                      <CardMenu items={[
                        { key: "config", label: "编辑配置", onSelect: () => handleOpenEditConfig(agent) },
                        ...(projectAgentConfig
                          ? [
                              {
                                key: "publish",
                                label: "发布到资源市场",
                                onSelect: () =>
                                  setPublishTarget({
                                    projectAgentId: projectAgentConfig.id,
                                    key: agent.preset_name ?? agent.display_name,
                                    displayName: agent.display_name,
                                    description: agent.description,
                                  }),
                              },
                            ]
                          : []),
                        { key: "---", label: "", onSelect: () => {} },
                        { key: "delete", label: "删除 Agent", danger: true, onSelect: () => void handleUnlink(agent.key) },
                      ]} />
                    </div>
                  </div>

                  {/* ── 默认态核心 Tag：执行器 + 模型 + 推理级别 ── */}
                  {!isExpanded && (
                    <div className="mt-2 flex flex-wrap items-center gap-1.5">
                      <span
                        className="rounded-[6px] border border-border bg-secondary/60 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground"
                        title="执行器"
                      >
                        {agent.executor.executor}
                      </span>
                      {agent.executor.model_id && (
                        <span
                          className="max-w-[160px] truncate rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
                          title={`模型: ${agent.executor.model_id}`}
                        >
                          {agent.executor.model_id}
                        </span>
                      )}
                      {thinkingActive && (
                        <span
                          className="rounded-[6px] border border-warning/30 bg-warning/10 px-1.5 py-0.5 text-[10px] text-warning"
                          title="推理级别"
                        >
                          思考: {THINKING_LEVEL_OPTIONS.find((o) => o.value === agent.executor.thinking_level)?.label ?? agent.executor.thinking_level}
                        </span>
                      )}
                    </div>
                  )}

                  {/* ── 展开态详情 ── */}
                  {isExpanded && (
                    <div
                      id={`agent-detail-${agent.key}`}
                      className="mt-3 space-y-3 border-t border-border/40 pt-3"
                    >
                      {/* 描述（完整） */}
                      {agent.description && (
                        <p className="text-xs leading-5 text-muted-foreground">{agent.description}</p>
                      )}

                      {/* Executor 详情 */}
                      <div className="flex flex-wrap items-center gap-1.5 text-[10px]">
                        <span
                          className="rounded-[6px] border border-border bg-secondary/60 px-1.5 py-0.5 font-medium uppercase tracking-[0.14em] text-muted-foreground"
                          title="执行器"
                        >
                          {agent.executor.executor}
                        </span>
                        {agent.executor.model_id && (
                          <span className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 font-mono text-muted-foreground" title="模型">
                            {agent.executor.model_id}
                          </span>
                        )}
                        {thinkingActive && (
                          <span className="rounded-[6px] border border-warning/30 bg-warning/10 px-1.5 py-0.5 text-warning" title="推理级别">
                            思考: {THINKING_LEVEL_OPTIONS.find((o) => o.value === agent.executor.thinking_level)?.label ?? agent.executor.thinking_level}
                          </span>
                        )}
                        {agent.executor.permission_policy && (
                          <span
                            className={`rounded-[6px] border px-1.5 py-0.5 ${
                              agent.executor.permission_policy === "AUTO"
                                ? "border-success/30 bg-success/10 text-success"
                                : agent.executor.permission_policy === "SUPERVISED"
                                  ? "border-info/30 bg-info/10 text-info"
                                  : "border-border bg-secondary/40 text-muted-foreground"
                            }`}
                            title="权限策略"
                          >
                            {agent.executor.permission_policy}
                          </span>
                        )}
                      </div>

                      {/* 能力标签 */}
                      <div className="flex flex-wrap gap-1.5">
                        {isCompanionTarget && (
                          <span
                            className="rounded-[8px] border border-accent/30 bg-accent/10 px-2.5 py-0.5 text-[11px] text-accent"
                            title={`可被其他 Agent 通过 companion_request(agent_key="${agent.display_name}") 调用`}
                          >
                            Companion
                          </span>
                        )}
                        {toolClusters.length > 0 ? toolClusters.map((capKey) => {
                          const opt = CAPABILITY_OPTIONS.find((o) => o.value === capKey);
                          if (!opt) return null;
                          const colorCls =
                            capKey === "file_read" ? "border-info/30 bg-info/10 text-info"
                            : capKey === "file_write" ? "border-warning/30 bg-warning/10 text-warning"
                            : capKey === "shell_execute" ? "border-destructive/30 bg-destructive/10 text-destructive"
                            : capKey === "collaboration" ? "border-primary/30 bg-primary/10 text-primary"
                            : "border-border bg-secondary/40 text-muted-foreground";
                          return (
                            <span key={capKey} className={`rounded-[8px] border px-2 py-0.5 text-[10px] ${colorCls}`} title={opt.description}>
                              {opt.label}
                            </span>
                          );
                        }) : (
                          <span className="rounded-[8px] border border-border/40 px-2 py-0.5 text-[10px] text-muted-foreground/40" title="未限制工具集（全部可用）">全部工具</span>
                        )}
                        {allowedCompanions.length > 0 && (
                          <span
                            className="rounded-[8px] border border-primary/20 bg-primary/10 px-2 py-0.5 text-[10px] text-primary/70"
                            title={`可调用: ${allowedCompanions.join(", ")}`}
                          >
                            → {allowedCompanions.length} companion{allowedCompanions.length > 1 ? "s" : ""}
                          </span>
                        )}
                        {projectAgentConfig?.default_lifecycle_key && (
                          <span className="rounded-[8px] border border-primary/30 bg-primary/10 px-2.5 py-0.5 text-[11px] text-primary">
                            Lifecycle: {projectAgentConfig.default_lifecycle_key}
                          </span>
                        )}
                        <button
                          type="button"
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleToggleLinkDefault(agent.key, "is_default_for_story", projectAgentConfig?.is_default_for_story ?? false);
                          }}
                          className={`rounded-full border px-2.5 py-0.5 text-[11px] transition-colors ${
                            projectAgentConfig?.is_default_for_story
                              ? "border-primary/30 bg-primary/10 text-primary"
                              : "border-border/50 bg-transparent text-muted-foreground/50 hover:border-border hover:text-muted-foreground"
                          }`}
                          title={projectAgentConfig?.is_default_for_story ? "取消 Story 默认" : "设为 Story 默认"}
                        >
                          Story 默认
                        </button>
                        <button
                          type="button"
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleToggleLinkDefault(agent.key, "is_default_for_task", projectAgentConfig?.is_default_for_task ?? false);
                          }}
                          className={`rounded-full border px-2.5 py-0.5 text-[11px] transition-colors ${
                            projectAgentConfig?.is_default_for_task
                              ? "border-primary/30 bg-primary/10 text-primary"
                              : "border-border/50 bg-transparent text-muted-foreground/50 hover:border-border hover:text-muted-foreground"
                          }`}
                          title={projectAgentConfig?.is_default_for_task ? "取消 Task 默认" : "设为 Task 默认"}
                        >
                          Task 默认
                        </button>
                      </div>

                      {/* 当前会话信息 */}
                      {agent.session && (
                        <div className="flex items-center justify-between text-[11px] text-muted-foreground">
                          <span className="truncate">{agent.session.session_title ?? "会话进行中"}</span>
                          <span className="ml-2 shrink-0">{formatRelativeTime(agent.session.last_activity)}</span>
                        </div>
                      )}

                      {/* 历史会话面板 */}
                      <div onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
                        <SessionHistoryPanel
                          projectId={project.id}
                          agentKey={agent.key}
                          agentDisplayName={agent.display_name}
                          executorHint={agent.executor.executor}
                        />
                      </div>

                      {/* 操作按钮 */}
                      <div
                        className="flex gap-2"
                        onClick={(e) => e.stopPropagation()}
                        onKeyDown={(e) => e.stopPropagation()}
                      >
                        {(!onForceNewSession || !agent.session) && (
                          <button
                            type="button"
                            onClick={() => onOpenAgent(agent)}
                            className="flex-1 rounded-[8px] border border-primary bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-95"
                          >
                            {agent.session ? "继续对话" : "打开 Agent 会话"}
                          </button>
                        )}
                        {agent.session && onForceNewSession && (
                          <button
                            type="button"
                            onClick={() => onForceNewSession(agent)}
                            className="flex-1 rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                          >
                            新对话
                          </button>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          {agents.length === 0 && (
            <p className="mt-6 px-4 text-center text-sm text-muted-foreground">暂无 Agent，点击右上角新建</p>
          )}
          {agents.length > 0 && visibleAgents.length === 0 && (
            <p className="mt-6 px-4 text-center text-sm text-muted-foreground">未匹配到 agent，试试其他关键词</p>
          )}
        </div>
      </div>

      <CreateAgentDialog
        open={isCreateOpen}
        projectId={project.id}
        siblingAgents={agents.map((a) => ({ name: a.preset_name ?? a.display_name, display_name: a.display_name }))}
        onClose={() => setIsCreateOpen(false)}
      />

      <SinglePresetDialog
        open={editingAgent !== null}
        initialPreset={editingAgent?.preset}
        existingNames={[]}
        onSave={handleSaveEditConfig}
        onClose={() => setEditingAgent(null)}
        isSaving={isEditSaving}
        siblingAgents={agents.map((a) => ({ name: a.preset_name ?? a.display_name, display_name: a.display_name }))}
        knowledgeEnabled={
          editingAgent
            ? projectAgentConfigs.find((item) => item.id === editingAgent.agentId)?.knowledge_enabled
            : undefined
        }
        onToggleKnowledge={
          editingAgent
            ? (enabled) => {
                void updateProjectAgent(project.id, editingAgent.agentId, {
                  knowledge_enabled: enabled,
                });
              }
            : undefined
        }
        knowledgeProjectId={editingAgent ? project.id : undefined}
        knowledgeAgentId={editingAgent?.agentId}
      />

      {publishTarget && (
        <PublishLibraryAssetDialog
          projectId={project.id}
          assetKind="project_agent"
          projectAssetId={publishTarget.projectAgentId}
          defaults={{
            key: publishTarget.key,
            display_name: publishTarget.displayName,
            description: publishTarget.description,
          }}
          currentUserId={currentUserId}
          onClose={() => setPublishTarget(null)}
          onPublished={(message) => {
            setNotice({ tone: "success", message });
            void fetchProjectAgents(project.id);
          }}
        />
      )}
    </>
  );
}
