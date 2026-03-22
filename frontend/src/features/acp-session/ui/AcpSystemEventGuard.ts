import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { extractAgentDashMetaFromUpdate } from "../model/agentdashMeta";

const VISIBLE_SYSTEM_EVENT_TYPES = new Set<string>([
  "system_message",
  "error",
  "permission_denied",
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

export function isRenderableSystemEventUpdate(update: SessionUpdate): boolean {
  if (update.sessionUpdate !== "session_info_update") return false;

  const event = extractAgentDashMetaFromUpdate(update)?.event;
  if (!event) return false;

  if (typeof event.type === "string" && VISIBLE_SYSTEM_EVENT_TYPES.has(event.type)) {
    return true;
  }

  return typeof event.severity === "string" && VISIBLE_SYSTEM_EVENT_SEVERITIES.has(event.severity);
}
