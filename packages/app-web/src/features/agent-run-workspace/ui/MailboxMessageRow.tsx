import { useState, useCallback } from "react";
import type {
  MailboxMessageStatus,
  MailboxMessageView,
} from "../../../generated/agent-run-mailbox-contracts";
import type { ConversationWaitingItemView } from "../../../generated/workflow-contracts";
import type {
  AgentRunChatCommandModel,
  AgentRunChatMailboxModel,
} from "../model/conversationCommandState";
import { useSessionWorkspacePanelAction } from "../../session/ui/SessionWorkspacePanelActionContext";
import { terminalUriForWaitingItem } from "../model/waitingTerminal";
import { mailboxHasContent } from "./mailboxContent";

interface MailboxMessageListProps {
  messages: MailboxMessageView[];
  mailbox?: AgentRunChatMailboxModel;
  onPromote: (messageId: string) => void;
  onDelete: (messageId: string) => void;
  onResume?: () => void;
  onRecall?: (messageId: string) => void;
  onMove?: (messageId: string, afterMessageId: string | null) => void;
}

export function MailboxMessageList(props: MailboxMessageListProps) {
  if (!mailboxHasContent(props.messages, props.mailbox)) return null;
  return (
    <div className="shrink-0 pb-2">
      <div className="mx-auto w-full max-w-4xl px-5">
        <div className="relative rounded-[12px] border border-border/60 bg-background pb-1 shadow-sm">
          <MailboxSections {...props} />
        </div>
      </div>
    </div>
  );
}

/** mailbox 内部分区（banner + steer + pending），不含外层卡片 chrome；供状态栏内嵌。 */
export function MailboxSections({
  messages,
  mailbox,
  onPromote,
  onDelete,
  onResume,
  onRecall,
  onMove,
}: MailboxMessageListProps) {
  const waitingItems = mailbox?.waiting_items ?? [];
  const steerMessages = messages.filter(
    (m) => m.delivery.kind === "steer_active_turn" &&
      (!mailbox?.hide_system_steer_messages || m.origin === "user"),
  );
  const pendingMessages = messages.filter(
    (m) => m.delivery.kind !== "steer_active_turn",
  );

  const showBanner = Boolean(
    mailbox?.paused || (mailbox?.user_attention && (mailbox.paused || mailbox.resumeAction)),
  );
  const canResume = Boolean(mailbox?.can_resume && mailbox.resumeAction?.enabled && onResume);

  return (
    <>
      {/* Waiting 区 */}
      {waitingItems.length > 0 && (
        <div>
          <SectionLabel label="Waiting" count={waitingItems.length} />
          {waitingItems.map((item, i) => (
            <div key={item.wait_id}>
              {i > 0 && <div className="mx-4 border-t border-border/20" />}
              <WaitingItemRow item={item} />
            </div>
          ))}
        </div>
      )}

      {waitingItems.length > 0 && (steerMessages.length > 0 || pendingMessages.length > 0) && (
        <div className="border-t border-border/50" />
      )}

      {/* Banner */}
      {showBanner && (
        <div className="border-b border-border/40 bg-warning/5 px-3 py-2">
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <div className="text-xs font-medium text-warning">消息投递已暂停</div>
              <div className="truncate text-[11px] text-warning/70">
                等待恢复后继续投递排队消息
              </div>
            </div>
            {canResume && (
              <button
                type="button"
                onClick={onResume}
                className="shrink-0 rounded-[8px] border border-warning/30 bg-background px-2.5 py-1 text-[11px] font-medium text-warning transition-colors hover:bg-warning/10"
              >
                恢复
              </button>
            )}
          </div>
        </div>
      )}

      {/* Steer 区 */}
      {steerMessages.length > 0 && (
            <div>
              <SectionLabel label="Steer" count={steerMessages.length} />
              {steerMessages.map((msg, i) => (
                <div key={msg.id}>
                  {i > 0 && <div className="mx-4 border-t border-border/20" />}
                  <MessageRow
                    message={msg}
                    section="steer"
                    index={i}
                    totalInSection={steerMessages.length}
                    pendingMessages={pendingMessages}
                    promoteCommand={mailbox?.promoteAction}
                    deleteCommand={mailbox?.deleteAction}
                    onPromote={onPromote}
                    onDelete={onDelete}
                    onRecall={onRecall}
                    onMove={onMove}
                  />
                </div>
              ))}
            </div>
          )}

          {/* 区分线 */}
          {steerMessages.length > 0 && pendingMessages.length > 0 && (
            <div className="border-t border-border/50" />
          )}

          {/* Pending 区 */}
          {pendingMessages.length > 0 && (
            <div>
              {steerMessages.length > 0 && (
                <SectionLabel label="Pending" count={pendingMessages.length} />
              )}
              {pendingMessages.map((msg, i) => (
                <div key={msg.id}>
                  {i > 0 && <div className="mx-4 border-t border-border/20" />}
                  <MessageRow
                    message={msg}
                    section="pending"
                    index={i}
                    totalInSection={pendingMessages.length}
                    pendingMessages={pendingMessages}
                    promoteCommand={mailbox?.promoteAction}
                    deleteCommand={mailbox?.deleteAction}
                    onPromote={onPromote}
                    onDelete={onDelete}
                    onRecall={onRecall}
                    onMove={onMove}
                  />
                </div>
              ))}
            </div>
          )}
    </>
  );
}

const SOURCE_LABELS: Record<string, string> = {
  "mailbox.source.core.composer": "用户输入",
  "mailbox.source.core.canvas_action": "Canvas",
  "mailbox.source.core.draft_start": "草稿输入",
  "mailbox.source.core.hook_after_turn": "Hook",
  "mailbox.source.core.hook_before_stop": "Hook",
  "mailbox.source.core.hook_auto_resume": "Hook",
  "mailbox.source.core.local_relay_prompt": "本机输入",
  "mailbox.source.routine.trigger": "Routine 触发",
  "mailbox.source.companion.dispatch": "Companion 派发",
  "mailbox.source.companion.result": "Companion 结果",
  "mailbox.source.companion.parent_request": "Parent 请求",
  "mailbox.source.companion.parent_response": "Parent 回应",
  "mailbox.source.companion.human_response": "用户回应",
  "mailbox.source.companion.parent_resume": "Parent 续跑",
  "mailbox.source.workflow.orchestrator": "Workflow",
};

const STATUS_LABELS: Record<MailboxMessageStatus, string> = {
  accepted: "已接收",
  queued: "排队",
  ready_to_consume: "待投递",
  consuming: "投递中",
  dispatched: "已投递",
  steered: "已注入",
  paused: "暂停",
  blocked: "阻塞",
  failed: "失败",
  deleted: "已删除",
};

const WAIT_KIND_LABELS: Record<string, string> = {
  companion: "Companion",
  subagent: "Subagent",
  human: "用户回应",
  exec: "Exec",
  workflow: "Workflow",
};

const WAIT_STATUS_LABELS: Record<string, string> = {
  open: "等待中",
  resolved: "已完成",
  cancelled: "已取消",
  failed: "失败",
  expired: "已超时",
};

function mailboxSourceLabel(message: MailboxMessageView): string {
  const source = message.source;
  const explicitLabel = SOURCE_LABELS[source.display_label_key];
  if (explicitLabel) return explicitLabel;

  const namespaceKindLabel = SOURCE_LABELS[`mailbox.source.${source.namespace}.${source.kind}`];
  if (namespaceKindLabel) return namespaceKindLabel;

  switch (source.namespace) {
    case "routine":
      return "Routine";
    case "companion":
      return "Companion";
    case "workflow":
      return "Workflow";
    case "core":
      return source.kind === "canvas_action" ? "Canvas" : "用户输入";
    default:
      return formatSourceKind(source.kind || source.namespace);
  }
}

function waitingKindLabel(kind: string): string {
  return WAIT_KIND_LABELS[kind] ?? formatSourceKind(kind);
}

function waitingStatusLabel(status: string): string {
  return WAIT_STATUS_LABELS[status] ?? formatSourceKind(status);
}

function formatSourceKind(value: string): string {
  return value
    .split(/[_-]+/)
    .filter((part) => part.length > 0)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function formatTimestamp(value: string): string {
  const normalized = value.trim();
  if (normalized.length >= 16 && normalized[10] === "T") {
    return normalized.slice(0, 16).replace("T", " ");
  }
  return normalized;
}

function mailboxStatusClassName(status: MailboxMessageStatus): string {
  switch (status) {
    case "blocked":
    case "failed":
      return "bg-destructive/10 text-destructive";
    case "paused":
      return "bg-warning/10 text-warning";
    case "consuming":
    case "ready_to_consume":
      return "bg-info/10 text-info";
    default:
      return "border border-border bg-secondary text-muted-foreground";
  }
}

function waitingStatusClassName(status: string): string {
  switch (status) {
    case "failed":
      return "bg-destructive/10 text-destructive";
    case "expired":
      return "bg-warning/10 text-warning";
    case "resolved":
    case "cancelled":
      return "border border-border bg-secondary text-muted-foreground";
    default:
      return "bg-info/10 text-info";
  }
}

// ─── Section Label ────────────────────────────────────

function SectionLabel({ label, count }: { label: string; count: number }) {
  return (
    <div className="px-3 py-0.5">
      <span className="text-[10px] text-muted-foreground/60">
        {label}
        <span className="ml-1 text-muted-foreground/30">·</span>
        <span className="ml-1 tabular-nums text-muted-foreground/30">{count}</span>
      </span>
    </div>
  );
}

function WaitingItemRow({ item }: { item: ConversationWaitingItemView }) {
  const openWorkspacePanel = useSessionWorkspacePanelAction();
  const kindLabel = waitingKindLabel(item.kind);
  const sourceLabel = item.source_label?.trim() || kindLabel;
  const preview = item.preview?.trim() || "等待外部事件";
  const statusLabel = waitingStatusLabel(item.status);
  const timeLabel = item.resolved_at
    ? `完成 ${formatTimestamp(item.resolved_at)}`
    : `创建 ${formatTimestamp(item.created_at)}`;
  const terminalUri = terminalUriForWaitingItem(item);
  const canOpenTerminal = Boolean(openWorkspacePanel && terminalUri);
  const handleOpenTerminal = useCallback(() => {
    if (!openWorkspacePanel || !terminalUri) return;
    openWorkspacePanel({
      typeId: "terminal",
      uri: terminalUri,
      options: { refreshContent: true },
    });
  }, [openWorkspacePanel, terminalUri]);

  return (
    <div className="group relative">
      <div className="flex h-9 items-center gap-2 px-3">
        <div className="flex w-5 shrink-0 items-center justify-center">
          <span className="h-2 w-2 rounded-[8px] bg-info/70" />
        </div>
        <span className="max-w-24 shrink-0 truncate rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
          {kindLabel}
        </span>
        <span className="max-w-32 shrink-0 truncate text-[11px] text-muted-foreground">
          {sourceLabel}
        </span>
        <span className="min-w-0 flex-1 truncate text-[13px] leading-tight text-foreground/80">
          {preview}
        </span>
        <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground/60">
          {timeLabel}
        </span>
        <span className={`shrink-0 rounded-[6px] px-1.5 py-0.5 text-[10px] font-medium ${waitingStatusClassName(item.status)}`}>
          {statusLabel}
        </span>
        {terminalUri && (
          <button
            type="button"
            onClick={handleOpenTerminal}
            disabled={!canOpenTerminal}
            title={canOpenTerminal ? "打开对应终端" : "当前页面没有工作区面板"}
            className="shrink-0 rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
          >
            打开终端
          </button>
        )}
      </div>
    </div>
  );
}

// ─── 统一消息行 ────────────────────────────────────────

function MessageRow({
  message,
  section,
  index,
  totalInSection,
  pendingMessages,
  promoteCommand,
  deleteCommand,
  onPromote,
  onDelete,
  onRecall,
  onMove,
}: {
  message: MailboxMessageView;
  section: "steer" | "pending";
  index: number;
  totalInSection: number;
  pendingMessages: MailboxMessageView[];
  promoteCommand?: AgentRunChatCommandModel;
  deleteCommand?: AgentRunChatCommandModel;
  onPromote: (id: string) => void;
  onDelete: (id: string) => void;
  onRecall?: (id: string) => void;
  onMove?: (messageId: string, afterMessageId: string | null) => void;
}) {
  const handleMoveUp = useCallback(() => {
    if (!onMove || index <= 0) return;
    const afterId = index >= 2 ? pendingMessages[index - 2].id : null;
    onMove(message.id, afterId);
  }, [message.id, index, pendingMessages, onMove]);

  const handleMoveDown = useCallback(() => {
    if (!onMove || index >= totalInSection - 1) return;
    const afterId = pendingMessages[index + 1].id;
    onMove(message.id, afterId);
  }, [message.id, index, totalInSection, pendingMessages, onMove]);

  const isFailed = message.status === "failed" || message.status === "blocked";
  const isSteer = section === "steer";
  const canRecall = Boolean(message.can_recall && onRecall);
  const canRetry = Boolean(isFailed && onRecall);
  const sourceLabel = mailboxSourceLabel(message);
  const statusLabel = STATUS_LABELS[message.status];

  return (
    <div className="group relative">
      <div className="flex h-8 items-center gap-2 px-3">
        {/* 左侧固定宽度区域 — 保证等高 */}
        <div className="flex w-5 shrink-0 items-center justify-center">
          {isSteer ? (
            <span className="text-[10px] text-muted-foreground/60">
              {message.origin === "user" ? "You" : message.origin === "hook" ? "Hook" : "Sys"}
            </span>
          ) : message.can_reorder && onMove ? (
            <DragHandle
              canMoveUp={index > 0}
              canMoveDown={index < totalInSection - 1}
              onMoveUp={handleMoveUp}
              onMoveDown={handleMoveDown}
            />
          ) : (
            <GripIcon />
          )}
        </div>

        {/* 内容 */}
        <span className="max-w-28 shrink-0 truncate rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
          {sourceLabel}
        </span>
        <span className={`min-w-0 flex-1 truncate text-[13px] leading-tight ${isFailed ? "text-destructive/80" : "text-foreground/80"}`}>
          {message.preview || "(空)"}
          {message.has_images && (
            <span className="ml-1.5 text-muted-foreground/50">[图]</span>
          )}
        </span>

        <span className={`shrink-0 rounded-[6px] px-1.5 py-0.5 text-[10px] font-medium ${mailboxStatusClassName(message.status)}`}>
          {statusLabel}
        </span>

        {/* hover 操作 */}
        <div className={`flex shrink-0 items-center gap-0.5 transition-opacity ${isFailed ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}>
          {!isSteer && promoteCommand?.enabled && message.can_promote && (
            <ActionButton onClick={() => onPromote(message.id)} title="注入当前轮">
              <SteerArrowIcon />
            </ActionButton>
          )}

          {canRetry && (
            <ActionButton onClick={() => onRecall?.(message.id)} title="重试">
              <RetryIcon />
            </ActionButton>
          )}

          {!canRetry && canRecall && (
            <ActionButton onClick={() => onRecall?.(message.id)} title="编辑">
              <EditIcon />
            </ActionButton>
          )}

          {message.can_delete && deleteCommand?.enabled && (
            <ActionButton onClick={() => onDelete(message.id)} title="删除" destructive>
              <TrashIcon />
            </ActionButton>
          )}
        </div>
      </div>
      {isFailed && message.last_error && (
        <div className="mx-3 mb-2 max-h-32 overflow-auto whitespace-pre-wrap break-words rounded-[6px] border border-destructive/30 bg-destructive/10 px-2 py-1.5 text-[11px] leading-relaxed text-destructive">
          {message.last_error}
        </div>
      )}
    </div>
  );
}

// ─── Drag Handle ────────────────────────────────────────

function DragHandle({
  canMoveUp,
  canMoveDown,
  onMoveUp,
  onMoveDown,
}: {
  canMoveUp: boolean;
  canMoveDown: boolean;
  onMoveUp: () => void;
  onMoveDown: () => void;
}) {
  const [showArrows, setShowArrows] = useState(false);

  return (
    <div
      className="relative flex h-full items-center"
      onMouseEnter={() => setShowArrows(true)}
      onMouseLeave={() => setShowArrows(false)}
    >
      {showArrows ? (
        <div className="flex flex-col items-center gap-px">
          <button
            type="button"
            onClick={onMoveUp}
            disabled={!canMoveUp}
            className="flex h-3 w-4 items-center justify-center text-muted-foreground/60 transition-colors hover:text-foreground disabled:opacity-20"
          >
            <ChevronUpIcon />
          </button>
          <button
            type="button"
            onClick={onMoveDown}
            disabled={!canMoveDown}
            className="flex h-3 w-4 items-center justify-center text-muted-foreground/60 transition-colors hover:text-foreground disabled:opacity-20"
          >
            <ChevronDownIcon />
          </button>
        </div>
      ) : (
        <GripIcon />
      )}
    </div>
  );
}

// ─── 共用小组件 ─────────────────────────────────────────

function ActionButton({
  onClick,
  title,
  destructive,
  children,
}: {
  onClick: () => void;
  title: string;
  destructive?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-label={title}
      className={`flex h-6 w-6 items-center justify-center rounded-[6px] transition-colors ${
        destructive
          ? "text-muted-foreground/60 hover:bg-destructive/10 hover:text-destructive"
          : "text-muted-foreground/60 hover:bg-foreground/5 hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

// ─── Icons ────────────────────────────────────────────

function GripIcon() {
  return (
    <svg width="10" height="14" viewBox="0 0 10 14" fill="none" className="text-muted-foreground/30">
      <circle cx="3" cy="3" r="1" fill="currentColor" />
      <circle cx="7" cy="3" r="1" fill="currentColor" />
      <circle cx="3" cy="7" r="1" fill="currentColor" />
      <circle cx="7" cy="7" r="1" fill="currentColor" />
      <circle cx="3" cy="11" r="1" fill="currentColor" />
      <circle cx="7" cy="11" r="1" fill="currentColor" />
    </svg>
  );
}

function ChevronUpIcon() {
  return (
    <svg width="10" height="6" viewBox="0 0 10 6" fill="none">
      <path d="M1.5 4.5l3.5-3 3.5 3" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function ChevronDownIcon() {
  return (
    <svg width="10" height="6" viewBox="0 0 10 6" fill="none">
      <path d="M1.5 1.5l3.5 3 3.5-3" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function EditIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <path d="M8.5 1.5l2 2L4 10H2v-2l6.5-6.5Z" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function RetryIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <path d="M9.5 3.5A4 4 0 1 0 10 7" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M9.5 1.5v2h-2" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function SteerArrowIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <path d="M2 6h7M6.5 3.5L9 6l-2.5 2.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <path d="M2.5 3.5h7M4.5 3.5V2.5a.5.5 0 0 1 .5-.5h2a.5.5 0 0 1 .5.5v1M3.5 3.5l.5 7h4l.5-7" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
