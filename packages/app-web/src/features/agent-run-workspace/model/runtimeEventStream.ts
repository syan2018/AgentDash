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

export function parseRuntimeEventStreamItem(value: unknown): AgentRunRuntimeEventStreamItem | null {
  if (!isRecord(value) || typeof value.kind !== "string") return null;
  if (value.kind === "event") {
    if (
      !(value.durable_cursor === null || typeof value.durable_cursor === "number")
      || !isRuntimeEventEnvelope(value.envelope)
    ) {
      return null;
    }
    return { kind: "event", durable_cursor: value.durable_cursor, envelope: value.envelope };
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
  private unregister: (() => void) | null;
  private readonly options: RuntimeEventStreamOptions;

  constructor(options: RuntimeEventStreamOptions) {
    this.options = options;
    this.cursor = options.after ?? 0;
    this.unregister = registerStreamConnection({ close: () => this.close() });
    void this.connect();
  }

  private async connect(): Promise<void> {
    if (this.closed) return;
    this.controller = new AbortController();
    this.options.onLifecycleChange(this.connectedOnce ? "reconnecting" : "connecting");
    const params = new URLSearchParams({
      after: String(this.cursor),
      include_transient: "false",
    });
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
    if (item.durable_cursor != null && item.durable_cursor > this.cursor) {
      this.cursor = item.durable_cursor;
    }
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
