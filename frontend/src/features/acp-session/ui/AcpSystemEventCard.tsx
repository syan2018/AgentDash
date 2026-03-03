/**
 * ACP 系统事件卡片
 *
 * 渲染 session_info_update 事件：系统消息、错误、用户反馈等。
 * _meta.agentdash.event 携带事件元数据（type, severity, message, data）。
 */

import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { extractAgentDashMetaFromUpdate } from "../model/agentdashMeta";

export interface AcpSystemEventCardProps {
  update: SessionUpdate;
}

const SEVERITY_STYLES: Record<string, { border: string; bg: string; text: string; icon: string }> = {
  error: {
    border: "border-destructive/40",
    bg: "bg-destructive/5",
    text: "text-destructive",
    icon: "⚠",
  },
  warning: {
    border: "border-warning/40",
    bg: "bg-warning/5",
    text: "text-warning",
    icon: "⚡",
  },
  info: {
    border: "border-primary/30",
    bg: "bg-primary/5",
    text: "text-primary",
    icon: "ℹ",
  },
};

const DEFAULT_STYLE = SEVERITY_STYLES.info!;

const EVENT_TYPE_LABELS: Record<string, string> = {
  executor_session_bound: "会话已绑定",
  turn_completed: "执行完成",
  turn_failed: "执行失败",
  system_message: "系统消息",
  error: "错误",
  user_feedback: "用户反馈",
  user_answered_questions: "用户回答",
  permission_denied: "权限拒绝",
};

const EVENT_TYPE_DEFAULT_MESSAGES: Record<string, string> = {
  executor_session_bound: "已绑定到底层执行会话",
  turn_completed: "本轮执行已完成",
  turn_failed: "本轮执行失败",
  system_message: "系统消息",
  error: "执行出现错误",
  user_feedback: "已收到用户反馈",
  user_answered_questions: "已收到用户回答",
  permission_denied: "操作被权限策略拒绝",
};

export function AcpSystemEventCard({ update }: AcpSystemEventCardProps) {
  if (update.sessionUpdate !== "session_info_update") return null;

  const meta = extractAgentDashMetaFromUpdate(update);
  const event = meta?.event;

  const severity = event?.severity ?? "info";
  const style = SEVERITY_STYLES[severity] ?? DEFAULT_STYLE;
  const eventType = event?.type ?? "system";
  const typeLabel = EVENT_TYPE_LABELS[eventType] ?? eventType;
  const message = event?.message ?? (update as Record<string, unknown>).message as string | undefined;

  const u = update as Record<string, unknown>;
  const sessionInfo = u.sessionInfo as Record<string, unknown> | undefined;
  const fallbackMessage = typeof sessionInfo?.message === "string" ? sessionInfo.message : undefined;

  const displayMessage = message || fallbackMessage || EVENT_TYPE_DEFAULT_MESSAGES[eventType] || "系统事件";

  return (
    <div className={`rounded-md border ${style.border} ${style.bg} px-3 py-2`}>
      <div className="flex items-center gap-2">
        <span className="text-sm">{style.icon}</span>
        <span className={`text-xs font-medium ${style.text}`}>{typeLabel}</span>
        <span className="flex-1 text-sm text-foreground/80">{displayMessage}</span>
      </div>
      {event?.data != null && (
        <pre className="mt-1.5 overflow-auto rounded bg-muted/30 p-2 text-xs text-muted-foreground">
          {typeof event.data === "string" ? event.data : JSON.stringify(event.data, null, 2)}
        </pre>
      )}
    </div>
  );
}

export default AcpSystemEventCard;
