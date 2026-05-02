/**
 * 会话流管理 Hook
 *
 * 先从数据库历史事件 hydrate，再连接增量流。
 * `rawEvents` 才是事实源；`entries` 只是基于事件流派生出来的显示状态。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import type { BackboneEvent, BackboneEnvelope, ThreadItem } from "../../../generated/backbone-protocol";
import {
  cancelSession,
  fetchSessionEvents,
} from "../../../services/session";
import type {
  AcpDisplayEntry,
  SessionEventEnvelope,
  TokenUsageInfo,
} from "./types";
import { extractTokenUsageFromEvent, parseContentBlock } from "./types";
import { createSessionStreamTransport, type SessionStreamTransport } from "./streamTransport";

export interface UseSessionStreamOptions {
  sessionId: string;
  /** 设为 false 时跳过连接，返回空的初始状态。默认 true。 */
  enabled?: boolean;
  endpoint?: string;
  initialEntries?: AcpDisplayEntry[];
  onEntry?: (entry: AcpDisplayEntry) => void;
  onConnectionChange?: (connected: boolean) => void;
  onError?: (error: Error) => void;
}

export interface UseSessionStreamResult {
  entries: AcpDisplayEntry[];
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
const EMPTY_INITIAL_ENTRIES: AcpDisplayEntry[] = [];

interface SessionStreamState {
  entries: AcpDisplayEntry[];
  rawEvents: SessionEventEnvelope[];
  tokenUsage: TokenUsageInfo | null;
  lastAppliedSeq: number;
}

type StreamInputEvent = {
  session_id: string;
  event_seq: number;
  notification: BackboneEnvelope;
  occurred_at_ms?: number | null;
  committed_at_ms?: number | null;
  session_update_type?: string | null;
  turn_id?: string | null;
  entry_index?: number | null;
  tool_call_id?: string | null;
};

function createInitialState(initialEntries: AcpDisplayEntry[]): SessionStreamState {
  const lastAppliedSeq = initialEntries.reduce((max, entry) => Math.max(max, entry.eventSeq), 0);
  return {
    entries: initialEntries,
    rawEvents: [],
    tokenUsage: null,
    lastAppliedSeq,
  };
}

function backboneEventTypeName(event: BackboneEvent): string {
  return event.type;
}

/** 从 ThreadItem 提取 item ID */
function threadItemId(item: ThreadItem): string {
  return item.id;
}

/** 从 BackboneEvent 提取关联的 item ID（用于合并 item lifecycle） */
function getItemIdFromEvent(event: BackboneEvent): string | undefined {
  switch (event.type) {
    case "item_started":
    case "item_completed":
      return threadItemId(event.payload.item);
    case "command_output_delta":
    case "file_change_delta":
    case "mcp_tool_call_progress":
    case "agent_message_delta":
    case "reasoning_text_delta":
    case "reasoning_summary_delta":
    case "plan_delta":
      return event.payload.itemId;
    default:
      return undefined;
  }
}

function toEventEnvelope(event: StreamInputEvent): SessionEventEnvelope {
  return {
    session_id: event.session_id,
    event_seq: event.event_seq,
    notification: event.notification,
    occurred_at_ms: event.occurred_at_ms ?? null,
    committed_at_ms: event.committed_at_ms ?? null,
    session_update_type: event.session_update_type ?? backboneEventTypeName(event.notification.event),
    turn_id: event.turn_id ?? event.notification.trace?.turnId ?? null,
    entry_index: event.entry_index ?? event.notification.trace?.entryIndex ?? null,
    tool_call_id: event.tool_call_id ?? null,
  };
}

function buildEntryId(event: SessionEventEnvelope, bbEvent: BackboneEvent): string {
  const itemId = getItemIdFromEvent(bbEvent);
  if (itemId) {
    return `item:${itemId}`;
  }

  const turnId = event.turn_id;
  const entryIndex = event.entry_index;

  if (bbEvent.type === "agent_message_delta" || bbEvent.type === "reasoning_text_delta" ||
      bbEvent.type === "reasoning_summary_delta") {
    if (turnId && entryIndex != null) {
      return `delta:${bbEvent.type}:${turnId}:${entryIndex}`;
    }
    const payloadItemId = (bbEvent.payload as { itemId?: string }).itemId;
    if (payloadItemId) {
      return `delta:${bbEvent.type}:${payloadItemId}`;
    }
  }

  if (bbEvent.type === "platform") {
    const platform = bbEvent.payload;
    if (platform.kind === "session_meta_update" && platform.data.key === "user_message_chunk") {
      if (turnId && entryIndex != null) {
        return `user:${turnId}:${entryIndex}`;
      }
    }
  }

  return `event:${event.event_seq}`;
}

function makeDisplayEntry(event: SessionEventEnvelope, bbEvent: BackboneEvent): AcpDisplayEntry {
  return {
    id: buildEntryId(event, bbEvent),
    sessionId: event.notification.sessionId,
    timestamp: event.committed_at_ms ?? event.occurred_at_ms ?? Date.now(),
    eventSeq: event.event_seq,
    event: bbEvent,
    turnId: event.turn_id ?? undefined,
    entryIndex: event.entry_index ?? undefined,
  };
}

function applyEventToEntries(prev: AcpDisplayEntry[], event: SessionEventEnvelope): AcpDisplayEntry[] {
  const bbEvent: BackboneEvent = event.notification.event;

  // ── agent_message_delta — 累积文本增量 ──
  if (bbEvent.type === "agent_message_delta") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      if (prev[i]!.id === entryId) {
        const existing = prev[i]!;
        const accumulated = (existing.accumulatedText ?? "") + bbEvent.payload.delta;
        const next = [...prev];
        next[i] = { ...existing, eventSeq: event.event_seq, event: bbEvent, accumulatedText: accumulated, isStreaming: true };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), accumulatedText: bbEvent.payload.delta, isStreaming: true }];
  }

  // ── reasoning_text_delta — 累积推理文本 ──
  if (bbEvent.type === "reasoning_text_delta") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      if (prev[i]!.id === entryId) {
        const existing = prev[i]!;
        const accumulated = (existing.accumulatedText ?? "") + bbEvent.payload.delta;
        const next = [...prev];
        next[i] = { ...existing, eventSeq: event.event_seq, event: bbEvent, accumulatedText: accumulated };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), accumulatedText: bbEvent.payload.delta }];
  }

  // ── reasoning_summary_delta ──
  if (bbEvent.type === "reasoning_summary_delta") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      if (prev[i]!.id === entryId) {
        const existing = prev[i]!;
        const accumulated = (existing.accumulatedText ?? "") + bbEvent.payload.delta;
        const next = [...prev];
        next[i] = { ...existing, eventSeq: event.event_seq, event: bbEvent, accumulatedText: accumulated };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), accumulatedText: bbEvent.payload.delta }];
  }

  // ── item_started — 创建工具调用条目 ──
  if (bbEvent.type === "item_started") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      if (prev[i]!.id === entryId) {
        const next = [...prev];
        next[i] = { ...prev[i]!, eventSeq: event.event_seq, event: bbEvent };
        return next;
      }
    }
    return [...prev, makeDisplayEntry(event, bbEvent)];
  }

  // ── item_completed — 更新工具调用条目为终态 ──
  if (bbEvent.type === "item_completed") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      if (prev[i]!.id === entryId) {
        const next = [...prev];
        next[i] = {
          ...prev[i]!,
          eventSeq: event.event_seq,
          event: bbEvent,
          isStreaming: false,
          isPendingApproval: false,
        };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), isStreaming: false }];
  }

  // ── command_output_delta / file_change_delta / mcp_tool_call_progress ──
  // 关联到已存在的 item 条目，追加输出增量
  if (bbEvent.type === "command_output_delta" || bbEvent.type === "file_change_delta" ||
      bbEvent.type === "mcp_tool_call_progress") {
    const itemId = bbEvent.payload.itemId;
    const targetId = `item:${itemId}`;
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      if (prev[i]!.id === targetId) {
        const existing = prev[i]!;
        const deltaText = bbEvent.type === "mcp_tool_call_progress"
          ? bbEvent.payload.message
          : bbEvent.payload.delta;
        const accumulated = (existing.accumulatedText ?? "") + deltaText;
        const next = [...prev];
        next[i] = { ...existing, eventSeq: event.event_seq, accumulatedText: accumulated };
        return next;
      }
    }
    return prev;
  }

  // ── turn_started / turn_completed — 静默，不创建条目 ──
  if (bbEvent.type === "turn_started" || bbEvent.type === "turn_completed") {
    return prev;
  }

  // ── turn_plan_updated — 计划条目 ──
  if (bbEvent.type === "turn_plan_updated") {
    return [...prev, makeDisplayEntry(event, bbEvent)];
  }

  // ── plan_delta — 计划增量 ──
  if (bbEvent.type === "plan_delta") {
    return prev;
  }

  // ── token_usage_updated — 静默 ──
  if (bbEvent.type === "token_usage_updated") {
    return prev;
  }

  // ── approval_request — 审批请求 ──
  if (bbEvent.type === "approval_request") {
    return [...prev, { ...makeDisplayEntry(event, bbEvent), isPendingApproval: true }];
  }

  // ── error — 错误条目 ──
  if (bbEvent.type === "error") {
    return [...prev, makeDisplayEntry(event, bbEvent)];
  }

  // ── platform — 平台事件分发 ──
  if (bbEvent.type === "platform") {
    const platform = bbEvent.payload;

    if (platform.kind === "session_meta_update") {
      const key = platform.data.key;

      // 用户消息：累积文本
      if (key === "user_message_chunk") {
        const entryId = buildEntryId(event, bbEvent);
        const value = platform.data.value;
        const parsedBlock = parseContentBlock(value);
        const chunkText =
          typeof value === "string"
            ? value
            : parsedBlock?.type === "text"
              ? parsedBlock.text
              : null;

        // 仅 text block 走增量拼接；resource/resource_link 等结构化块保持原事件给 UI 专用卡片渲染。
        if (chunkText != null) {
          for (let i = prev.length - 1; i >= 0; i -= 1) {
            if (prev[i]!.id === entryId) {
              const existing = prev[i]!;
              const accumulated = (existing.accumulatedText ?? "") + chunkText;
              const next = [...prev];
              next[i] = { ...existing, eventSeq: event.event_seq, event: bbEvent, accumulatedText: accumulated };
              return next;
            }
          }
          return [...prev, { ...makeDisplayEntry(event, bbEvent), accumulatedText: chunkText }];
        }

        for (let i = prev.length - 1; i >= 0; i -= 1) {
          if (prev[i]!.id === entryId) {
            const next = [...prev];
            next[i] = { ...prev[i]!, eventSeq: event.event_seq, event: bbEvent };
            return next;
          }
        }
        return [...prev, makeDisplayEntry(event, bbEvent)];
      }

      // session_meta_updated — 静默
      if (key === "session_meta_updated" || key === "acp_passthrough") {
        return prev;
      }

      // 可渲染的系统/任务/协作事件
      return [...prev, makeDisplayEntry(event, bbEvent)];
    }

    // executor_session_bound / hook_trace — 系统事件
    return [...prev, makeDisplayEntry(event, bbEvent)];
  }

  // ── thread_status_changed / context_compacted / turn_diff_updated — 静默 ──
  if (bbEvent.type === "thread_status_changed" || bbEvent.type === "context_compacted" ||
      bbEvent.type === "turn_diff_updated") {
    return prev;
  }

  return [...prev, makeDisplayEntry(event, bbEvent)];
}

export function reduceStreamState(
  prev: SessionStreamState,
  incomingEvents: StreamInputEvent[],
): SessionStreamState {
  if (incomingEvents.length === 0) {
    return prev;
  }

  const normalized = incomingEvents
    .map(toEventEnvelope)
    .sort((a, b) => a.event_seq - b.event_seq);

  let entries = prev.entries;
  let rawEvents = prev.rawEvents;
  let tokenUsage = prev.tokenUsage;
  let lastAppliedSeq = prev.lastAppliedSeq;

  for (const event of normalized) {
    if (event.event_seq <= lastAppliedSeq) {
      continue;
    }
    rawEvents = [...rawEvents, event];
    entries = applyEventToEntries(entries, event);
    const usage = extractTokenUsageFromEvent(event.notification.event);
    if (usage) {
      tokenUsage = tokenUsage ? { ...tokenUsage, ...usage } : usage;
    }
    lastAppliedSeq = event.event_seq;
  }

  return {
    entries,
    rawEvents,
    tokenUsage,
    lastAppliedSeq,
  };
}

/** 判断事件是否需要立即 flush（item 生命周期事件） */
function shouldFlushImmediately(event: SessionEventEnvelope): boolean {
  const t = event.notification.event.type;
  return t === "item_started" || t === "item_completed" || t === "approval_request";
}

export function useSessionStream(options: UseSessionStreamOptions): UseSessionStreamResult {
  const {
    sessionId,
    enabled = true,
    endpoint,
    initialEntries,
    onEntry,
    onConnectionChange,
    onError,
  } = options;
  const normalizedInitialEntries = initialEntries ?? EMPTY_INITIAL_ENTRIES;

  const [streamState, setStreamState] = useState<SessionStreamState>(() =>
    createInitialState(normalizedInitialEntries),
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

  const callbackRefs = useRef({ onEntry, onConnectionChange, onError });
  useEffect(() => {
    callbackRefs.current = { onEntry, onConnectionChange, onError };
  }, [onEntry, onConnectionChange, onError]);

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
    pendingEventsRef.current.push(event);
    markReceiving();

    if (shouldFlushImmediately(event)) {
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
      setStreamState(createInitialState(initialEntriesRef.current));
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
      ? createInitialState(initialEntriesRef.current)
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
