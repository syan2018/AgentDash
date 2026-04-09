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
  "companion_human_request",
  "companion_human_response",
  "companion_review_request",
  "canvas_presented",
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
  "allow",             // before_tool 放行——常规工具调用不需要在对话流占位
  "effects_applied",   // after_tool 效果记录——高频且通常无用户可感知内容
  "noop",              // 多个 trigger 的"无操作"决策——无实际效果
  "notified",          // after_compact 通知——compaction 发生已由摘要消息表达
  "baseline_initialized", // session_start baseline 初始化——一次性技术事件
  "baseline_refreshed",   // session_start baseline 刷新——一次性技术事件
]);

/**
 * 这些 diagnostics 是快照绑定层的背景信息，默认不应把 hook_event 提升为“可见事件”。
 * 例如 owner/session binding 已命中，这类信息更适合放在 runtime 面板而非会话流。
 */
const NON_SUBSTANTIVE_DIAGNOSTIC_CODES = new Set<string>([
  "session_binding_found",
  "active_workflow_resolved",
]);

function hasMeaningfulHookDiagnostic(value: unknown): boolean {
  if (!isRecord(value)) return false;

  const code = typeof value.code === "string" ? value.code : "";
  if (code && NON_SUBSTANTIVE_DIAGNOSTIC_CODES.has(code)) {
    return false;
  }

  const summary = typeof value.summary === "string" ? value.summary.trim() : "";
  const message = typeof value.message === "string" ? value.message.trim() : "";
  const detail = typeof value.detail === "string" ? value.detail.trim() : "";
  return summary.length > 0 || message.length > 0 || detail.length > 0 || code.length > 0;
}

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
    if (typeof data.block_reason === "string" && data.block_reason.trim().length > 0) return true;
    if (data.completion != null) return true;
    if (Array.isArray(data.injections) && data.injections.length > 0) return true;
    if (Array.isArray(data.diagnostics) && data.diagnostics.some(hasMeaningfulHookDiagnostic)) {
      return true;
    }
  }

  return false;
}

export function isRenderableSystemEventUpdate(update: SessionUpdate): boolean {
  if (update.sessionUpdate !== "session_info_update") return false;

  const event = extractAgentDashMetaFromUpdate(update)?.event;
  if (!event) return false;

  if (typeof event.type === "string" && VISIBLE_SYSTEM_EVENT_TYPES.has(event.type)) {
    if (event.type === "hook_event") {
      return isSignificantHookEvent(event);
    }
    return true;
  }

  return false;
}
