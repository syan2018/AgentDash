import { authenticatedFetch } from "../../../api/client";
import { buildApiPath } from "../../../api/origin";
import { registerStreamConnection } from "../../../api/streamRegistry";
import type { RuntimeEventEnvelope } from "../../../generated/agent-runtime-contracts";
import type { RuntimeSubscribeError } from "../../../generated/agent-runtime-contracts";
import {
  agentRunScopedPath,
  type AgentRunRuntimeEventStreamItem,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";

const RETRY_BASE_MS = 800;
const RETRY_MAX_MS = 8000;

export type RuntimeEventStreamLifecycle = "connecting" | "connected" | "reconnecting" | "closed";

export interface RuntimeEventStreamOptions {
  target: AgentRunRuntimeTarget;
  after?: number;
  onEvent: (event: RuntimeEventEnvelope, durableCursor: number | null) => void;
  onLifecycleChange: (lifecycle: RuntimeEventStreamLifecycle) => void;
  onError: (error: Error) => void;
}

export interface RuntimeEventStream {
  close: () => void;
}

export interface RuntimeStreamCursorState {
  targetKey: string;
  durable: number;
  transient: number | null;
  generation: number | null;
}

export function advanceRuntimeStreamCursor(
  state: RuntimeStreamCursorState,
  item: AgentRunRuntimeEventStreamItem,
  targetKey: string,
): { state: RuntimeStreamCursorState; accepted: boolean } {
  let next = state.targetKey === targetKey
    ? { ...state }
    : { targetKey, durable: 0, transient: null, generation: null };
  if (item.kind === "error") return { state: next, accepted: false };
  if (item.durable_cursor != null) next.durable = Math.max(next.durable, item.durable_cursor);
  if (item.transient_cursor != null) {
    const generation = Number(item.transient_cursor.stream_generation);
    const sequence = Number(item.transient_cursor.sequence);
    if (next.generation !== generation) next.transient = null;
    if (next.transient != null && sequence <= next.transient) return { state: next, accepted: false };
    next.generation = generation;
    next.transient = sequence;
  }
  if (item.envelope.event.kind === "turn_terminal" || item.envelope.event.kind === "binding_lost" || item.envelope.event.kind === "binding_reestablished") {
    next.transient = null;
    next.generation = null;
  }
  return { state: next, accepted: true };
}

export function runtimeStreamSearchParams(state: RuntimeStreamCursorState): URLSearchParams {
  const params = new URLSearchParams({ after: String(state.durable), include_transient: "true" });
  if (state.transient != null) params.set("transient_after", String(state.transient));
  if (state.generation != null) params.set("stream_generation", String(state.generation));
  return params;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isRuntimeEventEnvelope(value: unknown): value is RuntimeEventEnvelope {
  return isRecord(value)
    && typeof value.thread_id === "string"
    && isRecord(value.event)
    && typeof value.event.kind === "string";
}

function isRuntimeSubscribeError(value: unknown): value is RuntimeSubscribeError {
  return isRecord(value) && typeof value.kind === "string";
}

function isTransientCursor(value: unknown): boolean {
  return isRecord(value)
    && typeof value.binding_id === "string"
    && typeof value.stream_generation === "number"
    && typeof value.sequence === "number"
    && typeof value.event_id === "string";
}

export function parseRuntimeEventStreamItem(value: unknown): AgentRunRuntimeEventStreamItem | null {
  if (!isRecord(value) || typeof value.kind !== "string") return null;
  if (value.kind === "event") {
    if (
      !(value.durable_cursor === null || typeof value.durable_cursor === "number")
      || !(value.transient_cursor === null || isTransientCursor(value.transient_cursor))
      || !isRuntimeEventEnvelope(value.envelope)
    ) {
      return null;
    }
    return { kind: "event", durable_cursor: value.durable_cursor, transient_cursor: value.transient_cursor, envelope: value.envelope } as AgentRunRuntimeEventStreamItem;
  }
  if (value.kind === "error" && isRuntimeSubscribeError(value.error)) {
    return { kind: "error", error: value.error };
  }
  return null;
}

function errorMessage(error: unknown, fallback: string): Error {
  return error instanceof Error ? error : new Error(fallback);
}

class FetchRuntimeEventStream implements RuntimeEventStream {
  private closed = false;
  private controller: AbortController | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;
  private cursor: number;
  private connectedOnce = false;
  private transientCursor: number | null = null;
  private streamGeneration: number | null = null;
  private unregister: (() => void) | null;
  private readonly options: RuntimeEventStreamOptions;
  private readonly targetKey: string;

  constructor(options: RuntimeEventStreamOptions) {
    this.options = options;
    this.cursor = options.after ?? 0;
    this.targetKey = `${options.target.runId}:${options.target.agentId}`;
    this.unregister = registerStreamConnection({ close: () => this.close() });
    void this.connect();
  }

  private async connect(): Promise<void> {
    if (this.closed) return;
    this.controller = new AbortController();
    this.options.onLifecycleChange(this.connectedOnce ? "reconnecting" : "connecting");
    const params = runtimeStreamSearchParams({ targetKey: this.targetKey, durable: this.cursor, transient: this.transientCursor, generation: this.streamGeneration });
    try {
      const response = await authenticatedFetch(buildApiPath(
        agentRunScopedPath(this.options.target, `/runtime/events/stream/ndjson?${params.toString()}`),
      ), {
        method: "GET",
        headers: { Accept: "application/x-ndjson", "Cache-Control": "no-cache" },
        signal: this.controller.signal,
        cache: "no-store",
      });
      if (!response.ok || !response.body) {
        throw new Error(`Agent Runtime event stream 连接失败: HTTP ${response.status}`);
      }
      this.connectedOnce = true;
      this.reconnectAttempt = 0;
      this.options.onLifecycleChange("connected");
      await this.consume(response.body.getReader());
    } catch (error) {
      if (this.closed || (error instanceof DOMException && error.name === "AbortError")) return;
      this.options.onError(errorMessage(error, "Agent Runtime event stream 连接异常"));
    }
    this.scheduleReconnect();
  }

  private async consume(reader: ReadableStreamDefaultReader<Uint8Array>): Promise<void> {
    const decoder = new TextDecoder();
    let buffer = "";
    while (!this.closed) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let newline = buffer.indexOf("\n");
      while (newline >= 0) {
        this.handleLine(buffer.slice(0, newline).trim());
        buffer = buffer.slice(newline + 1);
        newline = buffer.indexOf("\n");
      }
    }
    this.handleLine(buffer.trim());
  }

  private handleLine(line: string): void {
    if (!line) return;
    let payload: unknown;
    try {
      payload = JSON.parse(line);
    } catch (error) {
      this.options.onError(errorMessage(error, "Agent Runtime event stream JSON 无效"));
      return;
    }
    const item = parseRuntimeEventStreamItem(payload);
    if (!item) {
      this.options.onError(new Error("Agent Runtime event stream envelope 无效"));
      return;
    }
    if (item.kind === "error") {
      this.options.onError(new Error(`Agent Runtime event stream error: ${item.error.kind}`));
      return;
    }
    const advanced = advanceRuntimeStreamCursor({ targetKey: this.targetKey, durable: this.cursor, transient: this.transientCursor, generation: this.streamGeneration }, item, this.targetKey);
    this.cursor = advanced.state.durable;
    this.transientCursor = advanced.state.transient;
    this.streamGeneration = advanced.state.generation;
    if (!advanced.accepted) return;
    this.options.onEvent(item.envelope, item.durable_cursor);
  }

  private scheduleReconnect(): void {
    if (this.closed || this.reconnectTimer) return;
    this.options.onLifecycleChange("reconnecting");
    const delay = Math.min(RETRY_BASE_MS * 2 ** this.reconnectAttempt, RETRY_MAX_MS);
    this.reconnectAttempt += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      void this.connect();
    }, delay);
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.controller?.abort();
    this.unregister?.();
    this.unregister = null;
    this.options.onLifecycleChange("closed");
  }
}

export function createRuntimeEventStream(options: RuntimeEventStreamOptions): RuntimeEventStream {
  return new FetchRuntimeEventStream(options);
}
