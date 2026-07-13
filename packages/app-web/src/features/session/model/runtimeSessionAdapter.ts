import type {
  RuntimeEventEnvelope,
  RuntimeSnapshot,
  RuntimeTokenUsage,
} from "../../../generated/agent-runtime-contracts";
import type {
  SessionPresentationEvent,
  SessionEventEnvelope,
} from "./types";
import type { JsonValue } from "../../../generated/common-contracts";

function numeric(value: bigint | number): number {
  return typeof value === "bigint" ? Number(value) : value;
}

function assertNever(value: never): never {
  throw new Error(`unhandled Runtime event: ${JSON.stringify(value)}`);
}

function jsonValue(value: unknown): JsonValue {
  if (value == null || typeof value === "string" || typeof value === "boolean") return value ?? null;
  if (typeof value === "number") return value;
  if (typeof value === "bigint") return Number(value);
  if (Array.isArray(value)) return value.map(jsonValue);
  if (typeof value === "object") {
    const result: Record<string, JsonValue> = {};
    for (const [key, field] of Object.entries(value)) {
      result[key] = jsonValue(field);
    }
    return result;
  }
  return null;
}

function eventOf(envelope: RuntimeEventEnvelope): SessionPresentationEvent | null {
  const event = envelope.event;
  const threadId = envelope.thread_id;
  const occurredAtMs = numeric(envelope.occurred_at_ms);
  switch (event.kind) {
    case "turn_started":
      return { type: "platform", payload: { kind: "session_meta_update", data: { key: "runtime_turn_started", value: event } } };
    case "turn_terminal":
      return { type: "platform", payload: { kind: "session_meta_update", data: { key: "runtime_turn_terminal", value: event } } };
    case "item_started":
      return { type: "item_started", payload: { threadId, turnId: event.turn_id, item: event.initial_content, startedAtMs: occurredAtMs } };
    case "item_terminal":
      if (event.terminal.kind === "completed") {
        return { type: "item_completed", payload: { threadId, turnId: event.turn_id, item: event.terminal.final_content, completedAtMs: occurredAtMs } };
      }
      return {
        type: "item_terminal",
        payload: {
          threadId,
          turnId: event.turn_id,
          itemId: event.item_id,
          terminal: event.terminal.kind,
          message: event.terminal.message,
        },
      };
    case "conversation_delta": {
      const base = { threadId, turnId: event.turn_id, itemId: event.item_id };
      switch (event.delta.kind) {
        case "agent_message": return { type: "agent_message_delta", payload: { ...base, delta: event.delta.delta } };
        case "reasoning_text": return { type: "reasoning_text_delta", payload: { ...base, contentIndex: 0, delta: event.delta.delta } };
        case "reasoning_summary": return { type: "reasoning_summary_delta", payload: { ...base, summaryIndex: 0, delta: event.delta.delta } };
        case "command_output": return { type: "command_output_delta", payload: { ...base, delta: event.delta.delta } };
        case "file_change_output": return { type: "file_change_delta", payload: { ...base, delta: event.delta.delta } };
        case "plan": return { type: "plan_delta", payload: { ...base, delta: event.delta.delta } };
        case "mcp_progress": return { type: "mcp_tool_call_progress", payload: { ...base, message: event.delta.message } };
        case "tool_progress": return { type: "platform", payload: { kind: "session_meta_update", data: { key: "runtime_tool_progress", value: { item_id: event.item_id, content_items: event.delta.content_items } } } };
      }
      return null;
    }
    case "token_usage_updated":
      return { type: "token_usage_updated", payload: { threadId, turnId: event.turn_id, tokenUsage: tokenUsage(event.usage) } };
    case "conversation_error":
      return { type: "error", payload: { threadId, turnId: event.turn_id ?? "", willRetry: event.error.retryable, error: { message: event.error.message, additionalDetails: event.error.code } } };
    case "provider_status":
      return { type: "platform", payload: { kind: "session_meta_update", data: { key: "runtime_provider_status", value: { turn_id: event.turn_id, ...event.status, delay_ms: event.status.delay_ms == null ? null : numeric(event.status.delay_ms) } } } };
    case "interaction_requested": {
      return { type: "interaction_requested", payload: { interactionId: event.interaction_id, itemId: event.item_id, request: event.request } };
    }
    case "interaction_terminal":
      return { type: "interaction_terminal", payload: { interactionId: event.interaction_id, terminal: event.terminal } };
    case "thread_status_changed": return { type: "platform", payload: { kind: "session_meta_update", data: { key: "runtime_thread_status", value: event.status } } };
    case "context_checkpoint_prepared":
    case "context_activation_applied":
    case "context_compaction_terminal":
    case "context_checkpoint_activated":
    case "driver_context_compacted_opaque":
    case "hook_run_accepted":
    case "hook_run_started":
    case "hook_run_terminal":
    case "hook_plan_bound":
      return { type: "platform", payload: { kind: "session_meta_update", data: { key: `runtime_${event.kind}`, value: jsonValue(event) } } };
    case "operation_accepted":
    case "operation_terminal":
    case "protocol_violation":
    case "binding_established":
    case "binding_lost":
    case "binding_reestablished":
      return null;
  }
  return assertNever(event);
}

function tokenUsage(usage: RuntimeTokenUsage) {
  const total = {
    totalTokens: numeric(usage.total_tokens), inputTokens: numeric(usage.input_tokens),
    cachedInputTokens: numeric(usage.cached_input_tokens), outputTokens: numeric(usage.output_tokens),
    reasoningOutputTokens: numeric(usage.reasoning_output_tokens),
  };
  return { total, last: total, modelContextWindow: null, context: { providerContextTokens: total.totalTokens, pendingEstimateTokens: 0, currentContextTokens: total.totalTokens, cumulativeTotalTokens: total.totalTokens, modelContextWindow: null, effectiveContextWindow: null, reserveTokens: 0, source: "provider" as const } };
}

export function projectRuntimeEnvelope(envelope: RuntimeEventEnvelope): SessionEventEnvelope | null {
  const event = eventOf(envelope);
  if (!event) return null;
  const sequence = envelope.sequence == null
    ? numeric(envelope.transient?.sequence ?? 0)
    : numeric(envelope.sequence);
  const turnId = ("turn_id" in envelope.event ? envelope.event.turn_id : envelope.transient?.turn_id) ?? undefined;
  return {
    session_id: envelope.thread_id, event_seq: sequence, occurred_at_ms: numeric(envelope.occurred_at_ms),
    session_update_type: envelope.event.kind, turn_id: turnId,
    event,
    ephemeral: envelope.sequence == null,
    transient_generation: envelope.transient == null
      ? undefined
      : numeric(envelope.transient.stream_generation),
  };
}

export function projectRuntimeSnapshot(snapshot: RuntimeSnapshot | null): SessionEventEnvelope[] {
  if (!snapshot) return [];
  const capturedAtMs = numeric(snapshot.captured_at_ms);
  return snapshot.transcript.map((item, index) => ({
    session_id: snapshot.thread_id,
    event_seq: index + 1,
    occurred_at_ms: capturedAtMs,
    session_update_type: "snapshot_item",
    turn_id: item.turn_id,
    entry_index: index,
    event: { type: "item_completed", payload: { threadId: snapshot.thread_id, turnId: item.turn_id, item: item.final_content, completedAtMs: capturedAtMs } },
    ephemeral: false,
  }));
}

export function runtimeSnapshotCursor(snapshot: RuntimeSnapshot | null): number {
  return Number(snapshot?.latest_event_sequence ?? 0);
}
