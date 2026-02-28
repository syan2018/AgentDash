/**
 * ACP 会话流管理 Hook
 *
 * 处理 Streaming HTTP（SSE/NDJSON）连接和 SessionNotification 消息流。
 * 采用 entries 数组作为唯一数据源（single source of truth），
 * tool_call / tool_call_update 直接原地合并到 entries 中。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type {
  SessionNotification,
  SessionUpdate,
} from "@agentclientprotocol/sdk";
import { buildApiPath } from "../../../api/origin";
import type { AcpDisplayEntry } from "./types";
import type { PromptSessionRequest } from "../../../services/executor";
import { createAcpStreamTransport, type AcpStreamTransport } from "./streamTransport";

export interface UseAcpStreamOptions {
  sessionId: string;
  endpoint?: string;
  initialEntries?: AcpDisplayEntry[];
  executeRequest?: PromptSessionRequest;
  onEntry?: (entry: AcpDisplayEntry) => void;
  onConnectionChange?: (connected: boolean) => void;
  onError?: (error: Error) => void;
}

export interface UseAcpStreamResult {
  entries: AcpDisplayEntry[];
  isConnected: boolean;
  isLoading: boolean;
  /** True while actively receiving notifications (resets after a short idle timeout) */
  isReceiving: boolean;
  error: Error | null;
  reconnect: () => void;
  close: () => void;
  sendCancel: () => void;
}

const FLUSH_INTERVAL_MS = 50;
const RECEIVING_IDLE_TIMEOUT_MS = 600;

/**
 * Merge incoming text chunk into accumulated text.
 * Matches ABCCraft's mergeStreamChunk — standard ACP handling only.
 */
function mergeStreamChunk(previous: string, incoming: string): string {
  if (!incoming) return previous;
  if (!previous) return incoming;
  if (incoming === previous) return previous;

  // Some providers send full snapshot chunks.
  if (incoming.startsWith(previous)) return incoming;
  // Some providers resend the same tail chunk.
  if (previous.endsWith(incoming)) return previous;

  // Merge by overlap to avoid duplicate boundaries.
  const maxOverlap = Math.min(previous.length, incoming.length);
  for (let size = maxOverlap; size > 0; size -= 1) {
    if (previous.slice(-size) === incoming.slice(0, size)) {
      return `${previous}${incoming.slice(size)}`;
    }
  }

  return `${previous}${incoming}`;
}

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/** 从 SessionUpdate 中提取 toolCallId（tool_call 或 tool_call_update） */
function getToolCallId(update: SessionUpdate): string | undefined {
  if (update.sessionUpdate === "tool_call" || update.sessionUpdate === "tool_call_update") {
    return (update as { toolCallId?: string }).toolCallId;
  }
  return undefined;
}

/**
 * 将 tool_call_update 的字段合并到已有 tool_call entry 中。
 * 仿照 ABCCraft 的 mergeToolCallUpdate 策略：只覆盖非空字段。
 */
function mergeToolCallUpdateIntoEntry(
  existingUpdate: SessionUpdate,
  incomingUpdate: SessionUpdate,
): SessionUpdate {
  const existing = existingUpdate as Record<string, unknown>;
  const incoming = incomingUpdate as Record<string, unknown>;
  return {
    ...existing,
    sessionUpdate: existing.sessionUpdate,
    title: incoming.title ?? existing.title,
    kind: incoming.kind ?? existing.kind,
    status: incoming.status ?? existing.status,
    content: incoming.content ?? existing.content,
    locations: incoming.locations ?? existing.locations,
    rawInput: incoming.rawInput !== undefined ? incoming.rawInput : existing.rawInput,
    rawOutput: incoming.rawOutput !== undefined ? incoming.rawOutput : existing.rawOutput,
  } as unknown as SessionUpdate;
}

export function useAcpStream(options: UseAcpStreamOptions): UseAcpStreamResult {
  const {
    sessionId,
    endpoint,
    initialEntries = [],
    onEntry,
    onConnectionChange,
    onError,
  } = options;

  const [entries, setEntries] = useState<AcpDisplayEntry[]>(initialEntries);
  const [isConnected, setIsConnected] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [isReceiving, setIsReceiving] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [connectKey, setConnectKey] = useState(0);

  const transportRef = useRef<AcpStreamTransport | null>(null);
  const mountedRef = useRef(true);
  const pendingNotificationsRef = useRef<SessionNotification[]>([]);
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const receivingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const callbackRefs = useRef({ onEntry, onConnectionChange, onError });
  useEffect(() => {
    callbackRefs.current = { onEntry, onConnectionChange, onError };
  }, [onEntry, onConnectionChange, onError]);

  const applyNotification = useCallback((prev: AcpDisplayEntry[], notification: SessionNotification) => {
    const { update } = notification;

    const makeEntry = (u: SessionUpdate, extra?: Partial<AcpDisplayEntry>): AcpDisplayEntry => ({
      id: generateId(),
      sessionId: notification.sessionId,
      timestamp: Date.now(),
      update: u,
      ...extra,
    });

    // ── tool_call ──────────────────────────────────────────────
    if (update.sessionUpdate === "tool_call") {
      const id = getToolCallId(update)!;
      const existingIndex = prev.findIndex((e) => getToolCallId(e.update) === id);
      if (existingIndex >= 0) {
        const next = [...prev];
        next[existingIndex] = { ...prev[existingIndex]!, update, isPendingApproval: update.status === "pending" };
        return next;
      }
      return [...prev, makeEntry(update, { isPendingApproval: update.status === "pending" })];
    }

    // ── tool_call_update ───────────────────────────────────────
    if (update.sessionUpdate === "tool_call_update") {
      const id = getToolCallId(update)!;
      const existingIndex = prev.findIndex((e) => getToolCallId(e.update) === id);
      if (existingIndex >= 0) {
        const merged = mergeToolCallUpdateIntoEntry(prev[existingIndex]!.update, update);
        const next = [...prev];
        next[existingIndex] = { ...prev[existingIndex]!, update: merged, isPendingApproval: update.status === "pending" };
        return next;
      }
      return [...prev, makeEntry(update, { isPendingApproval: update.status === "pending" })];
    }

    // ── chunk 类型（agent_message / user_message / agent_thought）───
    const isChunkUpdate =
      update.sessionUpdate === "agent_message_chunk" ||
      update.sessionUpdate === "user_message_chunk" ||
      update.sessionUpdate === "agent_thought_chunk";

    if (!isChunkUpdate) {
      return [...prev, makeEntry(update)];
    }

    // 合并同类型的连续 chunk
    if (prev.length === 0) {
      return [makeEntry(update)];
    }

    const lastEntry = prev[prev.length - 1]!;
    if (lastEntry.update.sessionUpdate !== update.sessionUpdate) {
      return [...prev, makeEntry(update)];
    }

    const targetUpdateAny = lastEntry.update as unknown as { content?: { type?: string; text?: string } };
    const newUpdateAny = update as unknown as { content?: { type?: string; text?: string } };
    if (targetUpdateAny.content?.type !== "text" || newUpdateAny.content?.type !== "text") {
      return [...prev, makeEntry(update)];
    }

    const previousText = targetUpdateAny.content.text ?? "";
    const incomingText = newUpdateAny.content.text ?? "";
    const mergedText = mergeStreamChunk(previousText, incomingText);

    if (mergedText === previousText) {
      return prev;
    }

    const mergedUpdate: SessionUpdate = {
      ...lastEntry.update,
      content: { type: "text" as const, text: mergedText },
    } as SessionUpdate;

    const next = [...prev];
    next[next.length - 1] = { ...lastEntry, update: mergedUpdate };
    return next;
  }, []);

  const applyNotificationRef = useRef(applyNotification);
  applyNotificationRef.current = applyNotification;

  const markReceiving = useCallback(() => {
    setIsReceiving(true);
    if (receivingTimerRef.current) clearTimeout(receivingTimerRef.current);
    receivingTimerRef.current = setTimeout(() => {
      receivingTimerRef.current = null;
      if (mountedRef.current) setIsReceiving(false);
    }, RECEIVING_IDLE_TIMEOUT_MS);
  }, []);

  const enqueueNotification = useCallback((notification: SessionNotification) => {
    pendingNotificationsRef.current.push(notification);
    markReceiving();
    if (flushTimerRef.current) return;
    flushTimerRef.current = setTimeout(() => {
      flushTimerRef.current = null;
      if (!mountedRef.current) return;
      const pending = pendingNotificationsRef.current;
      if (pending.length === 0) return;
      pendingNotificationsRef.current = [];

      setEntries((prev) => {
        let next = prev;
        for (const n of pending) {
          next = applyNotificationRef.current(next, n);
        }
        return next;
      });
    }, FLUSH_INTERVAL_MS);
  }, [markReceiving]);

  const sendCancel = useCallback(() => {
    void fetch(buildApiPath(`/sessions/${encodeURIComponent(sessionId)}/cancel`), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
    }).catch((e) => {
      const err = e instanceof Error ? e : new Error("取消执行失败");
      setError(err);
      callbackRefs.current.onError?.(err);
    });
  }, [sessionId]);

  useEffect(() => {
    mountedRef.current = true;

    setEntries(initialEntries);
    setIsLoading(true);
    setError(null);
    setIsConnected(false);

    if (transportRef.current) {
      transportRef.current.close();
      transportRef.current = null;
    }

    transportRef.current = createAcpStreamTransport({
      sessionId,
      endpoint,
      onNotification: (notification) => {
        if (!mountedRef.current) return;
        enqueueNotification(notification);
      },
      onLifecycleChange: (lifecycle) => {
        if (!mountedRef.current) return;

        if (lifecycle === "connected") {
          setIsConnected(true);
          setIsLoading(false);
          setError(null);
          callbackRefs.current.onConnectionChange?.(true);
          return;
        }

        if (lifecycle === "connecting" || lifecycle === "reconnecting") {
          setIsConnected(false);
          setIsLoading(true);
          callbackRefs.current.onConnectionChange?.(false);
          return;
        }

        if (lifecycle === "closed") {
          setIsConnected(false);
          setIsLoading(false);
          callbackRefs.current.onConnectionChange?.(false);
        }
      },
      onError: (transportError) => {
        if (!mountedRef.current) return;
        setError(transportError);
        callbackRefs.current.onError?.(transportError);
      },
    });

    return () => {
      mountedRef.current = false;
      if (flushTimerRef.current) {
        clearTimeout(flushTimerRef.current);
        flushTimerRef.current = null;
      }
      if (receivingTimerRef.current) {
        clearTimeout(receivingTimerRef.current);
        receivingTimerRef.current = null;
      }
      pendingNotificationsRef.current = [];

      if (transportRef.current) {
        transportRef.current.close();
        transportRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, endpoint, connectKey]);

  const close = useCallback(() => {
    if (transportRef.current) {
      transportRef.current.close();
      transportRef.current = null;
    }
    setIsConnected(false);
    setIsLoading(false);
  }, []);

  const reconnect = useCallback(() => {
    if (transportRef.current) {
      transportRef.current.close();
      transportRef.current = null;
    }
    setEntries([]);
    setError(null);
    setIsLoading(true);
    setIsConnected(false);
    setIsReceiving(false);
    setConnectKey((k) => k + 1);
  }, []);

  return {
    entries,
    isConnected,
    isLoading,
    isReceiving,
    error,
    reconnect,
    close,
    sendCancel,
  };
}

export default useAcpStream;
