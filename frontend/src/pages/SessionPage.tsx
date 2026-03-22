import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { SessionChatView, type PromptTemplate } from "../features/acp-session";
import { fetchSessionBindings, fetchSessionHookRuntime } from "../services/session";
import { useProjectStore } from "../stores/projectStore";
import { useSessionHistoryStore } from "../stores/sessionHistoryStore";
import { useStoryStore } from "../stores/storyStore";
import type {
  ActiveWorkflowHookMetadata,
  AgentBinding,
  ContextContainerDefinition,
  ExecutionAddressSpace,
  HookDiagnosticEntry,
  HookSourceRef,
  HookTraceEntry,
  HookSessionRuntimeInfo,
  MountDerivationPolicy,
  ProjectSessionAgentContext,
  ProjectSessionInfo,
  SessionContextSnapshot,
  SessionBindingOwner,
  SessionNavigationState,
  SessionComposition,
  Story,
  StoryNavigationState,
  TaskSessionExecutorSummary,
} from "../types";

// ─── Prompt 模板 ────────────────────────────────────────

const defaultPromptTemplates: PromptTemplate[] = [
  {
    id: "project-assistant",
    label: "创建项目助手",
    content: [
      `你是一个\u201C创建项目/Story 辅助 Agent\u201D。`,
      "",
      "请按步骤引导我澄清需求，并最终输出：",
      "1) 建议的 Story 标题",
      "2) 建议的 Story 描述（2-4 句）",
      "3) 3~6 条可执行的下一步任务清单（中文）",
      "",
      "约束：",
      "- 只问一个问题再等待我的回答",
      "- 不要假设我已经决定技术栈/语言/平台",
      "- 先确认目标用户与核心价值",
    ].join("\n"),
  },
  {
    id: "plan",
    label: "生成执行计划",
    content: [
      "请基于我接下来描述的目标，生成一个清晰、可执行的计划：",
      "- 目标",
      "- 里程碑",
      "- 风险与验证方式",
      "- 第一件马上能做的事情",
      "",
      "注意：内容必须使用中文。",
    ].join("\n"),
  },
];

const EMPTY_SESSION_BINDINGS: SessionBindingOwner[] = [];

// ─── SessionPage ────────────────────────────────────────

interface SessionPageProps {
  sessionId?: string;
}

// ─── Session 上下文面板辅助 ──────────────────────────

function hasStoryContextInfo(story: Story): boolean {
  const ctx = story.context;
  return (
    ctx.context_containers.length > 0
    || ctx.session_composition_override != null
    || ctx.mount_policy_override != null
    || ctx.disabled_container_ids.length > 0
  );
}

const CAPABILITY_LABELS: Record<string, string> = {
  read: "读", write: "写", list: "列", search: "搜", exec: "执行",
};

const EXECUTOR_SOURCE_LABELS: Record<string, string> = {
  "task.agent_binding.agent_type": "Task 显式 agent_type",
  "task.agent_binding.preset_name": "Task 预设",
  "project.config.default_agent_type": "Project 默认 Agent",
  "session.meta.executor_config": "当前 Session 实际执行器",
  unresolved: "未解析",
};

interface ContextFolderItem {
  id: string;
  title: string;
  mount: string;
  writable: boolean;
}

function describeExecutorSource(source: string): string {
  if (EXECUTOR_SOURCE_LABELS[source]) {
    return EXECUTOR_SOURCE_LABELS[source];
  }
  if (source.startsWith("project.config.agent_presets[")) {
    return "Project Agent 预设";
  }
  return source;
}

function resolveEffectiveStoryContextFolders(
  story: Story,
  contextSnapshot?: SessionContextSnapshot | null,
): ContextFolderItem[] {
  if (!contextSnapshot) {
    return story.context.context_containers.map((container) => ({
      id: container.id,
      title: container.display_name || container.mount_id || container.id,
      mount: container.mount_id,
      writable: container.default_write || container.capabilities.includes("write"),
    }));
  }

  const disabled = new Set(contextSnapshot.story_overrides.disabled_container_ids);
  const effective = [...contextSnapshot.project_defaults.context_containers]
    .filter((container) => !disabled.has(container.id));

  for (const container of contextSnapshot.story_overrides.context_containers) {
    const filtered = effective.filter((item) => item.id !== container.id && item.mount_id !== container.mount_id);
    filtered.push(container);
    effective.splice(0, effective.length, ...filtered);
  }

  return effective.map((container) => ({
    id: container.id,
    title: container.display_name || container.mount_id || container.id,
    mount: container.mount_id,
    writable: container.default_write || container.capabilities.includes("write"),
  }));
}

function resolveProjectContextFolders(
  projectSessionInfo?: ProjectSessionInfo | null,
): ContextFolderItem[] {
  const mounts = projectSessionInfo?.context_snapshot?.shared_context_mounts ?? [];
  return mounts.map((mount) => ({
    id: mount.container_id || mount.mount_id,
    title: mount.display_name || mount.mount_id || mount.container_id,
    mount: mount.mount_id,
    writable: mount.writable,
  }));
}

function ContextPanelShell({
  title,
  subtitle,
  badges,
  isOpen,
  onToggle,
  children,
}: {
  title: string;
  subtitle: string;
  badges: string[];
  isOpen: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div className="shrink-0 border-b border-border">
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center justify-between px-5 py-3 text-left transition-colors hover:bg-secondary/20"
      >
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground/70">
            <svg
              className={`h-3.5 w-3.5 shrink-0 transition-transform ${isOpen ? "rotate-90" : ""}`}
              fill="none" viewBox="0 0 24 24" stroke="currentColor"
            >
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
            </svg>
            <span>协作上下文</span>
          </div>
          <p className="mt-1 text-sm font-semibold text-foreground">{title}</p>
          <p className="mt-1 text-xs text-muted-foreground">{subtitle}</p>
        </div>
        {badges.length > 0 && (
          <div className="ml-4 flex flex-wrap justify-end gap-1.5">
            {badges.map((badge) => (
              <span
                key={badge}
                className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground"
              >
                {badge}
              </span>
            ))}
          </div>
        )}
      </button>

      {isOpen && (
        <div className="max-h-[42vh] space-y-3 overflow-y-auto border-t border-border bg-secondary/10 px-5 py-4">
          {children}
        </div>
      )}
    </div>
  );
}

function SurfaceCard({
  eyebrow,
  title,
  children,
}: {
  eyebrow: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="rounded-[14px] border border-border bg-background/75 px-4 py-3">
      <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground/70">
        {eyebrow}
      </p>
      <h3 className="mt-1 text-sm font-semibold text-foreground">{title}</h3>
      <div className="mt-2">{children}</div>
    </section>
  );
}

function AgentSummarySurfaceCard({
  label,
  executor,
  helperText,
}: {
  label: string;
  executor?: TaskSessionExecutorSummary | null;
  helperText: string;
}) {
  return (
    <SurfaceCard eyebrow="当前协作 Agent" title={label}>
      {executor ? (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <span className="rounded-full border border-border bg-secondary/60 px-2 py-1 text-[11px] font-medium text-foreground">
              {executor.executor ?? "未解析"}
            </span>
            <span className="text-[11px] text-muted-foreground">
              {describeExecutorSource(executor.source)}
            </span>
          </div>
          <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
            {executor.preset_name && <span>预设：{executor.preset_name}</span>}
            {executor.variant && <span>variant：{executor.variant}</span>}
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
      <p className="mt-2 text-[11px] leading-5 text-muted-foreground">{helperText}</p>
    </SurfaceCard>
  );
}

function SharedFoldersSurfaceCard({
  folders,
  emptyText,
  helperText,
}: {
  folders: ContextFolderItem[];
  emptyText: string;
  helperText: string;
}) {
  return (
    <SurfaceCard eyebrow="共享资料" title={folders.length > 0 ? `${folders.length} 个可见目录` : "暂无共享资料"}>
      {folders.length > 0 ? (
        <div className="flex flex-wrap gap-2">
          {folders.map((folder, index) => (
            <span
              key={`${folder.id || folder.mount}-${index}`}
              className="rounded-[10px] border border-border bg-secondary/40 px-2.5 py-1 text-xs text-foreground/85"
            >
              {folder.title}
              <span className="ml-1 font-mono text-[10px] text-muted-foreground">/{folder.mount}</span>
              {folder.writable && (
                <span className="ml-1 text-[10px] text-amber-600">可写</span>
              )}
            </span>
          ))}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">{emptyText}</p>
      )}
      <p className="mt-2 text-[11px] leading-5 text-muted-foreground">{helperText}</p>
    </SurfaceCard>
  );
}

function SessionBehaviorSurfaceCard({
  composition,
  emptyText,
}: {
  composition?: SessionComposition | null;
  emptyText: string;
}) {
  const hasContent = composition ? hasCompositionContent(composition) : false;
  return (
    <SurfaceCard eyebrow="当前会话行为" title={composition?.persona_label || "默认协作方式"}>
      {hasContent && composition ? (
        <div className="space-y-2">
          {composition.persona_prompt && (
            <p className="text-xs leading-5 text-muted-foreground">
              {composition.persona_prompt.length > 140
                ? `${composition.persona_prompt.slice(0, 140)}…`
                : composition.persona_prompt}
            </p>
          )}
          {composition.workflow_steps.length > 0 && (
            <div>
              <p className="text-[11px] text-muted-foreground">工作流步骤</p>
              <ol className="mt-1 space-y-1 pl-4 text-xs text-foreground/85">
                {composition.workflow_steps.map((step, index) => (
                  <li key={`${step}-${index}`} className="list-decimal">
                    {step}
                  </li>
                ))}
              </ol>
            </div>
          )}
          {composition.required_context_blocks.length > 0 && (
            <p className="text-[11px] leading-5 text-muted-foreground">
              需要固定带上的上下文块：{composition.required_context_blocks.map((block) => block.title).join("、")}
            </p>
          )}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">{emptyText}</p>
      )}
    </SurfaceCard>
  );
}

function StorySourceSummaryCard({
  story,
  contextSnapshot,
}: {
  story: Story;
  contextSnapshot?: SessionContextSnapshot | null;
}) {
  const projectCount = contextSnapshot?.project_defaults.context_containers.length ?? 0;
  const storyCount = contextSnapshot?.story_overrides.context_containers.length ?? story.context.context_containers.length;
  const disabledCount = contextSnapshot?.story_overrides.disabled_container_ids.length ?? story.context.disabled_container_ids.length;

  return (
    <SurfaceCard eyebrow="上下文来源" title="Project 默认 + Story 定向整理">
      <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          Project 默认 {projectCount}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          Story 追加 {storyCount}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          已禁用 {disabledCount}
        </span>
      </div>
      <p className="mt-2 text-[11px] leading-5 text-muted-foreground">
        用户面看到的是最终可协作的资料目录；Project 默认负责提供基础背景，Story 只补充当前需求真正相关的上下文。
      </p>
    </SurfaceCard>
  );
}

function TechnicalOverviewCard({
  runtimePolicy,
  toolVisibility,
  addressSpace,
  extraBadges = [],
}: {
  runtimePolicy: SessionContextSnapshot["effective"]["runtime_policy"];
  toolVisibility: SessionContextSnapshot["effective"]["tool_visibility"];
  addressSpace?: ExecutionAddressSpace | null;
  extraBadges?: string[];
}) {
  const badges = [
    toolVisibility.resolved ? "工具面已解析" : "工具面未解析",
    runtimePolicy.workspace_attached ? "已附着 workspace" : "未附着 workspace",
    runtimePolicy.mcp_enabled ? "MCP 已启用" : "MCP 未启用",
    addressSpace?.mounts.length ? `${addressSpace.mounts.length} 个运行时 mount` : "无运行时 mount",
    ...extraBadges,
  ];

  return (
    <SurfaceCard eyebrow="技术摘要" title="仅保留会影响当前交互判断的运行状态">
      <div className="flex flex-wrap gap-2">
        {badges.map((badge) => (
          <span
            key={badge}
            className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[11px] text-muted-foreground"
          >
            {badge}
          </span>
        ))}
      </div>
      <p className="mt-2 text-[11px] leading-5 text-muted-foreground">
        路径规则：{runtimePolicy.path_policy}
      </p>
      {!toolVisibility.resolved && (
        <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
          当前还没有解析出最终运行时工具面，因此不会把推测能力展示成事实。
        </p>
      )}
    </SurfaceCard>
  );
}


export function HookRuntimeSurfaceCard({
  hookRuntime,
}: {
  hookRuntime: HookSessionRuntimeInfo;
}) {
  const { snapshot } = hookRuntime;
  const activeWorkflow = snapshot.metadata?.active_workflow ?? null;
  const unresolvedActions = hookRuntime.pending_actions.filter(
    (action) => action.status === "pending" || action.status === "injected",
  );
  const resolvedActions = hookRuntime.pending_actions.filter(
    (action) => action.status === "resolved" || action.status === "dismissed",
  );
  return (
    <SurfaceCard eyebrow="运行中 Hook Runtime" title={`revision ${hookRuntime.revision}`}>
      <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          owners: {snapshot.owners.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          sources: {snapshot.sources.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          policies: {snapshot.policies.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          constraints: {snapshot.constraints.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          fragments: {snapshot.context_fragments.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          diagnostics: {hookRuntime.diagnostics.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          trace: {hookRuntime.trace.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          actions: {hookRuntime.pending_actions.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          open: {unresolvedActions.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          resolved: {resolvedActions.length}
        </span>
      </div>
      {snapshot.tags.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-2">
          {snapshot.tags.map((tag) => (
            <span
              key={tag}
              className="rounded-full border border-border bg-background px-2 py-1 text-[10px] text-muted-foreground"
            >
              {tag}
            </span>
          ))}
        </div>
      )}
      {activeWorkflow && <HookRuntimeWorkflowMetaCard metadata={activeWorkflow} />}
      {snapshot.sources.length > 0 && (
        <div className="mt-3 rounded-[10px] border border-border bg-background/70 px-3 py-2">
          <p className="text-xs font-medium text-foreground">Hook 来源注册表</p>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {snapshot.sources.map((source) => (
              <HookSourceBadge key={`${source.layer}:${source.key}`} source={source} />
            ))}
          </div>
        </div>
      )}
      {snapshot.policies.length > 0 && (
        <div className="mt-3 space-y-2">
          {snapshot.policies.map((policy) => (
            <div
              key={policy.key}
              className="rounded-[10px] border border-border bg-background/70 px-3 py-2"
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
                  {policy.key}
                </span>
                <span className="text-xs text-foreground/85">{policy.description}</span>
              </div>
              <HookSourceBadgeList
                className="mt-2"
                sourceRefs={policy.source_refs}
                sourceSummary={policy.source_summary}
              />
            </div>
          ))}
        </div>
      )}
      {snapshot.constraints.length > 0 && (
        <div className="mt-3 space-y-2">
          {snapshot.constraints.map((constraint) => (
            <div key={constraint.key} className="rounded-[10px] border border-border bg-background/70 px-3 py-2">
              <p className="text-xs leading-5 text-foreground/85">- {constraint.description}</p>
              <HookSourceBadgeList
                className="mt-2"
                sourceRefs={constraint.source_refs}
                sourceSummary={constraint.source_summary}
              />
            </div>
          ))}
        </div>
      )}
      <p className="mt-2 text-[11px] leading-5 text-muted-foreground">
        这里显示的是执行层真实加载并参与 loop 的 session 级 hook snapshot，而不是 owner 级静态上下文推导。
      </p>
    </SurfaceCard>
  );
}

function HookRuntimeWorkflowMetaCard({
  metadata,
}: {
  metadata: ActiveWorkflowHookMetadata;
}) {
  return (
    <div className="mt-3 rounded-[10px] border border-border bg-background/70 px-3 py-2">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs font-medium text-foreground">
          {metadata.workflow_name} / {metadata.phase_title}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
          run: {metadata.run_status}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
          completion: {metadata.completion_mode}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
          {metadata.requires_session ? "需要 session" : "不依赖 session"}
        </span>
      </div>
      <div className="mt-2 flex flex-wrap gap-2 text-[10px] text-muted-foreground">
        <span className="rounded-full border border-border bg-background px-2 py-1">
          workflow_id: {metadata.workflow_id}
        </span>
        <span className="rounded-full border border-border bg-background px-2 py-1">
          run_id: {metadata.run_id}
        </span>
      </div>
    </div>
  );
}

function HookSourceBadge({ source }: { source: HookSourceRef }) {
  return (
    <span className="rounded-full border border-border bg-background px-2 py-1 text-[10px] text-muted-foreground">
      {source.layer} · {source.label}
    </span>
  );
}

function HookSourceSummaryBadge({ summary }: { summary: string }) {
  return (
    <span className="rounded-full border border-dashed border-border bg-background px-2 py-1 text-[10px] text-muted-foreground">
      {summary}
    </span>
  );
}

function HookSourceBadgeList({
  sourceRefs,
  sourceSummary,
  className = "",
}: {
  sourceRefs: HookSourceRef[];
  sourceSummary: string[];
  className?: string;
}) {
  if (sourceRefs.length === 0 && sourceSummary.length === 0) {
    return null;
  }

  return (
    <div className={`${className} flex flex-wrap gap-1.5`}>
      {sourceRefs.map((source) => (
        <HookSourceBadge key={`${source.layer}:${source.key}`} source={source} />
      ))}
      {sourceRefs.length === 0 && sourceSummary.map((summary) => (
        <HookSourceSummaryBadge key={summary} summary={summary} />
      ))}
    </div>
  );
}

export function HookRuntimeDiagnosticsCard({
  hookRuntime,
}: {
  hookRuntime: HookSessionRuntimeInfo;
}) {
  return (
    <SurfaceCard eyebrow="Hook 诊断" title="运行时命中记录">
      {hookRuntime.diagnostics.length > 0 ? (
        <div className="space-y-2">
          {hookRuntime.diagnostics.map((entry, index) => (
            <div
              key={`${entry.code}-${index}`}
              className="rounded-[10px] border border-border bg-background/70 px-3 py-2"
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
                  {entry.code}
                </span>
                <span className="text-xs text-foreground/85">{entry.summary}</span>
              </div>
              {entry.detail && (
                <p className="mt-1 text-[11px] leading-5 text-muted-foreground">{entry.detail}</p>
              )}
              <HookDiagnosticSourceMeta entry={entry} className="mt-2" />
            </div>
          ))}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">当前还没有记录到额外的 Hook 诊断。</p>
      )}
    </SurfaceCard>
  );
}

export function HookRuntimeTraceCard({
  hookRuntime,
}: {
  hookRuntime: HookSessionRuntimeInfo;
}) {
  return (
    <SurfaceCard eyebrow="Hook Trace" title="最近触发记录">
      {hookRuntime.trace.length > 0 ? (
        <div className="space-y-2">
          {hookRuntime.trace
            .slice()
            .reverse()
            .map((entry) => (
              <HookTraceEntryCard key={`${entry.sequence}-${entry.revision}`} entry={entry} />
            ))}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">当前还没有记录到 Hook trigger trace。</p>
      )}
    </SurfaceCard>
  );
}

export function HookRuntimePendingActionsCard({
  hookRuntime,
}: {
  hookRuntime: HookSessionRuntimeInfo;
}) {
  return (
    <SurfaceCard eyebrow="Hook Actions" title="干预项状态">
      {hookRuntime.pending_actions.length > 0 ? (
        <div className="space-y-2">
          {hookRuntime.pending_actions.map((action) => {
            const createdAt = Number.isFinite(action.created_at_ms)
              ? new Date(action.created_at_ms).toLocaleTimeString("zh-CN", {
                hour12: false,
                hour: "2-digit",
                minute: "2-digit",
                second: "2-digit",
              })
              : "-";
            const resolvedAt = typeof action.resolved_at_ms === "number" && Number.isFinite(action.resolved_at_ms)
              ? new Date(action.resolved_at_ms).toLocaleTimeString("zh-CN", {
                hour12: false,
                hour: "2-digit",
                minute: "2-digit",
                second: "2-digit",
              })
              : null;
            return (
              <div key={action.id} className="rounded-[10px] border border-border bg-background/70 px-3 py-2">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
                    {action.action_type}
                  </span>
                  <span className="rounded-full border border-border bg-background px-2 py-1 text-[10px] text-muted-foreground">
                    {action.status}
                  </span>
                  <span className="text-xs font-medium text-foreground/90">{action.title}</span>
                  <span className="text-[11px] text-muted-foreground">{createdAt}</span>
                </div>
                <p className="mt-2 text-[11px] leading-5 text-muted-foreground">{action.summary}</p>
                <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
                  <span>action: {action.id}</span>
                  {action.turn_id && <span>turn: {action.turn_id}</span>}
                  <span>trigger: {action.source_trigger}</span>
                  <span>fragments: {action.context_fragments.length}</span>
                  <span>constraints: {action.constraints.length}</span>
                  {action.last_injected_at_ms != null && <span>last_injected: 已注入</span>}
                  {action.resolution_kind && <span>resolution: {action.resolution_kind}</span>}
                  {action.resolution_turn_id && <span>resolution_turn: {action.resolution_turn_id}</span>}
                  {resolvedAt && <span>resolved_at: {resolvedAt}</span>}
                </div>
                {action.resolution_note && (
                  <p className="mt-2 text-[11px] leading-5 text-foreground/80">
                    结案说明：{action.resolution_note}
                  </p>
                )}
                {action.constraints.length > 0 && (
                  <div className="mt-2 space-y-1">
                    {action.constraints.map((constraint) => (
                      <p key={`${action.id}-${constraint.key}`} className="text-[11px] leading-5 text-foreground/80">
                        - {constraint.description}
                      </p>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">当前还没有记录到 hook 干预项。</p>
      )}
    </SurfaceCard>
  );
}

function HookTraceEntryCard({ entry }: { entry: HookTraceEntry }) {
  const timestamp = Number.isFinite(entry.timestamp_ms)
    ? new Date(entry.timestamp_ms).toLocaleTimeString("zh-CN", {
      hour12: false,
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    })
    : "-";
  const completionStatus = entry.completion
    ? entry.completion.advanced
      ? "已推进"
      : entry.completion.satisfied
        ? "已满足"
        : "未满足"
    : null;

  return (
    <div className="rounded-[10px] border border-border bg-background/70 px-3 py-2">
      <div className="flex flex-wrap items-center gap-2">
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
          #{entry.sequence}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
          {entry.trigger}
        </span>
        <span className="text-xs font-medium text-foreground/90">{entry.decision}</span>
        <span className="text-[11px] text-muted-foreground">
          rev {entry.revision} · {timestamp}
        </span>
      </div>
      <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
        {entry.tool_name && <span>tool: {entry.tool_name}</span>}
        {entry.tool_call_id && <span>call: {entry.tool_call_id}</span>}
        {entry.subagent_type && <span>subagent: {entry.subagent_type}</span>}
        {entry.refresh_snapshot && <span>已刷新 snapshot</span>}
      </div>
      {entry.completion && completionStatus && (
        <p className="mt-2 text-[11px] leading-5 text-muted-foreground">
          completion: {entry.completion.mode} · {completionStatus} · {entry.completion.reason}
        </p>
      )}
      {entry.block_reason && (
        <p className="mt-2 text-[11px] leading-5 text-destructive">{entry.block_reason}</p>
      )}
      {entry.matched_rule_keys.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {entry.matched_rule_keys.map((ruleKey) => (
            <span
              key={ruleKey}
              className="rounded-full border border-border bg-secondary/40 px-2 py-1 text-[10px] text-muted-foreground"
            >
              {ruleKey}
            </span>
          ))}
        </div>
      )}
      {entry.diagnostics.length > 0 && (
        <div className="mt-2 space-y-1">
          {entry.diagnostics.map((diagnostic, index) => (
            <div key={`${diagnostic.code}-${index}`} className="rounded-[8px] border border-border/70 bg-background/60 px-2 py-1.5">
              <p className="text-[11px] leading-5 text-muted-foreground">
                {diagnostic.code}: {diagnostic.summary}
              </p>
              <HookDiagnosticSourceMeta entry={diagnostic} className="mt-1" />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function HookDiagnosticSourceMeta({
  entry,
  className = "",
}: {
  entry: HookDiagnosticEntry;
  className?: string;
}) {
  return (
    <HookSourceBadgeList
      className={className}
      sourceRefs={entry.source_refs}
      sourceSummary={entry.source_summary}
    />
  );
}

function RawDiagnosticsSection({ children }: { children: ReactNode }) {
  return (
    <details className="rounded-[12px] border border-dashed border-border bg-background/60 px-3 py-2">
      <summary className="cursor-pointer text-xs font-medium text-muted-foreground">
        查看原始结构化诊断信息
      </summary>
      <div className="mt-3 space-y-3">
        {children}
      </div>
    </details>
  );
}

function StorySessionContextPanel({
  story,
  contextSnapshot,
  executorSummary,
  addressSpace,
  hookRuntime,
  isOpen,
  onToggle,
}: {
  story: Story;
  contextSnapshot?: SessionContextSnapshot | null;
  executorSummary?: TaskSessionExecutorSummary | null;
  addressSpace?: ExecutionAddressSpace | null;
  hookRuntime?: HookSessionRuntimeInfo | null;
  isOpen: boolean;
  onToggle: () => void;
}) {
  const effectiveComposition = contextSnapshot?.effective.session_composition
    ?? story.context.session_composition_override
    ?? null;
  const folders = resolveEffectiveStoryContextFolders(story, contextSnapshot);
  const badges = [
    `${folders.length} 个资料目录`,
    effectiveComposition?.persona_label ? `Persona · ${effectiveComposition.persona_label}` : "",
    effectiveComposition && effectiveComposition.workflow_steps.length > 0
      ? `${effectiveComposition.workflow_steps.length} 个协作步骤`
      : "",
  ].filter((item): item is string => Boolean(item));

  return (
    <ContextPanelShell
      title={story.title}
      subtitle="这条会话会把 Project 默认上下文与当前 Story 的补充资料整理成可直接协作的工作面。"
      badges={badges}
      isOpen={isOpen}
      onToggle={onToggle}
    >
      <div className="grid gap-3 lg:grid-cols-3">
        <AgentSummarySurfaceCard
          label={executorSummary?.executor || "Story 会话 Agent"}
          executor={executorSummary}
          helperText="这里强调的是用户当前实际协作的 Agent，而不是所有潜在的执行器配置来源。"
        />
        <SharedFoldersSurfaceCard
          folders={folders}
          emptyText="当前 Story 还没有整理出额外共享资料目录。"
          helperText="这些目录才是对用户真正可见的上下文表面，底层 provider / mount 细节默认不直接暴露。"
        />
        <SessionBehaviorSurfaceCard
          composition={effectiveComposition}
          emptyText="当前会话没有显式 persona 或工作流要求，会按普通协作对话方式运行。"
        />
      </div>

      <StorySourceSummaryCard story={story} contextSnapshot={contextSnapshot} />
      {hookRuntime && <HookRuntimeSurfaceCard hookRuntime={hookRuntime} />}
      {hookRuntime && <HookRuntimePendingActionsCard hookRuntime={hookRuntime} />}
      {hookRuntime && <HookRuntimeTraceCard hookRuntime={hookRuntime} />}

      <details className="rounded-[14px] border border-border bg-background/75 px-4 py-3">
        <summary className="cursor-pointer text-sm font-medium text-foreground">
          技术详情与来源说明
        </summary>
        <div className="mt-3 space-y-3">
          {contextSnapshot && (
            <TechnicalOverviewCard
              runtimePolicy={contextSnapshot.effective.runtime_policy}
              toolVisibility={contextSnapshot.effective.tool_visibility}
              addressSpace={addressSpace}
              extraBadges={[
                `${contextSnapshot.project_defaults.context_containers.length} 个 Project 容器`,
                `${contextSnapshot.story_overrides.context_containers.length} 个 Story 追加容器`,
              ]}
            />
          )}
          {executorSummary && <ExecutorSummaryCard executor={executorSummary} />}
          {contextSnapshot ? (
            <RawDiagnosticsSection>
              <ContainerGroup
                title="Project 默认容器"
                containers={contextSnapshot.project_defaults.context_containers}
                emptyText="Project 未配置容器"
              />
              <ContainerGroup
                title="Story 追加容器"
                containers={contextSnapshot.story_overrides.context_containers}
                emptyText="Story 未追加容器"
              />
              <DisabledContainerCard ids={contextSnapshot.story_overrides.disabled_container_ids} />
              <MountPolicyCard title="当前生效挂载策略" policy={contextSnapshot.effective.mount_policy} />
              <SessionCompositionCard title="当前生效会话编排" composition={contextSnapshot.effective.session_composition} />
              <ToolVisibilityCard summary={contextSnapshot.effective.tool_visibility} />
              <RuntimePolicyCard summary={contextSnapshot.effective.runtime_policy} />
              <AddressSpaceCard addressSpace={addressSpace} />
              {hookRuntime && <HookRuntimeDiagnosticsCard hookRuntime={hookRuntime} />}
            </RawDiagnosticsSection>
          ) : (
            <RawDiagnosticsSection>
              <ContainerGroup
                title="Story 级容器"
                containers={story.context.context_containers}
                emptyText="Story 暂无容器"
              />
              <DisabledContainerCard ids={story.context.disabled_container_ids} />
              {story.context.mount_policy_override && (
                <MountPolicyCard title="Story 挂载策略覆盖" policy={story.context.mount_policy_override} />
              )}
              {story.context.session_composition_override && (
                <SessionCompositionCard title="Story 会话编排覆盖" composition={story.context.session_composition_override} />
              )}
              <AddressSpaceCard addressSpace={addressSpace} />
              {hookRuntime && <HookRuntimeDiagnosticsCard hookRuntime={hookRuntime} />}
            </RawDiagnosticsSection>
          )}
        </div>
      </details>
    </ContextPanelShell>
  );
}

function ProjectSessionContextPanel({
  projectName,
  projectSessionInfo,
  addressSpace,
  hookRuntime,
  isOpen,
  onToggle,
}: {
  projectName: string;
  projectSessionInfo: ProjectSessionInfo;
  addressSpace?: ExecutionAddressSpace | null;
  hookRuntime?: HookSessionRuntimeInfo | null;
  isOpen: boolean;
  onToggle: () => void;
}) {
  const snapshot = projectSessionInfo.context_snapshot;
  const folders = resolveProjectContextFolders(projectSessionInfo);
  const composition = snapshot?.effective.session_composition ?? null;
  const badges = [
    snapshot?.agent_display_name ? `Agent · ${snapshot.agent_display_name}` : "",
    `${folders.length} 个共享目录`,
    composition?.persona_label ? `Persona · ${composition.persona_label}` : "",
  ].filter((item): item is string => Boolean(item));

  return (
    <ContextPanelShell
      title={projectName}
      subtitle="Project 会话默认用于沉淀跨 Story 的背景资料、共享目录和长期协作习惯。"
      badges={badges}
      isOpen={isOpen}
      onToggle={onToggle}
    >
      <div className="grid gap-3 lg:grid-cols-3">
        <AgentSummarySurfaceCard
          label={snapshot?.agent_display_name || "Project Agent"}
          executor={snapshot?.executor}
          helperText="Project Session 绑定的是一个明确的协作 Agent，后续 Story 会话可以低存在感地继承它的默认做法。"
        />
        <SharedFoldersSurfaceCard
          folders={folders}
          emptyText="当前 Project Session 还没有对用户暴露可用共享目录。"
          helperText="共享上下文默认表达成近似文件系统的目录，而不是 provider、mount policy 或权限矩阵。"
        />
        <SessionBehaviorSurfaceCard
          composition={composition}
          emptyText="当前 Project 没有为这个 Agent 定义额外的 persona 或固定工作流。"
        />
      </div>

      <SurfaceCard eyebrow="使用定位" title="项目级知识维护入口">
        <p className="text-xs leading-6 text-muted-foreground">
          这类会话更适合维护长期背景、共识说明、参考资料和后续 Story 都可能复用的共享上下文，而不是直接暴露编排实现细节。
        </p>
      </SurfaceCard>
      {hookRuntime && <HookRuntimeSurfaceCard hookRuntime={hookRuntime} />}
      {hookRuntime && <HookRuntimePendingActionsCard hookRuntime={hookRuntime} />}
      {hookRuntime && <HookRuntimeTraceCard hookRuntime={hookRuntime} />}

      <details className="rounded-[14px] border border-border bg-background/75 px-4 py-3">
        <summary className="cursor-pointer text-sm font-medium text-foreground">
          技术详情与来源说明
        </summary>
        <div className="mt-3 space-y-3">
          {snapshot && (
            <TechnicalOverviewCard
              runtimePolicy={snapshot.effective.runtime_policy}
              toolVisibility={snapshot.effective.tool_visibility}
              addressSpace={addressSpace}
              extraBadges={[
                `${snapshot.project_defaults.context_containers.length} 个 Project 容器`,
              ]}
            />
          )}
          {snapshot?.executor && <ExecutorSummaryCard executor={snapshot.executor} />}
          {snapshot && (
            <RawDiagnosticsSection>
              <ContainerGroup
                title="Project 默认容器"
                containers={snapshot.project_defaults.context_containers}
                emptyText="Project 未配置容器"
              />
              <MountPolicyCard title="Project 默认挂载策略" policy={snapshot.project_defaults.mount_policy} />
              <SessionCompositionCard title="Project 默认会话编排" composition={snapshot.project_defaults.session_composition} />
              <ToolVisibilityCard summary={snapshot.effective.tool_visibility} />
              <RuntimePolicyCard summary={snapshot.effective.runtime_policy} />
              <AddressSpaceCard addressSpace={addressSpace} />
              {hookRuntime && <HookRuntimeDiagnosticsCard hookRuntime={hookRuntime} />}
            </RawDiagnosticsSection>
          )}
        </div>
      </details>
    </ContextPanelShell>
  );
}

function ExecutorSummaryCard({ executor }: { executor: TaskSessionExecutorSummary }) {
  return (
    <div>
      <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">当前 Agent 解析结果</p>
      <div className="rounded-[8px] border border-border bg-background/60 px-2.5 py-2 text-xs">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-medium text-foreground">{executor.executor ?? "未解析"}</span>
          <span className="rounded-[4px] bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
            {describeExecutorSource(executor.source)}
          </span>
          {executor.preset_name && (
            <span className="rounded-[4px] bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
              preset: {executor.preset_name}
            </span>
          )}
        </div>
        <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-[10px] text-muted-foreground">
          {executor.variant && <span>variant: <span className="font-mono text-foreground/80">{executor.variant}</span></span>}
          {executor.model_id && <span>model: <span className="font-mono text-foreground/80">{executor.model_id}</span></span>}
          {executor.agent_id && <span>agent_id: <span className="font-mono text-foreground/80">{executor.agent_id}</span></span>}
          {executor.reasoning_id && <span>reasoning: <span className="font-mono text-foreground/80">{executor.reasoning_id}</span></span>}
          {executor.permission_policy && <span>permission: <span className="font-mono text-foreground/80">{executor.permission_policy}</span></span>}
        </div>
        {executor.resolution_error && (
          <p className="mt-1.5 text-[10px] text-destructive">{executor.resolution_error}</p>
        )}
      </div>
    </div>
  );
}

function ContainerGroup({
  title,
  containers,
  emptyText,
}: {
  title: string;
  containers: ContextContainerDefinition[];
  emptyText: string;
}) {
  if (containers.length === 0) {
    return (
      <div>
        <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">{title}</p>
        <p className="text-xs text-muted-foreground">{emptyText}</p>
      </div>
    );
  }

  return (
    <div>
      <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">{title}</p>
      <div className="space-y-1.5">
        {containers.map((container) => (
          <div key={container.id} className="rounded-[8px] border border-border bg-background/60 px-2.5 py-2">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs font-medium text-foreground">{container.display_name}</span>
              <span className="rounded-[4px] bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">{container.mount_id}</span>
              {container.default_write && (
                <span className="rounded-[4px] bg-amber-500/15 px-1.5 py-0.5 text-[10px] text-amber-600">默认写</span>
              )}
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] text-muted-foreground">
              <span>ID: <span className="font-mono text-foreground/80">{container.id}</span></span>
              <span>provider: <span className="font-mono text-foreground/80">{describeContainerProvider(container)}</span></span>
              <span>暴露: {describeExposure(container)}</span>
            </div>
            <div className="mt-1 flex flex-wrap gap-1">
              {container.capabilities.map((cap) => (
                <span key={cap} className="rounded-full border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {CAPABILITY_LABELS[cap] ?? cap}
                </span>
              ))}
            </div>
            {container.exposure.allowed_agent_types.length > 0 && (
              <div className="mt-1 flex flex-wrap gap-1">
                {container.exposure.allowed_agent_types.map((agentType) => (
                  <span key={agentType} className="rounded-[4px] bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
                    {agentType}
                  </span>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function DisabledContainerCard({ ids }: { ids: string[] }) {
  if (ids.length === 0) return null;

  return (
    <div>
      <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">已禁用的项目容器</p>
      <div className="flex flex-wrap gap-1.5">
        {ids.map((id) => (
          <span key={id} className="rounded-[6px] bg-destructive/10 px-2 py-1 text-xs text-destructive">{id}</span>
        ))}
      </div>
    </div>
  );
}

function MountPolicyCard({
  title,
  policy,
}: {
  title: string;
  policy: MountDerivationPolicy;
}) {
  return (
    <div>
      <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">{title}</p>
      <div className="rounded-[8px] border border-border bg-background/60 px-2.5 py-2 text-xs">
        <div className="flex items-center gap-2">
          <span className={policy.include_local_workspace ? "text-emerald-600" : "text-muted-foreground"}>
            {policy.include_local_workspace ? "✓" : "✗"} 包含本地工作空间
          </span>
        </div>
        {policy.local_workspace_capabilities.length > 0 && (
          <div className="mt-1 flex flex-wrap gap-1">
            {policy.local_workspace_capabilities.map((cap) => (
              <span key={cap} className="rounded-full border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {CAPABILITY_LABELS[cap] ?? cap}
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export function SessionPage({ sessionId: propSessionId }: SessionPageProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams] = useSearchParams();
  const selectProject = useProjectStore((state) => state.selectProject);
  const fetchProjectSessionInfo = useProjectStore((state) => state.fetchProjectSessionInfo);
  const fetchTaskSession = useStoryStore((state) => state.fetchTaskSession);
  const fetchStorySessionInfo = useStoryStore((state) => state.fetchStorySessionInfo);
  const { createNew, setActiveSessionId, reload: reloadSessions } = useSessionHistoryStore();
  const hookRuntimeRefreshTimerRef = useRef<number | null>(null);

  const [loadedSessionBindings, setLoadedSessionBindings] = useState<SessionBindingOwner[]>([]);
  const [loadedHookRuntime, setLoadedHookRuntime] = useState<HookSessionRuntimeInfo | null>(null);
  const [loadedSessionContext, setLoadedSessionContext] = useState<{
    source_key: string;
    task_agent_binding: AgentBinding | null;
    address_space: ExecutionAddressSpace | null;
    context_snapshot: SessionContextSnapshot | null;
    project_session_info: ProjectSessionInfo | null;
    task_executor_summary: TaskSessionExecutorSummary | null;
  } | null>(null);
  const [loadedOwnerStory, setLoadedOwnerStory] = useState<{
    story_id: string;
    story: Story | null;
  } | null>(null);
  const [isContextPanelOpen, setIsContextPanelOpen] = useState(false);

  const routeState = useMemo(
    () => (location.state as SessionNavigationState | null) ?? null,
    [location.state],
  );
  const taskIdFromQuery = searchParams.get("task_id")?.trim() || "";
  const taskContextFromRoute = routeState?.task_context ?? null;
  const projectAgentContext = (routeState?.project_agent ?? null) as ProjectSessionAgentContext | null;
  const returnTarget = routeState?.return_to ?? null;
  const routeTaskIdHint = taskContextFromRoute?.task_id ?? taskIdFromQuery;
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
    if (!currentSessionId) {
      setLoadedHookRuntime(null);
      return;
    }
    void refreshHookRuntime(currentSessionId);
    return () => {
      if (hookRuntimeRefreshTimerRef.current) {
        window.clearTimeout(hookRuntimeRefreshTimerRef.current);
        hookRuntimeRefreshTimerRef.current = null;
      }
    };
  }, [currentSessionId, refreshHookRuntime]);

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
  const taskIdHint = routeTaskIdHint || sessionOwnerBinding?.task_id || "";

  const sessionContextSourceKey = useMemo(() => {
    if (taskIdHint) {
      return `task:${taskIdHint}`;
    }
    if (
      sessionOwnerBinding?.owner_type === "story"
      && sessionOwnerBinding.story_id
      && sessionOwnerBinding.id
    ) {
      return `story:${sessionOwnerBinding.story_id}:${sessionOwnerBinding.id}`;
    }
    if (
      sessionOwnerBinding?.owner_type === "project"
      && sessionOwnerBinding.project_id
      && sessionOwnerBinding.id
    ) {
      return `project:${sessionOwnerBinding.project_id}:${sessionOwnerBinding.id}`;
    }
    return null;
  }, [sessionOwnerBinding, taskIdHint]);

  useEffect(() => {
    let cancelled = false;

    if (taskIdHint) {
      void (async () => {
        const taskSession = await fetchTaskSession(taskIdHint);
        if (cancelled) return;
        setLoadedSessionContext({
          source_key: `task:${taskIdHint}`,
          task_agent_binding: taskSession?.agent_binding ?? null,
          address_space: taskSession?.address_space ?? null,
          context_snapshot: taskSession?.context_snapshot ?? null,
          project_session_info: null,
          task_executor_summary: taskSession?.context_snapshot?.executor ?? null,
        });
      })();
      return () => { cancelled = true; };
    }

    if (
      sessionOwnerBinding?.owner_type === "story"
      && sessionOwnerBinding.story_id
      && sessionOwnerBinding.id
    ) {
      const storyId = sessionOwnerBinding.story_id;
      const bindingId = sessionOwnerBinding.id;
      void (async () => {
        const storySession = await fetchStorySessionInfo(
          storyId,
          bindingId,
        );
        if (cancelled) return;
        setLoadedSessionContext({
          source_key: `story:${storyId}:${bindingId}`,
          task_agent_binding: null,
          address_space: storySession?.address_space ?? null,
          context_snapshot: storySession?.context_snapshot ?? null,
          project_session_info: null,
          task_executor_summary: storySession?.context_snapshot?.executor ?? null,
        });
      })();
      return () => { cancelled = true; };
    }

    if (
      sessionOwnerBinding?.owner_type === "project"
      && sessionOwnerBinding.project_id
      && sessionOwnerBinding.id
    ) {
      const projectId = sessionOwnerBinding.project_id;
      const bindingId = sessionOwnerBinding.id;
      void (async () => {
        const info = await fetchProjectSessionInfo(projectId, bindingId);
        if (cancelled) return;
        setLoadedSessionContext({
          source_key: `project:${projectId}:${bindingId}`,
          task_agent_binding: null,
          address_space: info?.address_space ?? null,
          context_snapshot: null,
          project_session_info: info,
          task_executor_summary: info?.context_snapshot?.executor ?? null,
        });
      })();
      return () => { cancelled = true; };
    }

    return () => { cancelled = true; };
  }, [fetchProjectSessionInfo, fetchStorySessionInfo, fetchTaskSession, sessionOwnerBinding, taskIdHint]);

  const activeSessionContext = loadedSessionContext?.source_key === sessionContextSourceKey
    ? loadedSessionContext
    : null;
  const taskAgentBinding = taskContextFromRoute?.agent_binding
    ?? activeSessionContext?.task_agent_binding
    ?? null;
  const sessionAddressSpace = activeSessionContext?.address_space ?? null;
  const sessionContextSnapshot = activeSessionContext?.context_snapshot ?? null;
  const projectSessionInfo = activeSessionContext?.project_session_info ?? null;
  const taskExecutorSummary = activeSessionContext?.task_executor_summary ?? null;

  // 按需加载关联 Story 的上下文信息
  const fetchStoryById = useStoryStore((s) => s.fetchStoryById);
  const stories = useStoryStore((s) => s.stories);
  const ownerStoryId = sessionOwnerBinding?.story_id ?? null;
  const ownerProjectName = sessionOwnerBinding?.owner_type === "project"
    ? sessionOwnerBinding.owner_title?.trim() || sessionOwnerBinding.owner_id
    : "";

  useEffect(() => {
    const cached = ownerStoryId ? stories.find((story) => story.id === ownerStoryId) : null;
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
  }, [ownerStoryId, stories, fetchStoryById]);

  const ownerStory = useMemo(() => {
    if (!ownerStoryId) return null;
    const cached = stories.find((story) => story.id === ownerStoryId);
    if (cached) return cached;
    if (loadedOwnerStory?.story_id === ownerStoryId) {
      return loadedOwnerStory.story;
    }
    return null;
  }, [loadedOwnerStory, ownerStoryId, stories]);

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
    ?? projectSessionInfo?.context_snapshot?.executor.executor
    ?? taskExecutorSummary?.executor
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

  const handleSystemEvent = useCallback((eventType: string) => {
    switch (eventType) {
      case "hook_event":
      case "hook_action_resolved":
      case "companion_dispatch_registered":
      case "companion_result_available":
      case "companion_result_returned":
        scheduleHookRuntimeRefresh(eventType);
        break;
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
      {sessionOwnerBinding.owner_type === "project" && projectSessionInfo?.context_snapshot?.agent_display_name && (
        <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[11px] text-foreground/80">
          Agent · {projectSessionInfo.context_snapshot.agent_display_name}
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
        </div>
      </header>

      {projectSessionInfo?.context_snapshot && (
        <ProjectSessionContextPanel
          projectName={ownerProjectName}
          projectSessionInfo={projectSessionInfo}
          addressSpace={sessionAddressSpace}
          hookRuntime={activeHookRuntime}
          isOpen={isContextPanelOpen}
          onToggle={() => setIsContextPanelOpen((value) => !value)}
        />
      )}

      {!projectSessionInfo?.context_snapshot && ownerStory && (
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
          isOpen={isContextPanelOpen}
          onToggle={() => setIsContextPanelOpen((value) => !value)}
        />
      )}

      {/* 复用的聊天视图 */}
      <div className="flex-1 overflow-hidden">
        <SessionChatView
          sessionId={currentSessionId}
          onCreateSession={handleCreateSession}
          onSessionIdChange={handleSessionIdChange}
          onMessageSent={handleMessageSent}
          onTurnEnd={handleTurnEnd}
          onSystemEvent={handleSystemEvent}
          executorHint={executorHint}
          promptTemplates={defaultPromptTemplates}
          inputPrefix={ownerBindingBar}
        />
      </div>
    </div>
  );
}

function SessionCompositionCard({
  title,
  composition,
}: {
  title: string;
  composition: SessionComposition;
}) {
  if (!hasCompositionContent(composition)) {
    return (
      <div>
        <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">{title}</p>
        <p className="text-xs text-muted-foreground">未配置显式 persona / workflow / 必需上下文块</p>
      </div>
    );
  }

  return (
    <div>
      <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">{title}</p>
      <div className="space-y-1.5 rounded-[8px] border border-border bg-background/60 px-2.5 py-2 text-xs">
        {composition.persona_label && (
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground">Persona:</span>
            <span className="font-medium text-foreground">{composition.persona_label}</span>
          </div>
        )}
        {composition.persona_prompt && (
          <pre className="max-h-20 overflow-y-auto whitespace-pre-wrap rounded-[6px] bg-muted/50 px-2 py-1.5 text-[11px] leading-5 text-foreground/80">
            {composition.persona_prompt}
          </pre>
        )}
        {composition.workflow_steps.length > 0 && (
          <div>
            <span className="text-muted-foreground">工作流步骤:</span>
            <ol className="mt-1 space-y-0.5 pl-4">
              {composition.workflow_steps.map((step, i) => (
                <li key={i} className="list-decimal text-[11px] text-foreground/80">{step}</li>
              ))}
            </ol>
          </div>
        )}
        {composition.required_context_blocks.length > 0 && (
          <div>
            <span className="text-muted-foreground">必需上下文块:</span>
            <div className="mt-1 space-y-1">
              {composition.required_context_blocks.map((block, i) => (
                <div key={`${block.title}-${i}`} className="rounded-[6px] bg-muted/50 px-2 py-1">
                  <span className="text-[10px] font-medium text-foreground">{block.title}</span>
                  {block.content && (
                    <p className="mt-0.5 text-[10px] leading-4 text-muted-foreground">
                      {block.content.length > 140 ? `${block.content.slice(0, 140)}…` : block.content}
                    </p>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ToolVisibilityCard({ summary }: { summary: SessionContextSnapshot["effective"]["tool_visibility"] }) {
  return (
    <div>
      <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">当前工具可见性</p>
      <div className="rounded-[8px] border border-border bg-background/60 px-2.5 py-2 text-xs">
        <div className="flex flex-wrap items-center gap-2">
          <span className={summary.resolved ? "text-emerald-600" : "text-amber-600"}>
            {summary.resolved ? "✓ 已解析" : "△ 未解析"}
          </span>
          <span className="text-muted-foreground">toolset:</span>
          <span className="font-mono text-foreground/80">{summary.toolset_label}</span>
        </div>
        {summary.tool_names.length > 0 ? (
          <div className="mt-1 flex flex-wrap gap-1">
            {summary.tool_names.map((tool) => (
              <span key={tool} className="rounded-full border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {tool}
              </span>
            ))}
          </div>
        ) : (
          <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
            当前还没有解析出最终运行时工具面，因此这里不会再把推测值伪装成“当前可见工具”。
          </p>
        )}
        {summary.mcp_servers.length > 0 && (
          <div className="mt-2 space-y-1">
            {summary.mcp_servers.map((server) => (
              <div key={`${server.transport}-${server.name}`} className="text-[10px] text-muted-foreground">
                <span className="font-medium text-foreground">{server.name}</span>
                <span> · {server.transport}</span>
                <span className="ml-1 font-mono text-foreground/70">{server.target}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function RuntimePolicyCard({ summary }: { summary: SessionContextSnapshot["effective"]["runtime_policy"] }) {
  return (
    <div>
      <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">当前运行策略</p>
      <div className="space-y-1 rounded-[8px] border border-border bg-background/60 px-2.5 py-2 text-xs">
        <div className="flex flex-wrap gap-x-3 gap-y-1">
          <span className={summary.workspace_attached ? "text-emerald-600" : "text-muted-foreground"}>
            {summary.workspace_attached ? "✓" : "✗"} workspace
          </span>
          <span className={summary.address_space_attached ? "text-emerald-600" : "text-muted-foreground"}>
            {summary.address_space_attached ? "✓" : "✗"} address_space
          </span>
          <span className={summary.mcp_enabled ? "text-emerald-600" : "text-muted-foreground"}>
            {summary.mcp_enabled ? "✓" : "✗"} MCP
          </span>
        </div>
        <p className="text-[10px] text-muted-foreground">path_policy: <span className="text-foreground/80">{summary.path_policy}</span></p>
        <RuntimeListRow label="visible_mounts" items={summary.visible_mounts} />
        <RuntimeListRow label="visible_tools" items={summary.visible_tools} />
        <RuntimeListRow label="writable_mounts" items={summary.writable_mounts} />
        <RuntimeListRow label="exec_mounts" items={summary.exec_mounts} />
      </div>
    </div>
  );
}

function RuntimeListRow({ label, items }: { label: string; items: string[] }) {
  return (
    <div>
      <p className="text-[10px] text-muted-foreground">{label}</p>
      <div className="mt-0.5 flex flex-wrap gap-1">
        {items.length > 0 ? items.map((item) => (
          <span key={item} className="rounded-full border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
            {item}
          </span>
        )) : (
          <span className="text-[10px] text-muted-foreground">-</span>
        )}
      </div>
    </div>
  );
}

function AddressSpaceCard({ addressSpace }: { addressSpace?: ExecutionAddressSpace | null }) {
  if (!addressSpace || addressSpace.mounts.length === 0) return null;

  return (
    <div>
      <p className="mb-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">运行时 Address Space</p>
      {addressSpace.default_mount_id && (
        <p className="mb-1 text-[10px] text-muted-foreground">默认 mount: <span className="font-mono text-foreground/80">{addressSpace.default_mount_id}</span></p>
      )}
      <div className="space-y-1.5">
        {addressSpace.mounts.map((mount) => (
          <div key={mount.id} className="rounded-[8px] border border-border bg-background/60 px-2.5 py-2">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs font-medium text-foreground">{mount.display_name}</span>
              <span className="rounded-[4px] bg-muted px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground">{mount.id}</span>
              {mount.default_write && (
                <span className="rounded-[4px] bg-amber-500/15 px-1.5 py-0.5 text-[10px] text-amber-600">默认写</span>
              )}
              {addressSpace.default_mount_id === mount.id && (
                <span className="rounded-[4px] bg-primary/15 px-1.5 py-0.5 text-[10px] text-primary">默认</span>
              )}
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[10px] text-muted-foreground">
              <span>provider: <span className="font-mono text-foreground/70">{mount.provider}</span></span>
              <span>root: <span className="font-mono text-foreground/70">{mount.root_ref}</span></span>
            </div>
            <div className="mt-1 flex flex-wrap gap-1">
              {mount.capabilities.map((cap) => (
                <span key={cap} className="rounded-full border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {CAPABILITY_LABELS[cap] ?? cap}
                </span>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function describeContainerProvider(container: ContextContainerDefinition): string {
  if (container.provider.kind === "inline_files") {
    return `inline_files (${container.provider.files.length})`;
  }
  return `external_service:${container.provider.service_id}`;
}

function describeExposure(container: ContextContainerDefinition): string {
  const targets: string[] = [];
  if (container.exposure.include_in_project_sessions ?? true) targets.push("project");
  if (container.exposure.include_in_story_sessions ?? true) targets.push("story");
  if (container.exposure.include_in_task_sessions ?? true) targets.push("task");
  return targets.length > 0 ? targets.join("/") : "none";
}

function hasCompositionContent(composition: SessionComposition): boolean {
  return Boolean(
    composition.persona_label
    || composition.persona_prompt
    || composition.workflow_steps.length > 0
    || composition.required_context_blocks.length > 0,
  );
}

export default SessionPage;
