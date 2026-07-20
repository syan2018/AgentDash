import type { AgentLiveEvent } from "../../../generated/agent-service-api";
import type { ManagedRuntimeSnapshot } from "../../../generated/agent-runtime-validators";

type RuntimeItem = ManagedRuntimeSnapshot["items"][number];
type RuntimeTurn = ManagedRuntimeSnapshot["turns"][number];

function liveDigest(event: AgentLiveEvent, suffix: string): string {
  return `live:${event.sequence}:${suffix}`;
}

function appendText(
  item: RuntimeItem | undefined,
  event: AgentLiveEvent,
  kind: "agent_message" | "reasoning",
  delta: string,
): RuntimeItem {
  const id =
    kind === "agent_message"
      ? `agent-item:${event.item_id}`
      : `agent-item:${event.item_id}:reasoning`;
  const previousText =
    item?.presentation.body.kind === "agent_message"
      ? item.presentation.body.content
          .filter((block) => block.kind === "text")
          .map((block) => block.text)
          .join("")
      : item?.presentation.body.kind === "reasoning"
        ? item.presentation.body.content
            .filter((block) => block.kind === "text")
            .map((block) => block.text)
            .join("")
        : "";
  const body: RuntimeItem["presentation"]["body"] =
    kind === "agent_message"
      ? {
          kind,
          content: [{ kind: "text", text: `${previousText}${delta}` }],
          phase: null,
        }
      : {
          kind,
          summary: [],
          content: [{ kind: "text", text: `${previousText}${delta}` }],
        };
  return {
    id,
    turn_id: `agent-turn:${event.turn_id}`,
    status: "running",
    presentation: {
      body,
      started_at_ms: item?.presentation.started_at_ms ?? null,
      updated_at_ms: null,
      terminal: null,
      body_digest: liveDigest(event, "body"),
      presentation_digest: liveDigest(event, "presentation"),
    },
  };
}

function toolItem(
  item: RuntimeItem | undefined,
  event: AgentLiveEvent,
): RuntimeItem | null {
  const payload = event.payload;
  if (
    payload.kind !== "tool_call_requested" &&
    payload.kind !== "tool_call_completed"
  ) {
    return null;
  }
  const existing =
    item?.presentation.body.kind === "generic_tool_activity"
      ? item.presentation.body
      : null;
  const body =
    payload.kind === "tool_call_requested"
      ? {
          kind: "generic_tool_activity" as const,
          name: payload.name,
          arguments: payload.arguments,
          result: null,
          progress: [],
        }
      : {
          kind: "generic_tool_activity" as const,
          name: existing?.name ?? payload.call_id,
          arguments: existing?.arguments ?? {},
          result: {
            content: payload.content,
            is_error: payload.is_error,
          },
          progress: existing?.progress ?? [],
        };
  return {
    id: `agent-live-tool:${payload.call_id}`,
    turn_id: `agent-turn:${event.turn_id}`,
    status: payload.kind === "tool_call_completed" ? "completed" : "running",
    presentation: {
      body,
      started_at_ms: item?.presentation.started_at_ms ?? null,
      updated_at_ms: null,
      terminal: null,
      body_digest: liveDigest(event, "tool-body"),
      presentation_digest: liveDigest(event, "tool-presentation"),
    },
  };
}

/**
 * Folds process-local Agent deltas into a disposable UI overlay.
 *
 * The returned snapshot is never sent back to the server and never becomes command evidence.
 * Reconnect and terminal boundaries replace it from Complete Agent `read`.
 */
export function applyAgentLiveEvent(
  snapshot: ManagedRuntimeSnapshot,
  event: AgentLiveEvent,
): ManagedRuntimeSnapshot {
  const turnId = `agent-turn:${event.turn_id}`;
  const itemId =
    event.payload.kind === "reasoning_delta"
      ? `agent-item:${event.item_id}:reasoning`
      : event.payload.kind === "tool_call_requested" ||
          event.payload.kind === "tool_call_completed"
        ? `agent-live-tool:${event.payload.call_id}`
        : `agent-item:${event.item_id}`;
  const previous = snapshot.items.find((item) => item.id === itemId);
  const nextItem =
    event.payload.kind === "text_delta"
      ? appendText(previous, event, "agent_message", event.payload.delta)
      : event.payload.kind === "reasoning_delta"
        ? appendText(previous, event, "reasoning", event.payload.delta)
        : toolItem(previous, event);

  if (!nextItem) {
    return snapshot;
  }

  const existingTurn = snapshot.turns.find((turn) => turn.id === turnId);
  const nextTurn: RuntimeTurn = {
    id: turnId,
    source_turn_id: event.turn_id,
    status: "running",
    item_ids: existingTurn?.item_ids.includes(nextItem.id)
      ? existingTurn.item_ids
      : [...(existingTurn?.item_ids ?? []), nextItem.id],
  };
  return {
    ...snapshot,
    lifecycle: "active",
    active_turn_id: turnId,
    turns: existingTurn
      ? snapshot.turns.map((turn) => (turn.id === turnId ? nextTurn : turn))
      : [...snapshot.turns, nextTurn],
    items: previous
      ? snapshot.items.map((item) => (item.id === nextItem.id ? nextItem : item))
      : [...snapshot.items, nextItem],
  };
}
