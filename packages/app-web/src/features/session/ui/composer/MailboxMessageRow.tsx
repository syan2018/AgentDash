/**
 * Mailbox 消息列表
 *
 * 每行: 文档图标 | 投影状态 | 预览文本 | 引导按钮 | 删除
 * 列表定位于 Composer 上方，居中对齐。
 */

import { useCallback } from "react";
import type {
  ConversationMailboxSnapshotView,
  ConversationCommandView,
  MailboxStateView,
  MailboxMessageView,
} from "../../../../generated/workflow-contracts";

interface MailboxMessageListProps {
  messages: MailboxMessageView[];
  mailbox?: ConversationMailboxSnapshotView;
  mailboxState?: MailboxStateView;
  promoteCommand?: ConversationCommandView;
  deleteCommand?: ConversationCommandView;
  onPromote: (messageId: string) => void;
  onDelete: (messageId: string) => void;
  onResume?: () => void;
}

export function MailboxMessageList({
  messages,
  mailbox,
  mailboxState,
  promoteCommand,
  deleteCommand,
  onPromote,
  onDelete,
  onResume,
}: MailboxMessageListProps) {
  if (messages.length === 0 && !mailbox?.user_attention && !mailboxState?.paused) return null;
  const resumeCommand = mailbox?.resume_command;
  const showBanner = Boolean(
    mailboxState?.paused || (mailbox?.user_attention && (mailbox.paused || resumeCommand)),
  );
  const bannerMessage = mailboxState?.message
    ?? mailboxState?.pause_reason
    ?? resumeCommand?.unavailable_reason
    ?? "等待用户恢复后继续投递消息。";
  const canResume = Boolean(mailboxState?.can_resume && resumeCommand?.enabled && onResume);

  return (
    <div className="shrink-0 pb-2">
      <div className="mx-auto w-full max-w-4xl space-y-1 px-5">
        {showBanner && (
          <div className="flex items-center justify-between gap-3 rounded-[12px] border border-warning/25 bg-warning/10 px-3 py-2 text-xs text-warning">
            <div className="min-w-0">
              <div className="font-medium">Mailbox 已暂停</div>
              <div className="truncate text-warning/80">
                {bannerMessage}
              </div>
            </div>
            {canResume && (
              <button
                type="button"
                onClick={onResume}
                className="shrink-0 rounded-[8px] border border-warning/30 bg-background px-2.5 py-1 text-xs font-medium text-warning transition-colors hover:bg-warning/10"
              >
                恢复
              </button>
            )}
          </div>
        )}
        {messages.map((msg) => (
          <MailboxMessageRow
            key={msg.id}
            message={msg}
            promoteCommand={promoteCommand}
            deleteCommand={deleteCommand}
            onPromote={onPromote}
            onDelete={onDelete}
          />
        ))}
      </div>
    </div>
  );
}

function MailboxMessageRow({
  message,
  promoteCommand,
  deleteCommand,
  onPromote,
  onDelete,
}: {
  message: MailboxMessageView;
  promoteCommand?: ConversationCommandView;
  deleteCommand?: ConversationCommandView;
  onPromote: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const handlePromote = useCallback(() => {
    onPromote(message.id);
  }, [message.id, onPromote]);

  const handleDelete = useCallback(() => {
    onDelete(message.id);
  }, [message.id, onDelete]);

  return (
    <div className="group flex items-start gap-2 rounded-[8px] border border-border/40 bg-muted/30 px-2.5 py-2 transition-colors hover:bg-muted/50">
      <DocIcon className="mt-0.5 shrink-0 text-muted-foreground/60" />

      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex min-w-0 items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-sm text-foreground/80">
            {message.preview || "(空)"}
            {message.has_images && (
              <span className="ml-1.5 text-muted-foreground">[图]</span>
            )}
          </span>
          <span className="shrink-0 rounded-[6px] bg-secondary/60 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
            {statusLabel(message.status)}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
          <span>{barrierLabel(message.barrier)}</span>
          <span className="text-muted-foreground/50">/</span>
          <span>{deliveryLabel(message.delivery)}</span>
          {message.attempt_count > 0 && (
            <>
              <span className="text-muted-foreground/50">/</span>
              <span>{message.attempt_count} 次尝试</span>
            </>
          )}
        </div>
        {message.last_error && (
          <div className="truncate text-[11px] text-destructive">
            {message.last_error}
          </div>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
        {promoteCommand?.enabled && message.can_promote && (
          <button
            type="button"
            onClick={handlePromote}
            title="引导"
            className="flex h-7 items-center gap-1 rounded-[8px] px-2 text-xs text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
          >
            <SteerArrowIcon />
            <span>引导</span>
          </button>
        )}

        {message.can_delete && deleteCommand?.enabled && (
          <button
            type="button"
            onClick={handleDelete}
            title="删除"
            className="flex h-7 w-7 items-center justify-center rounded-[8px] text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
          >
            <TrashIcon />
          </button>
        )}
      </div>
    </div>
  );
}

function statusLabel(status: MailboxMessageView["status"]): string {
  switch (status) {
    case "accepted":
      return "已接收";
    case "queued":
      return "排队中";
    case "ready_to_consume":
      return "待消费";
    case "consuming":
      return "消费中";
    case "dispatched":
      return "已投递";
    case "steered":
      return "已引导";
    case "paused":
      return "已暂停";
    case "blocked":
      return "已阻塞";
    case "failed":
      return "失败";
    case "deleted":
      return "已删除";
  }
}

function barrierLabel(barrier: MailboxMessageView["barrier"]): string {
  switch (barrier) {
    case "immediate_if_idle":
      return "空闲即投递";
    case "agent_loop_turn_boundary":
      return "Loop 边界";
    case "agent_run_turn_boundary":
      return "Run 边界";
    case "manual_resume":
      return "手动恢复";
  }
}

function deliveryLabel(delivery: MailboxMessageView["delivery"]): string {
  switch (delivery.kind) {
    case "launch_or_continue_turn":
      return "启动或继续";
    case "steer_active_turn":
      return delivery.stop_effect === "continue_on_stop"
        ? "Stop continuation"
        : "引导当前轮";
    case "resume_launch_source":
      return `恢复 ${delivery.launch_source}`;
  }
}

// ─── Icons ────────────────────────────────────────────

function DocIcon({ className }: { className?: string }) {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" className={className}>
      <path d="M4 1.5h4l3 3V12a.5.5 0 0 1-.5.5h-6A.5.5 0 0 1 4 12V2a.5.5 0 0 1 .5-.5Z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
      <path d="M8 1.5V5h3.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M6 8h3M6 10h2" stroke="currentColor" strokeWidth="1" strokeLinecap="round" />
    </svg>
  );
}

function SteerArrowIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <path d="M1 6h8M6 3l3 3-3 3" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
      <path d="M3 4h8M5.5 4V3a.5.5 0 0 1 .5-.5h2a.5.5 0 0 1 .5.5v1M4 4l.5 8h5L10 4" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
