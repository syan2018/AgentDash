import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { extractAgentDashMetaFromUpdate, isRecord } from "../model/agentdashMeta";

/**
 * 在会话流中显示的系统事件类型白名单。
 *
 * turn_started / turn_completed 静默——会话是否在运行已通过发送按钮状态表达，
 * 不需要在对话流中额外占位。
 *
 * executor_session_bound 保留——用户需要知道会话已和执行器绑定。
 */
const VISIBLE_SYSTEM_EVENT_TYPES = new Set<string>([
  "executor_session_bound",
  "turn_interrupted",
  "hook_event",
  "system_message",
  "error",
  "permission_denied",
  "approval_requested",
  "approval_resolved",
  "hook_action_resolved",
  "turn_failed",
  "user_feedback",
  "user_answered_questions",
  "companion_dispatch_registered",
  "companion_result_available",
  "companion_result_returned",
]);

const VISIBLE_SYSTEM_EVENT_SEVERITIES = new Set<string>([
  "error",
  "warning",
]);

/**
 * 后端已在 should_emit_hook_trace_event 过滤大多数静默决策，
 * 但前端做第二道防线，防止后端逻辑松动导致噪音流入 UI。
 *
 * 这些决策即使到达前端也应静默：完全无实际效果、或已由其他 UI 元素表达。
 */
const SILENT_HOOK_DECISIONS = new Set<string>([
  "stop",              // 自然结束放行——turn 结束已由消息列表末尾表达
  "terminal_observed", // 纯技术终态记录，无用户感知价值
  "refresh_requested", // 内部快照刷新机制，用户无需感知
]);

/**
 * 从 event.code 中提取 hook decision。
 * code 格式：hook:{trigger}:{decision}
 */
function extractHookDecision(code: string | null | undefined): string | null {
  if (!code) return null;
  const parts = code.split(":");
  // hook:{trigger}:{decision} → 第3段
  return parts.length >= 3 ? (parts[2] ?? null) : null;
}

/**
 * 判断一个 hook_event 是否值得在会话流中显示。
 *
 * 规则：
 * - 静默决策集合中的 decision → 不显示
 * - 但若携带 block_reason 或 completion → 仍显示（对用户有意义）
 * - 其余所有 hook_event → 显示
 */
function isSignificantHookEvent(event: {
  code?: string | null;
  severity?: string | null;
  data?: unknown;
}): boolean {
  const decision = extractHookDecision(event.code);

  // 无法解析 decision 时安全放行
  if (!decision) return true;

  // 不在静默集合 → 直接显示
  if (!SILENT_HOOK_DECISIONS.has(decision)) return true;

  // 静默决策但携带重要附加信息 → 仍显示
  const data = event.data;
  if (isRecord(data)) {
    if (data.block_reason) return true;
    if (data.completion != null) return true;
  }

  return false;
}

export function isRenderableSystemEventUpdate(update: SessionUpdate): boolean {
  if (update.sessionUpdate !== "session_info_update") return false;

  const event = extractAgentDashMetaFromUpdate(update)?.event;
  if (!event) return false;

  if (typeof event.type === "string" && VISIBLE_SYSTEM_EVENT_TYPES.has(event.type)) {
    // hook_event 做额外的 decision 级过滤
    if (event.type === "hook_event") {
      return isSignificantHookEvent(event);
    }
    return true;
  }

  // 未在类型白名单中的事件，按 severity 兜底（error/warning 始终可见）
  return typeof event.severity === "string" && VISIBLE_SYSTEM_EVENT_SEVERITIES.has(event.severity);
}
