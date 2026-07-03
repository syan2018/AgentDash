import type { BackboneEvent } from "../../../generated/backbone-protocol";
import { extractPlatformEventData, extractPlatformEventType, isRecord } from "./platformEvent";

export type PlatformFeedBoundary = "hard" | "soft" | "neutral";
export type NotificationVisibility = "renderable" | "all";

export interface PlatformEventPolicyOptions {
  includeVerboseEvents?: boolean;
}

export interface PlatformEventPolicy {
  eventType: string | null;
  isTaskEvent: boolean;
  isRenderableSystemEvent: boolean;
  isRenderablePlatformEvent: boolean;
  feedBoundary: PlatformFeedBoundary;
  notificationVisibility: NotificationVisibility;
}

const RENDERABLE_SYSTEM_EVENT_TYPES = new Set<string>([
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
  "workspace_module_presented",
  "workspace_module_present_failed",
  "context_frame",
  "session_branch_forked",
  "session_rewound",
  "session_rebuilt",
  "turn_discarded",
]);

const VERBOSE_SYSTEM_EVENT_TYPES = new Set<string>([
  "provider_attempt_status",
  "provider_retry",
  "provider_status",
]);

const SILENT_HOOK_DECISIONS = new Set<string>([
  "stop",
  "terminal_observed",
  "observed",
  "refresh_requested",
  "allow",
  "effects_applied",
  "noop",
  "notified",
  "context_injected",
  "steering_injected",
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

  const code =
    typeof data.code === "string"
      ? data.code
      : typeof data.event_type === "string"
        ? data.event_type
        : null;
  const decision = extractHookDecision(code);
  if (!decision) return true;
  if (decision === "context_injected" || decision === "steering_injected") {
    return Array.isArray(data.injections) && data.injections.length > 0;
  }
  if (!SILENT_HOOK_DECISIONS.has(decision)) return true;

  if (typeof data.block_reason === "string" && data.block_reason.trim().length > 0) return true;
  if (data.completion != null) return true;
  if (Array.isArray(data.injections) && data.injections.length > 0) return true;
  if (Array.isArray(data.diagnostics) && data.diagnostics.some(hasMeaningfulHookDiagnostic)) {
    return true;
  }

  return false;
}

function isContextFrameEvent(event: BackboneEvent): boolean {
  return (
    event.type === "platform" &&
    event.payload.kind === "session_meta_update" &&
    event.payload.data.key === "context_frame"
  );
}

export function getPlatformEventPolicy(
  event: BackboneEvent,
  options: PlatformEventPolicyOptions = {},
): PlatformEventPolicy {
  const eventType = extractPlatformEventType(event);
  const isTaskEvent = typeof eventType === "string" && eventType.startsWith("task_");
  let isRenderableSystemEvent = false;

  if (event.type === "platform" && eventType && RENDERABLE_SYSTEM_EVENT_TYPES.has(eventType)) {
    if (eventType === "hook_event") {
      isRenderableSystemEvent = isSignificantHookEvent(extractPlatformEventData(event));
    } else {
      isRenderableSystemEvent = true;
    }
  }
  if (
    event.type === "platform" &&
    eventType &&
    options.includeVerboseEvents === true &&
    VERBOSE_SYSTEM_EVENT_TYPES.has(eventType)
  ) {
    isRenderableSystemEvent = true;
  }

  const isRenderablePlatformEvent = isTaskEvent || isRenderableSystemEvent;
  const feedBoundary: PlatformFeedBoundary = isContextFrameEvent(event)
    ? "hard"
    : isRenderablePlatformEvent
      ? "hard"
      : "neutral";

  return {
    eventType,
    isTaskEvent,
    isRenderableSystemEvent,
    isRenderablePlatformEvent,
    feedBoundary,
    notificationVisibility: isRenderableSystemEvent ? "renderable" : "all",
  };
}

export function isTaskEventUpdate(event: BackboneEvent): boolean {
  return getPlatformEventPolicy(event).isTaskEvent;
}

export function isRenderableSystemEventUpdate(
  event: BackboneEvent,
  options?: PlatformEventPolicyOptions,
): boolean {
  return getPlatformEventPolicy(event, options).isRenderableSystemEvent;
}

export function isRenderablePlatformEvent(
  event: BackboneEvent,
  options?: PlatformEventPolicyOptions,
): boolean {
  return getPlatformEventPolicy(event, options).isRenderablePlatformEvent;
}

export function shouldNotifyRenderableSystemEvent(event: BackboneEvent): boolean {
  return getPlatformEventPolicy(event).isRenderableSystemEvent;
}
