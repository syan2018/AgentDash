import { describe, expect, it } from "vitest";
import type { AgentConversationFeedSnapshot } from "../../../generated/workflow-contracts";
import { agentRunConversationFeedEntries } from "./agentRunConversationFeed";

function feed(messages: AgentConversationFeedSnapshot["messages"]): AgentConversationFeedSnapshot {
  return {
    run_ref: { run_id: "run-1" },
    agent_ref: { run_id: "run-1", agent_id: "agent-1" },
    runtime_session_ref: { runtime_session_id: "sess-1" },
    projection_kind: "canonical",
    projection_version: 1,
    head_event_seq: 42,
    message_count: messages.length,
    messages,
  };
}

describe("agentRunConversationFeedEntries", () => {
  it("keeps assistant tool calls and pairs them with tool results", () => {
    const entries = agentRunConversationFeedEntries(feed([
      {
        message_ref: { turn_id: "turn-1", entry_index: 0 },
        role: "assistant",
        text: "",
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
