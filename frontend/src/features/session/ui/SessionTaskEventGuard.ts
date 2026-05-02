import type { BackboneEvent } from "../../../generated/backbone-protocol";
import { extractPlatformEventType } from "../model/platformEvent";

export function isTaskEventUpdate(event: BackboneEvent): boolean {
  if (event.type !== "platform") return false;
  const eventType = extractPlatformEventType(event);
  return typeof eventType === "string" && eventType.startsWith("task_");
}
