import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAcpSession, AcpSessionEntry } from "../features/acp-session";
import { isAggregatedGroup, isAggregatedThinkingGroup } from "../features/acp-session/model/types";
import type { AcpDisplayItem } from "../features/acp-session/model/types";
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

  // 只有 currentSessionId 存在时才连接流
  const streamSessionId = currentSessionId ?? "__placeholder__";
  const {
    displayItems,
    isConnected,
    isLoading,
    error: wsError,
    reconnect,
    sendCancel,
    streamingEntryId,
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

      // 如果还没有 session，先创建
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
      <header className="flex shrink-0 flex-col gap-3 border-b border-border bg-card px-6 py-4 md:flex-row md:items-center md:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h2 className="text-base font-semibold text-foreground">会话（Session）</h2>
            <span className="flex items-center gap-1.5 rounded-full border border-border px-2 py-0.5 text-xs text-muted-foreground">
              <span className={`inline-block h-2 w-2 rounded-full ${connectionColor}`} />
              {connectionLabel}
            </span>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {hasSession
              ? "在当前会话中继续对话，或点击「新会话」开始新的对话。"
              : "输入 prompt 发送后将自动创建新会话。"}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {hasSession && (
            <>
              <div className="rounded-md border border-border bg-background px-3 py-2 text-xs text-muted-foreground">
                <span className="mr-2">sessionId</span>
                <span className="font-mono text-foreground">{currentSessionId!.slice(0, 24)}…</span>
              </div>
              <button
                type="button"
                onClick={() => void handleCopySessionId()}
                className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground hover:bg-secondary"
              >
                复制
              </button>
            </>
          )}
          <button type="button" onClick={handleNewSession} className="rounded-md bg-secondary px-3 py-2 text-sm text-foreground hover:bg-secondary/80">
            新会话
          </button>
        </div>
      </header>

      {/* Error banner */}
      {displayError && (
        <div className="flex items-center justify-between border-b border-destructive/40 bg-destructive/10 px-6 py-2 text-sm text-destructive">
          <span>{displayError}</span>
          {wsError && !isConnected && hasSession && (
            <button type="button" onClick={reconnect} className="ml-4 rounded-md bg-destructive/20 px-2 py-0.5 text-xs hover:bg-destructive/30">
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
              <div className="space-y-1 p-4">
                {displayItems.map((item) => (
                  <div key={getItemKey(item)}>
                    <AcpSessionEntry item={item} streamingEntryId={streamingEntryId} />
                  </div>
                ))}
              </div>
            ) : (
              <div className="flex h-full items-center justify-center">
                <div className="text-center">
                  <p className="text-sm text-muted-foreground">
                    {hasSession ? "会话已就绪，继续发送消息" : "输入 prompt 并发送开始会话"}
                  </p>
                </div>
              </div>
            )}
          </div>

          {/* Input area */}
          <div className="shrink-0 border-t border-border bg-card px-6 py-4">
            {!hasSession && (
              <div className="mb-3 flex flex-wrap gap-2">
                {promptTemplates.map((tpl) => (
                  <button
                    key={tpl.id}
                    type="button"
                    onClick={() => setPrompt(tpl.content)}
                    className="rounded-md border border-border bg-background px-3 py-1.5 text-xs text-foreground hover:bg-secondary"
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
                className="min-h-[72px] flex-1 resize-y rounded-md border border-border bg-background px-3 py-2 text-sm outline-none ring-ring focus:ring-1"
              />
              <div className="flex flex-col gap-2">
                <button
                  type="button"
                  disabled={isSending || !prompt.trim()}
                  onClick={() => void handleSend()}
                  className="h-9 w-24 rounded-md bg-primary text-sm font-medium text-primary-foreground disabled:opacity-50"
                >
                  {isSending ? "发送中…" : "发送"}
                </button>
                {hasSession && (
                  <button
                    type="button"
                    disabled={!isConnected}
                    onClick={handleCancel}
                    className="h-9 w-24 rounded-md border border-border bg-background text-sm text-foreground hover:bg-secondary disabled:opacity-50"
                  >
                    取消执行
                  </button>
                )}
              </div>
            </div>
            <p className="mt-1 text-xs text-muted-foreground">Ctrl+Enter 快捷发送</p>
          </div>
        </div>
      </section>
    </div>
  );
}

export default SessionPage;
