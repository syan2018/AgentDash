/**
 * Morphing 发送/停止按钮
 *
 * 状态机:
 * - 无内容 + 非 running → 隐藏（不渲染任何按钮）
 * - 有内容 + idle/enqueue → 深色圆形发送按钮
 * - running + 有内容 + enqueue → 排队按钮 + 可选 steer
 * - running + 无内容 → 深色圆形 stop 按钮
 */

interface ComposerSendButtonProps {
  isRunning: boolean;
  hasInput: boolean;
  isSending: boolean;
  isCancelling: boolean;
  sendDisabled: boolean;
  cancelDisabled: boolean;
  primaryKind?: string;
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

  // Running + 无内容 → Stop
  if (showStop) {
    return (
      <button
        type="button"
        disabled={cancelDisabled}
        onClick={onCancel}
        title="停止"
        className="flex h-8 w-8 items-center justify-center rounded-[50%] bg-foreground text-background transition-opacity hover:opacity-80 disabled:opacity-30"
      >
        {isCancelling ? <Spinner /> : <StopIcon />}
      </button>
    );
  }

  // Running + 有内容 + enqueue → 排队 + 可选 steer
  if (isEnqueueMode && isRunning && hasInput) {
    return (
      <div className="flex items-center gap-1.5">
        {canSteer && onSteer && (
          <button
            type="button"
            disabled={isSending}
            onClick={onSteer}
            title="立即 Steer (Ctrl+Enter)"
            className="flex h-7 items-center gap-1 rounded-[12px] bg-primary/10 px-2.5 text-xs text-primary transition-colors hover:bg-primary/20 disabled:opacity-40"
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
          className="flex h-8 w-8 items-center justify-center rounded-[50%] bg-foreground text-background transition-opacity hover:opacity-80 disabled:opacity-30"
        >
          {isSending ? <Spinner /> : <QueueIcon />}
        </button>
      </div>
    );
  }

  // 无内容 + 非 running → 不显示发送按钮
  if (!hasInput && !isRunning) return null;

  // 有内容 → 发送
  return (
    <button
      type="button"
      disabled={sendDisabled}
      onClick={onSend}
      title={isSending ? "发送中…" : "发送"}
      className="flex h-8 w-8 items-center justify-center rounded-[50%] bg-foreground text-background transition-opacity hover:opacity-80 disabled:opacity-30"
    >
      {isSending ? <Spinner /> : <ArrowUpIcon />}
    </button>
  );
}

function Spinner() {
  return (
    <span className="inline-block h-3.5 w-3.5 animate-spin rounded-[50%] border-2 border-background border-t-transparent" />
  );
}

function ArrowUpIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <path d="M8 13V3M8 3L3.5 7.5M8 3L12.5 7.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

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
      <rect x="2" y="2" width="10" height="10" rx="2" fill="currentColor" />
    </svg>
  );
}

function SteerIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <path d="M6.5 1L2.5 7H6L5.5 11L9.5 5H6L6.5 1Z" fill="currentColor" />
    </svg>
  );
}
