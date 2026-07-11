import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { SessionDisplayEntry } from "./types";
import type { TurnSegment } from "./useSessionFeed";

export interface RoundActionModel {
  copyLastAgentReply: {
    text: string;
    enabled: boolean;
  };
}

type AgentMessageEvent = Extract<BackboneEvent, { type: "agent_message_delta" }>;
type AgentMessageDisplayEntry = SessionDisplayEntry & { event: AgentMessageEvent };

function isAgentMessageEntry(value: unknown): value is AgentMessageDisplayEntry {
  return Boolean(
    value
      && typeof value === "object"
      && "event" in value
      && (value as SessionDisplayEntry).event.type === "agent_message_delta",
  );
}

export function lastAgentReplyText(segment: TurnSegment): string {
  const output = segment.finalOutput;
  if (!isAgentMessageEntry(output)) return "";
  return (output.accumulatedText ?? output.event.payload.delta ?? "").trim();
}

export function buildRoundActionModel(segment: TurnSegment): RoundActionModel {
  const text = lastAgentReplyText(segment);
  return {
    copyLastAgentReply: { text, enabled: text.length > 0 },
  };
}
