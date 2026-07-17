/**
 * 可复用的会话聊天视图
 *
 * 包含完整的会话交互能力：流式输出、富文本输入（@ 文件引用）、
 * 执行器选择、上下文用量指示、发送/取消。
 *
 * AgentRun workspace 等 runtime trace 场景复用此组件，
 * 由父组件提供 AgentRun journal target 与外层导航。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSessionFeed } from "../model";
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
  SessionChatComposer,
  SessionChatStatusBar,
  SessionChatStream,
} from "./SessionChatViewParts";
import {
  computeProjectionRefreshKey,
  dispatchLiveSessionEvents,
  isAgentRunWorkspaceActionRunning,
  rawEventsBelongToRuntimeStreamTarget,
  resolveExecutorFromHint,
  toExecutorConfigSource,
} from "./SessionChatViewModel";
import type { SessionChatCommandModel, SessionChatViewProps } from "./SessionChatViewTypes";
import { useImageAttachments } from "./composer/useImageAttachments";
import { SessionStatusBar } from "../../agent-run-workspace/ui";
import { isSessionModelRequirementSatisfied } from "./SessionChatComposerState";
import { SessionWorkspacePanelActionProvider } from "./SessionWorkspacePanelActionProvider";

// ─── 工具函数 ──────────────────────────────────────────

// ─── 主组件 ────────────────────────────────────────────

function isSilentCommandRefreshError(error: unknown): boolean {
  return Boolean(
    error
      && typeof error === "object"
      && (error as { silentCommandRefresh?: unknown }).silentCommandRefresh === true,
  );
}

function deferStateUpdate(update: () => void): void {
  queueMicrotask(update);
}

export function SessionChatView({
  model,
  intents,
  onMessageSent,
  onLiveEvent,
  headerSlot,
  inputPrefix,
  inputToolbarSlot,
  streamPrefixContent,
  showStatusBar = true,
  promptTemplates,
  initialInputValue,
  openWorkspacePanel,
}: SessionChatViewProps) {
  const {
    agentRunTarget,
    workspaceId,
    executorHint,
    agentDefaults,
    executorStateKey,
    showExecutorSelector = true,
    commandState,
    compactContextCommand,
    mailbox,
    statusBarRunId,
    statusBarAgentId,
    injectedInputValue,
    companionSubagents,
  } = model;
  const {
    submitComposer,
    cancelAction,
    setExecutorConfigOverride,
    promoteMailboxMessage,
    deleteMailboxMessage,
    resumeMailbox,
    recallMailboxMessage,
    moveMailboxMessage,
    injectedInputConsumed,
  } = intents;
  const [isSending, setIsSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [inputValue, setInputValue] = useState("");
  const [isCancelling, setIsCancelling] = useState(false);

  const richInputRef = useRef<RichInputRef>(null);
  const appliedHintRef = useRef<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const shouldScrollRef = useRef(true);
  const initialValueAppliedRef = useRef(false);
  const cancelInFlightRef = useRef(false);

  const fileRef = useFileReference(workspaceId);
  const imageAttach = useImageAttachments();

  const clearInput = useCallback(() => {
    richInputRef.current?.setValue("");
    setInputValue("");
    imageAttach.clearAll();
  }, [imageAttach]);

  // 首次挂载时填充初始值
  useEffect(() => {
    if (initialInputValue && !initialValueAppliedRef.current) {
      initialValueAppliedRef.current = true;
      richInputRef.current?.setValue(initialInputValue);
      deferStateUpdate(() => {
        setInputValue(initialInputValue);
      });
    }
  }, [initialInputValue]);

  // 外部注入输入值（recall 消息后填充 composer）
  useEffect(() => {
    if (injectedInputValue != null && injectedInputValue !== "") {
      richInputRef.current?.setValue(injectedInputValue);
      deferStateUpdate(() => {
        setInputValue(injectedInputValue);
        injectedInputConsumed?.();
      });
    }
  }, [injectedInputConsumed, injectedInputValue]);

  const agentRunTargetKey = agentRunTarget
    ? `${agentRunTarget.runId}:${agentRunTarget.agentId}`
    : null;

  // runtime stream target 变更时重置内部状态
  useEffect(() => {
    cancelInFlightRef.current = false;
    deferStateUpdate(() => {
      setSendError(null);
      setIsCancelling(false);
    });
  }, [agentRunTargetKey]);

  // ─── 执行器配置 ──────────────────────────────────────

  const discovery = useExecutorDiscovery();

  // 仅挂载时读一次 agentDefaults，作为 useExecutorConfig 的 initialSource
  const snapshotExecutorDefaults = commandState.modelConfig.effective_executor_config ?? null;
  const initialExecutorSource = useMemo<ExecutorConfigSource | null>(
    () => toExecutorConfigSource(snapshotExecutorDefaults ?? agentDefaults),
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
  const executorHydrationKey = useMemo(() => {
    if (executorStateKey) return executorStateKey;
    const source = toExecutorConfigSource(snapshotExecutorDefaults ?? agentDefaults);
    if (source) {
      return [
        "draft",
        source.executor ?? "",
        source.providerId ?? "",
        source.modelId ?? "",
        source.thinkingLevel ?? "",
      ].join(":");
    }
    return resolvedHint ? `draft:${resolvedHint}` : null;
  }, [agentDefaults, executorStateKey, resolvedHint, snapshotExecutorDefaults]);

  // 每个 session 仅 hydrate 一次（用户手改后切走再切回不会被再次覆盖）。
  // 首帧 agentDefaults 可能还没到，effect 会等 agentDefaults 就绪后再命中条件。
  const hydratedSessionRef = useRef<string | null>(null);
  useEffect(() => {
    if (!executorHydrationKey) return;
    if (hydratedSessionRef.current === executorHydrationKey) return;

    const source = toExecutorConfigSource(snapshotExecutorDefaults ?? agentDefaults);
    const hasSource = source && Object.keys(source).length > 0;
    if (hasSource) {
      hydratedSessionRef.current = executorHydrationKey;
      hydrateExecutor(source);
      return;
    }
    // 无 snapshot/agentDefaults 时回退到 hint。
    if (resolvedHint) {
      const marker = `${executorHydrationKey}:${resolvedHint}`;
      if (appliedHintRef.current !== marker) {
        appliedHintRef.current = marker;
        hydratedSessionRef.current = executorHydrationKey;
        setExecutor(resolvedHint);
      }
    }
  }, [executorHydrationKey, agentDefaults, resolvedHint, hydrateExecutor, setExecutor, snapshotExecutorDefaults]);

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
    };
  }, [
    execConfig.executor,
    execConfig.providerId,
    execConfig.modelId,
    execConfig.thinkingLevel,
  ]);

  const emitExplicitExecutorOverride = useCallback((config: {
    providerId: string;
    modelId: string;
    thinkingLevel: string;
  }) => {
    const executor = execConfig.executor.trim();
    if (!executor) {
      setExecutorConfigOverride?.(null);
      return;
    }
    setExecutorConfigOverride?.({
      executor,
      provider_id: config.providerId.trim() || undefined,
      model_id: config.modelId.trim() || undefined,
      thinking_level: (config.thinkingLevel.trim() as ExecutorConfig["thinking_level"]) || undefined,
    });
  }, [execConfig.executor, setExecutorConfigOverride]);

  // ─── 会话流 ──────────────────────────────────────────

  const hasRuntimeStreamTarget = agentRunTarget != null;

  const {
    displayItems,
    turnSegments,
    rawEntries,
    rawEvents,
    historyReplayBoundarySeq,
    isConnected,
    isLoading,
    error: wsError,
    reconnect,
    streamingEntryId,
    tokenUsage,
  } = useSessionFeed({
    agentRunTarget,
    activeTurnId: commandState.activeTurnId ?? null,
    enabled: hasRuntimeStreamTarget,
  });

  const projectionRefreshKey = useMemo(
    () => computeProjectionRefreshKey(rawEvents),
    [rawEvents],
  );
  const rawEventsBelongToCurrentSession = useMemo(
    () => rawEventsBelongToRuntimeStreamTarget({ rawEvents, agentRunTarget }),
    [agentRunTarget, rawEvents],
  );
  const canApplyLiveEventSideEffects =
    hasRuntimeStreamTarget &&
    rawEventsBelongToCurrentSession &&
    rawEvents.length > 0 &&
    historyReplayBoundarySeq != null;

  // ─── Action running 检测 ──────────────────────────────

  const isActionRunning = isAgentRunWorkspaceActionRunning({
    executionStatus: commandState.executionStatus,
  });

  const onLiveEventRef = useRef(onLiveEvent);
  const lastLiveEventSeqRef = useRef<number | null>(null);
  useEffect(() => { onLiveEventRef.current = onLiveEvent; }, [onLiveEvent]);
  useEffect(() => {
    lastLiveEventSeqRef.current = null;
  }, [agentRunTargetKey]);

  useEffect(() => {
    if (!canApplyLiveEventSideEffects || historyReplayBoundarySeq == null) return;
    lastLiveEventSeqRef.current = dispatchLiveSessionEvents(
      rawEvents,
      lastLiveEventSeqRef.current,
      historyReplayBoundarySeq,
      (event) => onLiveEventRef.current?.(event),
    );
  }, [canApplyLiveEventSideEffects, historyReplayBoundarySeq, rawEvents]);

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

  // ─── 控制动作 ───────────────────────────────────────

  const commandActionRef = useRef(submitComposer);
  useEffect(() => { commandActionRef.current = submitComposer; }, [submitComposer]);

  const handleSubmit = useCallback(async (command: SessionChatCommandModel | undefined, deliveryIntent?: string) => {
    const promptText = richInputRef.current?.getValue() ?? "";
    const trimmed = promptText.trim();
    const images = imageAttach.attachments;

    if (!command) return;
    if (!command.enabled) {
      setSendError(command.unavailable_reason ?? "当前 AgentRun 不可执行该命令。");
      return;
    }
    if (isSending) return;
    if (!isSessionModelRequirementSatisfied(commandState.modelConfig.status, executorConfig)) {
      setSendError(commandState.modelConfig.message ?? "请选择模型配置后再发送。");
      return;
    }
    if (command.requires_input && !trimmed && images.length === 0) {
      setSendError("请输入要发送的消息。");
      return;
    }

    setSendError(null);
    setIsSending(true);

    try {
      await commandActionRef.current({
        command_id: command.command_id,
        prompt: trimmed,
        executorConfig,
        imageAttachments: images.length > 0 ? images : undefined,
        deliveryIntent,
      });

      execConfig.recordUsage();
      clearInput();
      onMessageSent?.();
    } catch (e) {
      if (isSilentCommandRefreshError(e)) {
        setSendError(null);
        return;
      }
      setSendError(e instanceof Error ? e.message : "发送失败，请重试。");
    } finally {
      setIsSending(false);
    }
  }, [
    clearInput,
    commandState.modelConfig.message,
    commandState.modelConfig.status,
    execConfig,
    executorConfig,
    imageAttach.attachments,
    isSending,
    onMessageSent,
  ]);

  const commandById = useCallback((commandId: string | undefined): SessionChatCommandModel | undefined => {
    if (!commandId) return undefined;
    return commandState.commands.find((command) => command.command_id === commandId);
  }, [commandState.commands]);

  const handleCancel = useCallback(async () => {
    const cancelCommand = commandState.cancelCommand;
    if (!cancelCommand?.enabled) return;
    if (cancelInFlightRef.current) return;
    cancelInFlightRef.current = true;
    setSendError(null);
    setIsCancelling(true);
    try {
      if (cancelAction) {
        await cancelAction();
      }
    } catch (e) {
      setSendError(e instanceof Error ? e.message : "取消失败，请重试。");
    } finally {
      cancelInFlightRef.current = false;
      setIsCancelling(false);
    }
  }, [cancelAction, commandState.cancelCommand]);

  // ─── 文件引用 & 键盘 ─────────────────────────────────

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (fileRef.pickerOpen) {
        if (e.key === "ArrowDown") { e.preventDefault(); fileRef.moveSelection(1); return; }
        if (e.key === "ArrowUp") { e.preventDefault(); fileRef.moveSelection(-1); return; }
        if (e.key === "Enter" && !e.ctrlKey && !e.metaKey) { e.preventDefault(); fileRef.confirmSelection(); return; }
        if (e.key === "Escape") { e.preventDefault(); fileRef.closePicker(); return; }
      }

      if (e.key !== "Enter") return;
      if (e.shiftKey) return; // Shift+Enter = 换行

      const isSteer = e.ctrlKey || e.metaKey;
      const keyboardCommandId = isSteer
        ? commandState.keyboard.ctrl_enter
        : commandState.keyboard.enter;
      const command = commandById(keyboardCommandId);
      if (!command) return;

      e.preventDefault();
      void handleSubmit(command, isSteer ? "steer" : undefined);
    },
    [commandById, commandState.keyboard.ctrl_enter, commandState.keyboard.enter, fileRef, handleSubmit],
  );

  // 图片粘贴（Ctrl+V 含图片时拦截）
  const handlePaste = useCallback(
    (e: React.ClipboardEvent) => {
      if (e.clipboardData.files.length > 0) {
        const hasImage = Array.from(e.clipboardData.files).some((f) => f.type.startsWith("image/"));
        if (hasImage) {
          e.preventDefault();
          imageAttach.addFromClipboard(e.clipboardData.items);
        }
      }
    },
    [imageAttach],
  );

  // 图片拖拽
  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      if (e.dataTransfer.files.length > 0) {
        const hasImage = Array.from(e.dataTransfer.files).some((f) => f.type.startsWith("image/"));
        if (hasImage) {
          e.preventDefault();
          imageAttach.addFromDrop(e.dataTransfer);
        }
      }
    },
    [imageAttach],
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
  }, []);

  // 「+」菜单选择文件：图片走附件管线，其他文件走 @ 引用
  const handlePlusMenuFiles = useCallback(
    (files: FileList) => {
      const imageFiles: File[] = [];
      for (let i = 0; i < files.length; i++) {
        const f = files[i];
        if (f && f.type.startsWith("image/")) {
          imageFiles.push(f);
        }
      }
      if (imageFiles.length > 0) {
        imageAttach.addFromFiles(imageFiles);
      }
      // 非图片文件可通过 @ 引用添加（后续扩展）
    },
    [imageAttach],
  );

  const handleAtTrigger = useCallback((query: string) => {
    if (!fileRef.canAddMore) return;
    richInputRef.current?.saveSelection();
    fileRef.openPicker(query);
  }, [fileRef]);

  const handleFileSelected = useCallback((file: FileEntry) => {
    const alreadySelected = fileRef.references.some((r) => r.relPath === file.relPath);
    if (!fileRef.canAddMore && !alreadySelected) { fileRef.closePicker(); return; }
    fileRef.addReference(file);
    if (alreadySelected) return;
    requestAnimationFrame(() => { richInputRef.current?.insertFileReference(file); });
  }, [fileRef]);

  // ─── 派生状态 ────────────────────────────────────────

  const connectionLabel = !hasRuntimeStreamTarget
    ? "待创建"
    : isConnected ? "已连接" : isLoading ? "连接中…" : "未连接";
  const connectionColor = !hasRuntimeStreamTarget
    ? "bg-muted-foreground/40"
    : isConnected ? "bg-success" : isLoading ? "bg-warning animate-pulse" : "bg-destructive";

  const displayError = sendError ?? (hasRuntimeStreamTarget ? wsError?.message : null) ?? null;
  const mailboxMessages = mailbox.messages;

  // ─── 渲染 ────────────────────────────────────────────

  return (
    <SessionWorkspacePanelActionProvider openWorkspacePanel={openWorkspacePanel}>
      <div className="flex h-full flex-col overflow-hidden">
      {/* 内置状态栏 — 可通过 showStatusBar=false 隐藏 */}
      {showStatusBar && (
        <SessionChatStatusBar
          connectionColor={connectionColor}
          connectionLabel={connectionLabel}
        />
      )}

      {/* headerSlot — 外部注入区（如 Task 执行控制栏） */}
      {headerSlot}

      {/* 错误横幅 */}
      {displayError && (
        <div className="shrink-0 border-b border-destructive/40 bg-destructive/10 px-5 py-2 text-sm text-destructive">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <div className="font-medium">发送失败</div>
              <div className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-words text-xs leading-relaxed text-destructive/90">
                {displayError}
              </div>
            </div>
            {wsError && !isConnected && hasRuntimeStreamTarget && (
              <button type="button" onClick={reconnect} className="shrink-0 rounded-md bg-destructive/20 px-2 py-0.5 text-xs hover:bg-destructive/30">
                重新连接
              </button>
            )}
          </div>
        </div>
      )}

      <SessionChatStream
        containerRef={containerRef}
        displayItems={displayItems}
        turnSegments={turnSegments}
        agentRunTarget={agentRunTarget}
        companionSubagents={companionSubagents}
        hasRuntimeStreamTarget={hasRuntimeStreamTarget}
        isLoading={isLoading}
        streamingEntryId={streamingEntryId}
        streamPrefixContent={streamPrefixContent}
        onForkFromMessageRef={intents.forkFromMessageRef}
        onScroll={handleScroll}
      />

      {/* Mailbox 消息 + 输入区 */}
      <div onPaste={handlePaste} onDrop={handleDrop} onDragOver={handleDragOver}>
        <SessionStatusBar
          runId={statusBarRunId}
          agentId={statusBarAgentId}
          messages={mailboxMessages}
          mailbox={mailbox}
          onPromote={promoteMailboxMessage ?? (() => {})}
          onDelete={deleteMailboxMessage ?? (() => {})}
          onResume={resumeMailbox}
          onRecall={recallMailboxMessage}
          onMove={moveMailboxMessage}
        />

        <SessionChatComposer
          commandState={commandState}
          discovery={discovery}
          discovered={discovered}
          execConfig={execConfig}
          fileRef={fileRef}
          hasRuntimeStreamTarget={hasRuntimeStreamTarget}
          inputPrefix={inputPrefix}
          toolbarSlot={inputToolbarSlot}
          inputValue={inputValue}
          imageAttachments={imageAttach.attachments}
          imageError={imageAttach.error}
          isActionRunning={isActionRunning}
          isCancelling={isCancelling}
          isSending={isSending}
          promptTemplates={promptTemplates}
          richInputRef={richInputRef}
          showExecutorSelector={showExecutorSelector}
          workspaceId={workspaceId}
          tokenUsage={tokenUsage}
          agentRunTarget={agentRunTarget}
          projectionRefreshKey={projectionRefreshKey}
          compactContextCommand={compactContextCommand}
          onAtTrigger={handleAtTrigger}
          onFileSelected={handleFileSelected}
          onInputChange={setInputValue}
          onKeyDown={handleKeyDown}
          onCancelAction={() => { void handleCancel(); }}
          onCommandAction={(command) => { void handleSubmit(command); }}
          onExecutorConfigExplicitChange={emitExplicitExecutorOverride}
          onPlusMenuFiles={handlePlusMenuFiles}
          onRemoveImage={imageAttach.removeAttachment}
        />
      </div>
      </div>
    </SessionWorkspacePanelActionProvider>
  );
}
