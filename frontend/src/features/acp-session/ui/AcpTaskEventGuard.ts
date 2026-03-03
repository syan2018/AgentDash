import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { extractAgentDashMetaFromUpdate } from "../model/agentdashMeta";

export function isTaskEventUpdate(update: SessionUpdate): boolean {
  if (update.sessionUpdate !== "session_info_update") return false;
  const eventType = extractAgentDashMetaFromUpdate(update)?.event?.type;
  return typeof eventType === "string" && eventType.startsWith("task_");
}
