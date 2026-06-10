/**
 * ContextOverviewTab — 右栏 "上下文" Tab 内容
 *
 * 展示当前 Session 的 owner、Agent、共享资料、会话编排与 Workflow 上下文。
 */

import { useState } from "react";
import { VfsBrowser } from "../vfs";
import { SurfaceCard } from "../session-context";
import { RUNTIME_NODE_STATUS_LABEL, RUN_STATUS_LABEL } from "../workflow/shared-labels";
import {
  isDefaultExposedSkill,
  isModelInvocationVisibleSkill,
  skillDisplayLabel,
  skillIdentityKey,
} from "../../types";
import type {
  ActiveWorkflowHookMetadata,
  HookInjection,
  AgentFrameHookRuntimeInfo,
  LifecycleRunView,
  RuntimeNodeView,
  ResolvedMountSummary,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionComposition,
  SessionContextSnapshot,
  SkillCapabilityEntry,
  SkillEntry,
  SkillProviderCluster,
  Story,
  TaskSessionExecutorSummary,
} from "../../types";

// ─── Props ──────────────────────────────────────────────

export interface ContextOverviewTabProps {
  contextSnapshot: SessionContextSnapshot | null;
  ownerStory: Story | null;
  ownerProjectName: string;
  executorSummary: TaskSessionExecutorSummary | null;
  runtimeSurface: ResolvedVfsSurface | null;
  hookRuntime: AgentFrameHookRuntimeInfo | null;
  sessionCapabilities: SessionBaselineCapabilities | null;
  lifecycleRun: LifecycleRunView | null;
}

// ─── Constants ──────────────────────────────────────────

const EXECUTOR_SOURCE_LABELS: Record<string, string> = {
  "task.dispatch_preference.agent_type": "Task 显式 agent_type",
  "task.dispatch_preference.preset_name": "Task 预设",
  "project.config.default_agent_type": "Project 默认 Agent",
  unresolved: "未解析",
};

function describeExecutorSource(source: string): string {
  if (EXECUTOR_SOURCE_LABELS[source]) return EXECUTOR_SOURCE_LABELS[source];
  if (source.startsWith("project.config.agent_presets[")) return "Project Agent 预设";
  if (source.startsWith("project_agents[")) return "Project Agent 配置";
  return source;
}

function hasCompositionContent(composition: SessionComposition | null | undefined): boolean {
  return Boolean(
    composition?.persona_label
    || composition?.persona_prompt
    || composition?.workflow_steps.length
    || composition?.required_context_blocks.length,
  );
}

function formatRuntimeNodeStatus(status: string): string {
  return status in RUNTIME_NODE_STATUS_LABEL
    ? RUNTIME_NODE_STATUS_LABEL[status]
    : status;
}

function resolveActiveRun(
  lifecycleRun: LifecycleRunView | null,
  activeWorkflow: ActiveWorkflowHookMetadata | null,
): LifecycleRunView | null {
  if (!lifecycleRun) return null;
  if (activeWorkflow && lifecycleRun.run_ref.run_id !== activeWorkflow.run_id) return null;
  return lifecycleRun;
}

function collectRuntimeNodes(run: LifecycleRunView | null): RuntimeNodeView[] {
  const flatten = (node: RuntimeNodeView): RuntimeNodeView[] => [
    node,
    ...node.children.flatMap(flatten),
  ];
  return run?.orchestrations.flatMap((instance) => instance.nodes.flatMap(flatten)) ?? [];
}

function resolveActiveNode(
  run: LifecycleRunView | null,
  activeWorkflow: ActiveWorkflowHookMetadata | null,
): RuntimeNodeView | null {
  const nodes = collectRuntimeNodes(run);
  if (nodes.length === 0) return null;

  const latestByNode = (key: string): RuntimeNodeView | null => {
    let best: RuntimeNodeView | null = null;
    for (const node of nodes) {
      if (node.node_path !== key && node.node_id !== key) continue;
      if (!best || node.attempt >= best.attempt) best = node;
    }
    return best;
  };

  if (activeWorkflow?.activity_key) {
    const matched = latestByNode(activeWorkflow.activity_key);
    if (matched) return matched;
  }
  return (
    nodes.find((node) => node.status === "running" || node.status === "claiming")
    ?? nodes.find((node) => node.status === "ready")
    ?? nodes[nodes.length - 1]
    ?? null
  );
}

function activeRuntimeNodeLabels(run: LifecycleRunView | null): string[] {
  if (!run) return [];
  if (run.active_runtime_node_refs.length > 0) {
    return run.active_runtime_node_refs.map((active) => (
      `${active.orchestration_id}:${active.node_path}#${active.attempt}`
    ));
  }
  return collectRuntimeNodes(run)
    .filter((node) => (
      node.status === "running"
      || node.status === "claiming"
      || node.status === "ready"
    ))
    .map((node) => `${node.node_path}#${node.attempt}`);
}

function isWorkflowContextInjection(injection: HookInjection): boolean {
  return injection.slot === "workflow" || injection.slot === "workflow_context";
}

// ─── Component ──────────────────────────────────────────

export function ContextOverviewTab({
  contextSnapshot,
  ownerStory,
  ownerProjectName,
  executorSummary,
  runtimeSurface,
  hookRuntime,
  sessionCapabilities,
  lifecycleRun,
}: ContextOverviewTabProps) {
  const isProjectLevel = contextSnapshot?.owner_context.owner_level === "project";
  const title = isProjectLevel ? ownerProjectName : (ownerStory?.title ?? "会话上下文");
  const composition = contextSnapshot?.effective.session_composition ?? null;
  const hasRuntimeContext =
    Boolean(lifecycleRun) ||
    Boolean(hookRuntime) ||
    Boolean(runtimeSurface) ||
    Boolean(sessionCapabilities);

  if (!contextSnapshot && !ownerStory && !hasRuntimeContext) {
    return (
      <div className="flex h-full min-h-[200px] items-center justify-center px-6">
        <p className="text-center text-sm text-muted-foreground">
          当前会话还没有关联的上下文信息。
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-3 p-4">
      {/* 标题 */}
      <div>
        <p className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
          {isProjectLevel ? "Project 上下文" : "Story 上下文"}
        </p>
        <h3 className="mt-1 text-sm font-semibold text-foreground">{title}</h3>
      </div>

      {/* Agent 摘要 */}
      <AgentSummaryCard
        label={
          isProjectLevel
            ? (contextSnapshot?.owner_context.owner_level === "project"
                ? contextSnapshot.owner_context.agent_display_name
                : null)
              ?? "Project Agent"
            : executorSummary?.executor ?? (ownerStory ? "Story 会话 Agent" : "Session Agent")
        }
        executor={isProjectLevel ? contextSnapshot?.executor : executorSummary}
      />

      {/* 共享目录 */}
      <SharedFoldersCard
        runtimeSurface={runtimeSurface}
      />

      <WorkflowContextCard
        hookRuntime={hookRuntime}
        lifecycleRun={lifecycleRun}
        runtimeSurface={runtimeSurface}
        composition={composition}
      />

      <SessionCompositionCard composition={composition} />

      {/* Session 能力基线 */}
      {sessionCapabilities && (
        <SessionCapabilitiesCard capabilities={sessionCapabilities} />
      )}

      {/* 技术摘要 */}
      {contextSnapshot && (
        <TechnicalBadges
          contextSnapshot={contextSnapshot}
          runtimeSurface={runtimeSurface}
        />
      )}
    </div>
  );
}

function WorkflowContextCard({
  hookRuntime,
  lifecycleRun,
  runtimeSurface,
  composition,
}: {
  hookRuntime: AgentFrameHookRuntimeInfo | null;
  lifecycleRun: LifecycleRunView | null;
  runtimeSurface: ResolvedVfsSurface | null;
  composition: SessionComposition | null;
}) {
  const activeWorkflow = hookRuntime?.snapshot.metadata?.active_workflow ?? null;
  const activeRun = resolveActiveRun(lifecycleRun, activeWorkflow);
  const activeNode = resolveActiveNode(activeRun, activeWorkflow);
  const activeLabels = activeRuntimeNodeLabels(activeRun);
  const lifecycleMounts = runtimeSurface?.mounts.filter((mount) => mount.provider === "lifecycle_vfs") ?? [];
  const workflowInjections = hookRuntime?.snapshot.injections.filter(isWorkflowContextInjection) ?? [];
  const hasLegacySteps = (composition?.workflow_steps.length ?? 0) > 0;

  if (!activeWorkflow && !activeRun && lifecycleMounts.length === 0 && workflowInjections.length === 0 && !hasLegacySteps) {
    return (
      <SurfaceCard eyebrow="Workflow 上下文" title="未绑定活跃 Workflow">
        <p className="text-xs text-muted-foreground">
          当前会话没有解析到 lifecycle run、workflow_context 注入项或 workflow 步骤。
        </p>
      </SurfaceCard>
    );
  }

  const nodes = collectRuntimeNodes(activeRun);
  const completedCount = nodes.filter((node) => node.status === "completed").length;
  const totalCount = nodes.length;
  const workflowTitle =
    activeWorkflow?.lifecycle_name ??
    activeWorkflow?.primary_workflow_name ??
    activeWorkflow?.lifecycle_key ??
    "当前 Workflow";
  const activityTitle =
    activeWorkflow?.activity_title ?? activeWorkflow?.activity_key ?? "当前步骤";
  const lifecycleKey = activeWorkflow?.lifecycle_key ?? "unknown";
  const runIdPrefix = activeWorkflow?.run_id ? activeWorkflow.run_id.slice(0, 8) : "—";

  return (
    <SurfaceCard eyebrow="Workflow 上下文" title={workflowTitle}>
      <div className="flex flex-wrap gap-2">
        {activeRun && (
          <span className="rounded-[8px] border border-border bg-secondary/50 px-2 py-1 text-[11px] text-muted-foreground">
            Run · {RUN_STATUS_LABEL[activeRun.status] ?? activeRun.status}
          </span>
        )}
        {activeWorkflow?.procedure_key && (
          <span className="rounded-[8px] border border-border bg-secondary/50 px-2 py-1 text-[11px] text-muted-foreground">
            Workflow · {activeWorkflow.procedure_key}
          </span>
        )}
        {activeNode && (
          <span className="rounded-[8px] border border-border bg-secondary/50 px-2 py-1 text-[11px] text-muted-foreground">
            Node · {formatRuntimeNodeStatus(activeNode.status)}
          </span>
        )}
        {totalCount > 0 && (
          <span className="rounded-[8px] border border-border bg-secondary/50 px-2 py-1 text-[11px] text-muted-foreground">
            进度 {completedCount}/{totalCount}
          </span>
        )}
        {workflowInjections.length > 0 && (
          <span className="rounded-[8px] border border-border bg-secondary/50 px-2 py-1 text-[11px] text-muted-foreground">
            注入 {workflowInjections.length}
          </span>
        )}
      </div>

      {(activeWorkflow || activeNode) && (
        <div className="mt-3 space-y-1 rounded-[8px] border border-border bg-secondary/20 px-3 py-2 text-xs">
          {activeWorkflow && (
            <>
              <p className="font-medium text-foreground">{activityTitle}</p>
              <p className="text-[11px] text-muted-foreground">
                lifecycle: {lifecycleKey} · run: {runIdPrefix}
              </p>
            </>
          )}
          {activeLabels.length > 0 && (
            <p className="text-[11px] text-muted-foreground">
              Active nodes: {activeLabels.join(", ")}
            </p>
          )}
        </div>
      )}

      {lifecycleMounts.length > 0 && (
        <div className="mt-3">
          <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            Lifecycle Mounts
          </p>
          <div className="flex flex-wrap gap-2">
            {lifecycleMounts.map((mount) => (
              <span
                key={mount.id}
                className="rounded-[8px] border border-border bg-secondary/35 px-2 py-1 text-[11px] text-muted-foreground"
              >
                {mount.display_name || mount.id}
                <span className="ml-1 font-mono text-[10px]">/{mount.id}</span>
                {mount.default_write && <span className="ml-1 text-warning">可写</span>}
              </span>
            ))}
          </div>
        </div>
      )}

      {workflowInjections.length > 0 && (
        <div className="mt-3 space-y-1.5">
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            Workflow 注入项
          </p>
          {workflowInjections.slice(0, 4).map((injection, index) => (
            <WorkflowInjectionPreview
              key={`${injection.slot}-${injection.source}-${index}`}
              injection={injection}
            />
          ))}
          {workflowInjections.length > 4 && (
            <p className="text-[11px] text-muted-foreground">
              另有 {workflowInjections.length - 4} 条 workflow 注入项。
            </p>
          )}
        </div>
      )}
    </SurfaceCard>
  );
}

function WorkflowInjectionPreview({ injection }: { injection: HookInjection }) {
  const [expanded, setExpanded] = useState(false);
  const preview = injection.content.length > 180
    ? `${injection.content.slice(0, 180)}...`
    : injection.content;

  return (
    <div className="overflow-hidden rounded-[8px] border border-border bg-background/70">
      <button
        type="button"
        onClick={() => setExpanded((value) => !value)}
        className="flex w-full items-center gap-2 px-2.5 py-2 text-left transition-colors hover:bg-secondary/35"
      >
        <span className="rounded-[4px] border border-border bg-secondary/60 px-1.5 py-0 text-[9px] font-mono text-muted-foreground">
          {injection.slot}
        </span>
        <span className="min-w-0 flex-1 truncate text-[11px] text-foreground/80">
          {injection.source}
        </span>
        <span className="text-[10px] text-muted-foreground/50">{expanded ? "收起" : "展开"}</span>
      </button>
      <pre className="max-h-56 overflow-auto whitespace-pre-wrap border-t border-border/50 px-2.5 py-2 text-[11px] leading-5 text-muted-foreground">
        {expanded ? injection.content : preview}
      </pre>
    </div>
  );
}

function SessionCompositionCard({
  composition,
}: {
  composition: SessionComposition | null;
}) {
  if (!hasCompositionContent(composition)) {
    return (
      <SurfaceCard eyebrow="会话编排" title="默认协作方式">
        <p className="text-xs text-muted-foreground">
          当前会话没有配置显式 persona、协作步骤或必需上下文块。
        </p>
      </SurfaceCard>
    );
  }

  return (
    <SurfaceCard eyebrow="会话编排" title={composition?.persona_label || "当前生效编排"}>
      {composition?.persona_prompt && (
        <p className="text-xs leading-5 text-muted-foreground">
          {composition.persona_prompt.length > 180
            ? `${composition.persona_prompt.slice(0, 180)}...`
            : composition.persona_prompt}
        </p>
      )}
      {composition?.workflow_steps && composition.workflow_steps.length > 0 && (
        <div className="mt-3">
          <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            协作步骤
          </p>
          <ol className="space-y-1 pl-4 text-xs text-foreground/85">
            {composition.workflow_steps.map((step, index) => (
              <li key={`${step}-${index}`} className="list-decimal">
                {step}
              </li>
            ))}
          </ol>
        </div>
      )}
      {composition?.required_context_blocks && composition.required_context_blocks.length > 0 && (
        <div className="mt-3">
          <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            必需上下文块
          </p>
          <div className="space-y-1.5">
            {composition.required_context_blocks.map((block, index) => (
              <div
                key={`${block.title}-${index}`}
                className="rounded-[8px] border border-border bg-secondary/25 px-2.5 py-2"
              >
                <p className="text-xs font-medium text-foreground">{block.title}</p>
                {block.content && (
                  <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
                    {block.content.length > 180
                      ? `${block.content.slice(0, 180)}...`
                      : block.content}
                  </p>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </SurfaceCard>
  );
}

// ─── Sub-cards ──────────────────────────────────────────

function AgentSummaryCard({
  label,
  executor,
}: {
  label: string;
  executor?: TaskSessionExecutorSummary | null;
}) {
  return (
    <SurfaceCard eyebrow="当前协作 Agent" title={label}>
      {executor ? (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <span className="rounded-[8px] border border-border bg-secondary/60 px-2 py-1 text-[11px] font-medium text-foreground">
              {executor.executor ?? "未解析"}
            </span>
            <span className="text-[11px] text-muted-foreground">
              {describeExecutorSource(executor.source)}
            </span>
          </div>
          <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
            {executor.preset_name && <span>预设：{executor.preset_name}</span>}
            {executor.model_id && <span>model：{executor.model_id}</span>}
            {executor.permission_policy && <span>权限：{executor.permission_policy}</span>}
          </div>
          {executor.resolution_error && (
            <p className="mt-2 text-[11px] text-destructive">{executor.resolution_error}</p>
          )}
        </>
      ) : (
        <p className="text-xs text-muted-foreground">当前还没有解析到稳定的 Agent 执行信息。</p>
      )}
    </SurfaceCard>
  );
}

function SharedFoldersCard({
  runtimeSurface,
}: {
  runtimeSurface: ResolvedVfsSurface | null;
}) {
  const [browserOpen, setBrowserOpen] = useState(false);
  const hasMounts = Boolean(runtimeSurface?.mounts.length);

  const folders = runtimeSurface
    ? runtimeSurface.mounts
      .filter(isSharedFolderMount)
      .map((m) => ({
        id: m.id,
        title: m.display_name || m.id,
        mount: m.id,
        writable: m.default_write || m.capabilities.includes("write"),
      }))
    : [];

  return (
    <SurfaceCard
      eyebrow="共享资料"
      title={folders.length > 0 ? `${folders.length} 个可见目录` : "暂无共享资料"}
    >
      {folders.length > 0 ? (
        <div className="flex flex-wrap gap-2">
          {folders.map((folder, index) => (
            <span
              key={`${folder.id}-${index}`}
              className="rounded-[8px] border border-border bg-secondary/40 px-2.5 py-1 text-xs text-foreground/85"
            >
              {folder.title}
              <span className="ml-1 font-mono text-[10px] text-muted-foreground">
                /{folder.mount}
              </span>
              {folder.writable && (
                <span className="ml-1 text-[10px] text-warning">可写</span>
              )}
            </span>
          ))}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">当前还没有可用的共享目录。</p>
      )}

      {hasMounts && (
        <div className="mt-2">
          <button
            type="button"
            onClick={() => setBrowserOpen(!browserOpen)}
            className="rounded-[6px] border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            {browserOpen ? "收起地址空间" : "查看地址空间"}
          </button>
          {browserOpen && (
            <div className="mt-2">
              <VfsBrowser surface={runtimeSurface} />
            </div>
          )}
        </div>
      )}
    </SurfaceCard>
  );
}

function SessionCapabilitiesCard({
  capabilities,
}: {
  capabilities: SessionBaselineCapabilities;
}) {
  const clusters = visibleCapabilityClusters(capabilities);
  const usesClusters = clusters.length > 0;
  const visibleSkills = usesClusters ? [] : capabilities.skills.filter(isModelInvocationVisibleSkill);
  const skillCount = usesClusters
    ? clusters.reduce((total, cluster) => total + defaultExposedSkills(cluster).length, 0)
    : visibleSkills.length;

  if (!usesClusters && skillCount === 0) return null;

  return (
    <SurfaceCard
      eyebrow="Session 能力基线"
      title={usesClusters ? `${clusters.length} 个 Skill Provider` : `${skillCount} 个默认暴露 Skill`}
    >
      {usesClusters ? (
        <div className="space-y-2">
          {clusters.map((cluster) => (
            <SkillProviderClusterBlock key={cluster.provider_key} cluster={cluster} />
          ))}
        </div>
      ) : (
        <SkillListBlock skills={visibleSkills} title={`Skills (${skillCount})`} />
      )}
    </SurfaceCard>
  );
}

function SkillProviderClusterBlock({ cluster }: { cluster: SkillProviderCluster }) {
  const skills = defaultExposedSkills(cluster);
  const summary = cluster.ui_summary ?? cluster.model_summary ?? "";
  return (
    <div className="rounded-[8px] border border-border/70 bg-secondary/20 px-2.5 py-2">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium text-foreground">
            {cluster.display_name || cluster.provider_key}
          </p>
          {summary && (
            <p className="mt-0.5 text-[11px] leading-5 text-muted-foreground">
              {summary}
            </p>
          )}
        </div>
        {cluster.inventory_count != null && (
          <span className="shrink-0 rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground">
            inventory {cluster.inventory_count}
          </span>
        )}
      </div>
      {cluster.inventory_hint && (
        <p className="mt-2 rounded-[6px] border border-border/70 bg-background px-2 py-1.5 text-[11px] leading-5 text-muted-foreground">
          {cluster.inventory_hint}
        </p>
      )}
      {skills.length > 0 ? (
        <div className="mt-2">
          <SkillListBlock skills={skills} title={`默认暴露 Skills (${skills.length})`} />
        </div>
      ) : (
        <p className="mt-2 text-[11px] text-muted-foreground/70">当前没有默认暴露 Skill。</p>
      )}
    </div>
  );
}

function SkillListBlock({
  skills,
  title,
}: {
  skills: Array<SkillEntry | SkillCapabilityEntry>;
  title: string;
}) {
  return (
    <div>
      <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
        {title}
      </p>
      <div className="space-y-1">
        {skills.map((skill) => (
          <SkillSummaryRow key={skillIdentityKey(skill)} skill={skill} />
        ))}
      </div>
    </div>
  );
}

function SkillSummaryRow({ skill }: { skill: SkillEntry | SkillCapabilityEntry }) {
  const displayLabel = skillDisplayLabel(skill);
  const identity = skillIdentityKey(skill);
  return (
    <div className="rounded-[6px] border border-border/70 bg-secondary/25 px-2.5 py-1.5">
      <div className="flex items-start gap-2">
        <span className="shrink-0 text-xs font-medium text-foreground">{displayLabel}</span>
        <span className="flex-1 truncate text-[11px] text-muted-foreground">
          {skill.description.length > 100
            ? `${skill.description.slice(0, 100)}…`
            : skill.description}
        </span>
      </div>
      {(skill.provider_key || identity !== displayLabel) && (
        <div className="mt-1 flex flex-wrap gap-1">
          {skill.provider_key && <SkillIdentityChip label={skill.provider_key} />}
          {identity !== displayLabel && <SkillIdentityChip label={identity} />}
        </div>
      )}
    </div>
  );
}

function SkillIdentityChip({ label }: { label: string }) {
  return (
    <span className="rounded-[4px] border border-border bg-background px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground/70">
      {label}
    </span>
  );
}

function visibleCapabilityClusters(capabilities: SessionBaselineCapabilities): SkillProviderCluster[] {
  return (capabilities.skill_clusters ?? []).filter((cluster) => (
    Boolean(cluster.ui_summary)
    || Boolean(cluster.model_summary)
    || Boolean(cluster.inventory_hint)
    || cluster.inventory_count != null
    || defaultExposedSkills(cluster).length > 0
  ));
}

function defaultExposedSkills(cluster: SkillProviderCluster): SkillCapabilityEntry[] {
  return (cluster.default_exposed_skills ?? []).filter(isDefaultExposedSkill);
}

function TechnicalBadges({
  contextSnapshot,
  runtimeSurface,
}: {
  contextSnapshot: SessionContextSnapshot;
  runtimeSurface: ResolvedVfsSurface | null;
}) {
  const { runtime_policy, tool_visibility } = contextSnapshot.effective;
  const badges = [
    tool_visibility.resolved ? "工具面已解析" : "工具面未解析",
    runtime_policy.workspace_attached ? "已附着 workspace" : "未附着 workspace",
    runtime_policy.mcp_enabled ? "MCP 已启用" : "MCP 未启用",
    runtimeSurface?.mounts.length ? `${runtimeSurface.mounts.length} 个运行时 mount` : "无运行时 mount",
  ];

  return (
    <SurfaceCard eyebrow="技术摘要" title="运行状态概览">
      <div className="flex flex-wrap gap-2">
        {badges.map((badge) => (
          <span
            key={badge}
            className="rounded-[8px] border border-border bg-secondary/50 px-2 py-1 text-[11px] text-muted-foreground"
          >
            {badge}
          </span>
        ))}
      </div>
      <p className="mt-2 text-[11px] leading-5 text-muted-foreground">
        路径规则：{runtime_policy.path_policy}
      </p>
    </SurfaceCard>
  );
}

function isSharedFolderMount(mount: ResolvedMountSummary): boolean {
  return !["relay_fs", "lifecycle_vfs", "canvas_fs"].includes(mount.provider);
}
