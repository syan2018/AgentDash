import type { ConversationCommandView } from "../../../../generated/workflow-contracts";

interface ComposerSendButtonProps {
  isRunning: boolean;
  hasInput: boolean;
  isSending: boolean;
  isCancelling: boolean;
  cancelDisabled: boolean;
  submitCommand?: ConversationCommandView;
  alternateCommand?: ConversationCommandView;
  onSubmit: (command: ConversationCommandView) => void;
  onCancel: () => void;
}

function commandTitle(command: ConversationCommandView, fallback: string): string {
  if (!command.enabled) return command.unavailable_reason ?? fallback;
  switch (command.kind) {
    case "enqueue":
      return "排队";
    case "steer":
      return "Steer";
    case "send_next":
    case "start_draft":
      return "发送";
    default:
      return fallback;
  }
}

function optionalCommandTitle(
  command: ConversationCommandView | undefined,
  fallback: string,
): string {
  return command ? commandTitle(command, fallback) : fallback;
}

export function ComposerSendButton({
  isRunning,
  hasInput,
  isSending,
  isCancelling,
  cancelDisabled,
  submitCommand,
  alternateCommand,
  onSubmit,
  onCancel,
}: ComposerSendButtonProps) {
  const showStop = isRunning && !hasInput;
  const submitDisabled = isSending || !submitCommand?.enabled;

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

  if (isRunning && hasInput && submitCommand?.kind === "enqueue") {
    return (
      <div className="flex items-center gap-1.5">
        {alternateCommand?.kind === "steer" && (
          <button
            type="button"
            disabled={isSending || !alternateCommand.enabled}
            onClick={() => onSubmit(alternateCommand)}
            title={commandTitle(alternateCommand, "立即 Steer")}
            className="flex h-7 items-center gap-1 rounded-[12px] bg-primary/10 px-2.5 text-xs text-primary transition-colors hover:bg-primary/20 disabled:opacity-40"
          >
            <SteerIcon />
            <span>Steer</span>
          </button>
        )}
        <button
          type="button"
          disabled={submitDisabled}
          onClick={() => { if (submitCommand) onSubmit(submitCommand); }}
          title={isSending ? "排队中…" : optionalCommandTitle(submitCommand, "排队")}
          className="flex h-8 w-8 items-center justify-center rounded-[50%] bg-foreground text-background transition-opacity hover:opacity-80 disabled:opacity-30"
        >
          {isSending ? <Spinner /> : <QueueIcon />}
        </button>
      </div>
    );
  }

  // 默认：发送按钮常驻（无内容时 disabled 半透明）
  return (
    <button
      type="button"
      disabled={submitDisabled}
      onClick={() => { if (submitCommand) onSubmit(submitCommand); }}
      title={isSending ? "发送中…" : optionalCommandTitle(submitCommand, "发送")}
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
