import type { SessionMessageRefDto } from "../../../generated/agent-run-mailbox-contracts";
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

export function lastAgentReplyText(segment: AgentRunRuntimeTurnSegment): string {
  const output = segment.finalOutput;
  if (output?.presentation.body.kind !== "agent_message") return "";
  return output.presentation.body.content
    .map((block) => {
      switch (block.kind) {
        case "text":
          return block.text;
        case "image":
          return block.source;
        case "local_resource":
          return block.path;
        case "resource_link":
          return block.uri;
        case "skill_reference":
          return block.name;
        case "mention":
          return block.label;
        case "structured":
          return JSON.stringify(block.value);
      }
    })
    .join("\n")
    .trim();
}

export function forkPointRefFromFinalAgentReply(
  segment: AgentRunRuntimeTurnSegment,
): SessionMessageRefDto | undefined {
  // Canonical Runtime item identity does not contain the mailbox entry index
  // required by the legacy fork DTO. Fork remains unavailable until its
  // command contract accepts Runtime item identity directly.
  void segment;
  return undefined;
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
