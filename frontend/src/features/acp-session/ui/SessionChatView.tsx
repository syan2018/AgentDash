/**
 * 可复用的会话聊天视图
 *
 * 包含完整的 ACP 会话交互能力：流式输出、富文本输入（@ 文件引用）、
 * 执行器选择、上下文用量指示、发送/取消。
 *
 * SessionPage 和 StorySessionPanel 等场景复用此组件，
 * 由父组件管理 sessionId 生命周期和外层导航。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { useAcpSession } from "../model";
import { AcpSessionEntry } from "./AcpSessionEntry";
import { isAggregatedGroup, isAggregatedThinkingGroup } from "../model/types";
import type { AcpDisplayItem, SessionEventEnvelope, TokenUsageInfo } from "../model/types";
import { extractAgentDashMetaFromUpdate } from "../model/agentdashMeta";
import { promptSession, type ExecutorConfig } from "../../../services/executor";
import {
  useExecutorDiscovery,
  useExecutorConfig,
  useExecutorDiscoveredOptions,
  ExecutorSelector,
} from "../../executor-selector";
import {
  useFileReference,
  FilePickerPopup,
  FileReferenceTags,
  buildPromptBlocks,
  RichInput,
  type RichInputRef,
} from "../../file-reference";
import { batchReadFiles, type FileEntry } from "../../../services/filePicker";
import {
  fetchSessionExecutionState,
} from "../../../services/session";
import type { SessionExecutionState } from "../../../types";

// ─── 工具函数 ──────────────────────────────────────────

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

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function removeReferenceMarkers(prompt: string, relPath: string): string {
  const escapedPath = escapeRegExp(relPath);
  const fileMarker = new RegExp(`<file:${escapedPath}>`, "g");
  const atMarker = new RegExp(`@${escapedPath}(?=\\s|$)`, "g");

  let next = prompt.replace(fileMarker, "").replace(atMarker, "");
  next = next.replace(/[ \t]{2,}/g, " ");
  next = next.replace(/[ \t]+\n/g, "\n");
  next = next.replace(/\n{3,}/g, "\n\n");
  return next;
}

function normalizeExecutorToken(raw: string): string {
  return raw.trim().replace(/[-\s]+/g, "_").toUpperCase();
}

function resolveExecutorFromHint(
  hint: string | null | undefined,
  executors: Array<{ id: string }>,
): string | null {
  const trimmed = (hint ?? "").trim();
  if (!trimmed) return null;
  const exact = executors.find((item) => item.id === trimmed);
  if (exact) return exact.id;
  const normalized = normalizeExecutorToken(trimmed);
  const matched = executors.find((item) => normalizeExecutorToken(item.id) === normalized);
  return matched?.id ?? trimmed;
}

export function collectNewSystemEvents(
  rawEvents: SessionEventEnvelope[],
  afterSeq: number,
): {
  items: Array<{ eventSeq: number; eventType: string; update: SessionUpdate }>;
  lastSeenSeq: number;
} {
  const items: Array<{ eventSeq: number; eventType: string; update: SessionUpdate }> = [];
  let lastSeenSeq = afterSeq;

  for (const event of rawEvents) {
    if (event.event_seq <= afterSeq) {
      continue;
    }
    lastSeenSeq = Math.max(lastSeenSeq, event.event_seq);
    if (event.notification.update.sessionUpdate !== "session_info_update") {
      continue;
    }
    const eventType = extractAgentDashMetaFromUpdate(event.notification.update)?.event?.type;
    if (!eventType) {
      continue;
    }
    items.push({
      eventSeq: event.event_seq,
      eventType,
      update: event.notification.update,
    });
  }

  return { items, lastSeenSeq };
}

// ─── 子组件 ────────────────────────────────────────────

function ContextUsageRing({ usage }: { usage: TokenUsageInfo | null }) {
  const [showDetail, setShowDetail] = useState(false);
  if (!usage) return null;

  const { totalTokens, maxTokens, inputTokens, outputTokens } = usage;
  const hasAny = totalTokens != null || inputTokens != null || outputTokens != null;
  if (!hasAny) return null;

  const percent = (maxTokens && totalTokens)
    ? Math.min(Math.round((totalTokens / maxTokens) * 100), 100)
    : undefined;
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
      <svg width="20" height="20" className="shrink-0 -rotate-90">
        <circle cx="10" cy="10" r={radius} fill="none" stroke="currentColor" strokeWidth="2.5" className="text-muted/40" />
        {percent != null && (
          <circle
            cx="10" cy="10" r={radius}
            fill="none" strokeWidth="2.5" strokeLinecap="round"
            strokeDasharray={`${strokeDash} ${circumference}`}
            className={isHigh ? "text-warning" : "text-primary/70"}
            stroke="currentColor"
          />
        )}
      </svg>
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

// ─── 主组件 ────────────────────────────────────────────

export interface PromptTemplate {
  id: string;
  label: string;
  content: string;
}

export interface SessionChatViewProps {
  /** 当前会话 ID，null 表示尚未创建 */
  sessionId: string | null;
  /** 文件引用依赖的工作空间上下文 */
  workspaceId?: string | null;

  // ─── 会话生命周期 ────────────────────────────────────

  /** 无 session 时用户发送第一条消息，由父组件创建会话并返回新 ID */
  onCreateSession?: (title: string) => Promise<string>;

  /** session ID 变更后回调（创建新 session 时触发） */
  onSessionIdChange?: (id: string) => void;

  /** 消息发送成功后回调（父组件可刷新列表等） */
  onMessageSent?: () => void;

  /** Agent turn 结束时回调（turn_completed / turn_failed） */
  onTurnEnd?: () => void;

  /** 收到系统事件时回调，用于父层按事件驱动刷新额外状态面板 */
  onSystemEvent?: (eventType: string, update: SessionUpdate) => void;

  // ─── 执行器 ──────────────────────────────────────────

  /** 执行器提示（如 task 的 agent_type），自动映射为执行器选择 */
  executorHint?: string | null;

  /** 隐藏执行器选择器（当外部已确定执行器时，如 Task 场景） */
  showExecutorSelector?: boolean;

  // ─── 自定义发送流程 ──────────────────────────────────

  /**
   * 全接管发送流程 — 替换默认 onCreateSession + promptSession 链路。
   * sessionId 为 null 时代表首次发送（可在此创建会话）。
   * prompt 可为空（如 Task 无额外指令直接执行）。
   * 返回后 SessionChatView 自动清空输入。
   */
  customSend?: (
    sessionId: string | null,
    prompt: string,
    executorConfig?: ExecutorConfig,
  ) => Promise<void>;

  // ─── 布局插槽 ────────────────────────────────────────

  /** 渲染在状态栏下方、流区域上方 */
  headerSlot?: React.ReactNode;

  /** 渲染在执行器选择器上方（如 owner binding 信息） */
  inputPrefix?: React.ReactNode;

  /** 注入到流区域顶部的固定内容（如 Task 上下文卡片），始终显示 */
  streamPrefixContent?: React.ReactNode;

  /** 隐藏内置连接状态栏 */
  showStatusBar?: boolean;

  /** 无 session 时显示的 prompt 模板按钮 */
  promptTemplates?: PromptTemplate[];

  /** 输入框占位符 */
  inputPlaceholder?: string;

  /** 自定义主按钮文本（非运行状态时），默认 "发送" */
  idleSendLabel?: string;

  /** 初始输入值（仅首次挂载时填充） */
  initialInputValue?: string;
}

const ACTION_RUNNING_RELEASE_DELAY_MS = 300;

export function SessionChatView({
  sessionId,
  workspaceId,
  onCreateSession,
  onSessionIdChange,
  onMessageSent,
  onTurnEnd,
  onSystemEvent,
  executorHint,
  showExecutorSelector = true,
  customSend,
  headerSlot,
  inputPrefix,
  streamPrefixContent,
  showStatusBar = true,
  promptTemplates,
  inputPlaceholder,
  idleSendLabel = "发送",
  initialInputValue,
}: SessionChatViewProps) {
  const [isSending, setIsSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [inputValue, setInputValue] = useState("");
  const [optimisticRunning, setOptimisticRunning] = useState(false);
  const [stableActionRunning, setStableActionRunning] = useState(false);
  const [executionState, setExecutionState] = useState<SessionExecutionState | null>(null);
  const [isCancelling, setIsCancelling] = useState(false);

  const richInputRef = useRef<RichInputRef>(null);
  const appliedHintRef = useRef<string | null>(null);
  const optimisticRunningUntilRef = useRef(0);
  const actionRunningReleaseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const shouldScrollRef = useRef(true);
  const initialValueAppliedRef = useRef(false);

  const fileRef = useFileReference(workspaceId);

  const clearInput = useCallback(() => {
    richInputRef.current?.setValue("");
    setInputValue("");
  }, []);

  // 首次挂载时填充初始值
  useEffect(() => {
    if (initialInputValue && !initialValueAppliedRef.current) {
      initialValueAppliedRef.current = true;
      richInputRef.current?.setValue(initialInputValue);
      setInputValue(initialInputValue);
    }
  }, [initialInputValue]);

  // sessionId 变更时重置内部状态
  useEffect(() => {
    setSendError(null);
    setExecutionState(null);
    setIsCancelling(false);
  }, [sessionId]);

  const refreshExecutionState = useCallback(async () => {
    if (!sessionId) {
      setExecutionState(null);
      return null;
    }
    const next = await fetchSessionExecutionState(sessionId);
    setExecutionState(next);
    return next;
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId) return;
    void refreshExecutionState().catch(() => {});
  }, [sessionId, refreshExecutionState]);

  // ─── 执行器配置 ──────────────────────────────────────

  const discovery = useExecutorDiscovery();
  const execConfig = useExecutorConfig();
  const discovered = useExecutorDiscoveredOptions(execConfig.executor, execConfig.variant);
  const setExecutor = execConfig.setExecutor;
  const execProviderId = execConfig.providerId;
  const execModelId = execConfig.modelId;
  const setExecProviderId = execConfig.setProviderId;

  const resolvedHint = useMemo(
    () => resolveExecutorFromHint(executorHint, discovery.executors),
    [discovery.executors, executorHint],
  );

  useEffect(() => {
    if (!sessionId || !resolvedHint) return;
    const marker = `${sessionId}:${resolvedHint}`;
    if (appliedHintRef.current === marker) return;
    appliedHintRef.current = marker;
    setExecutor(resolvedHint);
  }, [resolvedHint, sessionId, setExecutor]);

  useEffect(() => {
    if (execProviderId.trim() || !execModelId.trim()) return;
    const matches = (discovered.options?.model_selector.models ?? []).filter(
      (model) => model.id === execModelId.trim(),
    );
    if (matches.length === 1) {
      setExecProviderId(matches[0].provider_id ?? "");
    }
  }, [
    discovered.options?.model_selector.models,
    execModelId,
    execProviderId,
    setExecProviderId,
  ]);

  const executorConfig: ExecutorConfig | undefined = useMemo(() => {
    const trimmed = execConfig.executor.trim();
    if (!trimmed) return undefined;
    return {
      executor: trimmed,
      variant: execConfig.variant.trim() || undefined,
      provider_id: execConfig.providerId.trim() || undefined,
      model_id: execConfig.modelId.trim() || undefined,
      // 将 camelCase 的 thinkingLevel 转为 snake_case 发给后端
      thinking_level: (execConfig.thinkingLevel.trim() as ExecutorConfig["thinking_level"]) || undefined,
      permission_policy: (execConfig.permissionPolicy.trim() as ExecutorConfig["permission_policy"]) || undefined,
    };
  }, [
    execConfig.executor,
    execConfig.variant,
    execConfig.providerId,
    execConfig.modelId,
    execConfig.thinkingLevel,
    execConfig.permissionPolicy,
  ]);

  // ─── ACP 会话流 ──────────────────────────────────────

  const streamSessionId = sessionId ?? "__placeholder__";
  const hasSession = sessionId !== null;

  const {
    displayItems,
    rawEntries,
    rawEvents,
    isConnected,
    isLoading,
    error: wsError,
    reconnect,
    sendCancel,
    streamingEntryId,
    tokenUsage,
  } = useAcpSession({ sessionId: streamSessionId, enabled: hasSession });

  useEffect(() => {
    if (!hasSession || executionState?.status !== "running") return;
    const timer = window.setInterval(() => {
      void refreshExecutionState().catch(() => {});
    }, 1500);
    return () => window.clearInterval(timer);
  }, [executionState?.status, hasSession, refreshExecutionState]);

  // ─── Action running 检测 ──────────────────────────────

  const streamRunning = executionState?.status === "running";

  const targetActionRunning = hasSession && (streamRunning || optimisticRunning);

  useEffect(() => {
    if (targetActionRunning) {
      if (actionRunningReleaseTimerRef.current) {
        clearTimeout(actionRunningReleaseTimerRef.current);
        actionRunningReleaseTimerRef.current = null;
      }
      setStableActionRunning(true);
      return;
    }
    if (actionRunningReleaseTimerRef.current) clearTimeout(actionRunningReleaseTimerRef.current);
    actionRunningReleaseTimerRef.current = setTimeout(() => {
      actionRunningReleaseTimerRef.current = null;
      setStableActionRunning(false);
    }, ACTION_RUNNING_RELEASE_DELAY_MS);
  }, [targetActionRunning]);

  useEffect(() => () => {
    if (actionRunningReleaseTimerRef.current) clearTimeout(actionRunningReleaseTimerRef.current);
  }, []);

  const isActionRunning = hasSession && stableActionRunning;

  useEffect(() => {
    if (!hasSession) {
      setOptimisticRunning(false);
      optimisticRunningUntilRef.current = 0;
    }
  }, [hasSession]);

  useEffect(() => {
    if (!optimisticRunning) return;
    const remainMs = Math.max(optimisticRunningUntilRef.current - Date.now(), 0);
    const timer = window.setTimeout(() => setOptimisticRunning(false), remainMs);
    return () => window.clearTimeout(timer);
  }, [optimisticRunning]);

  const onTurnEndRef = useRef(onTurnEnd);
  useEffect(() => { onTurnEndRef.current = onTurnEnd; }, [onTurnEnd]);
  const onSystemEventRef = useRef(onSystemEvent);
  const lastSystemEventSeqRef = useRef(0);
  useEffect(() => { onSystemEventRef.current = onSystemEvent; }, [onSystemEvent]);
  useEffect(() => {
    lastSystemEventSeqRef.current = 0;
  }, [sessionId]);

  useEffect(() => {
    if (!hasSession || rawEvents.length === 0) return;
    for (let i = rawEvents.length - 1; i >= 0; i -= 1) {
      const event = rawEvents[i];
      if (!event || event.notification.update.sessionUpdate !== "session_info_update") continue;
      const eventType = extractAgentDashMetaFromUpdate(event.notification.update)?.event?.type;
      if (eventType === "turn_started") {
        setOptimisticRunning(false);
        void refreshExecutionState().catch(() => {});
        return;
      }
      if (eventType === "turn_completed" || eventType === "turn_failed" || eventType === "turn_interrupted") {
        optimisticRunningUntilRef.current = 0;
        setOptimisticRunning(false);
        void refreshExecutionState().catch(() => {});
        onTurnEndRef.current?.();
        return;
      }
    }
  }, [hasSession, rawEvents, refreshExecutionState]);

  useEffect(() => {
    if (!hasSession || rawEvents.length === 0) return;
    const result = collectNewSystemEvents(rawEvents, lastSystemEventSeqRef.current);
    lastSystemEventSeqRef.current = result.lastSeenSeq;
    if (result.items.length === 0) return;
    for (const item of result.items) {
      onSystemEventRef.current?.(item.eventType, item.update);
    }
  }, [hasSession, rawEvents]);

  // ─── 自动滚动 ────────────────────────────────────────

  useEffect(() => {
    if (!containerRef.current || !shouldScrollRef.current) return;
    containerRef.current.scrollTop = containerRef.current.scrollHeight;
  }, [displayItems.length, rawEntries, streamingEntryId]);

  const handleScroll = useCallback(() => {
    if (!containerRef.current) return;
    const el = containerRef.current;
    shouldScrollRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 50;
  }, []);

  // ─── 发送 / 取消 ─────────────────────────────────────

  const customSendRef = useRef(customSend);
  useEffect(() => { customSendRef.current = customSend; }, [customSend]);

  const handleSend = useCallback(async () => {
    const promptText = richInputRef.current?.getValue() ?? "";
    const trimmed = promptText.trim();

    // customSend 模式允许空 prompt（如 Task 直接执行）
    if (!customSendRef.current && !trimmed) return;
    if (isSending) return;

    setSendError(null);
    setOptimisticRunning(true);
    optimisticRunningUntilRef.current = Date.now() + 2500;
    setIsSending(true);

    try {
      if (customSendRef.current) {
        // customSend 全接管：session 创建 + 消息发送一体处理
        await customSendRef.current(sessionId, trimmed, executorConfig);
      } else {
        // 默认流程：onCreateSession → promptSession
        let sid = sessionId;
        if (!sid) {
          if (!onCreateSession) { setSendError("当前无法创建新会话"); return; }
          const title = trimmed.slice(0, 30) + (trimmed.length > 30 ? "…" : "");
          sid = await onCreateSession(title);
          onSessionIdChange?.(sid);
        }

        if (fileRef.references.length > 0) {
          if (!workspaceId) {
            throw new Error("当前会话没有可用的工作空间，无法附加文件引用");
          }
          const paths = fileRef.references.map((r) => r.relPath);
          const batchResult = await batchReadFiles(workspaceId, paths);
          const blocks = buildPromptBlocks(trimmed, batchResult.files);
          await promptSession(sid, { promptBlocks: blocks, executorConfig });
          fileRef.clearReferences();
        } else {
          await promptSession(sid, {
            promptBlocks: buildPromptBlocks(trimmed, []),
            executorConfig,
          });
        }
      }

      execConfig.recordUsage();
      clearInput();
      void refreshExecutionState().catch(() => {});
      onMessageSent?.();
    } catch (e) {
      optimisticRunningUntilRef.current = 0;
      setOptimisticRunning(false);
      setSendError(e instanceof Error ? e.message : "发送失败，请重试。");
    } finally {
      setIsSending(false);
    }
  }, [isSending, sessionId, executorConfig, execConfig, onCreateSession, onSessionIdChange, onMessageSent, fileRef, clearInput, refreshExecutionState, workspaceId]);

  const handleCancel = useCallback(async () => {
    if (!hasSession || !sessionId || isCancelling) return;
    setSendError(null);
    setIsCancelling(true);
    try {
      await sendCancel();
      const next = await refreshExecutionState();
      if (next?.status === "interrupted" || next?.status === "idle") {
        optimisticRunningUntilRef.current = 0;
        setOptimisticRunning(false);
      }
    } catch (e) {
      setSendError(e instanceof Error ? e.message : "取消失败，请重试。");
    } finally {
      setIsCancelling(false);
    }
  }, [hasSession, isCancelling, refreshExecutionState, sendCancel, sessionId]);

  const handlePrimaryAction = useCallback(async () => {
    if (hasSession && isActionRunning) {
      await handleCancel();
      return;
    }
    await handleSend();
  }, [handleCancel, handleSend, hasSession, isActionRunning]);

  // ─── 文件引用 & 键盘 ─────────────────────────────────

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (fileRef.pickerOpen) {
        if (e.key === "ArrowDown") { e.preventDefault(); fileRef.moveSelection(1); return; }
        if (e.key === "ArrowUp") { e.preventDefault(); fileRef.moveSelection(-1); return; }
        if (e.key === "Enter" && !e.ctrlKey && !e.metaKey) { e.preventDefault(); fileRef.confirmSelection(); return; }
        if (e.key === "Escape") { e.preventDefault(); fileRef.closePicker(); return; }
      }
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); void handleSend(); }
    },
    [fileRef, handleSend],
  );

  const handleAtTrigger = useCallback((query: string) => {
    if (fileRef.canAddMore && fileRef.hasWorkspaceContext) {
      richInputRef.current?.saveSelection();
      fileRef.openPicker(query);
    }
  }, [fileRef]);

  const handleFileSelected = useCallback((file: FileEntry) => {
    const alreadySelected = fileRef.references.some((r) => r.relPath === file.relPath);
    if (!fileRef.canAddMore && !alreadySelected) { fileRef.closePicker(); return; }
    fileRef.addReference(file);
    if (alreadySelected) return;
    requestAnimationFrame(() => { richInputRef.current?.insertFileReference(file); });
  }, [fileRef]);

  // ─── 派生状态 ────────────────────────────────────────

  const connectionLabel = !hasSession
    ? "待创建"
    : isConnected ? "已连接" : isLoading ? "连接中…" : "未连接";
  const connectionColor = !hasSession
    ? "bg-muted-foreground/40"
    : isConnected ? "bg-success" : isLoading ? "bg-warning animate-pulse" : "bg-destructive";

  const displayError = sendError ?? (hasSession ? wsError?.message : null) ?? null;

  // ─── 渲染 ────────────────────────────────────────────

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* 内置状态栏 — 可通过 showStatusBar=false 隐藏 */}
      {showStatusBar && (
        <div className="flex shrink-0 items-center gap-2.5 border-b border-border bg-background px-5 py-2">
          <span className="flex items-center gap-1.5 rounded-full border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground">
            <span className={`inline-block h-1.5 w-1.5 rounded-full ${connectionColor}`} />
            {connectionLabel}
          </span>
          {isActionRunning && (
            <span className="flex items-center gap-1 rounded-full border border-primary/20 bg-primary/8 px-2.5 py-1 text-xs text-primary">
              <span className="inline-block h-1.5 w-1.5 rounded-full bg-primary" />
              {isConnected ? "接收中" : "执行中"}
            </span>
          )}
          <ContextUsageRing usage={tokenUsage} />
        </div>
      )}

      {/* headerSlot — 外部注入区（如 Task 执行控制栏） */}
      {headerSlot}

      {/* 错误横幅 */}
      {displayError && (
        <div className="flex shrink-0 items-center justify-between border-b border-destructive/40 bg-destructive/10 px-5 py-2 text-sm text-destructive">
          <span className="truncate">{displayError}</span>
          {wsError && !isConnected && hasSession && (
            <button type="button" onClick={reconnect} className="ml-4 shrink-0 rounded-md bg-destructive/20 px-2 py-0.5 text-xs hover:bg-destructive/30">
              重新连接
            </button>
          )}
        </div>
      )}

      {/* 流显示区 */}
      <div
        ref={containerRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto"
      >
        {hasSession && isLoading && displayItems.length === 0 && !streamPrefixContent ? (
          <div className="flex h-full items-center justify-center">
            <div className="text-center">
              <div className="mx-auto h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
              <p className="mt-2 text-sm text-muted-foreground">正在连接…</p>
            </div>
          </div>
        ) : (hasSession && displayItems.length > 0) || streamPrefixContent ? (
          <div className="mx-auto w-full max-w-4xl space-y-3 px-5 py-6">
            {streamPrefixContent}
            {displayItems.map((item) => (
              <div key={getItemKey(item)}>
                <AcpSessionEntry
                  item={item}
                  streamingEntryId={streamingEntryId}
                  sessionId={sessionId}
                />
              </div>
            ))}
          </div>
        ) : (
          <div className="flex h-full items-center justify-center">
            <div className="text-center">
              <div className="mx-auto mb-4 w-fit rounded-[10px] border border-dashed border-border bg-secondary px-3 py-1 text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                Session
              </div>
              <p className="text-sm text-muted-foreground">
                {hasSession ? "会话已就绪，继续发送消息" : "输入 prompt 并发送开始会话"}
              </p>
            </div>
          </div>
        )}
      </div>

      {/* 输入区 */}
      <div className="shrink-0 border-t border-border bg-background">
        <div className="mx-auto w-full max-w-4xl px-5 py-4">
          {/* prompt 模板（仅默认模式） */}
          {!hasSession && !customSend && promptTemplates && promptTemplates.length > 0 && (
            <div className="mb-3 flex flex-wrap gap-2">
              {promptTemplates.map((tpl) => (
                <button
                  key={tpl.id}
                  type="button"
                  onClick={() => richInputRef.current?.setValue(tpl.content)}
                  className="rounded-[10px] border border-border bg-background px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                >
                  {tpl.label}
                </button>
              ))}
            </div>
          )}

          {inputPrefix}

          {/* 执行器选择（可隐藏） */}
          {showExecutorSelector && (
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
              providerId={execConfig.providerId}
              modelId={execConfig.modelId}
              thinkingLevel={execConfig.thinkingLevel}
              permissionPolicy={execConfig.permissionPolicy}
              onExecutorChange={execConfig.setExecutor}
              onVariantChange={execConfig.setVariant}
              onProviderIdChange={execConfig.setProviderId}
              onModelIdChange={execConfig.setModelId}
              onThinkingLevelChange={execConfig.setThinkingLevel}
              onPermissionPolicyChange={execConfig.setPermissionPolicy}
              onReset={execConfig.reset}
              onRefetch={discovery.refetch}
            />
          )}

          {/* 富文本输入 */}
          <div className={`relative rounded-[14px] border border-border bg-secondary/60 p-3${showExecutorSelector ? " mt-3" : ""}`}>
            <FileReferenceTags
              references={fileRef.references}
              onRemove={(relPath) => {
                fileRef.removeReference(relPath);
                const cur = richInputRef.current?.getValue() ?? "";
                const next = removeReferenceMarkers(cur, relPath);
                richInputRef.current?.setValue(next);
              }}
            />

            <div className="relative flex gap-3">
              <div className="relative flex-1">
                <FilePickerPopup
                  open={fileRef.pickerOpen}
                  query={fileRef.pickerQuery}
                  files={fileRef.pickerFiles}
                  loading={fileRef.pickerLoading}
                  error={fileRef.pickerError}
                  selectedIndex={fileRef.selectedIndex}
                  onQueryChange={fileRef.updateQuery}
                  onSelect={handleFileSelected}
                  onClose={fileRef.closePicker}
                  onMoveSelection={fileRef.moveSelection}
                  onConfirmSelection={() => {
                    const selectedFile = fileRef.pickerFiles[fileRef.selectedIndex];
                    if (!selectedFile) return;
                    handleFileSelected(selectedFile);
                  }}
                />
                <RichInput
                  ref={richInputRef}
                  placeholder={inputPlaceholder ?? (hasSession ? "继续对话，@ 引用文件，Ctrl+Enter 发送…" : "输入 prompt，@ 引用文件，Ctrl+Enter 发送…")}
                  onChange={setInputValue}
                  onKeyDown={handleKeyDown}
                  onAtTrigger={handleAtTrigger}
                  onFileReferenceRemoved={(relPath) => { fileRef.removeReference(relPath); }}
                  disabled={isSending}
                />
              </div>
              <div className="flex flex-col gap-2 self-end">
                <button
                  type="button"
                  disabled={
                    isSending ||
                    isCancelling ||
                    (hasSession && isActionRunning
                      ? false
                      : customSend ? false : !inputValue.trim())
                  }
                  onClick={() => { void handlePrimaryAction(); }}
                  className={`h-10 w-20 rounded-[12px] border text-sm font-medium transition-colors disabled:opacity-50 ${
                    hasSession && isActionRunning
                      ? "border-border bg-background text-foreground hover:bg-secondary"
                      : "border-primary bg-primary text-primary-foreground hover:opacity-95"
                  }`}
                >
                  {isSending ? "…" : isCancelling ? "取消中…" : hasSession && isActionRunning ? "取消" : idleSendLabel}
                </button>
              </div>
            </div>
          </div>
          <p className="mt-1 text-xs text-muted-foreground/60">
            Ctrl+Enter 快捷发送 · {workspaceId ? "@ 引用工作空间文件" : "当前会话未绑定工作空间，@ 文件引用不可用"}
          </p>
        </div>
      </div>
    </div>
  );
}
