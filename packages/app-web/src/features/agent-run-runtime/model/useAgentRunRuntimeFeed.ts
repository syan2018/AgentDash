import { useMemo } from "react";

import type {
  ManagedRuntimeContentBlock,
  ManagedRuntimeInteraction,
  ManagedRuntimeItem,
} from "../../../generated/agent-runtime-contracts";
import type {
  ManagedRuntimeCommandAvailability,
  ManagedRuntimePlatformChange,
  ManagedRuntimeSnapshot,
} from "../../../generated/agent-runtime-validators";
import type {
  AgentDashThreadItem,
  BackboneEvent,
  UserInput,
} from "../../../generated/backbone-protocol";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import type {
  SessionDisplayEntry,
  SessionDisplayItem,
  TokenUsageInfo,
} from "../../session/model/types";
import { useManagedRuntimeFeed } from "./useManagedRuntimeFeed";

export interface UseAgentRunRuntimeFeedOptions {
  agentRunTarget?: AgentRunRuntimeTarget | null;
  enabled?: boolean;
}

export type AgentRunRuntimeTurnStatus =
  | "active"
  | "completed"
  | "failed"
  | "interrupted";

export interface AgentRunRuntimeTurnActivityStatus {
  kind: "connecting" | "reconnecting" | "retry_exhausted";
  label: string;
  phase?: string;
  attempt?: number;
  maxAttempts?: number;
}

export interface AgentRunRuntimeTurnSegment {
  turnId: string | null;
  status: AgentRunRuntimeTurnStatus;
  startedAtMs?: number;
  durationMs?: number;
  activity?: AgentRunRuntimeTurnActivityStatus;
  items: SessionDisplayItem[];
  finalOutput: SessionDisplayItem | null;
}

export interface UseAgentRunRuntimeFeedResult {
  snapshot: ManagedRuntimeSnapshot | null;
  changes: ManagedRuntimePlatformChange[];
  interactions: ManagedRuntimeInteraction[];
  commandAvailability: ManagedRuntimeSnapshot["command_availability"];
  displayItems: SessionDisplayItem[];
  turnSegments: AgentRunRuntimeTurnSegment[];
  rawEntries: SessionDisplayEntry[];
  isConnected: boolean;
  isLoading: boolean;
  isReceiving: boolean;
  error: Error | null;
  reconnect: () => void;
  close: () => void;
  streamingEntryId: string | null;
  tokenUsage: TokenUsageInfo | null;
}

function runtimeTimestampMs(value: bigint): number {
  if (value > 9_007_199_254_740_991n) {
    throw new RangeError("Managed Runtime timestamp exceeds JavaScript safe integer range");
  }
  return Number(value);
}

function blockText(block: ManagedRuntimeContentBlock): string {
  switch (block.kind) {
    case "text":
      return block.text;
    case "image":
      return `引用图片: ${block.source}`;
    case "resource":
      return `引用资源: ${block.uri}`;
    case "structured":
      return JSON.stringify(block.value);
  }
}

function userInput(block: ManagedRuntimeContentBlock): UserInput {
  if (block.kind === "image") {
    return { type: "image", url: block.source };
  }
  return {
    type: "text",
    text: blockText(block),
    text_elements: [],
  };
}

function itemStatus(
  item: ManagedRuntimeItem,
): "inProgress" | "completed" | "failed" {
  if (item.status === "accepted" || item.status === "running") return "inProgress";
  return item.status === "completed" ? "completed" : "failed";
}

function itemEvent(
  snapshot: ManagedRuntimeSnapshot,
  item: ManagedRuntimeItem,
): BackboneEvent {
  const content = item.content;
  const capturedAtMs = runtimeTimestampMs(snapshot.captured_at_ms);
  switch (content.kind) {
    case "user_input":
      return {
        type: "user_input_submitted",
        payload: {
          threadId: snapshot.thread_id,
          turnId: item.turn_id,
          itemId: item.id,
          submissionKind: "prompt",
          source: {
            namespace: "agentdash.runtime",
            kind: "managed_runtime_projection",
            actor: "user",
            displayLabelKey: "user",
          },
          content: content.content.map(userInput),
        },
      };
    case "agent_output":
      return {
        type: "agent_message_delta",
        payload: {
          delta: content.content.map(blockText).join("\n"),
          itemId: item.id,
          threadId: snapshot.thread_id,
          turnId: item.turn_id,
        },
      };
    case "tool_call":
    case "tool_result": {
      const threadItem: AgentDashThreadItem = {
        type: "dynamicToolCall",
        id: item.id,
        tool: content.name,
        arguments: content.kind === "tool_call" ? content.arguments : {},
        contentItems:
          content.kind === "tool_result"
            ? [{ type: "inputText", text: JSON.stringify(content.result) }]
            : null,
        status: itemStatus(item),
        success: item.status === "completed",
      };
      if (item.status === "accepted" || item.status === "running") {
        return {
          type: "item_started",
          payload: {
            item: threadItem,
            threadId: snapshot.thread_id,
            turnId: item.turn_id,
            startedAtMs: capturedAtMs,
          },
        };
      }
      return {
        type: "item_completed",
        payload: {
          item: threadItem,
          threadId: snapshot.thread_id,
          turnId: item.turn_id,
          completedAtMs: capturedAtMs,
        },
      };
    }
    case "context_compaction": {
      const threadItem: AgentDashThreadItem = {
        type: "contextCompaction",
        id: item.id,
      };
      if (item.status === "accepted" || item.status === "running") {
        return {
          type: "item_started",
          payload: {
            item: threadItem,
            threadId: snapshot.thread_id,
            turnId: item.turn_id,
            startedAtMs: capturedAtMs,
          },
        };
      }
      return {
        type: "item_completed",
        payload: {
          item: threadItem,
          threadId: snapshot.thread_id,
          turnId: item.turn_id,
          completedAtMs: capturedAtMs,
        },
      };
    }
    case "error":
      return {
        type: "error",
        payload: {
          error: { message: content.message, additionalDetails: content.code },
          threadId: snapshot.thread_id,
          turnId: item.turn_id,
          willRetry: false,
        },
      };
    case "extension":
      return {
        type: "agent_message_delta",
        payload: {
          delta: JSON.stringify(content.value),
          itemId: item.id,
          threadId: snapshot.thread_id,
          turnId: item.turn_id,
        },
      };
  }
}

function displayEntries(snapshot: ManagedRuntimeSnapshot): SessionDisplayEntry[] {
  const capturedAtMs = runtimeTimestampMs(snapshot.captured_at_ms);
  return snapshot.items.map((item, index) => {
    const event = itemEvent(snapshot, item);
    const accumulatedText =
      event.type === "agent_message_delta" ? event.payload.delta : undefined;
    return {
      id: item.id,
      sessionId: snapshot.thread_id,
      timestamp: capturedAtMs,
      eventSeq: index + 1,
      timelineOrder: { kind: "durable", seq: index + 1 },
      itemFreshness:
        item.status === "accepted" || item.status === "running"
          ? "started"
          : "completed",
      event,
      turnId: item.turn_id,
      accumulatedText,
      isStreaming: item.status === "running",
    };
  });
}

function turnStatus(
  status: ManagedRuntimeSnapshot["turns"][number]["status"],
): AgentRunRuntimeTurnStatus {
  if (status === "failed" || status === "lost") return "failed";
  if (status === "interrupted") return "interrupted";
  return status === "completed" ? "completed" : "active";
}

function turnSegments(
  snapshot: ManagedRuntimeSnapshot,
  entries: SessionDisplayEntry[],
): AgentRunRuntimeTurnSegment[] {
  const byId = new Map(entries.map((entry) => [entry.id, entry]));
  return snapshot.turns.map((turn) => {
    const items = turn.item_ids
      .map((id) => byId.get(id))
      .filter((entry): entry is SessionDisplayEntry => entry !== undefined);
    const finalOutput =
      [...items]
        .reverse()
        .find((entry) => entry.event.type === "agent_message_delta") ?? null;
    return {
      turnId: turn.id,
      status: turnStatus(turn.status),
      items,
      finalOutput,
    };
  });
}

export interface AgentRunRuntimeProjection {
  displayItems: SessionDisplayItem[];
  turnSegments: AgentRunRuntimeTurnSegment[];
  rawEntries: SessionDisplayEntry[];
  interactions: ManagedRuntimeInteraction[];
}

export function projectAgentRunRuntimeSnapshot(
  snapshot: ManagedRuntimeSnapshot,
): AgentRunRuntimeProjection {
  const rawEntries = displayEntries(snapshot);
  return {
    displayItems: rawEntries,
    turnSegments: turnSegments(snapshot, rawEntries),
    rawEntries,
    interactions: snapshot.interactions,
  };
}

export function useAgentRunRuntimeFeed({
  agentRunTarget = null,
  enabled = true,
}: UseAgentRunRuntimeFeedOptions): UseAgentRunRuntimeFeedResult {
  const feed = useManagedRuntimeFeed({ agentRunTarget, enabled });
  const projection = useMemo(
    () => (feed.snapshot ? projectAgentRunRuntimeSnapshot(feed.snapshot) : null),
    [feed.snapshot],
  );
  const rawEntries = projection?.rawEntries ?? [];
  const isReceiving = feed.snapshot?.active_turn_id != null;

  return {
    snapshot: feed.snapshot,
    changes: feed.changes,
    interactions: projection?.interactions ?? [],
    commandAvailability: feed.snapshot?.command_availability ?? {},
    displayItems: projection?.displayItems ?? [],
    turnSegments: projection?.turnSegments ?? [],
    rawEntries,
    isConnected: feed.lifecycle === "connected",
    isLoading: feed.isLoading,
    isReceiving,
    error: feed.error,
    reconnect: feed.reconnect,
    close: feed.close,
    streamingEntryId:
      isReceiving && rawEntries.length > 0
        ? rawEntries[rawEntries.length - 1]?.id ?? null
        : null,
    tokenUsage: null,
  };
}

export function commandIsAvailable(
  availability: ManagedRuntimeCommandAvailability | undefined,
): boolean {
  return availability?.status === "available";
}
