import type { AgentConversationFeedSnapshot } from "../../../generated/workflow-contracts";
import type {
  BackboneEvent,
  DynamicToolCallOutputContentItem,
  UserInput,
} from "../../../generated/backbone-protocol";
import { createInitialStreamState, reduceStreamState } from "./sessionStreamReducer";
import type { SessionDisplayEntry, SessionEventEnvelope } from "./types";

type FeedMessage = AgentConversationFeedSnapshot["messages"][number];
type FeedContentPart = NonNullable<FeedMessage["content_parts"]>[number];
type FeedToolCall = NonNullable<FeedMessage["tool_calls"]>[number];

export interface AgentRunStreamIdentityTarget {
  runId: string;
  agentId: string;
}

export function agentRunSyntheticSessionId(target: AgentRunStreamIdentityTarget): string {
  return `agentrun:${target.runId}:${target.agentId}`;
}

function agentRunThreadId(feed: AgentConversationFeedSnapshot): string {
  return agentRunSyntheticSessionId({
    runId: feed.run_ref.run_id,
    agentId: feed.agent_ref.agent_id,
  });
}

export function normalizeAgentRunSessionEventIdentity(
  event: SessionEventEnvelope,
  target: AgentRunStreamIdentityTarget,
): SessionEventEnvelope {
  const sessionId = agentRunSyntheticSessionId(target);
  if (event.session_id === sessionId && event.notification.sessionId === sessionId) {
    return event;
  }
  return {
    ...event,
    session_id: sessionId,
    notification: {
      ...event.notification,
      sessionId,
    },
  };
}

function messageTimestamp(message: FeedMessage): number {
  return message.timestamp_ms == null ? Date.now() : Number(message.timestamp_ms);
}

function messageItemId(message: FeedMessage, suffix: string): string {
  return `projection:${message.role}:${message.message_ref.turn_id}:${message.message_ref.entry_index}:${suffix}`;
}

function messageContentParts(message: FeedMessage): FeedContentPart[] {
  return message.content_parts ?? [];
}

function messageToolCalls(message: FeedMessage): FeedToolCall[] {
  return message.tool_calls ?? [];
}

function imageDataUrl(part: Extract<FeedContentPart, { type: "image" }>): string {
  if (part.data.startsWith("data:")) {
    return part.data;
  }
  return `data:${part.mime_type};base64,${part.data}`;
}

function userInputs(message: FeedMessage): UserInput[] {
  const parts = messageContentParts(message);
  if (parts.length === 0) {
    return [{ type: "text", text: message.text, text_elements: [] }];
  }
  return parts.flatMap((part): UserInput[] => {
    if (part.type === "image") {
      return [{ type: "image", url: imageDataUrl(part) }];
    }
    if (part.type === "reasoning") {
      return [{ type: "text", text: part.text, text_elements: [] }];
    }
    return [{ type: "text", text: part.text, text_elements: [] }];
  });
}

function textParts(message: FeedMessage): string[] {
  const parts = messageContentParts(message)
    .filter((part) => part.type === "text")
    .map((part) => part.text.trim())
    .filter((text) => text.length > 0);
  if (parts.length > 0) return parts;
  const fallback = message.text.trim();
  return fallback.length > 0 ? [fallback] : [];
}

function reasoningParts(message: FeedMessage): string[] {
  return messageContentParts(message)
    .filter((part) => part.type === "reasoning")
    .map((part) => part.text.trim())
    .filter((text) => text.length > 0);
}

function userMessageEvent(message: FeedMessage, threadId: string): BackboneEvent | null {
  const content = userInputs(message);
  if (content.length === 0) return null;
  return {
    type: "user_input_submitted",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      itemId: messageItemId(message, "user"),
      submissionKind: "prompt",
      content,
    },
  };
}

function assistantMessageEvent(message: FeedMessage, threadId: string): BackboneEvent | null {
  const text = textParts(message).join("\n");
  if (!text) return null;
  return {
    type: "item_completed",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      completedAtMs: messageTimestamp(message),
      item: {
        type: "agentMessage",
        id: messageItemId(message, "msg"),
        text,
        phase: null,
        memoryCitation: null,
      },
    },
  };
}

function reasoningEvent(message: FeedMessage, threadId: string): BackboneEvent | null {
  const content = reasoningParts(message);
  if (content.length === 0) return null;
  return {
    type: "item_completed",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      completedAtMs: messageTimestamp(message),
      item: {
        type: "reasoning",
        id: messageItemId(message, "reasoning"),
        summary: [],
        content,
      },
    },
  };
}

function compactionSummaryEvent(message: FeedMessage, threadId: string): BackboneEvent | null {
  const text = message.text.trim();
  if (!text) return null;
  return {
    type: "item_completed",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      completedAtMs: messageTimestamp(message),
      item: {
        type: "agentMessage",
        id: messageItemId(message, "compaction"),
        text,
        phase: null,
        memoryCitation: null,
      },
    },
  };
}

function toolCallItemId(message: FeedMessage, toolCallId: string): string {
  return `projection:tool:${message.message_ref.turn_id}:${toolCallId}`;
}

function toolCallOutputItems(result: FeedMessage | undefined): DynamicToolCallOutputContentItem[] | null {
  if (!result) return null;
  const contentItems = messageContentParts(result).flatMap((part): DynamicToolCallOutputContentItem[] => {
    if (part.type === "image") {
      return [{ type: "inputImage", imageUrl: imageDataUrl(part) }];
    }
    const text = part.text.trim();
    return text ? [{ type: "inputText", text }] : [];
  });
  if (contentItems.length > 0) return contentItems;
  const text = result.text.trim();
  return text ? [{ type: "inputText", text }] : null;
}

function toolCallEvent(
  message: FeedMessage,
  toolCall: FeedToolCall,
  result: FeedMessage | undefined,
  threadId: string,
): BackboneEvent {
  const toolResult = result?.tool_result;
  return {
    type: "item_completed",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      completedAtMs: messageTimestamp(result ?? message),
      item: {
        type: "dynamicToolCall",
        id: toolCallItemId(message, toolCall.id),
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

function orphanToolResultEvent(message: FeedMessage, threadId: string): BackboneEvent | null {
  const toolResult = message.tool_result;
  if (!toolResult) return null;
  return {
    type: "item_completed",
    payload: {
      threadId,
      turnId: message.message_ref.turn_id,
      completedAtMs: messageTimestamp(message),
      item: {
        type: "dynamicToolCall",
        id: toolCallItemId(message, toolResult.tool_call_id),
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
  if (message.role === "user") {
    const event = userMessageEvent(message, threadId);
    if (event) events.push(event);
    return events;
  }
  if (message.role === "assistant") {
    const reasoning = reasoningEvent(message, threadId);
    if (reasoning) events.push(reasoning);
    const text = assistantMessageEvent(message, threadId);
    if (text) events.push(text);
    for (const toolCall of messageToolCalls(message)) {
      events.push(toolCallEvent(message, toolCall, toolResults.get(toolCall.id), threadId));
    }
    return events;
  }
  if (message.role === "tool_result" && message.tool_result && !representedToolCallIds.has(message.tool_result.tool_call_id)) {
    const event = orphanToolResultEvent(message, threadId);
    if (event) events.push(event);
    return events;
  }
  if (message.role === "compaction_summary") {
    const event = compactionSummaryEvent(message, threadId);
    if (event) events.push(event);
  }
  return events;
}

function sessionUpdateType(event: BackboneEvent): string {
  return event.type;
}

function syntheticEnvelope(
  message: FeedMessage,
  event: BackboneEvent,
  threadId: string,
  eventSeq: number,
): SessionEventEnvelope {
  const timestamp = messageTimestamp(message);
  return {
    session_id: threadId,
    event_seq: eventSeq,
    occurred_at_ms: timestamp,
    committed_at_ms: timestamp,
    session_update_type: sessionUpdateType(event),
    turn_id: message.message_ref.turn_id,
    entry_index: message.message_ref.entry_index,
    tool_call_id: event.type === "item_completed" && event.payload.item.type === "dynamicToolCall"
      ? event.payload.item.id
      : undefined,
    notification: {
      sessionId: threadId,
      source: {
        connectorId: "agent-run-conversation-feed",
        connectorType: "projection",
        executorId: null,
      },
      trace: {
        turnId: message.message_ref.turn_id,
        entryIndex: message.message_ref.entry_index,
      },
      observedAt: new Date(timestamp).toISOString(),
      event,
    },
  };
}

export function agentRunConversationFeedEvents(
  feed: AgentConversationFeedSnapshot | null,
): SessionEventEnvelope[] {
  if (!feed) return [];
  const threadId = agentRunThreadId(feed);
  const toolResults = toolResultByCallId(feed.messages);
  const representedToolCallIdsValue = representedToolCallIds(feed.messages);
  const pending: Array<{ message: FeedMessage; event: BackboneEvent }> = [];
  for (const message of feed.messages) {
    for (const event of messageEvents(message, threadId, toolResults, representedToolCallIdsValue)) {
      pending.push({ message, event });
    }
  }
  const headSeq = Number(feed.head_event_seq);
  const startSeq = Math.max(1, headSeq - pending.length + 1);
  return pending.map(({ message, event }, index) => syntheticEnvelope(message, event, threadId, startSeq + index));
}

export function agentRunConversationFeedEntries(
  feed: AgentConversationFeedSnapshot | null,
): SessionDisplayEntry[] {
  const events = agentRunConversationFeedEvents(feed);
  const reduced = reduceStreamState(createInitialStreamState([]), events);
  return reduced.entries.map((entry) => ({
    ...entry,
    isStreaming: false,
    projectedTranscriptStable: true,
  }));
}
