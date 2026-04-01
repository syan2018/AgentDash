import { resolveApiUrl } from "../../../api/origin";
import { getStoredToken, authenticatedFetch } from "../../../api/client";
import { registerStreamConnection } from "../../../api/streamRegistry";
import type { SessionEventEnvelope } from "./types";

const RETRY_BASE_MS = 800;
const RETRY_MAX_MS = 8000;

export type AcpStreamLifecycle = "connecting" | "connected" | "reconnecting" | "closed";

export interface AcpStreamTransportOptions {
  sessionId: string;
  endpoint?: string;
  sinceId?: number;
  onEvent: (event: SessionEventEnvelope) => void;
  onLifecycleChange: (lifecycle: AcpStreamLifecycle) => void;
  onError: (error: Error) => void;
}

export interface AcpStreamTransport {
  close: () => void;
}

interface FetchNdjsonTransportOptions extends AcpStreamTransportOptions {
  onInitialFailure: (error: Error) => void;
}

function buildSseEndpoint(sessionId: string, endpoint?: string): string {
  if (endpoint && endpoint.trim().length > 0) {
    return endpoint;
  }
  return `/api/acp/sessions/${encodeURIComponent(sessionId)}/stream`;
}

function buildNdjsonEndpoint(sessionId: string, endpoint?: string): string {
  const sseEndpoint = buildSseEndpoint(sessionId, endpoint);
  const [path, query = ""] = sseEndpoint.split("?");
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

function isSessionNotification(value: unknown): value is import("@agentclientprotocol/sdk").SessionNotification {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return typeof record.sessionId === "string" && typeof record.update === "object";
}

function readOptionalNumber(value: unknown): number | null {
  if (value == null) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function readOptionalString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export function parseSessionEventEnvelopePayload(
  payload: unknown,
  fallbackEventSeq?: number,
): SessionEventEnvelope | null {
  if (!payload || typeof payload !== "object") {
    return null;
  }

  const record = payload as Record<string, unknown>;
  const notification = isSessionNotification(record.notification)
    ? record.notification
    : null;

  if (!notification) {
    return null;
  }

  const eventSeq =
    readOptionalNumber(record.event_seq ?? record.id ?? fallbackEventSeq) ?? 0;

  return {
    session_id: readOptionalString(record.session_id ?? record.sessionId) ?? notification.sessionId,
    event_seq: eventSeq,
    notification,
    occurred_at_ms: readOptionalNumber(record.occurred_at_ms ?? record.occurredAtMs),
    committed_at_ms: readOptionalNumber(record.committed_at_ms ?? record.committedAtMs),
    session_update_type:
      readOptionalString(record.session_update_type ?? record.sessionUpdateType) ??
      notification.update.sessionUpdate,
    turn_id: readOptionalString(record.turn_id ?? record.turnId),
    entry_index: readOptionalNumber(record.entry_index ?? record.entryIndex),
    tool_call_id: readOptionalString(record.tool_call_id ?? record.toolCallId),
  };
}

class EventSourceTransport implements AcpStreamTransport {
  private source: EventSource | null = null;
  private closed = false;
  private hadConnected = false;
  private sinceId: number;
  private unregister: (() => void) | null = null;
  private readonly options: AcpStreamTransportOptions;

  constructor(options: AcpStreamTransportOptions) {
    this.options = options;
    this.sinceId = options.sinceId ?? 0;
    this.unregister = registerStreamConnection({
      close: () => this.close(),
    });
    this.connect();
  }

  private connect(): void {
    if (this.closed) return;
    let url = resolveApiUrl(buildSseEndpoint(this.options.sessionId, this.options.endpoint));
    if (this.sinceId > 0) {
      const sep = url.includes("?") ? "&" : "?";
      url = `${url}${sep}since_id=${encodeURIComponent(String(this.sinceId))}`;
    }
    const token = getStoredToken();
    if (token) {
      const sep = url.includes("?") ? "&" : "?";
      url = `${url}${sep}token=${encodeURIComponent(token)}`;
    }
    this.options.onLifecycleChange("connecting");

    const source = new EventSource(url);
    this.source = source;

    source.onopen = () => {
      if (this.closed) return;
      this.hadConnected = true;
      this.options.onLifecycleChange("connected");
    };

    source.onmessage = (event) => {
      if (this.closed) return;
      try {
        const payload: unknown = JSON.parse(event.data);
        const fallbackEventSeq = readOptionalNumber(event.lastEventId) ?? 0;
        const normalizedEvent = parseSessionEventEnvelopePayload(payload, fallbackEventSeq);
        if (!normalizedEvent) return;
        if (normalizedEvent.event_seq > this.sinceId) {
          this.sinceId = normalizedEvent.event_seq;
        }
        this.options.onEvent(normalizedEvent);
      } catch (error) {
        this.options.onError(normalizeError(error, "解析 SSE 消息失败"));
      }
    };

    source.onerror = () => {
      if (this.closed) return;
      this.options.onLifecycleChange(this.hadConnected ? "reconnecting" : "connecting");
    };
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    if (this.source) {
      this.source.close();
      this.source = null;
    }
    this.unregister?.();
    this.unregister = null;
    this.options.onLifecycleChange("closed");
  }
}

class FetchNdjsonTransport implements AcpStreamTransport {
  private closed = false;
  private controller: AbortController | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;
  private sinceId: number;
  private hadConnected = false;
  private unregister: (() => void) | null = null;
  private readonly options: FetchNdjsonTransportOptions;

  constructor(options: FetchNdjsonTransportOptions) {
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
        if (!this.hadConnected) {
          this.options.onInitialFailure(error);
          this.scheduleReconnect();
          return;
        }
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
      if (!this.hadConnected) {
        this.options.onInitialFailure(normalized);
        this.scheduleReconnect();
        return;
      }
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

    if (!payload || typeof payload !== "object") return;
    const record = payload as Record<string, unknown>;
    const eventType = String(record.type ?? "");

    if (eventType === "connected") {
      const lastEventIdRaw = record.last_event_id ?? record.lastEventId;
      const lastEventId = Number(lastEventIdRaw);
      if (Number.isFinite(lastEventId) && lastEventId > this.sinceId) {
        this.sinceId = lastEventId;
      }
      return;
    }

    if (eventType === "event" || eventType === "notification") {
      const normalizedEvent = parseSessionEventEnvelopePayload(record);
      if (normalizedEvent) {
        if (normalizedEvent.event_seq > this.sinceId) {
          this.sinceId = normalizedEvent.event_seq;
        }
        this.options.onEvent(normalizedEvent);
      }
      return;
    }

    if (eventType === "heartbeat") {
      return;
    }
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

function preferSseOnly(): boolean {
  const mode = String(import.meta.env.VITE_ACP_STREAM_TRANSPORT ?? "").toLowerCase();
  return mode === "sse";
}

export function createAcpStreamTransport(options: AcpStreamTransportOptions): AcpStreamTransport {
  if (preferSseOnly()) {
    return new EventSourceTransport(options);
  }
  return new FetchNdjsonTransport({
    ...options,
    onInitialFailure: (error) => {
      options.onError(new Error(`NDJSON 不可用：${error.message}`));
    },
  });
}
