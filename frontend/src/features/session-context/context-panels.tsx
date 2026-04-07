import { useState, type ReactNode } from "react";
import { AddressSpaceBrowser } from "../address-space";
import type {
  ContextContainerDefinition,
  ExecutionAddressSpace,
  HookSessionRuntimeInfo,
  SessionComposition,
  SessionContextSnapshot,
  SessionStoryOverrides,
  Story,
  TaskSessionExecutorSummary,
} from "../../types";
import { SurfaceCard } from "./surface-card";
import {
  HookRuntimeDiagnosticsCard,
  HookRuntimePendingActionsCard,
  HookRuntimeSurfaceCard,
  HookRuntimeTraceCard,
  RawDiagnosticsSection,
} from "./hook-runtime-cards";

// ─── Constants & Utilities ─────────────────────────────

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

function getOwnerStoryOverrides(
  contextSnapshot?: SessionContextSnapshot | null,
): SessionStoryOverrides | null {
  if (!contextSnapshot) return null;
  const ownerContext = contextSnapshot.owner_context;
  if (ownerContext.owner_level === "task" || ownerContext.owner_level === "story") {
    return ownerContext.story_overrides;
  }
  return null;
}

function resolveEffectiveStoryContextFolders(
  story: Story,
  contextSnapshot?: SessionContextSnapshot | null,
): ContextFolderItem[] {
  const storyOverrides = getOwnerStoryOverrides(contextSnapshot);

  if (!contextSnapshot || !storyOverrides) {
    return story.context.context_containers.map((container) => ({
      id: container.id,
      title: container.display_name || container.mount_id || container.id,
      mount: container.mount_id,
      writable: container.default_write || container.capabilities.includes("write"),
    }));
  }

  const disabled = new Set(storyOverrides.disabled_container_ids);
  const effective = [...contextSnapshot.project_defaults.context_containers]
    .filter((container) => !disabled.has(container.id));

  for (const container of storyOverrides.context_containers) {
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
  contextSnapshot?: SessionContextSnapshot | null,
): ContextFolderItem[] {
  const mounts = contextSnapshot?.owner_context.owner_level === "project"
    ? contextSnapshot.owner_context.shared_context_mounts
    : [];
  return mounts.map((mount) => ({
    id: mount.container_id || mount.mount_id,
    title: mount.display_name || mount.mount_id || mount.container_id,
    mount: mount.mount_id,
    writable: mount.writable,
  }));
}

function hasCompositionContent(composition: SessionComposition): boolean {
  return Boolean(
    composition.persona_label
    || composition.persona_prompt
    || composition.workflow_steps.length > 0
    || composition.required_context_blocks.length > 0,
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

// ─── Shell & Layout ────────────────────────────────────

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

// ─── Surface Card Variants ─────────────────────────────

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
  addressSpace,
  preview,
}: {
  folders: ContextFolderItem[];
  emptyText: string;
  helperText: string;
  addressSpace?: ExecutionAddressSpace | null;
  preview?: { projectId: string; storyId?: string; ownerType?: string; ownerId?: string; target?: "project" | "story" | "task" };
}) {
  const [browserOpen, setBrowserOpen] = useState(false);

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

      {(preview?.projectId || (addressSpace && addressSpace.mounts.length > 0)) && (
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
              <AddressSpaceBrowser
                addressSpace={addressSpace}
                preview={preview}
              />
            </div>
          )}
        </div>
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
  const storyOverrides = getOwnerStoryOverrides(contextSnapshot);
  const projectCount = contextSnapshot?.project_defaults.context_containers.length ?? 0;
  const storyCount = storyOverrides?.context_containers.length ?? story.context.context_containers.length;
  const disabledCount = storyOverrides?.disabled_container_ids.length ?? story.context.disabled_container_ids.length;

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

// ─── Story Session Context Panel ───────────────────────

export function StorySessionContextPanel({
  story,
  contextSnapshot,
  executorSummary,
  addressSpace,
  hookRuntime,
  ownerType,
  ownerId,
  isOpen,
  onToggle,
}: {
  story: Story;
  contextSnapshot?: SessionContextSnapshot | null;
  executorSummary?: TaskSessionExecutorSummary | null;
  addressSpace?: ExecutionAddressSpace | null;
  hookRuntime?: HookSessionRuntimeInfo | null;
  ownerType?: string;
  ownerId?: string;
  isOpen: boolean;
  onToggle: () => void;
}) {
  const effectiveComposition = contextSnapshot?.effective.session_composition
    ?? story.context.session_composition
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
      <div className="grid gap-3 lg:grid-cols-2">
        <AgentSummarySurfaceCard
          label={executorSummary?.executor || "Story 会话 Agent"}
          executor={executorSummary}
          helperText="这里强调的是用户当前实际协作的 Agent，而不是所有潜在的执行器配置来源。"
        />
        <SharedFoldersSurfaceCard
          folders={folders}
          emptyText="当前 Story 还没有整理出额外共享资料目录。"
          helperText="这些目录才是对用户真正可见的上下文表面，底层 provider / mount 细节默认不直接暴露。"
          addressSpace={addressSpace}
          preview={{ projectId: story.project_id, storyId: story.id, ownerType, ownerId, target: "story" }}
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
                `${getOwnerStoryOverrides(contextSnapshot)?.context_containers.length ?? 0} 个 Story 追加容器`,
              ]}
            />
          )}
          {executorSummary && <ExecutorSummaryCard executor={executorSummary} />}
          {contextSnapshot && getOwnerStoryOverrides(contextSnapshot) ? (
            <RawDiagnosticsSection>
              <ContainerGroup
                title="Project 默认容器"
                containers={contextSnapshot.project_defaults.context_containers}
                emptyText="Project 未配置容器"
              />
              <ContainerGroup
                title="Story 追加容器"
                containers={getOwnerStoryOverrides(contextSnapshot)?.context_containers ?? []}
                emptyText="Story 未追加容器"
              />
              <DisabledContainerCard ids={getOwnerStoryOverrides(contextSnapshot)?.disabled_container_ids ?? []} />
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
              {story.context.session_composition && (
                <SessionCompositionCard title="Story 会话编排" composition={story.context.session_composition} />
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

// ─── Project Session Context Panel ─────────────────────

export function ProjectSessionContextPanel({
  projectId,
  projectName,
  contextSnapshot,
  addressSpace,
  hookRuntime,
  ownerType,
  ownerId,
  isOpen,
  onToggle,
}: {
  projectId: string;
  projectName: string;
  contextSnapshot: SessionContextSnapshot;
  addressSpace?: ExecutionAddressSpace | null;
  ownerType?: string;
  ownerId?: string;
  hookRuntime?: HookSessionRuntimeInfo | null;
  isOpen: boolean;
  onToggle: () => void;
}) {
  const snapshot = contextSnapshot;
  const projectOwner = snapshot.owner_context.owner_level === "project" ? snapshot.owner_context : null;
  const folders = resolveProjectContextFolders(contextSnapshot);
  const badges = [
    projectOwner?.agent_display_name ? `Agent · ${projectOwner.agent_display_name}` : "",
    `${folders.length} 个共享目录`,
  ].filter((item): item is string => Boolean(item));

  return (
    <ContextPanelShell
      title={projectName}
      subtitle="Project 会话默认用于沉淀跨 Story 的背景资料、共享目录和长期协作习惯。"
      badges={badges}
      isOpen={isOpen}
      onToggle={onToggle}
    >
      <div className="grid gap-3 lg:grid-cols-2">
        <AgentSummarySurfaceCard
          label={projectOwner?.agent_display_name || "Project Agent"}
          executor={snapshot.executor}
          helperText="Project Session 绑定的是一个明确的协作 Agent，后续 Story 会话可以低存在感地继承它的默认做法。"
        />
        <SharedFoldersSurfaceCard
          folders={folders}
          emptyText="当前 Project Session 还没有对用户暴露可用共享目录。"
          helperText="共享上下文默认表达成近似文件系统的目录，而不是 provider、mount policy 或权限矩阵。"
          addressSpace={addressSpace}
          preview={{ projectId, ownerType, ownerId, target: "project" }}
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

// ─── Diagnostic Sub-components ─────────────────────────

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
          {executor.provider_id && <span>provider: <span className="font-mono text-foreground/80">{executor.provider_id}</span></span>}
          {executor.model_id && <span>model: <span className="font-mono text-foreground/80">{executor.model_id}</span></span>}
          {executor.agent_id && <span>agent_id: <span className="font-mono text-foreground/80">{executor.agent_id}</span></span>}
          {executor.thinking_level && <span>thinking: <span className="font-mono text-foreground/80">{executor.thinking_level}</span></span>}
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
            当前还没有解析出最终运行时工具面，因此这里不会再把推测值伪装成"当前可见工具"。
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
