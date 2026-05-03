/**
 * ContextOverviewTab — 右栏 "上下文" Tab 内容
 *
 * 将原来 context-panels.tsx 中 ProjectSessionContextPanel / StorySessionContextPanel
 * 的核心内容（Agent 摘要、共享目录、会话行为、Hook Runtime 等）以始终展开的
 * 滚动列表形态呈现，不再有折叠/展开切换。
 */

import { useState } from "react";
import { VfsBrowser } from "../vfs";
import { SurfaceCard } from "../session-context";
import {
  HookRuntimeSurfaceCard,
  HookRuntimePendingActionsCard,
  HookRuntimeTraceCard,
} from "../session-context";
import type {
  ExecutionVfs,
  HookSessionRuntimeInfo,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionContextSnapshot,
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
  vfs: ExecutionVfs | null;
  hookRuntime: HookSessionRuntimeInfo | null;
  sessionCapabilities: SessionBaselineCapabilities | null;
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

// ─── Component ──────────────────────────────────────────

export function ContextOverviewTab({
  contextSnapshot,
  ownerStory,
  ownerProjectName,
  executorSummary,
  runtimeSurface,
  vfs,
  hookRuntime,
  sessionCapabilities,
}: ContextOverviewTabProps) {
  const isProjectLevel = contextSnapshot?.owner_context.owner_level === "project";
  const title = isProjectLevel ? ownerProjectName : (ownerStory?.title ?? "会话上下文");

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
        vfs={vfs}
        runtimeSurface={runtimeSurface}
      />

      {/* Session 能力基线 */}
      {sessionCapabilities && (
        <SessionCapabilitiesCard capabilities={sessionCapabilities} />
      )}

      {/* Hook Runtime */}
      {hookRuntime && <HookRuntimeSurfaceCard hookRuntime={hookRuntime} />}
      {hookRuntime && <HookRuntimePendingActionsCard hookRuntime={hookRuntime} />}
      {hookRuntime && <HookRuntimeTraceCard hookRuntime={hookRuntime} />}

      {/* 技术摘要 */}
      {contextSnapshot && (
        <TechnicalBadges
          contextSnapshot={contextSnapshot}
          vfs={vfs}
        />
      )}
    </div>
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
    </SurfaceCard>
  );
}

function SharedFoldersCard({
  vfs,
  runtimeSurface,
}: {
  vfs: ExecutionVfs | null;
  runtimeSurface: ResolvedVfsSurface | null;
}) {
  const [browserOpen, setBrowserOpen] = useState(false);
  const hasMounts =
    (vfs && vfs.mounts.length > 0) || (runtimeSurface && runtimeSurface.mounts.length > 0);

  const folders = vfs
    ? vfs.mounts
        .filter(
          (m) =>
            m.provider !== "relay_fs" &&
            m.provider !== "lifecycle_vfs" &&
            m.provider !== "canvas_fs",
        )
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
              className="rounded-[10px] border border-border bg-secondary/40 px-2.5 py-1 text-xs text-foreground/85"
            >
              {folder.title}
              <span className="ml-1 font-mono text-[10px] text-muted-foreground">
                /{folder.mount}
              </span>
              {folder.writable && (
                <span className="ml-1 text-[10px] text-amber-600">可写</span>
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
              <VfsBrowser surface={runtimeSurface} vfs={vfs} />
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
  const companionCount = capabilities.companion_agents.length;
  const visibleSkills = capabilities.skills.filter((s) => !s.disable_model_invocation);
  const skillCount = visibleSkills.length;

  if (companionCount === 0 && skillCount === 0) return null;

  return (
    <SurfaceCard
      eyebrow="Session 能力基线"
      title={[
        companionCount > 0 ? `${companionCount} 个关联 Agent` : "",
        skillCount > 0 ? `${skillCount} 个可用 Skill` : "",
      ].filter(Boolean).join(" · ")}
    >
      {companionCount > 0 && (
        <div>
          <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            Companion Agents
          </p>
          <div className="flex flex-wrap gap-2">
            {capabilities.companion_agents.map((agent) => (
              <span
                key={agent.name}
                className="flex items-center gap-1.5 rounded-[8px] border border-border bg-secondary/40 px-2.5 py-1.5"
              >
                <span className="text-xs font-medium text-foreground">{agent.display_name}</span>
                <span className="rounded-[4px] bg-muted px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground">
                  {agent.executor}
                </span>
              </span>
            ))}
          </div>
        </div>
      )}
      {skillCount > 0 && (
        <div className={companionCount > 0 ? "mt-3" : ""}>
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
  vfs,
}: {
  contextSnapshot: SessionContextSnapshot;
  vfs: ExecutionVfs | null;
}) {
  const { runtime_policy, tool_visibility } = contextSnapshot.effective;
  const badges = [
    tool_visibility.resolved ? "工具面已解析" : "工具面未解析",
    runtime_policy.workspace_attached ? "已附着 workspace" : "未附着 workspace",
    runtime_policy.mcp_enabled ? "MCP 已启用" : "MCP 未启用",
    vfs?.mounts.length ? `${vfs.mounts.length} 个运行时 mount` : "无运行时 mount",
  ];

  return (
    <SurfaceCard eyebrow="技术摘要" title="运行状态概览">
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
        路径规则：{runtime_policy.path_policy}
      </p>
    </SurfaceCard>
  );
}
