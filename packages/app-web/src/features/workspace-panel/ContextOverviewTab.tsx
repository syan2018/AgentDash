/**
 * ContextOverviewTab — 右栏 "上下文" Tab 内容
 *
 * 展示当前 Session 的 owner、Agent、共享资料、会话编排与 Workflow 上下文。
 */

import { useState } from "react";
import { VfsBrowser } from "../vfs";
import { SurfaceCard } from "../session-context";
import { ATTEMPT_STATUS_LABEL, RUN_STATUS_LABEL } from "../workflow/shared-labels";
import type {
  ActiveWorkflowHookMetadata,
  ActivityAttemptState,
  HookInjection,
  HookSessionRuntimeInfo,
  ResolvedMountSummary,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionComposition,
  SessionContextSnapshot,
  Story,
  TaskSessionExecutorSummary,
  WorkflowRun,
} from "../../types";

// ─── Props ──────────────────────────────────────────────

export interface ContextOverviewTabProps {
  contextSnapshot: SessionContextSnapshot | null;
  ownerStory: Story | null;
  ownerProjectName: string;
  executorSummary: TaskSessionExecutorSummary | null;
  runtimeSurface: ResolvedVfsSurface | null;
  hookRuntime: HookSessionRuntimeInfo | null;
  sessionCapabilities: SessionBaselineCapabilities | null;
  workflowRuns: WorkflowRun[];
}

// ─── Constants ──────────────────────────────────────────

const EXECUTOR_SOURCE_LABELS: Record<string, string> = {
  "task.agent_binding.agent_type": "Task 显式 agent_type",
  "task.agent_binding.preset_name": "Task 预设",
  "project.config.default_agent_type": "Project 默认 Agent",
  "session.meta.executor_config": "当前 Session 实际执行器",
  unresolved: "未解析",
};

function describeExecutorSource(source: string): string {
  if (EXECUTOR_SOURCE_LABELS[source]) return EXECUTOR_SOURCE_LABELS[source];
  if (source.startsWith("project.config.agent_presets[")) return "Project Agent 预设";
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

function resolveActiveRun(
  workflowRuns: WorkflowRun[],
  activeWorkflow: ActiveWorkflowHookMetadata | null,
): WorkflowRun | null {
  if (activeWorkflow) {
    const matched = workflowRuns.find((run) => run.id === activeWorkflow.run_id);
    if (matched) return matched;
  }
  return (
    workflowRuns.find((run) => run.status === "running")
    ?? workflowRuns.find((run) => run.status === "ready")
    ?? workflowRuns[0]
    ?? null
  );
}

function resolveActiveAttempt(
  run: WorkflowRun | null,
  activeWorkflow: ActiveWorkflowHookMetadata | null,
): ActivityAttemptState | null {
  const attempts = run?.activity_state?.attempts;
  if (!attempts || attempts.length === 0) return null;

  const latestByActivity = (key: string): ActivityAttemptState | null => {
    let best: ActivityAttemptState | null = null;
    for (const attempt of attempts) {
      if (attempt.activity_key !== key) continue;
      if (!best || attempt.attempt >= best.attempt) best = attempt;
    }
    return best;
  };

  if (activeWorkflow) {
    const matched = latestByActivity(activeWorkflow.activity_key);
    if (matched) return matched;
  }
  const activeKey = run?.active_node_keys?.[0] ?? null;
  if (activeKey) {
    const matched = latestByActivity(activeKey);
    if (matched) return matched;
  }
  return (
    attempts.find((a) => a.status === "running" || a.status === "claiming")
    ?? attempts.find((a) => a.status === "ready")
    ?? attempts[attempts.length - 1]
    ?? null
  );
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
  workflowRuns,
}: ContextOverviewTabProps) {
  const isProjectLevel = contextSnapshot?.owner_context.owner_level === "project";
  const title = isProjectLevel ? ownerProjectName : (ownerStory?.title ?? "会话上下文");
  const composition = contextSnapshot?.effective.session_composition ?? null;

  if (!contextSnapshot && !ownerStory) {
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
            : executorSummary?.executor ?? "Story 会话 Agent"
        }
        executor={isProjectLevel ? contextSnapshot?.executor : executorSummary}
      />

      {/* 共享目录 */}
      <SharedFoldersCard
        runtimeSurface={runtimeSurface}
      />

      <WorkflowContextCard
        hookRuntime={hookRuntime}
        workflowRuns={workflowRuns}
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
  workflowRuns,
  runtimeSurface,
  composition,
}: {
  hookRuntime: HookSessionRuntimeInfo | null;
  workflowRuns: WorkflowRun[];
  runtimeSurface: ResolvedVfsSurface | null;
  composition: SessionComposition | null;
}) {
  const activeWorkflow = hookRuntime?.snapshot.metadata?.active_workflow ?? null;
  const activeRun = resolveActiveRun(workflowRuns, activeWorkflow);
  const activeAttempt = resolveActiveAttempt(activeRun, activeWorkflow);
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

  const attempts = activeRun?.activity_state?.attempts ?? [];
  const completedCount = attempts.filter((a) => a.status === "completed").length;
  const totalCount = attempts.length;

  return (
    <SurfaceCard
      eyebrow="Workflow 上下文"
      title={activeWorkflow?.lifecycle_name ?? activeWorkflow?.primary_workflow_name ?? "当前 Workflow"}
    >
      <div className="flex flex-wrap gap-2">
        {activeRun && (
          <span className="rounded-[8px] border border-border bg-secondary/50 px-2 py-1 text-[11px] text-muted-foreground">
            Run · {RUN_STATUS_LABEL[activeRun.status] ?? activeRun.status}
          </span>
        )}
        {activeWorkflow?.workflow_key && (
          <span className="rounded-[8px] border border-border bg-secondary/50 px-2 py-1 text-[11px] text-muted-foreground">
            Workflow · {activeWorkflow.workflow_key}
          </span>
        )}
        {activeAttempt && (
          <span className="rounded-[8px] border border-border bg-secondary/50 px-2 py-1 text-[11px] text-muted-foreground">
            Attempt · {ATTEMPT_STATUS_LABEL[activeAttempt.status] ?? activeAttempt.status}
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

      {(activeWorkflow || activeAttempt) && (
        <div className="mt-3 space-y-1 rounded-[8px] border border-border bg-secondary/20 px-3 py-2 text-xs">
          {activeWorkflow && (
            <>
              <p className="font-medium text-foreground">{activeWorkflow.activity_title}</p>
              <p className="text-[11px] text-muted-foreground">
                lifecycle: {activeWorkflow.lifecycle_key} · run: {activeWorkflow.run_id.slice(0, 8)}
              </p>
            </>
          )}
          {activeAttempt?.summary && (
            <p className="text-[11px] leading-5 text-muted-foreground">{activeAttempt.summary}</p>
          )}
          {activeRun?.active_node_keys && activeRun.active_node_keys.length > 0 && (
            <p className="text-[11px] text-muted-foreground">
              Active nodes: {activeRun.active_node_keys.join(", ")}
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
  const visibleSkills = capabilities.skills.filter((s) => !s.disable_model_invocation);
  const skillCount = visibleSkills.length;

  if (skillCount === 0) return null;

  return (
    <SurfaceCard
      eyebrow="Session 能力基线"
      title={skillCount > 0 ? `${skillCount} 个可用 Skill` : ""}
    >
      {skillCount > 0 && (
        <div>
          <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            Skills ({skillCount})
          </p>
          <div className="space-y-1">
            {visibleSkills.map((skill) => (
              <div
                key={skill.name}
                className="flex items-start gap-2 rounded-[6px] border border-border/70 bg-secondary/25 px-2.5 py-1.5"
              >
                <span className="shrink-0 text-xs font-medium text-foreground">{skill.name}</span>
                <span className="flex-1 truncate text-[11px] text-muted-foreground">
                  {skill.description.length > 100
                    ? `${skill.description.slice(0, 100)}…`
                    : skill.description}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </SurfaceCard>
  );
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
