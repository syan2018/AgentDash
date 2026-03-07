import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { SessionChatView, type PromptTemplate } from "../features/acp-session";
import { fetchSessionBindings } from "../services/session";
import { useSessionHistoryStore } from "../stores/sessionHistoryStore";
import { useStoryStore } from "../stores/storyStore";
import type {
  AgentBinding,
  SessionBindingOwner,
  SessionNavigationState,
  StoryNavigationState,
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

export function SessionPage({ sessionId: propSessionId }: SessionPageProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams] = useSearchParams();
  const fetchTaskSession = useStoryStore((state) => state.fetchTaskSession);
  const { createNew, setActiveSessionId, reload: reloadSessions } = useSessionHistoryStore();

  const [currentSessionId, setCurrentSessionId] = useState<string | null>(propSessionId ?? null);
  const [taskAgentBinding, setTaskAgentBinding] = useState<AgentBinding | null>(null);
  const [sessionBindings, setSessionBindings] = useState<SessionBindingOwner[]>([]);

  const routeState = useMemo(
    () => (location.state as SessionNavigationState | null) ?? null,
    [location.state],
  );
  const taskIdFromQuery = searchParams.get("task_id")?.trim() || "";
  const taskContextFromRoute = routeState?.task_context ?? null;
  const returnTarget = routeState?.return_to ?? null;
  const taskIdHint = taskContextFromRoute?.task_id ?? taskIdFromQuery;

  // ─── session ID 同步 ──────────────────────────────────

  useEffect(() => {
    setCurrentSessionId(propSessionId ?? null);
    setActiveSessionId(propSessionId ?? null);
  }, [propSessionId, setActiveSessionId]);

  // ─── task agent binding 加载 ──────────────────────────

  useEffect(() => {
    setTaskAgentBinding(taskContextFromRoute?.agent_binding ?? null);
  }, [taskContextFromRoute?.agent_binding, propSessionId]);

  useEffect(() => {
    if (!taskIdHint) return;
    let cancelled = false;
    void (async () => {
      const taskSession = await fetchTaskSession(taskIdHint);
      if (cancelled || !taskSession?.agent_binding) return;
      setTaskAgentBinding(taskSession.agent_binding);
    })();
    return () => { cancelled = true; };
  }, [fetchTaskSession, taskIdHint]);

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

  const executorHint = taskAgentBinding?.agent_type ?? null;

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

export default SessionPage;
