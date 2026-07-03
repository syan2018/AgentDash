import type { AgentConversationFeedSnapshot } from "../../../generated/workflow-contracts";
import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { SessionDisplayEntry } from "./types";

function messageEventSeq(feed: AgentConversationFeedSnapshot): number {
  return Number(feed.head_event_seq);
}

function messageTimestamp(message: AgentConversationFeedSnapshot["messages"][number]): number {
  return message.timestamp_ms == null ? Date.now() : Number(message.timestamp_ms);
}

function messageItemId(message: AgentConversationFeedSnapshot["messages"][number]): string {
  return `projection:${message.role}:${message.message_ref.turn_id}:${message.message_ref.entry_index}`;
}

function userMessageEvent(
  message: AgentConversationFeedSnapshot["messages"][number],
  threadId: string,
): BackboneEvent {
  return {
    type: "user_input_submitted",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      itemId: messageItemId(message),
      submissionKind: "prompt",
      content: [{ type: "text", text: message.text, text_elements: [] }],
    },
  };
}

function assistantMessageEvent(
  message: AgentConversationFeedSnapshot["messages"][number],
  threadId: string,
): BackboneEvent {
  return {
    type: "agent_message_delta",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      itemId: messageItemId(message),
      delta: message.text,
    },
  };
}

function compactionSummaryEvent(
  message: AgentConversationFeedSnapshot["messages"][number],
  threadId: string,
): BackboneEvent {
  return {
    type: "agent_message_delta",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      itemId: messageItemId(message),
      delta: message.text,
    },
  };
}

function messageEvent(
  message: AgentConversationFeedSnapshot["messages"][number],
  threadId: string,
): BackboneEvent | null {
  if (message.role === "user") {
    return userMessageEvent(message, threadId);
  }
  if (message.role === "assistant") {
    return assistantMessageEvent(message, threadId);
  }
  if (message.role === "compaction_summary") {
    return compactionSummaryEvent(message, threadId);
  }
  return null;
}

export function agentRunConversationFeedEntries(
  feed: AgentConversationFeedSnapshot | null,
): SessionDisplayEntry[] {
  if (!feed) return [];
  const runtimeSessionId = feed.runtime_session_ref?.runtime_session_id ?? "";
  const eventSeq = messageEventSeq(feed);
  return feed.messages.flatMap((message): SessionDisplayEntry[] => {
    const event = messageEvent(message, runtimeSessionId);
    if (!event) return [];
    return [{
      id: messageItemId(message),
      sessionId: runtimeSessionId,
      timestamp: messageTimestamp(message),
      eventSeq,
      timelineOrder: { kind: "durable", seq: eventSeq },
      event,
      turnId: message.message_ref.turn_id,
      entryIndex: message.message_ref.entry_index,
      accumulatedText: message.text,
      isStreaming: false,
      projectedTranscriptStable: true,
    }];
  });
}
