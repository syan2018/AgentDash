/**
 * 排队消息列表 — 参照 Cursor 风格
 *
 * 每行: 拖拽手柄 | 文档图标 | 预览文本 | 引导按钮 | 删除 | 更多菜单
 * 列表定位于 Composer 上方，居中对齐。
 */

import { useState, useRef, useEffect, useCallback } from "react";
import type {
  ConversationCommandView,
  ConversationPendingSnapshotView,
  PendingMessageView,
} from "../../../../generated/workflow-contracts";

interface PendingMessageListProps {
  messages: PendingMessageView[];
  pending?: ConversationPendingSnapshotView;
  promoteCommand?: ConversationCommandView;
  onPromote: (messageId: string) => void;
  onDelete: (messageId: string) => void;
  onResume?: () => void;
}

export function PendingMessageList({
  messages,
  pending,
  promoteCommand,
  onPromote,
  onDelete,
  onResume,
}: PendingMessageListProps) {
  if (messages.length === 0 && !pending?.user_attention) return null;
  const resumeCommand = pending?.resume_command;
  const showBanner = Boolean(pending?.user_attention && (pending.paused || resumeCommand));

  return (
    <div className="shrink-0 pb-2">
      <div className="mx-auto w-full max-w-4xl space-y-1 px-5">
        {showBanner && (
          <div className="flex items-center justify-between gap-3 rounded-[12px] border border-warning/25 bg-warning/10 px-3 py-2 text-xs text-warning">
            <div className="min-w-0">
              <div className="font-medium">Pending 队列已暂停</div>
              <div className="truncate text-warning/80">
                {resumeCommand?.unavailable_reason ?? "等待用户恢复后继续投递排队消息。"}
              </div>
            </div>
            {resumeCommand?.enabled && onResume && (
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
          <PendingMessageRow
            key={msg.id}
            message={msg}
            promoteCommand={promoteCommand}
            onPromote={onPromote}
            onDelete={onDelete}
          />
        ))}
      </div>
    </div>
  );
}

function PendingMessageRow({
  message,
  promoteCommand,
  onPromote,
  onDelete,
}: {
  message: PendingMessageView;
  promoteCommand?: ConversationCommandView;
  onPromote: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuBtnRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    function handleClick(e: MouseEvent) {
      if (
        menuRef.current && !menuRef.current.contains(e.target as Node) &&
        menuBtnRef.current && !menuBtnRef.current.contains(e.target as Node)
      ) {
        setMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [menuOpen]);

  const handlePromote = useCallback(() => {
    onPromote(message.id);
  }, [message.id, onPromote]);

  const handleDelete = useCallback(() => {
    setMenuOpen(false);
    onDelete(message.id);
  }, [message.id, onDelete]);

  return (
    <div className="group flex items-center gap-2 rounded-[12px] border border-border/40 bg-muted/30 px-2.5 py-2 transition-colors hover:bg-muted/50">
      {/* 拖拽手柄 */}
      <GripIcon className="shrink-0 cursor-grab text-muted-foreground/40" />

      {/* 文档图标 */}
      <DocIcon className="shrink-0 text-muted-foreground/60" />

      {/* 预览文本 */}
      <span className="min-w-0 flex-1 truncate text-sm text-foreground/80">
        {message.preview || "(空)"}
        {message.has_images && (
          <span className="ml-1.5 text-muted-foreground">[图]</span>
        )}
      </span>

      {/* 行内操作 — hover 显示 */}
      <div className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
        {promoteCommand?.enabled && (
          <button
            type="button"
            onClick={handlePromote}
            title="引导"
            className="flex h-7 items-center gap-1 rounded-lg px-2 text-xs text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
          >
            <SteerArrowIcon />
            <span>引导</span>
          </button>
        )}

        <button
          type="button"
          onClick={handleDelete}
          title="删除"
          className="flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
        >
          <TrashIcon />
        </button>

        {/* 更多菜单 */}
        <div className="relative">
          <button
            ref={menuBtnRef}
            type="button"
            onClick={() => setMenuOpen((v) => !v)}
            className="flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
          >
            <MoreIcon />
          </button>

          {menuOpen && (
            <div
              ref={menuRef}
              className="absolute right-0 top-full z-50 mt-1 w-36 rounded-[12px] border border-border/60 bg-popover p-1 shadow-lg"
            >
              <button
                type="button"
                onClick={() => setMenuOpen(false)}
                className="flex w-full items-center gap-2 rounded-lg px-3 py-1.5 text-xs text-foreground transition-colors hover:bg-muted"
              >
                <EditIcon />
                编辑消息
              </button>
              <button
                type="button"
                onClick={handleDelete}
                className="flex w-full items-center gap-2 rounded-lg px-3 py-1.5 text-xs text-destructive transition-colors hover:bg-destructive/10"
              >
                <CloseQueueIcon />
                关闭排队
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── Icons ────────────────────────────────────────────

function GripIcon({ className }: { className?: string }) {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" className={className}>
      <circle cx="5" cy="3" r="1" fill="currentColor" />
      <circle cx="9" cy="3" r="1" fill="currentColor" />
      <circle cx="5" cy="7" r="1" fill="currentColor" />
      <circle cx="9" cy="7" r="1" fill="currentColor" />
      <circle cx="5" cy="11" r="1" fill="currentColor" />
      <circle cx="9" cy="11" r="1" fill="currentColor" />
    </svg>
  );
}

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

function MoreIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
      <circle cx="3.5" cy="7" r="1" fill="currentColor" />
      <circle cx="7" cy="7" r="1" fill="currentColor" />
      <circle cx="10.5" cy="7" r="1" fill="currentColor" />
    </svg>
  );
}

function EditIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" className="shrink-0 text-muted-foreground">
      <path d="M8.5 1.5 10.5 3.5 4 10H2V8L8.5 1.5Z" stroke="currentColor" strokeWidth="1.1" strokeLinejoin="round" />
    </svg>
  );
}

function CloseQueueIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" className="shrink-0">
      <path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  );
}
