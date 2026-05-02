import type { BackboneEvent } from "../../../generated/backbone-protocol";
import { extractPlatformEventType, extractPlatformEventData, isRecord } from "../model/platformEvent";

/**
 * 在会话流中显示的系统事件类型白名单。
 */
const VISIBLE_SYSTEM_EVENT_TYPES = new Set<string>([
  "executor_session_bound",
  "turn_interrupted",
  "hook_event",
  "hook_trace",
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

const SILENT_HOOK_DECISIONS = new Set<string>([
  "stop",
  "terminal_observed",
  "refresh_requested",
  "allow",
  "effects_applied",
  "noop",
  "notified",
  "baseline_initialized",
  "baseline_refreshed",
]);

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

function extractHookDecision(code: string | null | undefined): string | null {
  if (!code) return null;
  const parts = code.split(":");
  return parts.length >= 3 ? (parts[2] ?? null) : null;
}

function isSignificantHookEvent(data: Record<string, unknown> | null): boolean {
  if (!data) return true;

  const code = typeof data.code === "string" ? data.code : null;
  const decision = extractHookDecision(code);
  if (!decision) return true;
  if (!SILENT_HOOK_DECISIONS.has(decision)) return true;

  if (typeof data.block_reason === "string" && (data.block_reason as string).trim().length > 0) return true;
  if (data.completion != null) return true;
  if (Array.isArray(data.injections) && data.injections.length > 0) return true;
  if (Array.isArray(data.diagnostics) && data.diagnostics.some(hasMeaningfulHookDiagnostic)) {
    return true;
  }

  return false;
}

export function isRenderableSystemEventUpdate(event: BackboneEvent): boolean {
  if (event.type !== "platform") return false;
  const eventType = extractPlatformEventType(event);
  if (!eventType) return false;

  if (VISIBLE_SYSTEM_EVENT_TYPES.has(eventType)) {
    if (eventType === "hook_event" || eventType === "hook_trace") {
      const data = extractPlatformEventData(event);
      return isSignificantHookEvent(data);
    }
    return true;
  }

  return false;
}
