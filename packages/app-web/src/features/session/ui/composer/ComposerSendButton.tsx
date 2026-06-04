/**
 * Morphing 发送按钮
 *
 * 状态机:
 * - idle + 有内容 → ↑ 发送 (Ctrl+Enter)
 * - idle + 无内容 → ↑ 发送 (disabled)
 * - running + 有内容 + enqueue → ↑ 排队 (Enter) + 可选 ⚡ steer (Ctrl+Enter)
 * - running + 无内容 → ■ 停止 (cancel)
 */

interface ComposerSendButtonProps {
  isRunning: boolean;
  hasInput: boolean;
  isSending: boolean;
  isCancelling: boolean;
  sendDisabled: boolean;
  cancelDisabled: boolean;
  /** 当前主动作类型 */
  primaryKind?: string;
  /** steer 辅助动作是否可用 */
  canSteer?: boolean;
  onSend: () => void;
  onSteer?: () => void;
  onCancel: () => void;
}

export function ComposerSendButton({
  isRunning,
  hasInput,
  isSending,
  isCancelling,
  sendDisabled,
  cancelDisabled,
  primaryKind,
  canSteer,
  onSend,
  onSteer,
  onCancel,
}: ComposerSendButtonProps) {
  const showStop = isRunning && !hasInput;
  const isEnqueueMode = primaryKind === "enqueue";

  if (showStop) {
    return (
      <button
        type="button"
        disabled={cancelDisabled}
        onClick={onCancel}
        title="停止"
        className="flex h-8 w-8 items-center justify-center rounded-[8px] bg-foreground text-background transition-opacity hover:opacity-90 disabled:opacity-40"
      >
        {isCancelling ? (
          <span className="inline-block h-3.5 w-3.5 animate-spin rounded-[8px] border-2 border-background border-t-transparent" />
        ) : (
          <StopIcon />
        )}
      </button>
    );
  }

  // Running + 有内容 + enqueue 模式：显示排队按钮 + 可选 steer 按钮
  if (isEnqueueMode && isRunning && hasInput) {
    return (
      <div className="flex items-center gap-1">
        {canSteer && onSteer && (
          <button
            type="button"
            disabled={isSending}
            onClick={onSteer}
            title="立即 Steer (Ctrl+Enter)"
            className="flex h-8 items-center gap-1 rounded-[8px] border border-primary/30 bg-primary/10 px-2 text-xs text-primary transition-opacity hover:bg-primary/20 disabled:opacity-40"
          >
            <SteerIcon />
            <span>Steer</span>
          </button>
        )}
        <button
          type="button"
          disabled={sendDisabled}
          onClick={onSend}
          title={isSending ? "排队中…" : "排队 (Enter)"}
          className="flex h-8 w-8 items-center justify-center rounded-[8px] bg-foreground text-background transition-opacity hover:opacity-90 disabled:opacity-40"
        >
          {isSending ? (
            <span className="inline-block h-3.5 w-3.5 animate-spin rounded-[8px] border-2 border-background border-t-transparent" />
          ) : (
            <QueueIcon />
          )}
        </button>
      </div>
    );
  }

  return (
    <button
      type="button"
      disabled={sendDisabled}
      onClick={onSend}
      title={isSending ? "发送中…" : "发送"}
      className="flex h-8 w-8 items-center justify-center rounded-[8px] bg-foreground text-background transition-opacity hover:opacity-90 disabled:opacity-40"
    >
      {isSending ? (
        <span className="inline-block h-3.5 w-3.5 animate-spin rounded-[8px] border-2 border-background border-t-transparent" />
      ) : (
        <ArrowUpIcon />
      )}
    </button>
  );
}

function ArrowUpIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <path d="M8 13V3M8 3L3.5 7.5M8 3L12.5 7.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/** 排队图标 — 带小时钟的上箭头 */
function QueueIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <path d="M8 13V3M8 3L3.5 7.5M8 3L12.5 7.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx="12" cy="12" r="3" fill="currentColor" opacity="0.3" />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
      <rect x="2" y="2" width="10" height="10" rx="1.5" fill="currentColor" />
    </svg>
  );
}

/** Steer 图标 — 闪电 */
function SteerIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <path d="M6.5 1L2.5 7H6L5.5 11L9.5 5H6L6.5 1Z" fill="currentColor" />
    </svg>
  );
}
