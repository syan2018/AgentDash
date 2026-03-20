import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { SessionChatView, type PromptTemplate } from "../features/acp-session";
import { fetchSessionBindings } from "../services/session";
import { useSessionHistoryStore } from "../stores/sessionHistoryStore";
import { useStoryStore } from "../stores/storyStore";
import type {
  AgentBinding,
  ContextContainerDefinition,
  ExecutionAddressSpace,
  MountDerivationPolicy,
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
  unresolved: "未解析",
};

function SessionContextPanel({
  story,
  contextSnapshot,
  executorSummary,
  addressSpace,
  isOpen,
  onToggle,
}: {
  story: Story;
  contextSnapshot?: SessionContextSnapshot | null;
  executorSummary?: TaskSessionExecutorSummary | null;
  addressSpace?: ExecutionAddressSpace | null;
  isOpen: boolean;
  onToggle: () => void;
}) {
  const projectDefaults = contextSnapshot?.project_defaults ?? null;
  const storyOverrides = contextSnapshot?.story_overrides ?? null;
  const effective = contextSnapshot?.effective ?? null;
  const effectiveComposition = effective?.session_composition ?? story.context.session_composition_override ?? null;
  const projectContainerCount = projectDefaults?.context_containers.length ?? 0;
  const storyContainerCount = storyOverrides?.context_containers.length ?? story.context.context_containers.length;
  const mountCount = addressSpace?.mounts.length ?? 0;

  return (
    <div className="shrink-0 border-b border-border">
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center justify-between px-5 py-2 text-xs text-muted-foreground transition-colors hover:bg-secondary/25"
      >
        <div className="flex items-center gap-2">
          <svg
            className={`h-3.5 w-3.5 transition-transform ${isOpen ? "rotate-90" : ""}`}
            fill="none" viewBox="0 0 24 24" stroke="currentColor"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
          <span className="font-medium">运行上下文</span>
          <span className="text-muted-foreground/60">· {story.title}</span>
          {projectContainerCount > 0 && (
            <span className="rounded-full border border-violet-400/30 bg-violet-500/10 px-1.5 py-0.5 text-[10px] font-medium text-violet-600">
              项目 {projectContainerCount} 容器
            </span>
          )}
          {storyContainerCount > 0 && (
            <span className="rounded-full border border-rose-400/30 bg-rose-500/10 px-1.5 py-0.5 text-[10px] font-medium text-rose-600">
              Story {storyContainerCount} 容器
            </span>
          )}
          {effectiveComposition?.persona_label && (
            <span className="rounded-full border border-cyan-400/30 bg-cyan-500/10 px-1.5 py-0.5 text-[10px] font-medium text-cyan-600">
              🎭 {effectiveComposition.persona_label}
            </span>
          )}
          {effectiveComposition && effectiveComposition.workflow_steps.length > 0 && (
            <span className="rounded-full border border-emerald-400/30 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium text-emerald-600">
              📋 {effectiveComposition.workflow_steps.length} 步
            </span>
          )}
          {mountCount > 0 && (
            <span className="rounded-full border border-amber-400/30 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium text-amber-600">
              💾 {mountCount} mount
            </span>
          )}
        </div>
      </button>

      {isOpen && (
        <div className="max-h-[40vh] space-y-3 overflow-y-auto border-t border-border bg-secondary/10 px-5 py-4">
          <InheritanceRules
            story={story}
            contextSnapshot={contextSnapshot}
            executorSummary={executorSummary}
          />

          {contextSnapshot ? (
            <>
              {executorSummary && <ExecutorSummaryCard executor={executorSummary} />}
              <ContainerGroup
                title="Project 默认容器"
                containers={projectDefaults?.context_containers ?? []}
                emptyText="Project 未配置容器"
              />
              {projectDefaults && (
                <MountPolicyCard
                  title="Project 默认挂载策略"
                  policy={projectDefaults.mount_policy}
                />
              )}
              {projectDefaults && (
                <SessionCompositionCard
                  title="Project 默认会话编排"
                  composition={projectDefaults.session_composition}
                />
              )}
              <ContainerGroup
                title="Story 追加容器"
                containers={storyOverrides?.context_containers ?? []}
                emptyText="Story 未追加容器"
              />
              <DisabledContainerCard
                ids={storyOverrides?.disabled_container_ids ?? story.context.disabled_container_ids}
              />
              {storyOverrides?.mount_policy_override && (
                <MountPolicyCard
                  title="Story 挂载策略覆盖"
                  policy={storyOverrides.mount_policy_override}
                />
              )}
              {storyOverrides?.session_composition_override && (
                <SessionCompositionCard
                  title="Story 会话编排覆盖"
                  composition={storyOverrides.session_composition_override}
                />
              )}
              {effective && (
                <>
                  <MountPolicyCard title="当前生效挂载策略" policy={effective.mount_policy} />
                  <SessionCompositionCard
                    title="当前生效会话编排"
                    composition={effective.session_composition}
                  />
                  <ToolVisibilityCard summary={effective.tool_visibility} />
                  <RuntimePolicyCard summary={effective.runtime_policy} />
                </>
              )}
            </>
          ) : (
            <>
              <ContainerGroup
                title="Story 级容器"
                containers={story.context.context_containers}
                emptyText="Story 暂无容器"
              />
              <DisabledContainerCard ids={story.context.disabled_container_ids} />
              {story.context.mount_policy_override && (
                <MountPolicyCard
                  title="Story 挂载策略覆盖"
                  policy={story.context.mount_policy_override}
                />
              )}
              {story.context.session_composition_override && (
                <SessionCompositionCard
                  title="Story 会话编排覆盖"
                  composition={story.context.session_composition_override}
                />
              )}
            </>
          )}

          <AddressSpaceCard addressSpace={addressSpace} />
        </div>
      )}
    </div>
  );
}

function InheritanceRules({
  story,
  contextSnapshot,
  executorSummary,
}: {
  story: Story;
  contextSnapshot?: SessionContextSnapshot | null;
  executorSummary?: TaskSessionExecutorSummary | null;
}) {
  const disabledCount = contextSnapshot?.story_overrides.disabled_container_ids.length ?? story.context.disabled_container_ids.length;

  return (
    <div className="rounded-[10px] border border-border bg-background/70 px-3 py-3">
      <p className="mb-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">传递规则</p>
      <ol className="space-y-1.5 pl-4 text-[11px] leading-5 text-foreground/80">
        <li className="list-decimal">
          {executorSummary ? (
            <>
              Agent 身份按 `Task agent_type → Task preset → Project 默认 Agent` 解析。
              当前来源：
              <span className="font-medium text-foreground">
                {EXECUTOR_SOURCE_LABELS[executorSummary.source] ?? executorSummary.source}
              </span>
            </>
          ) : (
            <>
              Story 伴随会话没有固定的 Task Agent 绑定。
              运行时会优先采用“本次发送时显式选择的 executor”，未指定时再回退到
              <span className="font-medium text-foreground"> Project 默认 Agent </span>
              作为基线。
            </>
          )}
        </li>
        <li className="list-decimal">
          容器候选集先取 Project 默认容器，再叠加 Story 追加容器；如果 Story 禁用了项目容器，会先从候选集中移除。
          当前禁用数：<span className="font-medium text-foreground">{disabledCount}</span>
        </li>
        <li className="list-decimal">
          挂载策略与会话编排都遵循“Project 默认 + Story 非空覆盖”的规则，最终生效结果以本面板的“当前生效”区块为准。
        </li>
        <li className="list-decimal">
          运行时 mounts、工具可见性和路径规则最后由 Address Space 与 MCP 注入共同决定，所以它们属于最内层的 Session Runtime 结果。
        </li>
      </ol>
    </div>
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
            {EXECUTOR_SOURCE_LABELS[executor.source] ?? executor.source}
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
  const fetchTaskSession = useStoryStore((state) => state.fetchTaskSession);
  const fetchStorySessionInfo = useStoryStore((state) => state.fetchStorySessionInfo);
  const { createNew, setActiveSessionId, reload: reloadSessions } = useSessionHistoryStore();

  const [currentSessionId, setCurrentSessionId] = useState<string | null>(propSessionId ?? null);
  const [taskAgentBinding, setTaskAgentBinding] = useState<AgentBinding | null>(null);
  const [sessionAddressSpace, setSessionAddressSpace] = useState<ExecutionAddressSpace | null>(null);
  const [sessionContextSnapshot, setSessionContextSnapshot] = useState<SessionContextSnapshot | null>(null);
  const [taskExecutorSummary, setTaskExecutorSummary] = useState<TaskSessionExecutorSummary | null>(null);
  const [sessionBindings, setSessionBindings] = useState<SessionBindingOwner[]>([]);

  const routeState = useMemo(
    () => (location.state as SessionNavigationState | null) ?? null,
    [location.state],
  );
  const taskIdFromQuery = searchParams.get("task_id")?.trim() || "";
  const taskContextFromRoute = routeState?.task_context ?? null;
  const returnTarget = routeState?.return_to ?? null;
  const routeTaskIdHint = taskContextFromRoute?.task_id ?? taskIdFromQuery;

  // ─── session ID 同步 ──────────────────────────────────

  useEffect(() => {
    setCurrentSessionId(propSessionId ?? null);
    setActiveSessionId(propSessionId ?? null);
  }, [propSessionId, setActiveSessionId]);

  // ─── task agent binding 加载 ──────────────────────────

  useEffect(() => {
    setTaskAgentBinding(taskContextFromRoute?.agent_binding ?? null);
  }, [taskContextFromRoute?.agent_binding, propSessionId]);

  // ─── session bindings（用于 owner 展示） ──────────────

  useEffect(() => {
    if (!currentSessionId) { setSessionBindings([]); return; }
    let cancelled = false;
    void (async () => {
      try {
        const bindings = await fetchSessionBindings(currentSessionId);
        if (!cancelled) setSessionBindings(bindings);
      } catch {
        if (!cancelled) setSessionBindings([]);
      }
    })();
    return () => { cancelled = true; };
  }, [currentSessionId]);

  const sessionOwnerBinding = useMemo(() => {
    if (sessionBindings.length === 0) return null;
    return (
      sessionBindings.find((b) => b.owner_type === "story")
      ?? sessionBindings.find((b) => b.owner_type === "task")
      ?? sessionBindings[0]
      ?? null
    );
  }, [sessionBindings]);
  const taskIdHint = routeTaskIdHint || sessionOwnerBinding?.task_id || "";

  useEffect(() => {
    let cancelled = false;

    if (taskIdHint) {
      void (async () => {
        const taskSession = await fetchTaskSession(taskIdHint);
        if (cancelled) return;
        if (!taskSession) {
          setSessionAddressSpace(null);
          setSessionContextSnapshot(null);
          setTaskExecutorSummary(null);
          return;
        }
        setTaskAgentBinding(taskSession.agent_binding);
        setSessionAddressSpace(taskSession.address_space ?? null);
        setSessionContextSnapshot(taskSession.context_snapshot ?? null);
        setTaskExecutorSummary(taskSession.context_snapshot?.executor ?? null);
      })();
      return () => { cancelled = true; };
    }

    if (
      sessionOwnerBinding?.owner_type === "story"
      && sessionOwnerBinding.story_id
      && sessionOwnerBinding.id
    ) {
      void (async () => {
        const storySession = await fetchStorySessionInfo(
          sessionOwnerBinding.story_id,
          sessionOwnerBinding.id,
        );
        if (cancelled) return;
        setTaskAgentBinding(null);
        setSessionAddressSpace(storySession?.address_space ?? null);
        setSessionContextSnapshot(storySession?.context_snapshot ?? null);
        setTaskExecutorSummary(null);
      })();
      return () => { cancelled = true; };
    }

    setTaskAgentBinding(null);
    setSessionAddressSpace(null);
    setSessionContextSnapshot(null);
    setTaskExecutorSummary(null);
    return () => { cancelled = true; };
  }, [fetchStorySessionInfo, fetchTaskSession, sessionOwnerBinding, taskIdHint]);

  // 按需加载关联 Story 的上下文信息
  const fetchStoryById = useStoryStore((s) => s.fetchStoryById);
  const stories = useStoryStore((s) => s.stories);
  const ownerStoryId = sessionOwnerBinding?.story_id ?? null;
  const [ownerStory, setOwnerStory] = useState<Story | null>(null);
  const [isContextPanelOpen, setIsContextPanelOpen] = useState(false);

  useEffect(() => {
    if (!ownerStoryId) { setOwnerStory(null); return; }
    const cached = stories.find((s) => s.id === ownerStoryId);
    if (cached) { setOwnerStory(cached); return; }
    let cancelled = false;
    void (async () => {
      const result = await fetchStoryById(ownerStoryId);
      if (!cancelled) setOwnerStory(result);
    })();
    return () => { cancelled = true; };
  }, [ownerStoryId, stories, fetchStoryById]);

  const effectiveReturnTarget = useMemo(() => {
    if (returnTarget) return returnTarget;
    if (!sessionOwnerBinding?.story_id) return null;
    if (sessionOwnerBinding.owner_type === "story") {
      return { owner_type: "story" as const, story_id: sessionOwnerBinding.story_id };
    }
    if (!sessionOwnerBinding.task_id) return null;
    return { owner_type: "task" as const, story_id: sessionOwnerBinding.story_id, task_id: sessionOwnerBinding.task_id };
  }, [returnTarget, sessionOwnerBinding]);

  // ─── 页面级回调 ───────────────────────────────────────

  const executorHint = taskAgentBinding?.agent_type ?? taskExecutorSummary?.executor ?? null;

  const handleCreateSession = useCallback(async (title: string) => {
    const meta = await createNew(title);
    return meta.id;
  }, [createNew]);

  const handleSessionIdChange = useCallback((id: string) => {
    setCurrentSessionId(id);
    setActiveSessionId(id);
    navigate(`/session/${id}`, { replace: true });
  }, [navigate, setActiveSessionId]);

  const handleMessageSent = useCallback(() => {
    void reloadSessions();
  }, [reloadSessions]);

  const handleNewSession = useCallback(() => {
    setCurrentSessionId(null);
    setActiveSessionId(null);
    navigate("/session", { replace: true });
  }, [navigate, setActiveSessionId]);

  const handleBackToOwner = useCallback(() => {
    if (!effectiveReturnTarget) return;
    if (effectiveReturnTarget.owner_type === "task") {
      const state: StoryNavigationState = { open_task_id: effectiveReturnTarget.task_id };
      navigate(`/story/${effectiveReturnTarget.story_id}`, { state });
      return;
    }
    navigate(`/story/${effectiveReturnTarget.story_id}`);
  }, [effectiveReturnTarget, navigate]);

  const handleCopySessionId = useCallback(async () => {
    if (!currentSessionId) return;
    try { await navigator.clipboard.writeText(currentSessionId); } catch { /* noop */ }
  }, [currentSessionId]);

  const backButtonLabel = effectiveReturnTarget?.owner_type === "task" ? "返回任务" : "返回 Story";
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
      {sessionOwnerBinding.story_id && (
        <button
          type="button"
          onClick={handleBackToOwner}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] transition-colors hover:bg-secondary hover:text-foreground"
        >
          打开关联{sessionOwnerBinding.owner_type === "task" ? "任务" : "Story"}
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
                {currentSessionId!.slice(0, 12)}…
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

      {/* 上下文摘要面板（当有关联 Story 且包含容器/编排信息时展示） */}
      {ownerStory && (hasStoryContextInfo(ownerStory) || sessionContextSnapshot != null || (sessionAddressSpace && sessionAddressSpace.mounts.length > 0)) && (
        <SessionContextPanel
          story={ownerStory}
          contextSnapshot={sessionContextSnapshot}
          executorSummary={taskExecutorSummary}
          addressSpace={sessionAddressSpace}
          isOpen={isContextPanelOpen}
          onToggle={() => setIsContextPanelOpen((v) => !v)}
        />
      )}

      {/* 复用的聊天视图 */}
      <div className="flex-1 overflow-hidden">
        <SessionChatView
          sessionId={currentSessionId}
          onCreateSession={handleCreateSession}
          onSessionIdChange={handleSessionIdChange}
          onMessageSent={handleMessageSent}
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
          <span className="text-muted-foreground">toolset:</span>
          <span className="font-mono text-foreground/80">{summary.toolset_label}</span>
        </div>
        <div className="mt-1 flex flex-wrap gap-1">
          {summary.tool_names.map((tool) => (
            <span key={tool} className="rounded-full border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {tool}
            </span>
          ))}
        </div>
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
  if (container.exposure.include_in_story_sessions) targets.push("story");
  if (container.exposure.include_in_task_sessions) targets.push("task");
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
