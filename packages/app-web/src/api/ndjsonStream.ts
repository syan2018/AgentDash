import { authenticatedFetch } from "./client";

const RETRY_BASE_MS = 800;
const RETRY_MAX_MS = 8000;

export type NdjsonStreamLifecycle = "connecting" | "connected" | "reconnecting" | "closed";

export interface NdjsonStreamConnection {
  close: () => void;
}

export interface NdjsonStreamOptions<TEvent> {
  url: string;
  sinceId?: number;
  parsePayload: (payload: unknown) => TEvent | null;
  readCursor: (event: TEvent) => number | null;
  onEvent: (event: TEvent) => void;
  onLifecycleChange: (lifecycle: NdjsonStreamLifecycle) => void;
  onError: (error: Error) => void;
  connectionErrorMessage: string;
  parseErrorMessage: string;
}

function normalizeError(error: unknown, fallbackMessage: string): Error {
  if (error instanceof Error) return error;
  return new Error(fallbackMessage);
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

export class FetchNdjsonStream<TEvent> implements NdjsonStreamConnection {
  private closed = false;
  private controller: AbortController | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;
  private hadConnected = false;
  private sinceId: number;
  private readonly options: NdjsonStreamOptions<TEvent>;

  constructor(options: NdjsonStreamOptions<TEvent>) {
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

    const headers: Record<string, string> = {
      Accept: "application/x-ndjson",
      "Cache-Control": "no-cache",
      "x-stream-since-id": String(this.sinceId),
    };

    try {
      const response = await authenticatedFetch(this.options.url, {
        method: "GET",
        headers,
        signal: this.controller.signal,
        cache: "no-store",
      });

      if (!response.ok || !response.body) {
        this.options.onError(new Error(`${this.options.connectionErrorMessage}: HTTP ${response.status}`));
        this.scheduleReconnect();
        return;
      }

      this.hadConnected = true;
      this.reconnectAttempt = 0;
      this.options.onLifecycleChange("connected");
      await this.consumeStream(response.body.getReader());
    } catch (error) {
      if (this.closed || isAbortError(error)) return;
      this.options.onError(normalizeError(error, this.options.connectionErrorMessage));
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
      this.options.onError(normalizeError(error, this.options.parseErrorMessage));
      return;
    }

    const event = this.options.parsePayload(payload);
    if (!event) return;

    const cursor = this.options.readCursor(event);
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
