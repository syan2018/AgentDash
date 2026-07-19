import type { SessionMessageRefDto } from "../../../generated/agent-run-mailbox-contracts";
import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { SessionDisplayEntry } from "./types";
import type { AgentRunRuntimeTurnSegment } from "../../agent-run-runtime";

export interface RoundActionModel {
  copyLastAgentReply: {
    text: string;
    enabled: boolean;
  };
  forkFromHere: {
    forkPointRef?: SessionMessageRefDto;
    enabled: boolean;
    disabledReason?: string;
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

export function lastAgentReplyText(segment: AgentRunRuntimeTurnSegment): string {
  const output = segment.finalOutput;
  if (!isAgentMessageEntry(output)) return "";
  return (output.accumulatedText ?? output.event.payload.delta ?? "").trim();
}

export function forkPointRefFromFinalAgentReply(
  segment: AgentRunRuntimeTurnSegment,
): SessionMessageRefDto | undefined {
  const output = segment.finalOutput;
  if (!isAgentMessageEntry(output)) return undefined;
  const turnId = output.turnId ?? segment.turnId;
  const entryIndex = output.entryIndex;
  if (!turnId || entryIndex == null) return undefined;
  return { turn_id: turnId, entry_index: entryIndex };
}

export function buildRoundActionModel(segment: AgentRunRuntimeTurnSegment): RoundActionModel {
  const text = lastAgentReplyText(segment);
  const forkPointRef = forkPointRefFromFinalAgentReply(segment);

  if (segment.status === "active") {
    return {
      copyLastAgentReply: { text, enabled: text.length > 0 },
      forkFromHere: {
        enabled: false,
        disabledReason: "当前轮次仍在运行，完成后才能 fork。",
      },
    };
  }

  if (segment.status !== "completed") {
    return {
      copyLastAgentReply: { text, enabled: text.length > 0 },
      forkFromHere: {
        enabled: false,
        disabledReason: "只有稳定完成的轮次可以 fork。",
      },
    };
  }

  if (!forkPointRef) {
    return {
      copyLastAgentReply: { text, enabled: text.length > 0 },
      forkFromHere: {
        enabled: false,
        disabledReason: "当前轮次缺少稳定 message ref。",
      },
    };
  }

  return {
    copyLastAgentReply: { text, enabled: text.length > 0 },
    forkFromHere: {
      forkPointRef,
      enabled: true,
    },
  };
}
