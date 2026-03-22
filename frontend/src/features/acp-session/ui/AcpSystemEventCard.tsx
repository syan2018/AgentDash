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

interface HookEventData {
  trigger?: string;
  decision?: string;
  sequence?: number;
  revision?: number;
  tool_name?: string | null;
  tool_call_id?: string | null;
  subagent_type?: string | null;
  matched_rule_keys?: string[];
  refresh_snapshot?: boolean;
  block_reason?: string | null;
  completion?: {
    mode?: string;
    satisfied?: boolean;
    advanced?: boolean;
    reason?: string;
  } | null;
  diagnostic_codes?: string[];
  diagnostics?: Array<{
    code?: string;
    summary?: string;
    detail?: string | null;
  }>;
}

const SEVERITY_STYLES: Record<string, { border: string; tone: string; badge: string }> = {
  error: {
    border: "border-destructive/35",
    tone: "text-destructive",
    badge: "border-destructive/25 bg-destructive/10 text-destructive",
  },
  warning: {
    border: "border-warning/35",
    tone: "text-warning",
    badge: "border-warning/25 bg-warning/10 text-warning",
  },
  info: {
    border: "border-border",
    tone: "text-muted-foreground",
    badge: "border-border bg-secondary/50 text-muted-foreground",
  },
};

const DEFAULT_STYLE = SEVERITY_STYLES.info!;

const EVENT_TYPE_LABELS: Record<string, string> = {
  executor_session_bound: "会话已绑定",
  turn_started: "执行开始",
  turn_completed: "执行完成",
  turn_interrupted: "执行已中断",
  turn_failed: "执行失败",
  system_message: "系统消息",
  error: "错误",
  user_feedback: "用户反馈",
  user_answered_questions: "用户回答",
  permission_denied: "权限拒绝",
  approval_requested: "等待审批",
  approval_resolved: "审批结果",
  hook_action_resolved: "Hook 事项已结案",
  companion_dispatch_registered: "Companion 已派发",
  companion_result_available: "Companion 结果可用",
  companion_result_returned: "Companion 已回传",
  hook_event: "Hook 事件",
};

const EVENT_TYPE_DEFAULT_MESSAGES: Record<string, string> = {
  executor_session_bound: "已绑定到底层执行会话",
  turn_started: "本轮执行已开始",
  turn_completed: "本轮执行已完成",
  turn_interrupted: "本轮执行已中断",
  turn_failed: "本轮执行失败",
  system_message: "系统消息",
  error: "执行出现错误",
  user_feedback: "已收到用户反馈",
  user_answered_questions: "已收到用户回答",
  permission_denied: "操作被权限策略拒绝",
  approval_requested: "当前工具调用正在等待审批",
  approval_resolved: "当前工具调用审批已完成",
  hook_action_resolved: "Hook Runtime 中的一项干预已被显式结案",
  companion_dispatch_registered: "已注册 companion 派发上下文",
  companion_result_available: "Companion 已回传结构化结果",
  companion_result_returned: "当前 companion 结果已回传到主 session",
  hook_event: "Hook Runtime 已产生新的流程事件",
};

export function AcpSystemEventCard({ update }: AcpSystemEventCardProps) {
  if (update.sessionUpdate !== "session_info_update") return null;

  const meta = extractAgentDashMetaFromUpdate(update);
  const event = meta?.event;

  const severity = event?.severity ?? "info";
  const style = SEVERITY_STYLES[severity] ?? DEFAULT_STYLE;
  const eventType = event?.type ?? "system";
  const typeLabel = EVENT_TYPE_LABELS[eventType] ?? eventType;
  const severityLabel = resolveSeverityLabel(severity);
  const message = event?.message ?? (update as Record<string, unknown>).message as string | undefined;
  const hookData = eventType === "hook_event" ? extractHookEventData(event?.data) : null;

  const u = update as Record<string, unknown>;
  const sessionInfo = u.sessionInfo as Record<string, unknown> | undefined;
  const fallbackMessage = typeof sessionInfo?.message === "string" ? sessionInfo.message : undefined;

  const displayMessage = message || fallbackMessage || EVENT_TYPE_DEFAULT_MESSAGES[eventType] || "系统事件";
  const metadataChips = buildMetadataChips(eventType, hookData, meta?.trace?.turnId ?? null);
  const detailLines = buildDetailLines(eventType, event?.data, hookData);
  const extraData = eventType === "hook_event" ? null : formatExtraData(event?.data);

  return (
    <div className={`rounded-[12px] border ${style.border} bg-background px-4 py-3`}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className={`rounded-full border px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] ${style.badge}`}>
              {typeLabel}
            </span>
            {event?.code && (
              <span className="rounded-full border border-border bg-secondary/40 px-2 py-1 font-mono text-[10px] text-muted-foreground">
                {event.code}
              </span>
            )}
          </div>
          <p className="mt-2 text-sm leading-6 text-foreground/90">{displayMessage}</p>
        </div>
        <span className={`shrink-0 text-[11px] font-medium ${style.tone}`}>{severityLabel}</span>
      </div>

      {metadataChips.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-1.5">
          {metadataChips.map((chip) => (
            <span
              key={chip}
              className="rounded-full border border-border bg-secondary/35 px-2 py-1 text-[10px] text-muted-foreground"
            >
              {chip}
            </span>
          ))}
        </div>
      )}

      {detailLines.length > 0 && (
        <div className="mt-3 space-y-2 rounded-[10px] border border-border/70 bg-secondary/20 px-3 py-2.5">
          {detailLines.map((line) => (
            <p key={line} className="text-xs leading-5 text-muted-foreground">
              {line}
            </p>
          ))}
        </div>
      )}

      {extraData && (
        <pre className="mt-3 overflow-auto rounded-[10px] border border-border/70 bg-secondary/20 p-3 text-xs text-muted-foreground">
          {extraData}
        </pre>
      )}
    </div>
  );
}

function resolveSeverityLabel(severity: string): string {
  switch (severity) {
    case "error":
      return "错误";
    case "warning":
      return "注意";
    default:
      return "信息";
  }
}

function buildMetadataChips(
  eventType: string,
  hookData: HookEventData | null,
  turnId: string | null,
): string[] {
  const chips: string[] = [];
  if (turnId) {
    chips.push(`turn: ${turnId}`);
  }
  if (eventType !== "hook_event" || !hookData) {
    return chips;
  }

  if (hookData.trigger) {
    chips.push(`trigger: ${hookData.trigger}`);
  }
  if (hookData.decision) {
    chips.push(`decision: ${hookData.decision}`);
  }
  if (hookData.completion?.mode) {
    chips.push(`completion: ${hookData.completion.mode}`);
  }
  if (typeof hookData.revision === "number") {
    chips.push(`revision: ${hookData.revision}`);
  }
  if (typeof hookData.sequence === "number") {
    chips.push(`trace: #${hookData.sequence}`);
  }
  if (hookData.tool_name) {
    chips.push(`tool: ${hookData.tool_name}`);
  }
  if (hookData.subagent_type) {
    chips.push(`subagent: ${hookData.subagent_type}`);
  }
  if (hookData.refresh_snapshot) {
    chips.push("已请求刷新快照");
  }
  if (hookData.matched_rule_keys?.length) {
    chips.push(`rules: ${hookData.matched_rule_keys.length}`);
  }
  if (hookData.diagnostic_codes?.length) {
    chips.push(`diagnostics: ${hookData.diagnostic_codes.length}`);
  }
  return chips;
}

function buildDetailLines(
  eventType: string,
  eventData: unknown,
  hookData: HookEventData | null,
): string[] {
  if (eventType !== "hook_event") {
    return buildGenericDetailLines(eventType, eventData);
  }
  if (!hookData) {
    return [];
  }

  const lines: string[] = [];
  if (hookData.block_reason) {
    lines.push(`阻塞原因：${hookData.block_reason}`);
  }

  if (hookData.completion) {
    const status = hookData.completion.satisfied ? "已满足" : "未满足";
    const advanced = hookData.completion.advanced ? "，并已推进后续阶段" : "";
    const reason = hookData.completion.reason ? `：${hookData.completion.reason}` : "";
    lines.push(`完成判定：${status}${advanced}${reason}`);
  }

  if (hookData.matched_rule_keys?.length) {
    lines.push(`命中规则：${hookData.matched_rule_keys.join("，")}`);
  }

  if (hookData.tool_call_id) {
    lines.push(`tool_call_id：${hookData.tool_call_id}`);
  }

  if (hookData.diagnostics?.length) {
    for (const diagnostic of hookData.diagnostics) {
      const summary = typeof diagnostic?.summary === "string" ? diagnostic.summary : null;
      if (!summary) continue;
      const code = typeof diagnostic?.code === "string" ? diagnostic.code : null;
      const detail = typeof diagnostic?.detail === "string" ? diagnostic.detail : null;
      lines.push(
        `诊断${code ? ` ${code}` : ""}：${summary}${detail ? `；${detail}` : ""}`,
      );
    }
  }

  return lines;
}

function buildGenericDetailLines(eventType: string, value: unknown): string[] {
  if (!isRecord(value)) return [];
  const lines: string[] = [];

  if (eventType === "approval_requested" || eventType === "approval_resolved") {
    const toolName = typeof value.tool_name === "string" ? value.tool_name : null;
    const toolCallId = typeof value.tool_call_id === "string" ? value.tool_call_id : null;
    const reason = typeof value.reason === "string" ? value.reason : null;
    const approved = typeof value.approved === "boolean" ? value.approved : null;
    if (toolName) lines.push(`工具：${toolName}`);
    if (toolCallId) lines.push(`tool_call_id：${toolCallId}`);
    if (approved != null) lines.push(`审批结果：${approved ? "已批准" : "已拒绝"}`);
    if (reason) lines.push(`原因：${reason}`);
    return lines;
  }

  if (eventType === "hook_action_resolved") {
    const actionId = typeof value.action_id === "string" ? value.action_id : null;
    const actionType = typeof value.action_type === "string" ? value.action_type : null;
    const status = typeof value.status === "string" ? value.status : null;
    const resolutionKind = typeof value.resolution_kind === "string" ? value.resolution_kind : null;
    const resolutionNote = typeof value.resolution_note === "string" ? value.resolution_note : null;
    const resolutionTurnId = typeof value.resolution_turn_id === "string" ? value.resolution_turn_id : null;
    const summary = typeof value.summary === "string" ? value.summary : null;
    if (actionId) lines.push(`action_id：${actionId}`);
    if (actionType) lines.push(`action_type：${actionType}`);
    if (status) lines.push(`status：${status}`);
    if (resolutionKind) lines.push(`resolution_kind：${resolutionKind}`);
    if (resolutionTurnId) lines.push(`resolution_turn_id：${resolutionTurnId}`);
    if (summary) lines.push(`摘要：${summary}`);
    if (resolutionNote) lines.push(`说明：${resolutionNote}`);
    return lines;
  }

  if (eventType === "companion_dispatch_registered"
    || eventType === "companion_result_available"
    || eventType === "companion_result_returned") {
    const label = typeof value.companion_label === "string" ? value.companion_label : null;
    const sessionId = typeof value.companion_session_id === "string" ? value.companion_session_id : null;
    const dispatchId = typeof value.dispatch_id === "string" ? value.dispatch_id : null;
    const adoptionMode = typeof value.adoption_mode === "string" ? value.adoption_mode : null;
    const sliceMode = typeof value.slice_mode === "string" ? value.slice_mode : null;
    const status = typeof value.status === "string" ? value.status : null;
    const summary = typeof value.summary === "string" ? value.summary : null;
    if (label) lines.push(`companion：${label}`);
    if (dispatchId) lines.push(`dispatch_id：${dispatchId}`);
    if (sessionId) lines.push(`companion_session_id：${sessionId}`);
    if (sliceMode) lines.push(`slice_mode：${sliceMode}`);
    if (adoptionMode) lines.push(`adoption_mode：${adoptionMode}`);
    if (status) lines.push(`status：${status}`);
    if (summary) lines.push(`摘要：${summary}`);
    return lines;
  }

  return lines;
}

function extractHookEventData(value: unknown): HookEventData | null {
  return isRecord(value) ? value as HookEventData : null;
}

function formatExtraData(value: unknown): string | null {
  if (value == null) return null;
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

export default AcpSystemEventCard;
