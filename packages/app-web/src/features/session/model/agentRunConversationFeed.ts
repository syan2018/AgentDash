import type { AgentConversationFeedSnapshot } from "../../../generated/workflow-contracts";
import type { BackboneEvent, DynamicToolCallOutputContentItem } from "../../../generated/backbone-protocol";
import type { SessionDisplayEntry } from "./types";

type FeedMessage = AgentConversationFeedSnapshot["messages"][number];
type FeedToolCall = NonNullable<FeedMessage["tool_calls"]>[number];

function messageEventSeq(feed: AgentConversationFeedSnapshot): number {
  return Number(feed.head_event_seq);
}

function messageTimestamp(message: FeedMessage): number {
  return message.timestamp_ms == null ? Date.now() : Number(message.timestamp_ms);
}

function messageItemId(message: FeedMessage): string {
  return `projection:${message.role}:${message.message_ref.turn_id}:${message.message_ref.entry_index}`;
}

function messageToolCalls(message: FeedMessage): FeedToolCall[] {
  return message.tool_calls ?? [];
}

function userMessageEvent(
  message: FeedMessage,
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
  message: FeedMessage,
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
  message: FeedMessage,
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

function toolCallItemId(message: FeedMessage, toolCallId: string): string {
  return `projection:tool:${message.message_ref.turn_id}:${toolCallId}`;
}

function toolCallOutputItems(result: FeedMessage | undefined): DynamicToolCallOutputContentItem[] | null {
  const text = result?.text.trim();
  if (!text) return null;
  return [{ type: "inputText", text }];
}

function toolCallEvent(
  message: FeedMessage,
  toolCall: FeedToolCall,
  result: FeedMessage | undefined,
  threadId: string,
): BackboneEvent {
  const itemId = toolCallItemId(message, toolCall.id);
  const toolResult = result?.tool_result;
  return {
    type: "item_completed",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      completedAtMs: messageTimestamp(result ?? message),
      item: {
        type: "dynamicToolCall",
        id: itemId,
        namespace: null,
        tool: toolCall.name,
        arguments: toolCall.arguments,
        status: toolResult?.is_error ? "failed" : "completed",
        contentItems: toolCallOutputItems(result),
        success: toolResult ? !toolResult.is_error : null,
        durationMs: null,
      },
    },
  };
}

function orphanToolResultEvent(
  message: FeedMessage,
  threadId: string,
): BackboneEvent | null {
  const toolResult = message.tool_result;
  if (!toolResult) return null;
  const itemId = toolCallItemId(message, toolResult.tool_call_id);
  return {
    type: "item_completed",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      completedAtMs: messageTimestamp(message),
      item: {
        type: "dynamicToolCall",
        id: itemId,
        namespace: null,
        tool: toolResult.tool_name ?? "tool",
        arguments: toolResult.details ?? null,
        status: toolResult.is_error ? "failed" : "completed",
        contentItems: toolCallOutputItems(message),
        success: !toolResult.is_error,
        durationMs: null,
      },
    },
  };
}

function textMessageEvent(
  message: FeedMessage,
  threadId: string,
): BackboneEvent | null {
  if (message.text.trim().length === 0) {
    return null;
  }
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

function toolResultByCallId(messages: FeedMessage[]): Map<string, FeedMessage> {
  const results = new Map<string, FeedMessage>();
  for (const message of messages) {
    const toolResult = message.tool_result;
    if (!toolResult) continue;
    results.set(toolResult.tool_call_id, message);
  }
  return results;
}

function representedToolCallIds(messages: FeedMessage[]): Set<string> {
  const ids = new Set<string>();
  for (const message of messages) {
    for (const toolCall of messageToolCalls(message)) {
      ids.add(toolCall.id);
    }
  }
  return ids;
}

function messageEvents(
  message: FeedMessage,
  threadId: string,
  toolResults: Map<string, FeedMessage>,
  representedToolCallIds: Set<string>,
): BackboneEvent[] {
  const events: BackboneEvent[] = [];
  const textEvent = textMessageEvent(message, threadId);
  if (textEvent) {
    events.push(textEvent);
  }
  if (message.role === "assistant") {
    for (const toolCall of messageToolCalls(message)) {
      events.push(toolCallEvent(message, toolCall, toolResults.get(toolCall.id), threadId));
    }
  }
  if (message.role === "tool_result" && message.tool_result && !representedToolCallIds.has(message.tool_result.tool_call_id)) {
    const orphanEvent = orphanToolResultEvent(message, threadId);
    if (orphanEvent) {
      events.push(orphanEvent);
    }
  }
  return events;
}

export function agentRunConversationFeedEntries(
  feed: AgentConversationFeedSnapshot | null,
): SessionDisplayEntry[] {
  if (!feed) return [];
  const runtimeSessionId = feed.runtime_session_ref?.runtime_session_id ?? "";
  const eventSeq = messageEventSeq(feed);
  const toolResults = toolResultByCallId(feed.messages);
  const representedToolCallIdsValue = representedToolCallIds(feed.messages);
  return feed.messages.flatMap((message): SessionDisplayEntry[] => {
    return messageEvents(message, runtimeSessionId, toolResults, representedToolCallIdsValue).map((event, index) => ({
      id: index === 0 ? messageItemId(message) : `${messageItemId(message)}:${index}`,
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
    }));
  });
}
