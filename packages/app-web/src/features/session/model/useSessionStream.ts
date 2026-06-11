/**
 * 会话流管理 Hook
 *
 * 先从数据库历史事件 hydrate，再连接增量流。
 * `rawEvents` 才是事实源；`entries` 只是基于事件流派生出来的显示状态。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import {
  cancelSession,
  fetchSessionEvents,
} from "../../../services/session";
import type {
  SessionDisplayEntry,
  SessionEventEnvelope,
  TokenUsageInfo,
} from "./types";
import { createSessionStreamTransport, type SessionStreamTransport } from "./streamTransport";
import {
  createInitialStreamState,
  reduceStreamState,
  shouldFlushStreamEventImmediately,
  type SessionStreamState,
} from "./sessionStreamReducer";
import { dispatchSessionPlatformEvent } from "./sessionPlatformEventDispatcher";

export interface UseSessionStreamOptions {
  sessionId: string;
  /** 设为 false 时跳过连接，返回空的初始状态。默认 true。 */
  enabled?: boolean;
  endpoint?: string;
  initialEntries?: SessionDisplayEntry[];
  onConnectionChange?: (connected: boolean) => void;
  onError?: (error: Error) => void;
}

export interface UseSessionStreamResult {
  entries: SessionDisplayEntry[];
  rawEvents: SessionEventEnvelope[];
  isConnected: boolean;
  isLoading: boolean;
  isReceiving: boolean;
  error: Error | null;
  tokenUsage: TokenUsageInfo | null;
  reconnect: () => void;
  close: () => void;
  sendCancel: () => Promise<void>;
}

const FLUSH_INTERVAL_MS = 50;
const RECEIVING_IDLE_TIMEOUT_MS = 600;
const HISTORY_PAGE_SIZE = 500;
const EMPTY_INITIAL_ENTRIES: SessionDisplayEntry[] = [];

export function useSessionStream(options: UseSessionStreamOptions): UseSessionStreamResult {
  const {
    sessionId,
    enabled = true,
    endpoint,
    initialEntries,
    onConnectionChange,
    onError,
  } = options;
  const normalizedInitialEntries = initialEntries ?? EMPTY_INITIAL_ENTRIES;

  const [streamState, setStreamState] = useState<SessionStreamState>(() =>
    createInitialStreamState(normalizedInitialEntries),
  );
  const [isConnected, setIsConnected] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [isReceiving, setIsReceiving] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [connectKey, setConnectKey] = useState(0);

  const transportRef = useRef<SessionStreamTransport | null>(null);
  const mountedRef = useRef(true);
  const stateRef = useRef(streamState);
  const pendingEventsRef = useRef<SessionEventEnvelope[]>([]);
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const receivingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const sourceKeyRef = useRef<string | null>(null);
  const initialEntriesRef = useRef(normalizedInitialEntries);

  const callbackRefs = useRef({ onConnectionChange, onError });
  useEffect(() => {
    callbackRefs.current = { onConnectionChange, onError };
  }, [onConnectionChange, onError]);

  useEffect(() => {
    initialEntriesRef.current = normalizedInitialEntries;
  }, [normalizedInitialEntries]);

  useEffect(() => {
    stateRef.current = streamState;
  }, [streamState]);

  const markReceiving = useCallback(() => {
    setIsReceiving(true);
    if (receivingTimerRef.current) clearTimeout(receivingTimerRef.current);
    receivingTimerRef.current = setTimeout(() => {
      receivingTimerRef.current = null;
      if (mountedRef.current) setIsReceiving(false);
    }, RECEIVING_IDLE_TIMEOUT_MS);
  }, []);

  const flushPendingEvents = useCallback((mode: "async" | "sync" = "async") => {
    if (!mountedRef.current) return;
    const pending = pendingEventsRef.current;
    if (pending.length === 0) return;
    pendingEventsRef.current = [];

    const applyPending = () => {
      setStreamState((prev) => reduceStreamState(prev, pending));
    };

    if (mode === "sync") {
      flushSync(applyPending);
      return;
    }
    applyPending();
  }, []);

  const enqueueEventRef = useRef<(event: SessionEventEnvelope) => void>(() => {});

  const enqueueEvent = useCallback((event: SessionEventEnvelope) => {
    // 终端事件在此拦截，直接转发到 TerminalStore，不进入 React state 管道
    // （避免 StrictMode 下 reducer 双重执行导致输出重复）
    if (dispatchSessionPlatformEvent(event, callbackRefs.current.onError)) return;

    pendingEventsRef.current.push(event);
    markReceiving();

    if (shouldFlushStreamEventImmediately(event)) {
      if (flushTimerRef.current) {
        clearTimeout(flushTimerRef.current);
        flushTimerRef.current = null;
      }
      flushPendingEvents("sync");
      return;
    }

    if (flushTimerRef.current) return;
    flushTimerRef.current = setTimeout(() => {
      flushTimerRef.current = null;
      flushPendingEvents();
    }, FLUSH_INTERVAL_MS);
  }, [flushPendingEvents, markReceiving]);

  useEffect(() => {
    enqueueEventRef.current = enqueueEvent;
  }, [enqueueEvent]);

  const sendCancel = useCallback(async () => {
    try {
      await cancelSession(sessionId);
    } catch (e) {
      const err = e instanceof Error ? e : new Error("取消执行失败");
      setError(err);
      callbackRefs.current.onError?.(err);
      throw err;
    }
  }, [sessionId]);

  useEffect(() => {
    mountedRef.current = true;

    if (!enabled) {
      setStreamState(createInitialStreamState(initialEntriesRef.current));
      setIsLoading(false);
      setError(null);
      setIsConnected(false);
      return () => {
        mountedRef.current = false;
      };
    }

    const sourceKey = `${sessionId}|${endpoint ?? ""}`;
    const shouldResetState = sourceKeyRef.current !== sourceKey;
    sourceKeyRef.current = sourceKey;

    const baseState = shouldResetState
      ? createInitialStreamState(initialEntriesRef.current)
      : stateRef.current;

    if (shouldResetState) {
      setStreamState(baseState);
    }

    setIsLoading(true);
    setError(null);
    setIsConnected(false);

    if (transportRef.current) {
      transportRef.current.close();
      transportRef.current = null;
    }

    let cancelled = false;

    const start = async () => {
      let nextState = baseState;
      let afterSeq = shouldResetState ? 0 : baseState.lastAppliedSeq;

      try {
        while (!cancelled) {
          const page = await fetchSessionEvents(sessionId, afterSeq, HISTORY_PAGE_SIZE);
          nextState = reduceStreamState(nextState, page.events);
          afterSeq = page.next_after_seq;
          if (!mountedRef.current || cancelled) return;
          setStreamState(nextState);
          stateRef.current = nextState;
          if (!page.has_more) {
            break;
          }
        }

        if (cancelled || !mountedRef.current) return;

        transportRef.current = createSessionStreamTransport({
          sessionId,
          endpoint,
          sinceId: nextState.lastAppliedSeq,
          onEvent: (event) => {
            if (!mountedRef.current) return;
            enqueueEventRef.current(event);
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
      } catch (loadError) {
        if (cancelled || !mountedRef.current) return;
        const normalized = loadError instanceof Error
          ? loadError
          : new Error("加载会话历史失败");
        setError(normalized);
        setIsLoading(false);
        callbackRefs.current.onError?.(normalized);
      }
    };

    void start();

    return () => {
      cancelled = true;
      mountedRef.current = false;
      if (flushTimerRef.current) {
        clearTimeout(flushTimerRef.current);
        flushTimerRef.current = null;
      }
      if (receivingTimerRef.current) {
        clearTimeout(receivingTimerRef.current);
        receivingTimerRef.current = null;
      }
      pendingEventsRef.current = [];

      if (transportRef.current) {
        transportRef.current.close();
        transportRef.current = null;
      }
    };
  }, [connectKey, enabled, endpoint, flushPendingEvents, sessionId]);

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
    setError(null);
    setIsLoading(true);
    setIsConnected(false);
    setIsReceiving(false);
    setConnectKey((k) => k + 1);
  }, []);

  return {
    entries: streamState.entries,
    rawEvents: streamState.rawEvents,
    isConnected,
    isLoading,
    isReceiving,
    error,
    tokenUsage: streamState.tokenUsage,
    reconnect,
    close,
    sendCancel,
  };
}

export default useSessionStream;
