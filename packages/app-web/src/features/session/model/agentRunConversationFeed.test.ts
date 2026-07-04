import { describe, expect, it } from "vitest";
import type { AgentConversationFeedSnapshot } from "../../../generated/workflow-contracts";
import type { SessionEventEnvelope } from "./types";
import {
  agentRunConversationFeedEntries,
  agentRunConversationFeedEvents,
  normalizeAgentRunStreamEventIdentity,
} from "./agentRunConversationFeed";

function feed(messages: AgentConversationFeedSnapshot["messages"]): AgentConversationFeedSnapshot {
  return {
    run_ref: { run_id: "run-1" },
    agent_ref: { run_id: "run-1", agent_id: "agent-1" },
    runtime_session_ref: { runtime_session_id: "sess-1" },
    projection_kind: "canonical",
    projection_version: 1,
    head_event_seq: 42,
    runtime_replay_start_seq: 0,
    message_count: messages.length,
    messages,
  };
}

function runtimeEvent(sessionId: string): SessionEventEnvelope {
  return {
    session_id: sessionId,
    event_seq: 1,
    occurred_at_ms: 1,
    committed_at_ms: 1,
    session_update_type: "agent_message_delta",
    notification: {
      sessionId,
      source: {
        connectorId: "test",
        connectorType: "unit",
        executorId: null,
      },
      trace: {
        turnId: "turn-1",
        entryIndex: 0,
      },
      observedAt: "2026-07-04T00:00:00.000Z",
      event: {
        type: "agent_message_delta",
        payload: {
          threadId: "thread-1",
          turnId: "turn-1",
          itemId: "item-1",
          delta: "hello",
        },
      },
    },
  };
}

describe("agentRunConversationFeedEntries", () => {
  it("hydrates assistant text through the same completed agent message reducer path", () => {
    const entries = agentRunConversationFeedEntries(feed([
      {
        message_ref: { turn_id: "turn-1", entry_index: 0 },
        role: "assistant",
        text: "final answer",
        content_parts: [{ type: "text", text: "final answer" }],
        origin: "agent",
        synthetic: false,
        projection_kind: "canonical",
        timestamp_ms: 1000,
      },
    ]));

    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      accumulatedText: "final answer",
      isStreaming: false,
      projectedTranscriptStable: true,
    });
    expect(entries[0]!.event.type).toBe("agent_message_delta");
  });

  it("hydrates reasoning as thinking instead of merging it into assistant text", () => {
    const entries = agentRunConversationFeedEntries(feed([
      {
        message_ref: { turn_id: "turn-1", entry_index: 0 },
        role: "assistant",
        text: "thought\nanswer",
        content_parts: [
          { type: "reasoning", text: "thought" },
          { type: "text", text: "answer" },
        ],
        origin: "agent",
        synthetic: false,
        projection_kind: "canonical",
        timestamp_ms: 1000,
      },
    ]));

    expect(entries).toHaveLength(2);
    expect(entries.map((entry) => entry.event.type)).toEqual([
      "reasoning_text_delta",
      "agent_message_delta",
    ]);
    expect(entries.map((entry) => entry.accumulatedText)).toEqual(["thought", "answer"]);
    expect(entries.every((entry) => entry.isStreaming === false)).toBe(true);
  });

  it("keeps user image content as image input blocks", () => {
    const events = agentRunConversationFeedEvents(feed([
      {
        message_ref: { turn_id: "turn-1", entry_index: 0 },
        role: "user",
        text: "see image",
        content_parts: [
          { type: "text", text: "see image" },
          { type: "image", mime_type: "image/png", data: "abc123" },
        ],
        origin: "event",
        synthetic: false,
        projection_kind: "canonical",
        timestamp_ms: 1000,
      },
    ]));

    expect(events).toHaveLength(1);
    const event = events[0]!.notification.event;
    expect(event.type).toBe("user_input_submitted");
    if (event.type !== "user_input_submitted") return;
    expect(event.payload.content).toEqual([
      { type: "text", text: "see image", text_elements: [] },
      { type: "image", url: "data:image/png;base64,abc123" },
    ]);
  });

  it("uses AgentRun scoped identity for synthetic thread/session ids", () => {
    const events = agentRunConversationFeedEvents(feed([
      {
        message_ref: { turn_id: "turn-1", entry_index: 0 },
        role: "user",
        text: "hello",
        content_parts: [{ type: "text", text: "hello" }],
        origin: "event",
        synthetic: false,
        projection_kind: "canonical",
        timestamp_ms: 1000,
      },
    ]));

    expect(events).toHaveLength(1);
    expect(events[0]?.session_id).toBe("agentrun:run-1:agent-1");
    expect(events[0]?.session_id).not.toBe("sess-1");
    expect(events[0]?.notification.sessionId).toBe("agentrun:run-1:agent-1");
    const event = events[0]?.notification.event;
    expect(event?.type).toBe("user_input_submitted");
    if (event?.type !== "user_input_submitted") return;
    expect(event.payload.threadId).toBe("agentrun:run-1:agent-1");
  });

  it("normalizes AgentRun scoped runtime events to the synthetic stream identity", () => {
    const event = runtimeEvent("runtime-session-1");
    const normalized = normalizeAgentRunStreamEventIdentity(event, {
      runId: "run-1",
      agentId: "agent-1",
    });

    expect(normalized.session_id).toBe("agentrun:run-1:agent-1");
    expect(normalized.notification.sessionId).toBe("agentrun:run-1:agent-1");
    expect(event.session_id).toBe("runtime-session-1");
  });

  it("keeps already normalized AgentRun runtime events by reference", () => {
    const event = runtimeEvent("agentrun:run-1:agent-1");
    const normalized = normalizeAgentRunStreamEventIdentity(event, {
      runId: "run-1",
      agentId: "agent-1",
    });

    expect(normalized).toBe(event);
  });

  it("keeps assistant tool calls and pairs them with tool results", () => {
    const entries = agentRunConversationFeedEntries(feed([
      {
        message_ref: { turn_id: "turn-1", entry_index: 0 },
        role: "assistant",
        text: "",
        content_parts: [],
        tool_calls: [{
          id: "tool-1",
          name: "read",
          arguments: { path: "README.md" },
        }],
        origin: "agent",
        synthetic: false,
        projection_kind: "canonical",
        timestamp_ms: 1000,
      },
      {
        message_ref: { turn_id: "turn-1", entry_index: 1 },
        role: "tool_result",
        text: "hello from file",
        content_parts: [{ type: "text", text: "hello from file" }],
        tool_calls: [],
        tool_result: {
          tool_call_id: "tool-1",
          tool_name: "read",
          is_error: false,
        },
        origin: "agent",
        synthetic: false,
        projection_kind: "canonical",
        timestamp_ms: 1001,
      },
    ]));

    expect(entries).toHaveLength(1);
    const event = entries[0]!.event;
    expect(event.type).toBe("item_completed");
    if (event.type !== "item_completed") return;
    expect(event.payload.item).toMatchObject({
      type: "dynamicToolCall",
      id: "projection:tool:turn-1:tool-1",
      tool: "read",
      arguments: { path: "README.md" },
      status: "completed",
      success: true,
      contentItems: [{ type: "inputText", text: "hello from file" }],
    });
  });

  it("keeps orphan tool results instead of dropping them", () => {
    const entries = agentRunConversationFeedEntries(feed([
      {
        message_ref: { turn_id: "turn-1", entry_index: 2 },
        role: "tool_result",
        text: "late output",
        content_parts: [{ type: "text", text: "late output" }],
        tool_calls: [],
        tool_result: {
          tool_call_id: "tool-orphan",
          tool_name: "diagnose",
          is_error: true,
        },
        origin: "agent",
        synthetic: false,
        projection_kind: "canonical",
        timestamp_ms: 1002,
      },
    ]));

    expect(entries).toHaveLength(1);
    const event = entries[0]!.event;
    expect(event.type).toBe("item_completed");
    if (event.type !== "item_completed") return;
    expect(event.payload.item).toMatchObject({
      type: "dynamicToolCall",
      id: "projection:tool:turn-1:tool-orphan",
      tool: "diagnose",
      status: "failed",
      success: false,
      contentItems: [{ type: "inputText", text: "late output" }],
    });
  });
});
