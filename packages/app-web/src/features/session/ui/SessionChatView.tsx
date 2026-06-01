/**
 * 可复用的会话聊天视图
 *
 * 包含完整的会话交互能力：流式输出、富文本输入（@ 文件引用）、
 * 执行器选择、上下文用量指示、发送/取消。
 *
 * SessionPage 等 runtime trace 场景复用此组件，
 * 由父组件管理 sessionId 生命周期和外层导航。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSessionFeed } from "../model";
import { extractPlatformEventType } from "../model/platformEvent";
import type { ExecutorConfig } from "../../../services/executor";
import {
  useExecutorDiscovery,
  useExecutorConfig,
  useExecutorDiscoveredOptions,
} from "../../executor-selector";
import type { ExecutorConfigSource } from "../../executor-selector/model/types";
import {
  useFileReference,
  type RichInputRef,
} from "../../file-reference";
import type { FileEntry } from "../../../services/filePicker";
import {
  fetchSessionExecutionState,
} from "../../../services/session";
import type { SessionExecutionState } from "../../../types";
import { SessionProjectionView } from "./SessionProjectionView";
import { SessionLineageView } from "./SessionLineageView";
import {
  SessionChatComposer,
  SessionChatStatusBar,
  SessionChatStream,
} from "./SessionChatViewParts";
import {
  collectNewSystemEvents,
  computeProjectionRefreshKey,
  resolveExecutorFromHint,
  toExecutorConfigSource,
} from "./SessionChatViewModel";
import type { SessionChatViewProps } from "./SessionChatViewTypes";

// eslint-disable-next-line react-refresh/only-export-components
export { collectNewSystemEvents, computeProjectionRefreshKey } from "./SessionChatViewModel";
export type { PromptTemplate, SessionChatViewProps } from "./SessionChatViewTypes";

// ─── 工具函数 ──────────────────────────────────────────

// ─── 主组件 ────────────────────────────────────────────

const ACTION_RUNNING_RELEASE_DELAY_MS = 300;

export function SessionChatView({
  sessionId,
  workspaceId,
  onMessageSent,
  onTurnEnd,
  onSystemEvent,
  executorHint,
  agentDefaults,
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
  const [showProjectionView, setShowProjectionView] = useState(false);
  const [showLineageView, setShowLineageView] = useState(false);

  const richInputRef = useRef<RichInputRef>(null);
  const appliedHintRef = useRef<string | null>(null);
  const optimisticRunningUntilRef = useRef(0);
  const actionRunningReleaseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const shouldScrollRef = useRef(true);
  const initialValueAppliedRef = useRef(false);
  const cancelInFlightRef = useRef(false);

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
    cancelInFlightRef.current = false;
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

  useEffect(() => {
    setShowProjectionView(false);
    setShowLineageView(false);
  }, [sessionId]);

  // ─── 执行器配置 ──────────────────────────────────────

  const discovery = useExecutorDiscovery();

  // 仅挂载时读一次 agentDefaults，作为 useExecutorConfig 的 initialSource
  const initialExecutorSource = useMemo<ExecutorConfigSource | null>(
    () => toExecutorConfigSource(agentDefaults),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  const execConfig = useExecutorConfig({ initialSource: initialExecutorSource });
  const discovered = useExecutorDiscoveredOptions(execConfig.executor);
  const setExecutor = execConfig.setExecutor;
  const hydrateExecutor = execConfig.hydrate;
  const execProviderId = execConfig.providerId;
  const execModelId = execConfig.modelId;
  const setExecProviderId = execConfig.setProviderId;

  const resolvedHint = useMemo(
    () => resolveExecutorFromHint(executorHint, discovery.executors),
    [discovery.executors, executorHint],
  );

  // 每个 session 仅 hydrate 一次（用户手改后切走再切回不会被再次覆盖）。
  // 首帧 agentDefaults 可能还没到，effect 会等 agentDefaults 就绪后再命中条件。
  const hydratedSessionRef = useRef<string | null>(null);
  useEffect(() => {
    if (!sessionId) return;
    if (hydratedSessionRef.current === sessionId) return;

    const source = toExecutorConfigSource(agentDefaults);
    const hasSource = source && Object.keys(source).length > 0;
    if (hasSource) {
      hydratedSessionRef.current = sessionId;
      hydrateExecutor(source);
      return;
    }
    // 无 agentDefaults 时回退到 hint（保持旧行为）
    if (resolvedHint) {
      const marker = `${sessionId}:${resolvedHint}`;
      if (appliedHintRef.current !== marker) {
        appliedHintRef.current = marker;
        hydratedSessionRef.current = sessionId;
        setExecutor(resolvedHint);
      }
    }
  }, [sessionId, agentDefaults, resolvedHint, hydrateExecutor, setExecutor]);

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
      provider_id: execConfig.providerId.trim() || undefined,
      model_id: execConfig.modelId.trim() || undefined,
      // 将 camelCase 的 thinkingLevel 转为 snake_case 发给后端
      thinking_level: (execConfig.thinkingLevel.trim() as ExecutorConfig["thinking_level"]) || undefined,
      permission_policy: (execConfig.permissionPolicy.trim() as ExecutorConfig["permission_policy"]) || undefined,
    };
  }, [
    execConfig.executor,
    execConfig.providerId,
    execConfig.modelId,
    execConfig.thinkingLevel,
    execConfig.permissionPolicy,
  ]);

  // ─── 会话流 ──────────────────────────────────────────

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
  } = useSessionFeed({ sessionId: streamSessionId, enabled: hasSession });

  const projectionRefreshKey = useMemo(
    () => computeProjectionRefreshKey(rawEvents),
    [rawEvents],
  );

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
      if (!event) continue;
      const bbEvent = event.notification.event;
      const eventType = bbEvent.type === "turn_started" ? "turn_started"
        : bbEvent.type === "turn_completed" ? "turn_completed"
        : bbEvent.type === "platform" ? extractPlatformEventType(bbEvent)
        : null;
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
      onSystemEventRef.current?.(item.eventType, item.event);
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
        throw new Error("Runtime trace 页面不再支持直接发送 prompt；请从 Run、Subject 或 Agent 入口派发执行。");
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
  }, [isSending, sessionId, executorConfig, execConfig, onMessageSent, clearInput, refreshExecutionState]);

  const handleCancel = useCallback(async () => {
    if (!hasSession || !sessionId) return;
    if (cancelInFlightRef.current) return;
    cancelInFlightRef.current = true;
    setSendError(null);
    setIsCancelling(true);
    try {
      await sendCancel();
      // 不 await 状态刷新，避免 UI 卡在"取消中…"；
      // 1.5s 轮询 + 流事件会自然驱动 executionState 更新。
      void refreshExecutionState()
        .then((next) => {
          if (next?.status === "interrupted" || next?.status === "idle") {
            optimisticRunningUntilRef.current = 0;
            setOptimisticRunning(false);
          }
        })
        .catch(() => {});
    } catch (e) {
      setSendError(e instanceof Error ? e.message : "取消失败，请重试。");
    } finally {
      cancelInFlightRef.current = false;
      setIsCancelling(false);
    }
  }, [hasSession, refreshExecutionState, sendCancel, sessionId]);

  const handlePrimaryAction = useCallback(async () => {
    if (cancelInFlightRef.current) return;
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
        <SessionChatStatusBar
          connectionColor={connectionColor}
          connectionLabel={connectionLabel}
          hasSession={hasSession}
          isActionRunning={isActionRunning}
          isConnected={isConnected}
          sessionId={sessionId}
          showLineageView={showLineageView}
          showProjectionView={showProjectionView}
          tokenUsage={tokenUsage}
          onToggleLineage={() => setShowLineageView((value) => !value)}
          onToggleProjection={() => setShowProjectionView((value) => !value)}
        />
      )}

      {showLineageView && sessionId && (
        <SessionLineageView
          sessionId={sessionId}
          refreshKey={projectionRefreshKey}
        />
      )}

      {showProjectionView && sessionId && (
        <SessionProjectionView
          sessionId={sessionId}
          refreshKey={projectionRefreshKey}
          tokenUsage={tokenUsage}
        />
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

      <SessionChatStream
        containerRef={containerRef}
        displayItems={displayItems}
        hasSession={hasSession}
        isLoading={isLoading}
        sessionId={sessionId}
        streamingEntryId={streamingEntryId}
        streamPrefixContent={streamPrefixContent}
        onScroll={handleScroll}
      />

      {/* 输入区 */}
      <SessionChatComposer
        customSend={customSend}
        discovery={discovery}
        discovered={discovered}
        execConfig={execConfig}
        fileRef={fileRef}
        hasSession={hasSession}
        idleSendLabel={idleSendLabel}
        inputPlaceholder={inputPlaceholder}
        inputPrefix={inputPrefix}
        inputValue={inputValue}
        isActionRunning={isActionRunning}
        isCancelling={isCancelling}
        isSending={isSending}
        promptTemplates={promptTemplates}
        richInputRef={richInputRef}
        showExecutorSelector={showExecutorSelector}
        workspaceId={workspaceId}
        onAtTrigger={handleAtTrigger}
        onFileSelected={handleFileSelected}
        onInputChange={setInputValue}
        onKeyDown={handleKeyDown}
        onPrimaryAction={() => { void handlePrimaryAction(); }}
      />
    </div>
  );
}
