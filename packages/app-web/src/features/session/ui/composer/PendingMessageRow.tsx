/**
 * 排队消息行 — 投影服务端 pending queue 状态
 *
 * 每条消息显示预览文本 + 可选图片标记 + 操作按钮（引导/删除）。
 */

import type { PendingMessageView } from "../../../../generated/workflow-contracts";

interface PendingMessageRowProps {
  messages: PendingMessageView[];
  /** steer 能力是否可用（决定「引导」按钮是否显示） */
  canSteer: boolean;
  onPromote: (messageId: string) => void;
  onDelete: (messageId: string) => void;
}

export function PendingMessageList({
  messages,
  canSteer,
  onPromote,
  onDelete,
}: PendingMessageRowProps) {
  if (messages.length === 0) return null;

  return (
    <div className="flex flex-col gap-1 px-3 pb-2">
      <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
        排队消息 ({messages.length})
      </span>
      {messages.map((msg) => (
        <div
          key={msg.id}
          className="group flex items-center gap-2 rounded-[8px] border border-border/60 bg-secondary/30 px-2.5 py-1.5 text-xs"
        >
          <span className="inline-block h-1.5 w-1.5 shrink-0 rounded-[8px] bg-primary/40" />
          <span className="min-w-0 flex-1 truncate text-foreground/80">
            {msg.preview || "(空)"}
            {msg.has_images && (
              <span className="ml-1 text-muted-foreground">[图]</span>
            )}
          </span>
          <span className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
            {canSteer && (
              <button
                type="button"
                onClick={() => onPromote(msg.id)}
                title="立即引导（promote to steer）"
                className="rounded-[4px] px-1.5 py-0.5 text-[10px] text-primary hover:bg-primary/10"
              >
                引导
              </button>
            )}
            <button
              type="button"
              onClick={() => onDelete(msg.id)}
              title="删除排队消息"
              className="rounded-[4px] px-1.5 py-0.5 text-[10px] text-destructive hover:bg-destructive/10"
            >
              删除
            </button>
          </span>
        </div>
      ))}
    </div>
  );
}
