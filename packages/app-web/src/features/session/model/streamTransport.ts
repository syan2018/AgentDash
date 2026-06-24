import { resolveApiUrl } from "../../../api/origin";
import { authenticatedFetch } from "../../../api/client";
import { registerStreamConnection } from "../../../api/streamRegistry";
import type { BackboneEnvelope } from "../../../generated/backbone-protocol";
import type { SessionNdjsonEnvelope } from "../../../generated/session-contracts";
import type { SessionEventEnvelope } from "./types";

const RETRY_BASE_MS = 800;
const RETRY_MAX_MS = 8000;

export type SessionStreamLifecycle = "connecting" | "connected" | "reconnecting" | "closed";
type SessionNdjsonEventEnvelope = Extract<
  SessionNdjsonEnvelope,
  { type: "event" } | { type: "ephemeral_event" }
>;

export interface SessionStreamTransportOptions {
  sessionId: string;
  endpoint?: string;
  sinceId?: number;
  onEvent: (event: SessionEventEnvelope) => void;
  onLifecycleChange: (lifecycle: SessionStreamLifecycle) => void;
  onError: (error: Error) => void;
}

export interface SessionStreamTransport {
  close: () => void;
}

function buildStreamEndpoint(sessionId: string, endpoint?: string): string {
  if (endpoint && endpoint.trim().length > 0) {
    return endpoint;
  }
  return `/api/sessions/${encodeURIComponent(sessionId)}/stream/ndjson`;
}

function buildNdjsonEndpoint(sessionId: string, endpoint?: string): string {
  const streamEndpoint = buildStreamEndpoint(sessionId, endpoint);
  const [path, query = ""] = streamEndpoint.split("?");
  const ndjsonPath = path.endsWith("/stream/ndjson")
    ? path
    : path.endsWith("/stream")
      ? `${path}/ndjson`
      : `${path}/stream/ndjson`;
  return query ? `${ndjsonPath}?${query}` : ndjsonPath;
}

function normalizeError(error: unknown, fallbackMessage: string): Error {
  if (error instanceof Error) return error;
  return new Error(fallbackMessage);
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isBackboneEnvelope(value: unknown): value is BackboneEnvelope {
  if (!isRecord(value)) return false;
  const record = value;
  return typeof record.event === "object" && record.event !== null &&
    typeof record.sessionId === "string";
}

function isSessionNdjsonEventEnvelope(value: unknown): value is SessionNdjsonEventEnvelope {
  return isRecord(value) && (value.type === "event" || value.type === "ephemeral_event");
}

function readRequiredNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function readOptionalString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function readOptionalNumber(value: unknown): number | undefined {
  if (value == null) return undefined;
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

export function parseSessionEventEnvelopePayload(
  payload: unknown,
): { event: SessionEventEnvelope | null; error: Error | null } {
  if (!isRecord(payload)) {
    return { event: null, error: new Error("Session stream event 必须是对象") };
  }

  if (!isSessionNdjsonEventEnvelope(payload)) {
    return { event: null, error: new Error("Session stream payload 不是 event 分支") };
  }

  const eventSeq = readRequiredNumber(payload.event_seq);
  if (eventSeq == null) {
    return { event: null, error: new Error("Session stream event 缺少合法 event_seq") };
  }

  const sessionId = readOptionalString(payload.session_id);
  if (!sessionId) {
    return { event: null, error: new Error("Session stream event 缺少 session_id") };
  }

  const occurredAtMs = readRequiredNumber(payload.occurred_at_ms);
  if (occurredAtMs == null) {
    return { event: null, error: new Error("Session stream event 缺少 occurred_at_ms") };
  }

  const committedAtMs = readRequiredNumber(payload.committed_at_ms);
  if (committedAtMs == null) {
    return { event: null, error: new Error("Session stream event 缺少 committed_at_ms") };
  }

  const sessionUpdateType = readOptionalString(payload.session_update_type);
  if (!sessionUpdateType) {
    return { event: null, error: new Error("Session stream event 缺少 session_update_type") };
  }

  if (!isBackboneEnvelope(payload.notification)) {
    return { event: null, error: new Error("Session stream event 缺少合法 notification") };
  }

  return {
    event: {
      session_id: sessionId,
      event_seq: eventSeq,
      occurred_at_ms: occurredAtMs,
      committed_at_ms: committedAtMs,
      session_update_type: sessionUpdateType,
      turn_id: readOptionalString(payload.turn_id) ?? undefined,
      entry_index: readOptionalNumber(payload.entry_index),
      tool_call_id: readOptionalString(payload.tool_call_id) ?? undefined,
      notification: payload.notification,
      ephemeral: payload.type === "ephemeral_event",
    },
    error: null,
  };
}

class FetchNdjsonTransport implements SessionStreamTransport {
  private closed = false;
  private controller: AbortController | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;
  private sinceId: number;
  private hadConnected = false;
  private unregister: (() => void) | null = null;
  private readonly options: SessionStreamTransportOptions;

  constructor(options: SessionStreamTransportOptions) {
    this.options = options;
    this.sinceId = options.sinceId ?? 0;
    this.unregister = registerStreamConnection({
      close: () => this.close(),
    });
    void this.connect();
  }

  private async connect(): Promise<void> {
    if (this.closed) return;

    this.controller = new AbortController();
    this.options.onLifecycleChange(this.hadConnected ? "reconnecting" : "connecting");

    const headers: Record<string, string> = {
      Accept: "application/x-ndjson",
      "Cache-Control": "no-cache",
      "x-stream-since-id": String(this.sinceId),
    };

    try {
      const response = await authenticatedFetch(
        resolveApiUrl(buildNdjsonEndpoint(this.options.sessionId, this.options.endpoint)),
        {
          method: "GET",
          headers,
          signal: this.controller.signal,
          cache: "no-store",
        },
      );

      if (!response.ok || !response.body) {
        const error = new Error(`NDJSON 连接失败: HTTP ${response.status}`);
        this.options.onError(error);
        this.scheduleReconnect();
        return;
      }

      this.hadConnected = true;
      this.reconnectAttempt = 0;
      this.options.onLifecycleChange("connected");
      await this.consumeStream(response.body.getReader());
    } catch (error) {
      if (this.closed || isAbortError(error)) return;
      const normalized = normalizeError(error, "NDJSON 连接异常");
      this.options.onError(normalized);
    }

    if (!this.closed) {
      this.scheduleReconnect();
    }
  }

  private async consumeStream(reader: ReadableStreamDefaultReader<Uint8Array>): Promise<void> {
    const decoder = new TextDecoder();
    let buffer = "";

    while (!this.closed) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      let lineBreakIndex = buffer.indexOf("\n");

      while (lineBreakIndex >= 0) {
        const line = buffer.slice(0, lineBreakIndex).trim();
        buffer = buffer.slice(lineBreakIndex + 1);
        if (line) {
          this.handleLine(line);
        }
        lineBreakIndex = buffer.indexOf("\n");
      }
    }

    const trailing = buffer.trim();
    if (trailing) {
      this.handleLine(trailing);
    }
  }

  private handleLine(line: string): void {
    let payload: unknown;
    try {
      payload = JSON.parse(line);
    } catch (error) {
      this.options.onError(normalizeError(error, "解析 NDJSON 消息失败"));
      return;
    }

    if (!isRecord(payload)) {
      this.options.onError(new Error("NDJSON 消息必须是对象"));
      return;
    }
    const type = payload.type;

    if (type === "connected") {
      const lastEventId = readRequiredNumber(payload.last_event_id);
      if (lastEventId != null && lastEventId > this.sinceId) {
        this.sinceId = lastEventId;
      }
      return;
    }

    if (type === "event" || type === "ephemeral_event") {
      const result = parseSessionEventEnvelopePayload(payload);
      if (result.error) {
        this.options.onError(result.error);
        return;
      }
      if (!result.event) return;
      // ephemeral 事件 event_seq=0、live-only：不推进 resume 游标。
      if (!result.event.ephemeral && result.event.event_seq > this.sinceId) {
        this.sinceId = result.event.event_seq;
      }
      this.options.onEvent(result.event);
      return;
    }

    if (type === "heartbeat") {
      return;
    }

    this.options.onError(new Error(`未知 Session NDJSON 类型: ${String(type)}`));
  }

  private scheduleReconnect(): void {
    if (this.closed) return;
    if (this.reconnectTimer) return;

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

    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    if (this.controller) {
      this.controller.abort();
      this.controller = null;
    }

    this.unregister?.();
    this.unregister = null;
    this.options.onLifecycleChange("closed");
  }
}

export function createSessionStreamTransport(options: SessionStreamTransportOptions): SessionStreamTransport {
  return new FetchNdjsonTransport(options);
}
