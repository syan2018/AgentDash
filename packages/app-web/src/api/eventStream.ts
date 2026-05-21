import type { StreamEvent } from '../types';
import { buildApiPath } from './origin';
import { authenticatedFetch } from './client';

const RETRY_BASE_MS = 800;
const RETRY_MAX_MS = 8000;

export type ProjectEventStreamLifecycle =
  | 'connecting'
  | 'connected'
  | 'reconnecting'
  | 'closed';

export interface ProjectEventStreamConnection {
  close: () => void;
}

export interface ProjectEventStreamOptions {
  projectId: string;
  sinceId?: number;
  onEvent: (event: StreamEvent) => void;
  onLifecycleChange: (lifecycle: ProjectEventStreamLifecycle) => void;
  onError: (error: Error) => void;
}

function normalizeError(error: unknown, fallbackMessage: string): Error {
  if (error instanceof Error) return error;
  return new Error(fallbackMessage);
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object";
}

function readEventCursor(event: StreamEvent): number | null {
  switch (event.type) {
    case "Connected":
      return event.data.last_event_id;
    case "StateChanged":
      return event.data.id;
    case "BackendRuntimeChanged":
    case "Heartbeat":
      return null;
  }
}

function parseStreamEvent(value: unknown): StreamEvent | null {
  if (!isRecord(value) || typeof value.type !== "string" || !isRecord(value.data)) {
    return null;
  }

  switch (value.type) {
    case "Connected":
      return typeof value.data.last_event_id === "number"
        ? { type: "Connected", data: { last_event_id: value.data.last_event_id } }
        : null;
    case "StateChanged":
      return typeof value.data.id === "number" &&
        typeof value.data.project_id === "string" &&
        typeof value.data.entity_id === "string" &&
        typeof value.data.kind === "string" &&
        typeof value.data.created_at === "string"
        ? {
            type: "StateChanged",
            data: {
              id: value.data.id,
              project_id: value.data.project_id,
              entity_id: value.data.entity_id,
              kind: value.data.kind,
              payload: isRecord(value.data.payload) ? value.data.payload : {},
              backend_id: typeof value.data.backend_id === "string" ? value.data.backend_id : null,
              created_at: value.data.created_at,
            },
          }
        : null;
    case "BackendRuntimeChanged":
      return typeof value.data.backend_id === "string"
        ? { type: "BackendRuntimeChanged", data: { backend_id: value.data.backend_id } }
        : null;
    case "Heartbeat":
      return typeof value.data.timestamp === "number"
        ? { type: "Heartbeat", data: { timestamp: value.data.timestamp } }
        : null;
    default:
      return null;
  }
}

class FetchNdjsonProjectEventStream implements ProjectEventStreamConnection {
  private closed = false;
  private controller: AbortController | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;
  private hadConnected = false;
  private sinceId: number;
  private readonly options: ProjectEventStreamOptions;

  constructor(options: ProjectEventStreamOptions) {
    this.options = options;
    this.sinceId = options.sinceId ?? 0;
    queueMicrotask(() => {
      void this.connect();
    });
  }

  private async connect(): Promise<void> {
    if (this.closed) return;

    this.controller = new AbortController();
    this.options.onLifecycleChange(this.hadConnected ? "reconnecting" : "connecting");

    const params = new URLSearchParams({ project_id: this.options.projectId });
    const headers: Record<string, string> = {
      Accept: "application/x-ndjson",
      "Cache-Control": "no-cache",
      "x-stream-since-id": String(this.sinceId),
    };

    try {
      const response = await authenticatedFetch(
        buildApiPath(`/events/stream/ndjson?${params.toString()}`),
        {
          method: "GET",
          headers,
          signal: this.controller.signal,
          cache: "no-store",
        },
      );

      if (!response.ok || !response.body) {
        this.options.onError(new Error(`项目事件流连接失败: HTTP ${response.status}`));
        this.scheduleReconnect();
        return;
      }

      this.hadConnected = true;
      this.reconnectAttempt = 0;
      this.options.onLifecycleChange("connected");
      await this.consumeStream(response.body.getReader());
    } catch (error) {
      if (this.closed || isAbortError(error)) return;
      this.options.onError(normalizeError(error, "项目事件流连接异常"));
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
      this.options.onError(normalizeError(error, "解析项目事件流消息失败"));
      return;
    }

    const event = parseStreamEvent(payload);
    if (!event) return;

    const cursor = readEventCursor(event);
    if (cursor !== null && cursor > this.sinceId) {
      this.sinceId = cursor;
    }
    this.options.onEvent(event);
  }

  private scheduleReconnect(): void {
    if (this.closed || this.reconnectTimer) return;

    this.options.onLifecycleChange(this.hadConnected ? "reconnecting" : "connecting");
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

    this.options.onLifecycleChange("closed");
  }
}

export function connectProjectEventStream(
  options: ProjectEventStreamOptions,
): ProjectEventStreamConnection {
  return new FetchNdjsonProjectEventStream(options);
}
