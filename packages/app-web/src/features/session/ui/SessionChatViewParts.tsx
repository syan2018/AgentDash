import { useState } from "react";
import type { KeyboardEvent, ReactNode, RefObject } from "react";

import type {
  useExecutorConfig,
  useExecutorDiscoveredOptions,
  useExecutorDiscovery,
} from "../../executor-selector";
import { InlineModelSelector } from "../../executor-selector";
import type { FileEntry } from "../../../services/filePicker";
import {
  FilePickerPopup,
  FileReferenceTags,
  RichInput,
  type RichInputRef,
} from "../../file-reference";
import { isAggregatedGroup, isAggregatedThinkingGroup } from "../model/types";
import type { SessionDisplayItem, TokenUsageInfo } from "../model/types";
import { isSessionComposerSubmitDisabled } from "./SessionChatComposerState";
import { SessionEntry } from "./SessionEntry";
import type { SessionChatCommandState } from "./SessionChatViewTypes";
import type { ConversationCommandView } from "../../../generated/workflow-contracts";
import type { ImageAttachment } from "./composer/useImageAttachments";
import { ImageAttachmentPreview } from "./composer/ImageAttachmentPreview";
import { ComposerSendButton } from "./composer/ComposerSendButton";
import { ComposerPlusMenu } from "./composer/ComposerPlusMenu";

type ExecutorDiscoveryState = ReturnType<typeof useExecutorDiscovery>;
type ExecutorConfigState = ReturnType<typeof useExecutorConfig>;
type ExecutorDiscoveredState = ReturnType<typeof useExecutorDiscoveredOptions>;

interface FileReferenceState {
  references: Array<{ relPath: string; size: number }>;
  pickerOpen: boolean;
  pickerQuery: string;
  pickerFiles: FileEntry[];
  pickerLoading: boolean;
  pickerError: string | null;
  selectedIndex: number;
  closePicker: () => void;
  updateQuery: (query: string) => void;
  removeReference: (relPath: string) => void;
  moveSelection: (delta: number) => void;
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

function getItemKey(item: SessionDisplayItem): string {
  if (isAggregatedGroup(item)) return item.groupKey;
  if (isAggregatedThinkingGroup(item)) return item.groupKey;
  return item.id;
}

function ContextUsageRing({ usage }: { usage: TokenUsageInfo | null }) {
  const [showDetail, setShowDetail] = useState(false);
  if (!usage) return null;

  const {
    currentContextTokens,
    effectiveContextWindow,
    modelContextWindow,
    pendingEstimateTokens,
    total,
    last,
  } = usage;
  const maxTokens = effectiveContextWindow ?? modelContextWindow;
  const hasAny = currentContextTokens > 0 || total.totalTokens > 0 || last.totalTokens > 0;
  if (!hasAny) return null;

  const percent = maxTokens
    ? Math.min(Math.round((currentContextTokens / maxTokens) * 100), 100)
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
          {maxTokens != null && (
            <span className="text-muted-foreground"> ({formatTokens(currentContextTokens)}/{formatTokens(maxTokens)})</span>
          )}
          {(last.inputTokens > 0 || last.outputTokens > 0 || pendingEstimateTokens > 0) && (
            <span className="text-muted-foreground">
              {percent != null ? " · " : ""}
              {last.inputTokens > 0 && `↑${formatTokens(last.inputTokens)}`}
              {last.inputTokens > 0 && last.outputTokens > 0 && " "}
              {last.outputTokens > 0 && `↓${formatTokens(last.outputTokens)}`}
              {pendingEstimateTokens > 0 && ` +${formatTokens(pendingEstimateTokens)}估算`}
            </span>
          )}
        </span>
      )}
    </span>
  );
}

export function SessionChatStatusBar({
  connectionColor,
  connectionLabel,
  hasSession,
  isActionRunning,
  isConnected,
  sessionId,
  showLineageView,
  showProjectionView,
  tokenUsage,
  onToggleLineage,
  onToggleProjection,
}: {
  connectionColor: string;
  connectionLabel: string;
  hasSession: boolean;
  isActionRunning: boolean;
  isConnected: boolean;
  sessionId: string | null;
  showLineageView: boolean;
  showProjectionView: boolean;
  tokenUsage: TokenUsageInfo | null;
  onToggleLineage: () => void;
  onToggleProjection: () => void;
}) {
  return (
    <div className="flex shrink-0 items-center gap-2.5 border-b border-border bg-background px-5 py-2">
      <span className="flex items-center gap-1.5 rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground">
        <span className={`inline-block h-1.5 w-1.5 rounded-[8px] ${connectionColor}`} />
        {connectionLabel}
      </span>
      {isActionRunning && (
        <span className="flex items-center gap-1 rounded-[8px] border border-primary/20 bg-primary/8 px-2.5 py-1 text-xs text-primary">
          <span className="inline-block h-1.5 w-1.5 rounded-[8px] bg-primary" />
          {isConnected ? "接收中" : "执行中"}
        </span>
      )}
      <ContextUsageRing usage={tokenUsage} />
      {hasSession && sessionId && (
        <>
          <button
            type="button"
            onClick={onToggleLineage}
            className={`rounded-[8px] border px-2.5 py-1 text-xs transition-colors ${
              showLineageView
                ? "border-primary/30 bg-primary/10 text-primary"
                : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground"
            }`}
          >
            分支
          </button>
          <button
            type="button"
            onClick={onToggleProjection}
            className={`rounded-[8px] border px-2.5 py-1 text-xs transition-colors ${
              showProjectionView
                ? "border-primary/30 bg-primary/10 text-primary"
                : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground"
            }`}
          >
            上下文
          </button>
        </>
      )}
    </div>
  );
}

export function SessionChatStream({
  containerRef,
  displayItems,
  hasSession,
  isLoading,
  sessionId,
  streamingEntryId,
  streamPrefixContent,
  onScroll,
}: {
  containerRef: RefObject<HTMLDivElement | null>;
  displayItems: SessionDisplayItem[];
  hasSession: boolean;
  isLoading: boolean;
  sessionId: string | null;
  streamingEntryId: string | null;
  streamPrefixContent?: ReactNode;
  onScroll: () => void;
}) {
  return (
    <div ref={containerRef} onScroll={onScroll} className="flex-1 overflow-y-auto">
      {hasSession && isLoading && displayItems.length === 0 && !streamPrefixContent ? (
        <div className="flex h-full items-center justify-center">
          <div className="text-center">
            <div className="mx-auto h-8 w-8 animate-spin rounded-[12px] border-2 border-primary border-t-transparent" />
            <p className="mt-2 text-sm text-muted-foreground">正在连接…</p>
          </div>
        </div>
      ) : (hasSession && displayItems.length > 0) || streamPrefixContent ? (
        <div className="mx-auto w-full max-w-4xl space-y-3 px-5 py-6">
          {streamPrefixContent}
          {displayItems.map((item) => {
            const key = getItemKey(item);
            return (
              <div key={key}>
                <SessionEntry
                  item={item}
                  isStreaming={key === streamingEntryId}
                  sessionId={sessionId}
                />
              </div>
            );
          })}
        </div>
      ) : (
        <div className="flex h-full items-center justify-center">
          <div className="text-center">
            <div className="mx-auto mb-4 w-fit rounded-[8px] border border-dashed border-border bg-secondary px-3 py-1 text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
              Session
            </div>
            <p className="text-sm text-muted-foreground">
              {hasSession ? "会话已就绪，继续发送消息" : "输入 prompt 并发送开始会话"}
            </p>
          </div>
        </div>
      )}
    </div>
  );
}

export function SessionChatComposer({
  commandState,
  discovery,
  discovered,
  execConfig,
  fileRef,
  hasSession,
  inputPrefix,
  inputValue,
  imageAttachments,
  imageError,
  isActionRunning,
  isCancelling,
  isSending,
  promptTemplates,
  richInputRef,
  showExecutorSelector,
  workspaceId,
  onAtTrigger,
  onFileSelected,
  onInputChange,
  onKeyDown,
  onCancelAction,
  onCommandAction,
  onPlusMenuFiles,
  onRemoveImage,
}: {
  commandState: SessionChatCommandState;
  discovery: ExecutorDiscoveryState;
  discovered: ExecutorDiscoveredState;
  execConfig: ExecutorConfigState;
  fileRef: FileReferenceState;
  hasSession: boolean;
  inputPrefix?: ReactNode;
  inputValue: string;
  imageAttachments: ImageAttachment[];
  imageError: string | null;
  isActionRunning: boolean;
  isCancelling: boolean;
  isSending: boolean;
  promptTemplates?: Array<{ id: string; label: string; content: string }>;
  richInputRef: RefObject<RichInputRef | null>;
  showExecutorSelector: boolean;
  workspaceId?: string | null;
  onAtTrigger: (query: string) => void;
  onFileSelected: (file: FileEntry) => void;
  onInputChange: (value: string) => void;
  onKeyDown: (event: KeyboardEvent) => void;
  onCancelAction: () => void;
  onCommandAction: (command: ConversationCommandView) => void;
  onPlusMenuFiles: (files: FileList) => void;
  onRemoveImage: (id: string) => void;
}) {
  const enterCommandId = commandState.commands.keyboard.enter;
  const alternateCommandId = commandState.commands.keyboard.ctrl_enter !== enterCommandId
    ? commandState.commands.keyboard.ctrl_enter
    : undefined;
  const submitCommand = commandState.commands.commands.find(
    (command) => command.command_id === enterCommandId,
  ) ?? commandState.commands.commands.find(
    (command) => command.placement.includes("composer_primary") && command.enabled,
  ) ?? commandState.commands.commands.find(
    (command) => command.placement.includes("composer_primary"),
  );
  const alternateCommand = commandState.commands.commands.find(
    (command) => command.command_id === alternateCommandId,
  ) ?? commandState.commands.commands.find(
    (command) => command.placement.includes("composer_secondary") && command.enabled,
  );
  const cancelCommand = commandState.commands.commands.find((command) => command.kind === "cancel");
  const inputDisabled = isSending || !submitCommand?.enabled;

  const hasContent = Boolean(inputValue.trim()) || imageAttachments.length > 0;
  // 展开条件：有效多行（trim 后仍含换行） OR 有附件 OR 有文件引用
  const isExpanded = inputValue.trim().includes("\n") || imageAttachments.length > 0 || fileRef.references.length > 0;
  const sendDisabled = isSessionComposerSubmitDisabled({
    commandEnabled: Boolean(submitCommand?.enabled),
    requirePromptText: false,
    inputValue: hasContent ? "has_content" : "",
    isCancelling,
    isSending,
  });
  const cancelDisabled = isCancelling || !cancelCommand?.enabled;

  const executorName = discovery.executors.find((e) => e.id === execConfig.executor)?.name;
  const isDiscoveredLoading = Boolean(execConfig.executor.trim()) &&
    (!discovered.isInitialized || (discovered.options?.loading_models ?? false));

  const modelRequired = commandState.modelConfig.status === "model_required";
  const isModelReadonly = Boolean(
    submitCommand && submitCommand.executor_config_policy === "forbidden",
  );

  const actionReason = submitCommand?.enabled
    ? undefined
    : submitCommand?.unavailable_reason
      ?? commandState.modelConfig.message
      ?? commandState.helperText;
  const helperText = submitCommand?.enabled
    ? commandState.helperText ?? `Enter 提交 · ${workspaceId ? "@ 引用文件" : "@ 文件引用不可用"}`
    : actionReason ?? "当前 AgentRun 只能查看。";

  return (
    <div className="shrink-0 pb-4 pt-2">
      <div className="mx-auto w-full max-w-4xl px-5">
        {/* Prompt 模板（无 session + draft 模式） */}
        {!hasSession && !submitCommand?.enabled && promptTemplates && promptTemplates.length > 0 && (
          <div className="mb-3 flex flex-wrap gap-2">
            {promptTemplates.map((tpl) => (
              <button
                key={tpl.id}
                type="button"
                onClick={() => richInputRef.current?.setValue(tpl.content)}
                className="rounded-[8px] px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              >
                {tpl.label}
              </button>
            ))}
          </div>
        )}

        {inputPrefix}

        {/*
         * Composer — flex-wrap + order 单实例布局
         *
         * 非展开: [+] [input(flex-1)] [model] [send] — 单行，items-center 居中
         * 展开:   [图片+引用+input(w-full)] 换行 [+][gap][model][send]
         *
         * 展开条件: 文本换行 || 有附件 || 有文件引用
         */}
        <div className="relative flex flex-wrap items-center gap-x-1 rounded-[12px] bg-muted/40 px-2 py-1.5 transition-colors focus-within:bg-muted/60">
          {/* ① RichInput 区域 — 唯一实例，展开时 w-full 独占行 */}
          <div className={
            isExpanded
              ? "w-full px-2 pb-1 pt-1.5"
              : "order-2 min-w-0 flex-1 px-1"
          }>
            {/* 图片预览 — 在输入框上方 */}
            <ImageAttachmentPreview
              attachments={imageAttachments}
              onRemove={onRemoveImage}
            />

            {imageError && (
              <div className="mb-1 rounded-[8px] bg-destructive/10 px-2.5 py-1.5 text-xs text-destructive">
                {imageError}
              </div>
            )}

            {/* 文件引用药丸 — 有引用就显示 */}
            <FileReferenceTags
              references={fileRef.references}
              onRemove={(relPath) => {
                fileRef.removeReference(relPath);
                const cur = richInputRef.current?.getValue() ?? "";
                const next = removeReferenceMarkers(cur, relPath);
                richInputRef.current?.setValue(next);
              }}
            />

            {/* 文本输入 + @ 文件选择弹窗 */}
            <div className="relative">
              <FilePickerPopup
                open={fileRef.pickerOpen}
                query={fileRef.pickerQuery}
                files={fileRef.pickerFiles}
                loading={fileRef.pickerLoading}
                error={fileRef.pickerError}
                selectedIndex={fileRef.selectedIndex}
                onQueryChange={fileRef.updateQuery}
                onSelect={onFileSelected}
                onClose={fileRef.closePicker}
                onMoveSelection={fileRef.moveSelection}
                onConfirmSelection={() => {
                  const selectedFile = fileRef.pickerFiles[fileRef.selectedIndex];
                  if (!selectedFile) return;
                  onFileSelected(selectedFile);
                }}
              />
              <RichInput
                ref={richInputRef}
                placeholder={modelRequired ? "请选择模型后继续" : "Send follow-up"}
                onChange={onInputChange}
                onKeyDown={onKeyDown}
                onAtTrigger={onAtTrigger}
                onFileReferenceRemoved={(relPath) => { fileRef.removeReference(relPath); }}
                disabled={inputDisabled}
              />
            </div>
          </div>

          {/* ② + 菜单 */}
          <div className={isExpanded ? "order-2" : "order-1"}>
            <ComposerPlusMenu
              disabled={inputDisabled}
              onSelectFiles={onPlusMenuFiles}
            />
          </div>

          {/* ③ 弹性间隔（展开时推右） */}
          {isExpanded && <div className="order-3 flex-1" />}

          {/* ④ 模型选择器 */}
          {showExecutorSelector && (
            <div className={isExpanded ? "order-4" : "order-3"}>
              <InlineModelSelector
                execConfig={execConfig}
                discoveredOptions={discovered.options}
                isDiscoveredLoading={isDiscoveredLoading}
                executorName={executorName}
                readonly={isModelReadonly}
                status={commandState.modelConfig.status}
                message={commandState.modelConfig.message}
                onRefresh={() => {
                  discovery.refetch();
                  discovered.reconnect();
                }}
              />
            </div>
          )}

          {/* ⑤ 发送/状态按钮 — 常驻 */}
          <div className={isExpanded ? "order-5" : "order-4"}>
            <ComposerSendButton
              isRunning={isActionRunning}
              hasInput={hasContent}
              isSending={isSending}
              isCancelling={isCancelling}
              cancelDisabled={cancelDisabled}
              submitCommand={sendDisabled ? undefined : submitCommand}
              alternateCommand={alternateCommand}
              onSubmit={onCommandAction}
              onCancel={onCancelAction}
            />
          </div>
        </div>

        <p className="mt-1.5 px-1 text-[11px] text-muted-foreground/40">
          {helperText}
        </p>
      </div>
    </div>
  );
}
