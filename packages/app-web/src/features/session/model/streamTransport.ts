import { resolveApiUrl } from "../../../api/origin";
import { authenticatedFetch } from "../../../api/client";
import { registerStreamConnection } from "../../../api/streamRegistry";
import {
  parseSessionNdjsonEnvelope,
  toSessionEventEnvelope,
} from "./sessionNdjsonEnvelopeValidator";
import type { SessionEventEnvelope } from "./types";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";

const RETRY_BASE_MS = 800;
const RETRY_MAX_MS = 8000;

export type SessionStreamLifecycle = "connecting" | "connected" | "reconnecting" | "closed";

export interface SessionStreamTransportOptions {
  sessionId: string;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  endpoint?: string;
  sinceId?: number;
  onEvent: (event: SessionEventEnvelope) => void;
  onLifecycleChange: (lifecycle: SessionStreamLifecycle) => void;
  onError: (error: Error) => void;
  /** connected 帧携带的进程级 ephemeral epoch；epoch 变化代表后端重启。 */
  onEphemeralEpoch?: (epoch: number) => void;
}

export interface SessionStreamTransport {
  close: () => void;
}

function buildStreamEndpoint(
  sessionId: string,
  endpoint?: string,
  agentRunTarget?: AgentRunRuntimeTarget | null,
): string {
  if (endpoint && endpoint.trim().length > 0) {
    return endpoint;
  }
  if (agentRunTarget) {
    return `/api/agent-runs/${encodeURIComponent(agentRunTarget.runId)}/agents/${encodeURIComponent(agentRunTarget.agentId)}/runtime/stream/ndjson`;
  }
  return `/api/sessions/${encodeURIComponent(sessionId)}/stream/ndjson`;
}

function buildNdjsonEndpoint(
  sessionId: string,
  endpoint?: string,
  agentRunTarget?: AgentRunRuntimeTarget | null,
): string {
  const streamEndpoint = buildStreamEndpoint(sessionId, endpoint, agentRunTarget);
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
        resolveApiUrl(buildNdjsonEndpoint(
          this.options.sessionId,
          this.options.endpoint,
          this.options.agentRunTarget,
        )),
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

    const result = parseSessionNdjsonEnvelope(payload);
    if (!result.ok) {
      this.options.onError(result.error);
      return;
    }

    if (result.kind === "connected") {
      if (result.envelope.last_event_id > this.sinceId) {
        this.sinceId = result.envelope.last_event_id;
      }
      this.options.onEphemeralEpoch?.(result.envelope.ephemeral_epoch);
      return;
    }

    if (result.kind === "event" || result.kind === "ephemeral_event") {
      const event = toSessionEventEnvelope(result.envelope);
      // ephemeral 事件 event_seq=0、live-only：不推进 resume 游标。
      if (!event.ephemeral && event.event_seq > this.sinceId) {
        this.sinceId = event.event_seq;
      }
      this.options.onEvent(event);
      return;
    }

    if (result.kind === "heartbeat") {
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

export function createSessionStreamTransport(options: SessionStreamTransportOptions): SessionStreamTransport {
  return new FetchNdjsonTransport(options);
}
