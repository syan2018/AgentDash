import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAcpSession, AcpSessionEntry } from "../features/acp-session";
import { isAggregatedGroup, isAggregatedThinkingGroup } from "../features/acp-session/model/types";
import type { AcpDisplayItem, TokenUsageInfo } from "../features/acp-session/model/types";
import { promptSession, type ExecutorConfig } from "../services/executor";
import { useSessionHistoryStore } from "../stores/sessionHistoryStore";
import {
  useExecutorDiscovery,
  useExecutorConfig,
  useExecutorDiscoveredOptions,
  ExecutorSelector,
} from "../features/executor-selector";

const promptTemplates = [
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

function getItemKey(item: AcpDisplayItem): string {
  if (isAggregatedGroup(item)) return item.groupKey;
  if (isAggregatedThinkingGroup(item)) return item.groupKey;
  return item.id;
}

function formatTokens(n: number | undefined): string {
  if (n == null) return "-";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

/**
 * 上下文窗口用量指示器 — 仿 Claude/ChatGPT 的小圆环。
 * hover 时展示详细数值。
 */
function ContextUsageRing({ usage }: { usage: TokenUsageInfo | null }) {
  const [showDetail, setShowDetail] = useState(false);

  if (!usage) return null;
  const { totalTokens, maxTokens, inputTokens, outputTokens } = usage;
  const hasAny = totalTokens != null || inputTokens != null || outputTokens != null;
  if (!hasAny) return null;

  const percent = (maxTokens && totalTokens) ? Math.min(Math.round((totalTokens / maxTokens) * 100), 100) : undefined;
  const radius = 7;
  const circumference = 2 * Math.PI * radius;
  const strokeDash = percent != null ? (percent / 100) * circumference : 0;
  const isHigh = percent != null && percent > 80;

  return (
    <span
      className="relative flex items-center"
      onMouseEnter={() => setShowDetail(true)}
      onMouseLeave={() => setShowDetail(false)}
    >
      {/* 圆环 */}
      <svg width="20" height="20" className="shrink-0 -rotate-90">
        <circle cx="10" cy="10" r={radius} fill="none" stroke="currentColor" strokeWidth="2.5" className="text-muted/40" />
        {percent != null && (
          <circle
            cx="10" cy="10" r={radius}
            fill="none"
            strokeWidth="2.5"
            strokeLinecap="round"
            strokeDasharray={`${strokeDash} ${circumference}`}
            className={isHigh ? "text-warning" : "text-primary/70"}
            stroke="currentColor"
          />
        )}
      </svg>

      {/* Tooltip */}
      {showDetail && (
        <span className="absolute left-1/2 top-full z-50 mt-1.5 -translate-x-1/2 whitespace-nowrap rounded-md border border-border bg-popover px-2.5 py-1.5 text-xs text-popover-foreground shadow-md">
          {percent != null && <span className="font-medium">{percent}% 上下文</span>}
          {totalTokens != null && maxTokens != null && (
            <span className="text-muted-foreground"> ({formatTokens(totalTokens)}/{formatTokens(maxTokens)})</span>
          )}
          {(inputTokens != null || outputTokens != null) && (
            <span className="text-muted-foreground">
              {percent != null ? " · " : ""}
              {inputTokens != null && `↑${formatTokens(inputTokens)}`}
              {inputTokens != null && outputTokens != null && " "}
              {outputTokens != null && `↓${formatTokens(outputTokens)}`}
            </span>
          )}
        </span>
      )}
    </span>
  );
}

interface SessionPageProps {
  sessionId?: string;
}

export function SessionPage({ sessionId: propSessionId }: SessionPageProps) {
  const navigate = useNavigate();
  const { createNew, setActiveSessionId, reload: reloadSessions } = useSessionHistoryStore();

  const [currentSessionId, setCurrentSessionId] = useState<string | null>(propSessionId ?? null);
  const [prompt, setPrompt] = useState("");
  const [isSending, setIsSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);

  useEffect(() => {
    setCurrentSessionId(propSessionId ?? null);
    setActiveSessionId(propSessionId ?? null);
  }, [propSessionId, setActiveSessionId]);

  const discovery = useExecutorDiscovery();
  const execConfig = useExecutorConfig();
  const discovered = useExecutorDiscoveredOptions(execConfig.executor, execConfig.variant);

  const executorConfig: ExecutorConfig | undefined = useMemo(() => {
    const trimmedExecutor = execConfig.executor.trim();
    if (!trimmedExecutor) return undefined;
    return {
      executor: trimmedExecutor,
      variant: execConfig.variant.trim() || undefined,
      model_id: execConfig.modelId.trim() || undefined,
      reasoning_id: execConfig.reasoningId.trim() || undefined,
      permission_policy: (execConfig.permissionPolicy.trim() as ExecutorConfig["permission_policy"]) || undefined,
    };
  }, [execConfig.executor, execConfig.variant, execConfig.modelId, execConfig.reasoningId, execConfig.permissionPolicy]);

  const streamSessionId = currentSessionId ?? "__placeholder__";
  const {
    displayItems,
    isConnected,
    isLoading,
    isReceiving,
    error: wsError,
    reconnect,
    sendCancel,
    streamingEntryId,
    tokenUsage,
  } = useAcpSession({ sessionId: streamSessionId, enabled: currentSessionId !== null });

  const hasSession = currentSessionId !== null;

  const containerRef = useRef<HTMLDivElement>(null);
  const shouldScrollRef = useRef(true);

  useEffect(() => {
    if (!containerRef.current || !shouldScrollRef.current) return;
    containerRef.current.scrollTop = containerRef.current.scrollHeight;
  }, [displayItems.length]);

  const handleScroll = useCallback(() => {
    if (!containerRef.current) return;
    const el = containerRef.current;
    shouldScrollRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 50;
  }, []);

  const handleNewSession = useCallback(() => {
    setSendError(null);
    setPrompt("");
    setIsSending(false);
    setCurrentSessionId(null);
    setActiveSessionId(null);
    navigate("/session", { replace: true });
  }, [navigate, setActiveSessionId]);

  const handleCopySessionId = useCallback(async () => {
    if (!currentSessionId) return;
    try {
      await navigator.clipboard.writeText(currentSessionId);
    } catch {
      setSendError("复制失败：浏览器未授权访问剪贴板。");
    }
  }, [currentSessionId]);

  const handleSend = useCallback(async () => {
    const trimmed = prompt.trim();
    if (!trimmed || isSending) return;

    setSendError(null);
    setIsSending(true);

    try {
      let sid = currentSessionId;

      if (!sid) {
        const title = trimmed.slice(0, 30) + (trimmed.length > 30 ? "…" : "");
        const meta = await createNew(title);
        sid = meta.id;
        setCurrentSessionId(sid);
        setActiveSessionId(sid);
        navigate(`/session/${sid}`, { replace: true });
      }

      await promptSession(sid, { prompt: trimmed, executorConfig });
      execConfig.recordUsage();
      setPrompt("");
      void reloadSessions();
    } catch (e) {
      setSendError(e instanceof Error ? e.message : "发送失败，请重试。");
    } finally {
      setIsSending(false);
    }
  }, [prompt, isSending, currentSessionId, executorConfig, execConfig, createNew, setActiveSessionId, navigate, reloadSessions]);

  const handleCancel = sendCancel;

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        void handleSend();
      }
    },
    [handleSend],
  );

  const connectionLabel = !hasSession
    ? "待创建"
    : isConnected
      ? "已连接"
      : isLoading
        ? "连接中…"
        : "未连接";
  const connectionColor = !hasSession
    ? "bg-gray-400"
    : isConnected
      ? "bg-emerald-500"
      : isLoading
        ? "bg-amber-400 animate-pulse"
        : "bg-red-500";

  const displayError = sendError ?? (hasSession ? wsError?.message : null) ?? null;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Header */}
      <header className="flex shrink-0 items-center justify-between border-b border-border bg-card px-5 py-3">
        <div className="flex items-center gap-3 min-w-0">
          <h2 className="text-sm font-semibold text-foreground">会话</h2>
          <span className="flex items-center gap-1.5 rounded-full border border-border px-2 py-0.5 text-xs text-muted-foreground">
            <span className={`inline-block h-1.5 w-1.5 rounded-full ${connectionColor}`} />
            {connectionLabel}
          </span>
          {isReceiving && (
            <span className="flex items-center gap-1 text-xs text-primary animate-pulse">
              <span className="inline-block h-1.5 w-1.5 rounded-full bg-primary" />
              接收中
            </span>
          )}
          <ContextUsageRing usage={tokenUsage} />
        </div>

        <div className="flex items-center gap-2">
          {hasSession && (
            <>
              <span className="hidden lg:inline text-xs text-muted-foreground font-mono">
                {currentSessionId!.slice(0, 12)}…
              </span>
              <button
                type="button"
                onClick={() => void handleCopySessionId()}
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-secondary"
                title="复制 Session ID"
              >
                复制
              </button>
            </>
          )}
          <button type="button" onClick={handleNewSession} className="rounded-md bg-secondary px-2.5 py-1 text-xs font-medium text-foreground hover:bg-secondary/80">
            新会话
          </button>
        </div>
      </header>

      {/* Error banner */}
      {displayError && (
        <div className="flex items-center justify-between border-b border-destructive/40 bg-destructive/10 px-5 py-2 text-sm text-destructive">
          <span className="truncate">{displayError}</span>
          {wsError && !isConnected && hasSession && (
            <button type="button" onClick={reconnect} className="ml-4 shrink-0 rounded-md bg-destructive/20 px-2 py-0.5 text-xs hover:bg-destructive/30">
              重新连接
            </button>
          )}
        </div>
      )}

      {/* Stream area */}
      <section className="flex flex-1 overflow-hidden">
        <div className="flex flex-1 flex-col overflow-hidden">
          <div
            ref={containerRef}
            onScroll={handleScroll}
            className="flex-1 overflow-y-auto"
          >
            {hasSession && isLoading && displayItems.length === 0 ? (
              <div className="flex h-full items-center justify-center">
                <div className="text-center">
                  <div className="mx-auto h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
                  <p className="mt-2 text-sm text-muted-foreground">正在连接…</p>
                </div>
              </div>
            ) : hasSession && displayItems.length > 0 ? (
              <div className="mx-auto w-full max-w-3xl space-y-3 px-4 py-5">
                {displayItems.map((item) => (
                  <div key={getItemKey(item)}>
                    <AcpSessionEntry item={item} streamingEntryId={streamingEntryId} />
                  </div>
                ))}
              </div>
            ) : (
              <div className="flex h-full items-center justify-center">
                <div className="text-center">
                  <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-muted/50">
                    <span className="text-xl">💬</span>
                  </div>
                  <p className="text-sm text-muted-foreground">
                    {hasSession ? "会话已就绪，继续发送消息" : "输入 prompt 并发送开始会话"}
                  </p>
                </div>
              </div>
            )}
          </div>

          {/* Input area */}
          <div className="shrink-0 border-t border-border bg-card">
            <div className="mx-auto w-full max-w-3xl px-4 py-4">
              {!hasSession && (
                <div className="mb-3 flex flex-wrap gap-2">
                  {promptTemplates.map((tpl) => (
                    <button
                      key={tpl.id}
                      type="button"
                      onClick={() => setPrompt(tpl.content)}
                      className="rounded-md border border-border bg-background px-3 py-1.5 text-xs text-foreground hover:bg-secondary transition-colors"
                    >
                      {tpl.label}
                    </button>
                  ))}
                </div>
              )}

              <ExecutorSelector
                executors={discovery.executors}
                isLoading={discovery.isLoading}
                error={discovery.error}
                discoveredOptions={discovered.options}
                discoveredError={discovered.error}
                isDiscoveredLoading={Boolean(execConfig.executor.trim()) && !discovered.isInitialized}
                onDiscoveredReconnect={discovered.reconnect}
                executor={execConfig.executor}
                variant={execConfig.variant}
                modelId={execConfig.modelId}
                reasoningId={execConfig.reasoningId}
                permissionPolicy={execConfig.permissionPolicy}
                onExecutorChange={execConfig.setExecutor}
                onVariantChange={execConfig.setVariant}
                onModelIdChange={execConfig.setModelId}
                onReasoningIdChange={execConfig.setReasoningId}
                onPermissionPolicyChange={execConfig.setPermissionPolicy}
                onReset={execConfig.reset}
                onRefetch={discovery.refetch}
              />

              <div className="mt-3 flex gap-2">
                <textarea
                  value={prompt}
                  onChange={(e) => setPrompt(e.target.value)}
                  onKeyDown={handleKeyDown}
                  placeholder={hasSession ? "继续对话，Ctrl+Enter 发送…" : "输入 prompt，Ctrl+Enter 发送…"}
                  rows={3}
                  className="min-h-[72px] flex-1 resize-y rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none ring-ring focus:ring-1 transition-shadow"
                />
                <div className="flex flex-col gap-2">
                  <button
                    type="button"
                    disabled={isSending || !prompt.trim()}
                    onClick={() => void handleSend()}
                    className="h-9 w-20 rounded-lg bg-primary text-sm font-medium text-primary-foreground disabled:opacity-50 transition-opacity"
                  >
                    {isSending ? "…" : "发送"}
                  </button>
                  {hasSession && (
                    <button
                      type="button"
                      disabled={!isConnected}
                      onClick={handleCancel}
                      className="h-9 w-20 rounded-lg border border-border bg-background text-xs text-foreground hover:bg-secondary disabled:opacity-50 transition-colors"
                    >
                      取消
                    </button>
                  )}
                </div>
              </div>
              <p className="mt-1 text-xs text-muted-foreground/60">Ctrl+Enter 快捷发送</p>
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

export default SessionPage;
