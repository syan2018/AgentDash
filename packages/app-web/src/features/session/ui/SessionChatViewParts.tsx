import { useState } from "react";
import type { KeyboardEvent, ReactNode, RefObject } from "react";

import type {
  useExecutorConfig,
  useExecutorDiscoveredOptions,
  useExecutorDiscovery,
} from "../../executor-selector";
import { ExecutorSelector } from "../../executor-selector";
import type { FileEntry } from "../../../services/filePicker";
import {
  FilePickerPopup,
  FileReferenceTags,
  RichInput,
  type RichInputRef,
} from "../../file-reference";
import { isAggregatedGroup, isAggregatedThinkingGroup } from "../model/types";
import type { SessionDisplayItem, TokenUsageInfo } from "../model/types";
import { isSessionComposerSendDisabled } from "./SessionChatComposerState";
import { SessionEntry } from "./SessionEntry";

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
        <span className={`inline-block h-1.5 w-1.5 rounded-full ${connectionColor}`} />
        {connectionLabel}
      </span>
      {isActionRunning && (
        <span className="flex items-center gap-1 rounded-[8px] border border-primary/20 bg-primary/8 px-2.5 py-1 text-xs text-primary">
          <span className="inline-block h-1.5 w-1.5 rounded-full bg-primary" />
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
            <div className="mx-auto h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
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
  customSend,
  discovery,
  discovered,
  execConfig,
  fileRef,
  hasSession,
  idleSendLabel,
  inputPlaceholder,
  inputPrefix,
  inputValue,
  isActionRunning,
  isCancelling,
  isSending,
  promptTemplates,
  richInputRef,
  sendUnavailableReason,
  showExecutorSelector,
  workspaceId,
  onAtTrigger,
  onFileSelected,
  onInputChange,
  onKeyDown,
  onPrimaryAction,
}: {
  customSend?: unknown;
  discovery: ExecutorDiscoveryState;
  discovered: ExecutorDiscoveredState;
  execConfig: ExecutorConfigState;
  fileRef: FileReferenceState;
  hasSession: boolean;
  idleSendLabel: string;
  inputPlaceholder?: string;
  inputPrefix?: ReactNode;
  inputValue: string;
  isActionRunning: boolean;
  isCancelling: boolean;
  isSending: boolean;
  promptTemplates?: Array<{ id: string; label: string; content: string }>;
  richInputRef: RefObject<RichInputRef | null>;
  sendUnavailableReason?: string;
  showExecutorSelector: boolean;
  workspaceId?: string | null;
  onAtTrigger: (query: string) => void;
  onFileSelected: (file: FileEntry) => void;
  onInputChange: (value: string) => void;
  onKeyDown: (event: KeyboardEvent) => void;
  onPrimaryAction: () => void;
}) {
  const canSendPrompt = Boolean(customSend);
  const inputDisabled = isSending || !canSendPrompt;
  const sendDisabled = isSessionComposerSendDisabled({
    hasDispatcher: canSendPrompt,
    hasSession,
    inputValue,
    isActionRunning,
    isCancelling,
    isSending,
  });

  return (
    <div className="shrink-0 border-t border-border bg-background">
      <div className="mx-auto w-full max-w-4xl px-5 py-4">
        {!hasSession && !customSend && promptTemplates && promptTemplates.length > 0 && (
          <div className="mb-3 flex flex-wrap gap-2">
            {promptTemplates.map((tpl) => (
              <button
                key={tpl.id}
                type="button"
                onClick={() => richInputRef.current?.setValue(tpl.content)}
                className="rounded-[8px] border border-border bg-background px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
              >
                {tpl.label}
              </button>
            ))}
          </div>
        )}

        {inputPrefix}

        {showExecutorSelector && (
          <ExecutorSelector
            executors={discovery.executors}
            isLoading={discovery.isLoading}
            error={discovery.error}
            discoveredOptions={discovered.options}
            discoveredError={discovered.error}
            isDiscoveredLoading={
              Boolean(execConfig.executor.trim()) &&
              (!discovered.isInitialized || (discovered.options?.loading_models ?? false))
            }
            onDiscoveredReconnect={discovered.reconnect}
            executor={execConfig.executor}
            providerId={execConfig.providerId}
            modelId={execConfig.modelId}
            thinkingLevel={execConfig.thinkingLevel}
            permissionPolicy={execConfig.permissionPolicy}
            onExecutorChange={execConfig.setExecutor}
            onProviderIdChange={execConfig.setProviderId}
            onModelIdChange={execConfig.setModelId}
            onThinkingLevelChange={execConfig.setThinkingLevel}
            onPermissionPolicyChange={execConfig.setPermissionPolicy}
            onReset={execConfig.reset}
            onRefetch={discovery.refetch}
          />
        )}

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
                placeholder={inputPlaceholder ?? (canSendPrompt
                  ? "继续对话，@ 引用文件，Ctrl+Enter 发送…"
                  : "当前 Session 未连接到 Agent dispatcher")}
                onChange={onInputChange}
                onKeyDown={onKeyDown}
                onAtTrigger={onAtTrigger}
                onFileReferenceRemoved={(relPath) => { fileRef.removeReference(relPath); }}
                disabled={inputDisabled}
              />
            </div>
            <div className="flex flex-col gap-2 self-end">
              <button
                type="button"
                disabled={sendDisabled}
                onClick={onPrimaryAction}
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
          {canSendPrompt
            ? `Ctrl+Enter 快捷发送 · ${workspaceId ? "@ 引用工作空间文件" : "当前会话未绑定工作空间，@ 文件引用不可用"}`
            : sendUnavailableReason ?? "当前 Session 只能查看 runtime trace，请从 Agent 入口打开可继续对话的会话。"}
        </p>
      </div>
    </div>
  );
}
