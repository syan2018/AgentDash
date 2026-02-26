import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useAcpSession, AcpSessionEntry } from "../features/acp-session";
import { isAggregatedGroup, isAggregatedThinkingGroup } from "../features/acp-session/model/types";
import type { AcpDisplayItem } from "../features/acp-session/model/types";
import { promptSession, type ExecutorConfig } from "../services/executor";

function generateSessionId(): string {
  const random = Math.random().toString(36).slice(2, 10);
  return `sess-${Date.now()}-${random}`;
}

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

export function SessionPage() {
  const [sessionId, setSessionId] = useState(() => generateSessionId());
  const [prompt, setPrompt] = useState("");
  const [isSending, setIsSending] = useState(false);
  const [hasSentOnce, setHasSentOnce] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);

  const [executor, setExecutor] = useState("");
  const [modelId, setModelId] = useState("");
  const executorConfig: ExecutorConfig | undefined = useMemo(() => {
    const trimmedExecutor = executor.trim();
    const trimmedModelId = modelId.trim();
    if (!trimmedExecutor) return undefined;
    return {
      executor: trimmedExecutor,
      modelId: trimmedModelId || undefined,
    };
  }, [executor, modelId]);

  const {
    displayItems,
    isConnected,
    isLoading,
    error: wsError,
    reconnect,
    sendCancel,
  } = useAcpSession({ sessionId });

  const containerRef = useRef<HTMLDivElement>(null);
  const shouldScrollRef = useRef(true);

  useEffect(() => {
    if (!containerRef.current || !shouldScrollRef.current) return;
    containerRef.current.scrollTop = containerRef.current.scrollHeight;
  }, [displayItems]);

  const handleScroll = useCallback(() => {
    if (!containerRef.current) return;
    const el = containerRef.current;
    shouldScrollRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 50;
  }, []);

  const handleNewSession = useCallback(() => {
    setSendError(null);
    setPrompt("");
    setIsSending(false);
    setHasSentOnce(false);
    setSessionId(generateSessionId());
  }, []);

  const handleCopySessionId = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(sessionId);
    } catch {
      setSendError("复制失败：浏览器未授权访问剪贴板。");
    }
  }, [sessionId]);

  const handleSend = useCallback(async () => {
    const trimmed = prompt.trim();
    if (!trimmed || isSending) return;

    const nextSessionId = generateSessionId();
    setSessionId(nextSessionId);
    setPrompt("");
    setSendError(null);
    setIsSending(true);
    setHasSentOnce(true);

    try {
      await promptSession(nextSessionId, { prompt: trimmed, executorConfig });
    } catch (e) {
      setSendError(e instanceof Error ? e.message : "发送失败，请重试。");
      setPrompt(trimmed);
    } finally {
      setIsSending(false);
    }
  }, [prompt, isSending, executorConfig]);

  const handleCancel = useCallback(() => {
    sendCancel();
  }, [sendCancel]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        void handleSend();
      }
    },
    [handleSend],
  );

  const connectionLabel = isConnected ? "已连接" : isLoading ? "连接中…" : "未连接";
  const connectionColor = isConnected
    ? "bg-emerald-500"
    : isLoading
      ? "bg-amber-400 animate-pulse"
      : "bg-red-500";

  const displayError = sendError ?? wsError?.message ?? null;

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
            每次发送会自动创建新的 sessionId（后端同一 sessionId 只会启动一次）。
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div className="rounded-md border border-border bg-background px-3 py-2 text-xs text-muted-foreground">
            <span className="mr-2">sessionId</span>
            <span className="font-mono text-foreground">{sessionId.slice(0, 24)}…</span>
          </div>
          <button
            type="button"
            onClick={() => void handleCopySessionId()}
            className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground hover:bg-secondary"
          >
            复制
          </button>
          <button type="button" onClick={handleNewSession} className="rounded-md bg-secondary px-3 py-2 text-sm text-foreground hover:bg-secondary/80">
            新会话
          </button>
        </div>
      </header>

      {/* Error banner */}
      {displayError && (
        <div className="flex items-center justify-between border-b border-destructive/40 bg-destructive/10 px-6 py-2 text-sm text-destructive">
          <span>{displayError}</span>
          {wsError && !isConnected && (
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
            {isLoading && displayItems.length === 0 && hasSentOnce ? (
              <div className="flex h-full items-center justify-center">
                <div className="text-center">
                  <div className="mx-auto h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
                  <p className="mt-2 text-sm text-muted-foreground">正在连接…</p>
                </div>
              </div>
            ) : displayItems.length === 0 ? (
              <div className="flex h-full items-center justify-center">
                <p className="text-sm text-muted-foreground">输入 prompt 并发送开始会话</p>
              </div>
            ) : (
              <div className="space-y-1 p-4">
                {displayItems.map((item) => (
                  <div key={getItemKey(item)}>
                    <AcpSessionEntry item={item} />
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Input area */}
          <div className="shrink-0 border-t border-border bg-card px-6 py-4">
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

            <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
              <label className="md:col-span-1">
                <span className="mb-1 block text-xs font-medium text-muted-foreground">执行器（executor）</span>
                <input
                  value={executor}
                  onChange={(e) => setExecutor(e.target.value)}
                  placeholder="例如：claude_code"
                  className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none ring-ring focus:ring-1"
                />
              </label>
              <label className="md:col-span-1">
                <span className="mb-1 block text-xs font-medium text-muted-foreground">模型（modelId，可选）</span>
                <input
                  value={modelId}
                  onChange={(e) => setModelId(e.target.value)}
                  placeholder="例如：gpt-4.1 / claude-3.7-sonnet ..."
                  className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none ring-ring focus:ring-1"
                />
              </label>
              <div className="md:col-span-1" />
            </div>

            <div className="mt-3 flex gap-2">
              <textarea
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder="输入 prompt，Ctrl+Enter 发送…"
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
                <button
                  type="button"
                  disabled={!isConnected}
                  onClick={handleCancel}
                  className="h-9 w-24 rounded-md border border-border bg-background text-sm text-foreground hover:bg-secondary disabled:opacity-50"
                >
                  取消执行
                </button>
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
