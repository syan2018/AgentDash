import type { SessionNotification } from "@agentclientprotocol/sdk";
import { resolveApiUrl } from "../../../api/origin";
import { registerStreamConnection } from "../../../api/streamRegistry";

const RETRY_BASE_MS = 800;
const RETRY_MAX_MS = 8000;

export type AcpStreamLifecycle = "connecting" | "connected" | "reconnecting" | "closed";

export interface AcpStreamTransportOptions {
  sessionId: string;
  endpoint?: string;
  onNotification: (notification: SessionNotification) => void;
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

function isSessionNotification(value: unknown): value is SessionNotification {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return typeof record.sessionId === "string" && typeof record.update === "object";
}

class EventSourceTransport implements AcpStreamTransport {
  private source: EventSource | null = null;
  private closed = false;
  private hadConnected = false;
  private unregister: (() => void) | null = null;
  private readonly options: AcpStreamTransportOptions;

  constructor(options: AcpStreamTransportOptions) {
    this.options = options;
    this.unregister = registerStreamConnection({
      close: () => this.close(),
    });
    this.connect();
  }

  private connect(): void {
    if (this.closed) return;
    const url = resolveApiUrl(buildSseEndpoint(this.options.sessionId, this.options.endpoint));
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
        if (!isSessionNotification(payload)) return;
        this.options.onNotification(payload);
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
  private sinceId = 0;
  private hadConnected = false;
  private unregister: (() => void) | null = null;
  private readonly options: FetchNdjsonTransportOptions;

  constructor(options: FetchNdjsonTransportOptions) {
    this.options = options;
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
      const response = await fetch(
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
        return;
      }
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

    if (eventType === "notification") {
      const id = Number(record.id ?? 0);
      if (Number.isFinite(id) && id > this.sinceId) {
        this.sinceId = id;
      }
      const notification = record.notification;
      if (isSessionNotification(notification)) {
        this.options.onNotification(notification);
      }
      return;
    }

    if (eventType === "heartbeat") {
      return;
    }

    // 兼容：若服务端直接推 SessionNotification（无 envelope）
    if (isSessionNotification(record)) {
      this.options.onNotification(record);
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

class FallbackTransport implements AcpStreamTransport {
  private closed = false;
  private current: AcpStreamTransport;
  private readonly options: AcpStreamTransportOptions;

  constructor(options: AcpStreamTransportOptions) {
    this.options = options;
    if (this.preferSseOnly()) {
      this.current = new EventSourceTransport(options);
      return;
    }

    this.current = new FetchNdjsonTransport({
      ...options,
      onInitialFailure: (error) => {
        if (this.closed) return;
        this.options.onError(new Error(`NDJSON 不可用，已降级 SSE：${error.message}`));
        this.current = new EventSourceTransport(this.options);
      },
    });
  }

  private preferSseOnly(): boolean {
    const mode = String(import.meta.env.VITE_ACP_STREAM_TRANSPORT ?? "").toLowerCase();
    return mode === "sse";
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.current.close();
  }
}

export function createAcpStreamTransport(options: AcpStreamTransportOptions): AcpStreamTransport {
  return new FallbackTransport(options);
}
