import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode, RefObject } from "react";

import { SessionProjectionView } from "./SessionProjectionView";

import type { SessionMessageRefDto } from "../../../generated/agent-run-mailbox-contracts";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
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
import { isAggregatedGroup, isAggregatedThinkingGroup, isDisplayEntry } from "../model/types";
import type { SessionDisplayItem, SessionDisplayEntry, TokenUsageInfo } from "../model/types";
import { buildRoundActionModel, type RoundActionModel } from "../model/roundActions";
import type { TurnActivityStatus, TurnSegment } from "../model/useSessionFeed";
import { isSessionComposerSubmitDisabled } from "./SessionChatComposerState";
import { SessionEntry } from "./SessionEntry";
import type { SessionChatCommandModel, SessionChatCommandState } from "./SessionChatViewTypes";
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

/**
 * 上下文用量入口
 *
 * 输入框工具栏里的小圆环进度条，是查看上下文用量的唯一入口：
 * - hover 出轻量摘要（百分比 / 当前·上限 / 最近输入输出 / 估算）
 * - 点击向上展开锚定圆环的浮层，渲染完整明细（构成 / 消息明细 / Top Tools / segments）
 *
 * 只要存在 AgentRun target 或 raw session trace 就渲染（即便用量数据尚未到达，也能点开看投影明细），
 * 因此入口在 GUI 上始终可见、可发现。
 */
function ContextUsageRing({
  usage,
  sessionId,
  agentRunTarget,
  refreshKey,
}: {
  usage: TokenUsageInfo | null;
  sessionId: string | null;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  refreshKey: number;
}) {
  const [hover, setHover] = useState(false);
  const [open, setOpen] = useState(false);
  const anchorRef = useRef<HTMLDivElement>(null);

  // 点击外部 / Esc 关闭浮层
  useEffect(() => {
    if (!open) return;
    function onPointer(e: MouseEvent) {
      if (anchorRef.current && !anchorRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    function onKey(e: globalThis.KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  // 没有上下文投影目标时不渲染入口
  if (!agentRunTarget && !sessionId) return null;

  const maxTokens = usage ? usage.effectiveContextWindow ?? usage.modelContextWindow : undefined;
  const currentContextTokens = usage?.currentContextTokens ?? 0;
  const pendingEstimateTokens = usage?.pendingEstimateTokens ?? 0;
  const last = usage?.last;
  const percent = usage && maxTokens
    ? Math.min(Math.round((currentContextTokens / maxTokens) * 100), 100)
    : undefined;
  const hasLastFlow = Boolean(
    last && (last.inputTokens > 0 || last.outputTokens > 0 || pendingEstimateTokens > 0),
  );

  const radius = 7;
  const circumference = 2 * Math.PI * radius;
  const strokeDash = percent != null ? (percent / 100) * circumference : 0;
  const isHigh = percent != null && percent > 80;

  return (
    <div ref={anchorRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => setHover(false)}
        title="上下文用量"
        className={`flex items-center gap-1.5 rounded-[8px] px-2 py-1.5 text-xs transition-colors ${
          open
            ? "bg-secondary text-foreground"
            : "text-muted-foreground hover:bg-secondary hover:text-foreground"
        }`}
      >
        <svg width="16" height="16" className="shrink-0 -rotate-90">
          <circle cx="8" cy="8" r={radius} fill="none" stroke="currentColor" strokeWidth="2" className="text-muted/40" />
          {percent != null && (
            <circle
              cx="8" cy="8" r={radius}
              fill="none" strokeWidth="2" strokeLinecap="round"
              strokeDasharray={`${strokeDash} ${circumference}`}
              className={isHigh ? "text-warning" : "text-primary/70"}
              stroke="currentColor"
            />
          )}
        </svg>
        {percent != null && (
          <span className="tabular-nums font-medium">{percent}%</span>
        )}
      </button>

      {/* hover 摘要 — 仅在浮层未展开时显示，向上弹出 */}
      {hover && !open && (
        <span className="absolute bottom-full left-1/2 z-50 mb-1.5 -translate-x-1/2 whitespace-nowrap rounded-md border border-border bg-popover px-2.5 py-1.5 text-xs text-popover-foreground shadow-md">
          {percent != null ? (
            <>
              <span className="font-medium">{percent}% 上下文</span>
              {maxTokens != null && (
                <span className="text-muted-foreground"> ({formatTokens(currentContextTokens)}/{formatTokens(maxTokens)})</span>
              )}
              {hasLastFlow && last && (
                <span className="text-muted-foreground">
                  {" · "}
                  {last.inputTokens > 0 && `↑${formatTokens(last.inputTokens)}`}
                  {last.inputTokens > 0 && last.outputTokens > 0 && " "}
                  {last.outputTokens > 0 && `↓${formatTokens(last.outputTokens)}`}
                  {pendingEstimateTokens > 0 && ` +${formatTokens(pendingEstimateTokens)}估算`}
                </span>
              )}
            </>
          ) : (
            <span className="text-muted-foreground">查看上下文用量明细</span>
          )}
          <span className="mt-0.5 block text-[10px] text-muted-foreground/60">点击查看完整明细</span>
        </span>
      )}

      {/* 点击浮层 — 完整明细，向上弹出、右对齐避免越界 */}
      {open && (
        <div className="absolute bottom-full right-0 z-50 mb-1.5 w-[min(680px,calc(100vw-2rem))]">
          <SessionProjectionView
            sessionId={sessionId}
            agentRunTarget={agentRunTarget}
            refreshKey={refreshKey}
            tokenUsage={usage}
            embedded
          />
        </div>
      )}
    </div>
  );
}

export function SessionChatStatusBar({
  connectionColor,
  connectionLabel,
}: {
  connectionColor: string;
  connectionLabel: string;
}) {
  return (
    <div className="flex shrink-0 items-center gap-2.5 border-b border-border bg-background px-5 py-2">
      <span className="flex items-center gap-1.5 rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground">
        <span className={`inline-block h-1.5 w-1.5 rounded-[8px] ${connectionColor}`} />
        {connectionLabel}
      </span>
    </div>
  );
}

export function SessionChatStream({
  containerRef,
  displayItems,
  turnSegments,
  agentRunTarget,
  hasRuntimeStreamTarget,
  isLoading,
  sessionId,
  streamingEntryId,
  streamPrefixContent,
  onForkFromMessageRef,
  onScroll,
}: {
  containerRef: RefObject<HTMLDivElement | null>;
  displayItems: SessionDisplayItem[];
  turnSegments?: TurnSegment[];
  agentRunTarget?: AgentRunRuntimeTarget | null;
  hasRuntimeStreamTarget: boolean;
  isLoading: boolean;
  sessionId: string | null;
  streamingEntryId: string | null;
  streamPrefixContent?: ReactNode;
  onForkFromMessageRef?: (forkPointRef: SessionMessageRefDto) => Promise<void>;
  onScroll: () => void;
}) {
  return (
    <div ref={containerRef} onScroll={onScroll} className="flex-1 overflow-y-auto">
      {hasRuntimeStreamTarget && isLoading && displayItems.length === 0 && !streamPrefixContent ? (
        <div className="flex h-full items-center justify-center">
          <div className="text-center">
            <div className="mx-auto h-8 w-8 animate-spin rounded-[12px] border-2 border-primary border-t-transparent" />
            <p className="mt-2 text-sm text-muted-foreground">正在连接…</p>
          </div>
        </div>
      ) : (hasRuntimeStreamTarget && displayItems.length > 0) || streamPrefixContent ? (
        <div className="mx-auto w-full max-w-4xl space-y-1.5 px-5 py-6">
          {streamPrefixContent}
          {turnSegments && turnSegments.length > 0 ? (
            turnSegments.map((segment, idx) => (
              <TurnSection
                key={segment.turnId ?? `gap-${idx}`}
                segment={segment}
                agentRunTarget={agentRunTarget}
                sessionId={sessionId}
                streamingEntryId={streamingEntryId}
                onForkFromMessageRef={onForkFromMessageRef}
              />
            ))
          ) : (
            displayItems.map((item, idx) => {
              const key = getItemKey(item);
              const followed = isToolGroup(item) && hasFollowingAgentMessage(displayItems, idx);
              return (
                <div key={key}>
                  <SessionEntry
                    item={item}
                    agentRunTarget={agentRunTarget}
                    isStreaming={key === streamingEntryId}
                    sessionId={sessionId}
                    followedByMessage={followed}
                  />
                </div>
              );
            })
          )}
        </div>
      ) : (
        <div className="flex h-full items-center justify-center">
          <div className="text-center">
            <div className="mx-auto mb-4 w-fit rounded-[8px] border border-dashed border-border bg-secondary px-3 py-1 text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
              {agentRunTarget ? "Workspace" : "Session"}
            </div>
            <p className="text-sm text-muted-foreground">
              {hasRuntimeStreamTarget ? "会话已就绪，继续发送消息" : "对话尚未开始，仍可发送可用命令"}
            </p>
          </div>
        </div>
      )}
    </div>
  );
}

/** 判断 displayItem 是否是 agent 文本消息 */
function isAgentMessage(item: SessionDisplayItem): boolean {
  if (!isDisplayEntry(item)) return false;
  return (item as SessionDisplayEntry).event.type === "agent_message_delta";
}

/** 判断当前 item 是否是 aggregated tool group */
function isToolGroup(item: SessionDisplayItem): boolean {
  return isAggregatedGroup(item);
}

/** 列表中某 tool group 后面是否紧跟 agent message */
function hasFollowingAgentMessage(items: SessionDisplayItem[], idx: number): boolean {
  for (let i = idx + 1; i < items.length; i++) {
    const next = items[i]!;
    if (isAgentMessage(next)) return true;
    if (isToolGroup(next)) continue;
    if (isAggregatedThinkingGroup(next)) continue;
    break;
  }
  return false;
}

function formatTurnDuration(ms: number): string {
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}m ${s}s`;
}

function formatTurnDurationSuffix(ms: number | undefined): string {
  if (ms == null || !Number.isFinite(ms) || ms < 0) return "";
  return ` ${formatTurnDuration(ms)}`;
}

function terminalTurnLabel(status: TurnSegment["status"]): string | null {
  switch (status) {
    case "completed":
      return "已处理";
    case "failed":
      return "执行失败";
    case "interrupted":
      return "执行已中断";
    default:
      return null;
  }
}

function turnActivityClassName(activity: TurnActivityStatus): string {
  switch (activity.kind) {
    case "retry_exhausted":
      return "border-destructive/20 bg-destructive/8 text-destructive";
    case "reconnecting":
      return "border-warning/25 bg-warning/10 text-warning";
    case "connecting":
    default:
      return "border-info/20 bg-info/8 text-info";
  }
}

function TurnActivityStrip({ activity }: { activity: TurnActivityStatus }) {
  return (
    <div className={`flex w-fit items-center gap-1.5 rounded-[8px] border px-2.5 py-1 text-xs ${turnActivityClassName(activity)}`}>
      <span className="inline-block h-1.5 w-1.5 rounded-[8px] bg-current" />
      <span>{activity.label}</span>
    </div>
  );
}

function useActiveTurnElapsedMs(startedAtMs: number | undefined, active: boolean): number | undefined {
  const [clock, setClock] = useState(() => Date.now());

  useEffect(() => {
    if (!active || startedAtMs == null) return;
    const timer = window.setInterval(() => setClock(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [active, startedAtMs]);

  if (!active || startedAtMs == null) return undefined;
  return Math.max(clock - startedAtMs, 0);
}

function TurnSection({
  segment,
  agentRunTarget,
  sessionId,
  streamingEntryId,
  onForkFromMessageRef,
}: {
  segment: TurnSegment;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  sessionId: string | null;
  streamingEntryId: string | null;
  onForkFromMessageRef?: (forkPointRef: SessionMessageRefDto) => Promise<void>;
}) {
  const isTerminal = segment.status !== "active";
  const terminalLabel = terminalTurnLabel(segment.status);
  const headerLabel = terminalLabel ?? (segment.turnId ? "执行中" : null);
  const activeElapsedMs = useActiveTurnElapsedMs(segment.startedAtMs, segment.status === "active");
  const displayDurationMs = segment.durationMs ?? activeElapsedMs;
  const [collapsed, setCollapsed] = useState(false);
  const [prevStatus, setPrevStatus] = useState(segment.status);

  if (segment.status !== prevStatus) {
    setPrevStatus(segment.status);
    if (isTerminal && prevStatus === "active") {
      setCollapsed(true);
    }
  }

  if (!collapsed) {
    return (
      <div className="space-y-1.5">
        {segment.activity && (
          <TurnActivityStrip activity={segment.activity} />
        )}
        {headerLabel && (
          <button
            type="button"
            onClick={() => setCollapsed(true)}
            className="flex items-center gap-2 rounded-[6px] px-2 py-0.5 text-[11px] text-muted-foreground/40 transition-colors hover:text-muted-foreground/60 hover:bg-secondary/30"
          >
            <span className="h-px flex-1 max-w-6 bg-border/40" />
            <span>{headerLabel}{formatTurnDurationSuffix(displayDurationMs)}</span>
            <span className="h-px flex-1 bg-border/40" />
          </button>
        )}
        {segment.items.map((item, idx) => {
          const key = getItemKey(item);
          const followed = isToolGroup(item) && hasFollowingAgentMessage(segment.items, idx);
          return (
            <div key={key}>
              <SessionEntry
                item={item}
                agentRunTarget={agentRunTarget}
                isStreaming={key === streamingEntryId}
                sessionId={sessionId}
                followedByMessage={followed}
              />
            </div>
          );
        })}
        <RoundActionToolbar
          actionModel={buildRoundActionModel(segment)}
          onForkFromMessageRef={onForkFromMessageRef}
        />
      </div>
    );
  }

  // 折叠态：只显示 summary bar + 最终输出
  return (
    <div className="space-y-1.5">
      <button
        type="button"
        onClick={() => setCollapsed(false)}
        className="flex items-center gap-2 rounded-[6px] px-2 py-0.5 text-[11px] text-muted-foreground/50 transition-colors hover:text-muted-foreground/70 hover:bg-secondary/30"
      >
        <span className="text-muted-foreground/40">▶</span>
        <span>{headerLabel ?? "会话段落"}{formatTurnDurationSuffix(displayDurationMs)}</span>
        <span className="h-px flex-1 bg-border/40" />
      </button>
      {segment.finalOutput && (
        <SessionEntry
          item={segment.finalOutput}
          agentRunTarget={agentRunTarget}
          isStreaming={getItemKey(segment.finalOutput) === streamingEntryId}
          sessionId={sessionId}
        />
      )}
      <RoundActionToolbar
        actionModel={buildRoundActionModel(segment)}
        onForkFromMessageRef={onForkFromMessageRef}
      />
    </div>
  );
}

function CopyIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
    </svg>
  );
}

function ForkIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="6" cy="6" r="2" />
      <circle cx="18" cy="6" r="2" />
      <circle cx="12" cy="18" r="2" />
      <path d="M6 8v2a4 4 0 0 0 4 4h2" />
      <path d="M18 8v2a4 4 0 0 1-4 4h-2" />
      <path d="M12 14v2" />
    </svg>
  );
}

function RoundActionToolbar({
  actionModel,
  onForkFromMessageRef,
}: {
  actionModel: RoundActionModel;
  onForkFromMessageRef?: (forkPointRef: SessionMessageRefDto) => Promise<void>;
}) {
  const [copied, setCopied] = useState(false);
  const [forking, setForking] = useState(false);
  const forkPointRef = actionModel.forkFromHere.forkPointRef;
  const canFork = Boolean(actionModel.forkFromHere.enabled && forkPointRef && onForkFromMessageRef);
  const forkDisabledReason = onForkFromMessageRef
    ? actionModel.forkFromHere.disabledReason
    : "当前视图没有 AgentRun fork 入口。";

  const handleCopy = async () => {
    if (!actionModel.copyLastAgentReply.enabled) return;
    await navigator.clipboard.writeText(actionModel.copyLastAgentReply.text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  };

  const handleFork = async () => {
    if (!canFork || !forkPointRef || !onForkFromMessageRef || forking) return;
    setForking(true);
    try {
      await onForkFromMessageRef(forkPointRef);
    } finally {
      setForking(false);
    }
  };

  if (!actionModel.copyLastAgentReply.enabled && !actionModel.forkFromHere.forkPointRef) {
    return null;
  }

  return (
    <div className="group/round-actions flex justify-end pt-1">
      <div className="flex items-center gap-1 rounded-[8px] border border-border/40 bg-background/70 px-1 py-0.5 opacity-35 transition-opacity focus-within:opacity-100 hover:opacity-100">
        <button
          type="button"
          className="inline-flex h-7 w-7 items-center justify-center rounded-[6px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
          disabled={!actionModel.copyLastAgentReply.enabled}
          title={actionModel.copyLastAgentReply.enabled ? "复制当前轮次最后一条 Agent 回复" : "当前轮次没有可复制的 Agent 回复"}
          aria-label="复制当前轮次最后一条 Agent 回复"
          onClick={() => { void handleCopy(); }}
        >
          {copied ? <span className="text-[10px] font-medium">OK</span> : <CopyIcon />}
        </button>
        <button
          type="button"
          className="inline-flex h-7 w-7 items-center justify-center rounded-[6px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
          disabled={!canFork || forking}
          title={canFork ? "从当前稳定轮次 fork AgentRun" : forkDisabledReason ?? "当前轮次不可 fork"}
          aria-label="从当前稳定轮次 fork AgentRun"
          onClick={() => { void handleFork(); }}
        >
          {forking ? <span className="h-3 w-3 animate-spin rounded-[8px] border border-current border-t-transparent" /> : <ForkIcon />}
        </button>
      </div>
    </div>
  );
}

export function SessionChatComposer({
  commandState,
  discovery,
  discovered,
  execConfig,
  fileRef,
  hasRuntimeStreamTarget,
  inputPrefix,
  toolbarSlot,
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
  tokenUsage,
  sessionId,
  agentRunTarget,
  projectionRefreshKey,
  onAtTrigger,
  onFileSelected,
  onInputChange,
  onKeyDown,
  onCancelAction,
  onCommandAction,
  onExecutorConfigExplicitChange,
  onPlusMenuFiles,
  onRemoveImage,
}: {
  commandState: SessionChatCommandState;
  discovery: ExecutorDiscoveryState;
  discovered: ExecutorDiscoveredState;
  execConfig: ExecutorConfigState;
  fileRef: FileReferenceState;
  hasRuntimeStreamTarget: boolean;
  inputPrefix?: ReactNode;
  toolbarSlot?: ReactNode;
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
  tokenUsage: TokenUsageInfo | null;
  sessionId: string | null;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  projectionRefreshKey: number;
  onAtTrigger: (query: string) => void;
  onFileSelected: (file: FileEntry) => void;
  onInputChange: (value: string) => void;
  onKeyDown: (event: KeyboardEvent) => void;
  onCancelAction: () => void;
  onCommandAction: (command: SessionChatCommandModel) => void;
  onExecutorConfigExplicitChange?: (config: {
    providerId: string;
    modelId: string;
    thinkingLevel: string;
    permissionPolicy: string;
  }) => void;
  onPlusMenuFiles: (files: FileList) => void;
  onRemoveImage: (id: string) => void;
}) {
  const enterCommandId = commandState.keyboard.enter;
  const runtimeSubmitCommand = commandState.commands.find(
    (command) => command.command_id === enterCommandId,
  ) ?? commandState.commands.find(
    (command) => command.command_id === commandState.primaryCommandId && command.enabled,
  ) ?? commandState.commands.find(
    (command) => command.command_id === commandState.primaryCommandId,
  );
  const submitCommand = runtimeSubmitCommand;
  const cancelCommand = commandState.cancelCommand;

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
        {!hasRuntimeStreamTarget && !submitCommand?.enabled && promptTemplates && promptTemplates.length > 0 && (
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

        {/*
         * Composer — flex-wrap + order 单实例布局
         *
         * 非展开: [+] [input(flex-1)] [model] [send] — 单行，items-center 居中
         * 展开:   [图片+引用+input(w-full)] 换行 [+][gap][model][send]
         *
         * 展开条件: 文本换行 || 有附件 || 有文件引用
         */}
        <div className="relative flex flex-wrap items-center gap-x-1 rounded-[12px] bg-muted/40 px-2 py-1.5 transition-colors focus-within:bg-muted/60">
          {inputPrefix && (
            <div className="order-0 mb-1 flex w-full flex-wrap items-center gap-2 border-b border-border/40 px-2 pb-1.5 text-xs text-muted-foreground">
              {inputPrefix}
            </div>
          )}

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
              />
            </div>
          </div>

          {/* ② + 菜单 */}
          <div className={isExpanded ? "order-2" : "order-1"}>
            <ComposerPlusMenu
              disabled={isSending}
              onSelectFiles={onPlusMenuFiles}
            />
          </div>

          {/* ③ 弹性间隔（展开时推右） */}
          {isExpanded && <div className="order-3 flex-1" />}

          {/* ④ 上下文用量入口 + 模型选择器 */}
          <div className={isExpanded ? "order-4 flex items-center gap-0.5" : "order-3 flex items-center gap-0.5"}>
            <ContextUsageRing
              usage={tokenUsage}
              sessionId={sessionId}
              agentRunTarget={agentRunTarget}
              refreshKey={projectionRefreshKey}
            />
            {showExecutorSelector && (
              <InlineModelSelector
                execConfig={execConfig}
                discoveredOptions={discovered.options}
                isDiscoveredLoading={isDiscoveredLoading}
                executorName={executorName}
                readonly={isModelReadonly}
                status={commandState.modelConfig.status}
                message={commandState.modelConfig.message}
                onExplicitChange={onExecutorConfigExplicitChange}
                onRefresh={() => {
                  discovery.refetch();
                  discovered.reconnect();
                }}
              />
            )}
          </div>

          {/* ⑤ 发送/状态按钮 — 常驻 */}
          <div className={isExpanded ? "order-5" : "order-4"}>
            <ComposerSendButton
              isRunning={isActionRunning}
              hasInput={hasContent}
              isSending={isSending}
              isCancelling={isCancelling}
              cancelDisabled={cancelDisabled}
              submitCommand={sendDisabled ? undefined : submitCommand}
              onSubmit={onCommandAction}
              onCancel={onCancelAction}
            />
          </div>
        </div>

        <div className="mt-1.5 flex flex-wrap items-center justify-between gap-x-2 gap-y-1 px-1">
          <p className="min-w-0 text-[11px] text-muted-foreground/40">
            {helperText}
          </p>
          {toolbarSlot && (
            <div className="ml-auto flex items-center gap-0.5">
              {toolbarSlot}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
