import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import type {
  AgentPreset,
  Project,
  ProjectAgentLink,
  ProjectAgentSession,
  ProjectAgentSummary,
  SessionNavigationState,
  ToolCluster,
} from "../../types";
import { TOOL_CLUSTER_OPTIONS, THINKING_LEVEL_OPTIONS } from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import {
  PresetFormFields,
  useAgentTypeOptions,
  presetToForm,
  formToPreset,
  SinglePresetDialog,
} from "./agent-preset-editor";
import type { PresetFormState } from "./agent-preset-editor";

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

// ─── CardMenu 下拉菜单 ───

function CardMenu({
  items,
}: {
  items: Array<{ key: string; label: string; danger?: boolean; badge?: string; onSelect: () => void }>;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="inline-flex h-7 w-7 items-center justify-center rounded-[8px] border border-border bg-background text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        title="操作菜单"
      >
        &#x22EF;
      </button>
      {open && (
        <div className="absolute right-0 top-9 z-[80] min-w-[9rem] rounded-[10px] border border-border bg-background p-1 shadow-xl">
          {items.map((item) =>
            item.key === "---" ? (
              <div key={item.key} className="my-1 border-t border-border/60" />
            ) : (
              <button
                key={item.key}
                type="button"
                onClick={() => { setOpen(false); item.onSelect(); }}
                className={`flex w-full items-center gap-2 rounded-[6px] px-2.5 py-1.5 text-left text-xs transition-colors ${
                  item.danger
                    ? "text-destructive hover:bg-destructive/10"
                    : "text-foreground hover:bg-secondary"
                }`}
              >
                {item.label}
                {item.badge && (
                  <span className="ml-auto rounded-full bg-amber-500/15 px-1.5 py-0.5 text-[9px] text-amber-600 dark:text-amber-400">
                    {item.badge}
                  </span>
                )}
              </button>
            ),
          )}
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
  const { createAgent, createProjectAgentLink, fetchProjectAgents, fetchAgents } = useProjectStore();
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
      const agent = await createAgent({
        name: preset.name,
        agent_type: preset.agent_type,
        base_config: preset.config,
      });
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
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/18 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-2xl rounded-[16px] border border-border bg-background shadow-2xl">
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
  const { deleteProjectAgentLink, fetchProjectAgents, updateAgent, updateProjectAgentLink } = useProjectStore();
  const agentLinks = useProjectStore((s) => s.agentLinksByProjectId[project.id]) ?? EMPTY_LINKS;
  const fetchLinks = useProjectStore((s) => s.fetchProjectAgentLinks);

  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [isLinkOpen, setIsLinkOpen] = useState(false);
  const [editingAgent, setEditingAgent] = useState<{ agentId: string; preset: AgentPreset } | null>(null);
  const [isEditSaving, setIsEditSaving] = useState(false);

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

  const handleOpenEditConfig = (agent: ProjectAgentSummary) => {
    const link = findLinkForAgent(agent);
    const config = link?.merged_config ?? {};
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
      await updateAgent(editingAgent.agentId, {
        name: preset.name,
        agent_type: preset.agent_type,
        base_config: preset.config,
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
    await updateProjectAgentLink(project.id, agentId, { [field]: !current });
    await fetchLinks(project.id);
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
              const mergedConfig = link?.merged_config ?? {};
              const toolClusters = Array.isArray(mergedConfig.tool_clusters)
                ? (mergedConfig.tool_clusters as ToolCluster[])
                : [];
              const allowedCompanions = Array.isArray(mergedConfig.allowed_companions)
                ? (mergedConfig.allowed_companions as string[])
                : [];
              const isCompanionTarget = agentLinks.some(
                (otherLink) =>
                  otherLink.agent_id !== agent.key &&
                  Array.isArray(otherLink.merged_config?.allowed_companions) &&
                  (otherLink.merged_config.allowed_companions as string[]).includes(
                    agent.preset_name ?? agent.display_name,
                  ),
              );

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
                      <CardMenu items={[
                        { key: "config", label: "编辑配置", onSelect: () => handleOpenEditConfig(agent) },
                        { key: "---", label: "", onSelect: () => {} },
                        { key: "unlink", label: "解除关联", danger: true, onSelect: () => void handleUnlink(agent.key) },
                      ]} />
                    </div>
                  </div>

                  {/* ── Executor 详情 ── */}
                  {(agent.executor.model_id || (agent.executor.thinking_level && agent.executor.thinking_level !== "off") || agent.executor.permission_policy) && (
                    <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[10px]">
                      {agent.executor.model_id && (
                        <span className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 font-mono text-muted-foreground" title="模型">
                          {agent.executor.model_id}
                        </span>
                      )}
                      {agent.executor.thinking_level && agent.executor.thinking_level !== "off" && (
                        <span className="rounded-[6px] border border-amber-400/30 bg-amber-500/8 px-1.5 py-0.5 text-amber-600 dark:text-amber-400" title="推理级别">
                          思考: {THINKING_LEVEL_OPTIONS.find((o) => o.value === agent.executor.thinking_level)?.label ?? agent.executor.thinking_level}
                        </span>
                      )}
                      {agent.executor.permission_policy && (
                        <span
                          className={`rounded-[6px] border px-1.5 py-0.5 ${
                            agent.executor.permission_policy === "AUTO"
                              ? "border-emerald-400/30 bg-emerald-500/8 text-emerald-600 dark:text-emerald-400"
                              : agent.executor.permission_policy === "SUPERVISED"
                                ? "border-blue-400/30 bg-blue-500/8 text-blue-600 dark:text-blue-400"
                                : "border-border bg-secondary/40 text-muted-foreground"
                          }`}
                          title="权限策略"
                        >
                          {agent.executor.permission_policy}
                        </span>
                      )}
                    </div>
                  )}

                  {/* ── 能力标签 ── */}
                  <div className="mt-3 flex flex-wrap gap-1.5">
                    {isCompanionTarget && (
                      <span
                        className="rounded-full border border-violet-400/30 bg-violet-500/10 px-2.5 py-0.5 text-[11px] text-violet-600 dark:text-violet-400"
                        title={`可被其他 Agent 通过 companion_request(agent_key="${agent.display_name}") 调用`}
                      >
                        Companion
                      </span>
                    )}
                    {toolClusters.length > 0 ? toolClusters.map((cluster) => {
                      const opt = TOOL_CLUSTER_OPTIONS.find((o) => o.value === cluster);
                      if (!opt) return null;
                      const colorCls =
                        cluster === "read" ? "border-sky-400/30 bg-sky-500/8 text-sky-600 dark:text-sky-400"
                        : cluster === "write" ? "border-orange-400/30 bg-orange-500/8 text-orange-600 dark:text-orange-400"
                        : cluster === "execute" ? "border-red-400/30 bg-red-500/8 text-red-600 dark:text-red-400"
                        : cluster === "collaboration" ? "border-violet-400/30 bg-violet-500/8 text-violet-600 dark:text-violet-400"
                        : "border-border bg-secondary/40 text-muted-foreground";
                      return (
                        <span key={cluster} className={`rounded-full border px-2 py-0.5 text-[10px] ${colorCls}`} title={opt.description}>
                          {opt.label}
                        </span>
                      );
                    }) : (
                      <span className="rounded-full border border-border/40 px-2 py-0.5 text-[10px] text-muted-foreground/40" title="未限制工具集（全部可用）">全部工具</span>
                    )}
                    {allowedCompanions.length > 0 && (
                      <span
                        className="rounded-full border border-violet-400/20 bg-violet-500/5 px-2 py-0.5 text-[10px] text-violet-500/70"
                        title={`可调用: ${allowedCompanions.join(", ")}`}
                      >
                        → {allowedCompanions.length} companion{allowedCompanions.length > 1 ? "s" : ""}
                      </span>
                    )}
                    {link?.default_lifecycle_key && (
                      <span className="rounded-full border border-primary/30 bg-primary/10 px-2.5 py-0.5 text-[11px] text-primary">
                        Lifecycle: {link.default_lifecycle_key}
                      </span>
                    )}
                    <button
                      type="button"
                      onClick={() => void handleToggleLinkDefault(agent.key, "is_default_for_story", link?.is_default_for_story ?? false)}
                      className={`rounded-full border px-2.5 py-0.5 text-[11px] transition-colors ${
                        link?.is_default_for_story
                          ? "border-primary/30 bg-primary/10 text-primary"
                          : "border-border/50 bg-transparent text-muted-foreground/50 hover:border-border hover:text-muted-foreground"
                      }`}
                      title={link?.is_default_for_story ? "取消 Story 默认" : "设为 Story 默认"}
                    >
                      Story 默认
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleToggleLinkDefault(agent.key, "is_default_for_task", link?.is_default_for_task ?? false)}
                      className={`rounded-full border px-2.5 py-0.5 text-[11px] transition-colors ${
                        link?.is_default_for_task
                          ? "border-primary/30 bg-primary/10 text-primary"
                          : "border-border/50 bg-transparent text-muted-foreground/50 hover:border-border hover:text-muted-foreground"
                      }`}
                      title={link?.is_default_for_task ? "取消 Task 默认" : "设为 Task 默认"}
                    >
                      Task 默认
                    </button>
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
        siblingAgents={agents.map((a) => ({ name: a.preset_name ?? a.display_name, display_name: a.display_name }))}
        onClose={() => setIsCreateOpen(false)}
      />

      <LinkExistingAgentDialog
        open={isLinkOpen}
        projectId={project.id}
        excludeAgentIds={linkedAgentIds}
        onClose={() => setIsLinkOpen(false)}
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
            ? agentLinks.find((l) => l.agent_id === editingAgent.agentId)?.knowledge_enabled
            : undefined
        }
        onToggleKnowledge={
          editingAgent
            ? (enabled) => {
                void updateProjectAgentLink(project.id, editingAgent.agentId, {
                  knowledge_enabled: enabled,
                });
              }
            : undefined
        }
        knowledgeProjectId={editingAgent ? project.id : undefined}
        knowledgeAgentId={editingAgent?.agentId}
        knowledgeLinkId={
          editingAgent
            ? agentLinks.find((l) => l.agent_id === editingAgent.agentId)?.id
            : undefined
        }
      />
    </>
  );
}
