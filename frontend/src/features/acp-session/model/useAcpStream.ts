/**
 * ACP 会话流管理 Hook
 *
 * 先从数据库历史事件 hydrate，再连接增量流。
 * `rawEvents` 才是事实源；`entries` 只是基于事件流派生出来的显示状态。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import type {
  SessionNotification,
  SessionUpdate,
} from "@agentclientprotocol/sdk";
import {
  cancelSession,
  fetchSessionEvents,
} from "../../../services/session";
import type {
  AcpDisplayEntry,
  SessionEventEnvelope,
  TokenUsageInfo,
} from "./types";
import { createAcpStreamTransport, type AcpStreamTransport } from "./streamTransport";
import { extractAgentDashMetaFromUpdate } from "./agentdashMeta";

export interface UseAcpStreamOptions {
  sessionId: string;
  /** 设为 false 时跳过连接，返回空的初始状态。默认 true。 */
  enabled?: boolean;
  endpoint?: string;
  initialEntries?: AcpDisplayEntry[];
  onEntry?: (entry: AcpDisplayEntry) => void;
  onConnectionChange?: (connected: boolean) => void;
  onError?: (error: Error) => void;
}

export interface UseAcpStreamResult {
  entries: AcpDisplayEntry[];
  rawEvents: SessionEventEnvelope[];
  isConnected: boolean;
  isLoading: boolean;
  /** True while actively receiving notifications (resets after a short idle timeout) */
  isReceiving: boolean;
  error: Error | null;
  /** 最新的 token 用量信息（累计更新） */
  tokenUsage: TokenUsageInfo | null;
  reconnect: () => void;
  close: () => void;
  sendCancel: () => Promise<void>;
}

const FLUSH_INTERVAL_MS = 50;
const RECEIVING_IDLE_TIMEOUT_MS = 600;
const HISTORY_PAGE_SIZE = 500;
const EMPTY_INITIAL_ENTRIES: AcpDisplayEntry[] = [];

interface AcpStreamState {
  entries: AcpDisplayEntry[];
  rawEvents: SessionEventEnvelope[];
  tokenUsage: TokenUsageInfo | null;
  lastAppliedSeq: number;
}

type StreamInputEvent = {
  session_id: string;
  event_seq: number;
  notification: SessionNotification;
  occurred_at_ms?: number | null;
  committed_at_ms?: number | null;
  session_update_type?: string | null;
  turn_id?: string | null;
  entry_index?: number | null;
  tool_call_id?: string | null;
};

function createInitialState(initialEntries: AcpDisplayEntry[]): AcpStreamState {
  const lastAppliedSeq = initialEntries.reduce((max, entry) => Math.max(max, entry.eventSeq), 0);
  return {
    entries: initialEntries,
    rawEvents: [],
    tokenUsage: null,
    lastAppliedSeq,
  };
}

/**
 * Merge incoming text chunk into accumulated text.
 * Matches ABCCraft's mergeStreamChunk — standard ACP handling only.
 */
function mergeStreamChunk(previous: string, incoming: string): string {
  if (!incoming) return previous;
  if (!previous) return incoming;
  if (incoming === previous) return previous;

  if (incoming.startsWith(previous)) {
    return incoming;
  }

  const maxOverlap = Math.min(previous.length, incoming.length);
  for (let size = maxOverlap; size > 0; size -= 1) {
    if (previous.slice(-size) === incoming.slice(0, size)) {
      // 仅接受“部分重叠”；若 incoming 被完全重叠，按增量保守策略走 append，
      // 避免把合法尾部增量误判为重复而吞字。
      if (size >= incoming.length) {
        break;
      }
      return `${previous}${incoming.slice(size)}`;
    }
  }

  return `${previous}${incoming}`;
}

/** 从 SessionUpdate 中提取 toolCallId（tool_call 或 tool_call_update） */
function getToolCallId(update: SessionUpdate): string | undefined {
  if (update.sessionUpdate === "tool_call" || update.sessionUpdate === "tool_call_update") {
    return (update as { toolCallId?: string }).toolCallId;
  }
  return undefined;
}

function getTurnId(update: SessionUpdate): string | undefined {
  const meta = extractAgentDashMetaFromUpdate(update);
  return meta?.trace?.turnId ?? undefined;
}

function getEntryIndex(update: SessionUpdate): number | undefined {
  const meta = extractAgentDashMetaFromUpdate(update);
  const idx = meta?.trace?.entryIndex;
  return typeof idx === "number" ? idx : undefined;
}

function getMessageId(update: SessionUpdate): string | undefined {
  const candidate = (update as unknown as { messageId?: unknown }).messageId;
  if (typeof candidate !== "string") return undefined;
  const trimmed = candidate.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function extractTextContent(update: SessionUpdate): Record<string, unknown> | null {
  const content = (update as unknown as { content?: unknown }).content;
  if (!content || typeof content !== "object" || Array.isArray(content)) {
    return null;
  }
  const record = content as Record<string, unknown>;
  return record.type === "text" ? record : null;
}

function replaceTextContentPreservingMeta(
  existingUpdate: SessionUpdate,
  incomingUpdate: SessionUpdate,
  text: string,
): SessionUpdate {
  const existingContent = extractTextContent(existingUpdate);
  const incomingContent = extractTextContent(incomingUpdate);

  return {
    ...existingUpdate,
    content: {
      ...(existingContent ?? {}),
      ...(incomingContent ?? {}),
      type: "text",
      text,
    },
  } as SessionUpdate;
}

/**
 * 将 tool_call_update 的字段合并到已有 tool_call entry 中。
 * 仿照 Zed 的 update_fields 策略：只覆盖非空字段，保留已有值。
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

/**
 * 从 usage_update 提取 token 用量信息。
 * 支持 ACP 标准字段（size/used）和 AgentDash 扩展字段。
 */
function extractTokenUsage(update: SessionUpdate): TokenUsageInfo | null {
  if (update.sessionUpdate !== "usage_update") return null;
  const u = update as Record<string, unknown>;
  const usage: TokenUsageInfo = {};

  if (typeof u.size === "number") usage.maxTokens = u.size;
  if (typeof u.used === "number") usage.totalTokens = u.used;

  if (typeof u.inputTokens === "number") usage.inputTokens = u.inputTokens;
  if (typeof u.outputTokens === "number") usage.outputTokens = u.outputTokens;
  if (typeof u.totalTokens === "number") usage.totalTokens = u.totalTokens;
  if (typeof u.maxTokens === "number") usage.maxTokens = u.maxTokens;
  if (typeof u.cacheReadTokens === "number") usage.cacheReadTokens = u.cacheReadTokens;
  if (typeof u.cacheCreationTokens === "number") usage.cacheCreationTokens = u.cacheCreationTokens;

  return usage;
}

function isTerminalToolCallStatus(status: unknown): boolean {
  return status === "completed" || status === "failed" || status === "canceled" || status === "rejected";
}

function isApprovalPendingUpdate(update: SessionUpdate): boolean {
  if (update.sessionUpdate !== "tool_call" && update.sessionUpdate !== "tool_call_update") {
    return false;
  }
  const rawOutput = (update as Record<string, unknown>).rawOutput;
  return Boolean(
    rawOutput &&
    typeof rawOutput === "object" &&
    "approval_state" in rawOutput &&
    (rawOutput as { approval_state?: unknown }).approval_state === "pending"
  );
}

function sessionUpdateTypeName(update: SessionUpdate): string {
  return update.sessionUpdate;
}

function toEventEnvelope(event: StreamInputEvent): SessionEventEnvelope {
  return {
    session_id: event.session_id,
    event_seq: event.event_seq,
    notification: event.notification,
    occurred_at_ms: event.occurred_at_ms ?? null,
    committed_at_ms: event.committed_at_ms ?? null,
    session_update_type: event.session_update_type ?? sessionUpdateTypeName(event.notification.update),
    turn_id: event.turn_id ?? null,
    entry_index: event.entry_index ?? null,
    tool_call_id: event.tool_call_id ?? null,
  };
}

function buildEntryId(event: SessionEventEnvelope, update: SessionUpdate): string {
  const toolCallId = event.tool_call_id ?? getToolCallId(update);
  if (toolCallId) {
    return `tool:${toolCallId}`;
  }
  const messageId = getMessageId(update);
  if (messageId && (
    update.sessionUpdate === "agent_message_chunk" ||
    update.sessionUpdate === "user_message_chunk" ||
    update.sessionUpdate === "agent_thought_chunk"
  )) {
    return `chunk:${update.sessionUpdate}:msg:${messageId}`;
  }
  const turnId = event.turn_id ?? getTurnId(update);
  const entryIndex = event.entry_index ?? getEntryIndex(update);
  if (turnId && entryIndex != null && (
    update.sessionUpdate === "agent_message_chunk" ||
    update.sessionUpdate === "user_message_chunk" ||
    update.sessionUpdate === "agent_thought_chunk"
  )) {
    return `chunk:${update.sessionUpdate}:${turnId}:${entryIndex}`;
  }
  return `event:${event.event_seq}`;
}

function makeDisplayEntry(event: SessionEventEnvelope, update: SessionUpdate): AcpDisplayEntry {
  return {
    id: buildEntryId(event, update),
    sessionId: event.notification.sessionId,
    timestamp: event.committed_at_ms ?? event.occurred_at_ms ?? Date.now(),
    eventSeq: event.event_seq,
    update,
    turnId: event.turn_id ?? getTurnId(update),
  };
}

function applyEventToEntries(prev: AcpDisplayEntry[], event: SessionEventEnvelope): AcpDisplayEntry[] {
  const notification: SessionNotification = event.notification;
  const { update } = notification;

  // ── tool_call ──────────────────────────────────────────────
  if (update.sessionUpdate === "tool_call") {
    const id = event.tool_call_id ?? getToolCallId(update);
    let existingIndex = -1;
    if (id) {
      for (let i = prev.length - 1; i >= 0; i -= 1) {
        if (getToolCallId(prev[i]!.update) === id) {
          existingIndex = i;
          break;
        }
      }
    }
    const isPending = isApprovalPendingUpdate(update);
    if (existingIndex >= 0) {
      const next = [...prev];
      next[existingIndex] = {
        ...prev[existingIndex]!,
        eventSeq: event.event_seq,
        update,
        turnId: prev[existingIndex]!.turnId ?? event.turn_id ?? getTurnId(update),
        isPendingApproval: isPending,
      };
      return next;
    }
    return [...prev, {
      ...makeDisplayEntry(event, update),
      isPendingApproval: isPending,
    }];
  }

  // ── tool_call_update ───────────────────────────────────────
  if (update.sessionUpdate === "tool_call_update") {
    const id = event.tool_call_id ?? getToolCallId(update);
    let existingIndex = -1;
    if (id) {
      for (let i = prev.length - 1; i >= 0; i -= 1) {
        if (getToolCallId(prev[i]!.update) === id) {
          existingIndex = i;
          break;
        }
      }
    }
    if (existingIndex >= 0) {
      const existingEntry = prev[existingIndex]!;
      const incomingStatus = (update as Record<string, unknown>).status;
      const merged = mergeToolCallUpdateIntoEntry(existingEntry.update, update);
      const incomingApprovalPending = isApprovalPendingUpdate(update);
      let nextPendingApproval = existingEntry.isPendingApproval;
      if (isTerminalToolCallStatus(incomingStatus)) {
        nextPendingApproval = false;
      } else if (incomingApprovalPending) {
        nextPendingApproval = true;
      } else if (incomingStatus === "in_progress") {
        nextPendingApproval = false;
      } else if (incomingStatus === "pending") {
        nextPendingApproval = false;
      }

      const next = [...prev];
      next[existingIndex] = {
        ...existingEntry,
        eventSeq: event.event_seq,
        update: merged,
        turnId: existingEntry.turnId ?? event.turn_id ?? getTurnId(update),
        isPendingApproval: nextPendingApproval,
      };
      return next;
    }
    return [...prev, {
      ...makeDisplayEntry(event, update),
      isPendingApproval: isApprovalPendingUpdate(update),
    }];
  }

  if (update.sessionUpdate === "session_info_update") {
    return [...prev, makeDisplayEntry(event, update)];
  }

  if (update.sessionUpdate === "usage_update") {
    return [...prev, makeDisplayEntry(event, update)];
  }

  if (update.sessionUpdate === "plan") {
    return [...prev, makeDisplayEntry(event, update)];
  }

  const isChunkUpdate =
    update.sessionUpdate === "agent_message_chunk" ||
    update.sessionUpdate === "user_message_chunk" ||
    update.sessionUpdate === "agent_thought_chunk";

  if (!isChunkUpdate) {
    return [...prev, makeDisplayEntry(event, update)];
  }

  const incomingTurnId = event.turn_id ?? getTurnId(update);
  const incomingEntryIndex = event.entry_index ?? getEntryIndex(update);
  const incomingMessageId = getMessageId(update);
  const newUpdateAny = update as unknown as { content?: { type?: string; text?: string } };
  const incomingText = newUpdateAny.content?.type === "text" ? (newUpdateAny.content.text ?? "") : null;

  if (incomingMessageId !== undefined && incomingText !== null) {
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const candidate = prev[i]!;
      if (candidate.update.sessionUpdate !== update.sessionUpdate) continue;
      if (getMessageId(candidate.update) !== incomingMessageId) continue;

      const candidateContent = extractTextContent(candidate.update);
      const previousText =
        typeof candidateContent?.text === "string" ? candidateContent.text : "";
      const mergedText = mergeStreamChunk(previousText, incomingText);
      if (mergedText === previousText) {
        return prev;
      }

      const overwrittenUpdate = replaceTextContentPreservingMeta(
        candidate.update,
        update,
        mergedText,
      );
      const next = [...prev];
      next[i] = { ...candidate, eventSeq: event.event_seq, update: overwrittenUpdate };
      return next;
    }
  }

  if (incomingTurnId !== undefined && incomingEntryIndex !== undefined && incomingText !== null) {
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const candidate = prev[i]!;
      if (candidate.update.sessionUpdate !== update.sessionUpdate) continue;
      if (candidate.turnId !== incomingTurnId) continue;
      const candidateEntryIndex = getEntryIndex(candidate.update);
      if (candidateEntryIndex !== incomingEntryIndex) continue;

      const candidateContent = extractTextContent(candidate.update);
      const previousText =
        typeof candidateContent?.text === "string" ? candidateContent.text : "";
      const mergedText = mergeStreamChunk(previousText, incomingText);
      if (mergedText === previousText) {
        return prev;
      }

      const overwrittenUpdate = replaceTextContentPreservingMeta(
        candidate.update,
        update,
        mergedText,
      );
      const next = [...prev];
      next[i] = { ...candidate, eventSeq: event.event_seq, update: overwrittenUpdate };
      return next;
    }
  }

  if (prev.length === 0) {
    return [makeDisplayEntry(event, update)];
  }

  const lastEntry = prev[prev.length - 1]!;
  if (lastEntry.update.sessionUpdate !== update.sessionUpdate) {
    return [...prev, makeDisplayEntry(event, update)];
  }

  // 缺少稳定 trace 锚点时不做“猜测式尾部合并”，避免跨 turn 误拼接。
  if (incomingTurnId === undefined || lastEntry.turnId === undefined) {
    return [...prev, makeDisplayEntry(event, update)];
  }
  if (lastEntry.turnId !== incomingTurnId) {
    return [...prev, makeDisplayEntry(event, update)];
  }
  const lastEntryIndex = getEntryIndex(lastEntry.update);
  if (
    incomingEntryIndex !== undefined &&
    lastEntryIndex !== undefined &&
    lastEntryIndex !== incomingEntryIndex
  ) {
    return [...prev, makeDisplayEntry(event, update)];
  }

  const lastContent = extractTextContent(lastEntry.update);
  const incomingContent = extractTextContent(update);
  if (!lastContent || !incomingContent) {
    return [...prev, makeDisplayEntry(event, update)];
  }

  const previousText = typeof lastContent.text === "string" ? lastContent.text : "";
  const mergedText = mergeStreamChunk(previousText, incomingText ?? "");

  if (mergedText === previousText) {
    return prev;
  }

  const mergedUpdate = replaceTextContentPreservingMeta(
    lastEntry.update,
    update,
    mergedText,
  );

  const next = [...prev];
  next[next.length - 1] = { ...lastEntry, eventSeq: event.event_seq, update: mergedUpdate };
  return next;
}

export function reduceStreamState(
  prev: AcpStreamState,
  incomingEvents: StreamInputEvent[],
): AcpStreamState {
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
    const usage = extractTokenUsage(event.notification.update);
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

export function useAcpStream(options: UseAcpStreamOptions): UseAcpStreamResult {
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

  const [streamState, setStreamState] = useState<AcpStreamState>(() =>
    createInitialState(normalizedInitialEntries),
  );
  const [isConnected, setIsConnected] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [isReceiving, setIsReceiving] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [connectKey, setConnectKey] = useState(0);

  const transportRef = useRef<AcpStreamTransport | null>(null);
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

    // 工具生命周期事件需要先提交一次开始态，避免在同一批流里被完成态覆盖。
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

    // 工具生命周期事件立即刷新，避免“开始/结束在同批次被压缩后只看到完成态”。
    const updateType = event.notification.update.sessionUpdate;
    const shouldFlushNow =
      updateType === "tool_call" || updateType === "tool_call_update";
    if (shouldFlushNow) {
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
          // 历史回放按页增量渲染，避免“整批回放后只看到终态”。
          setStreamState(nextState);
          stateRef.current = nextState;
          if (!page.has_more) {
            break;
          }
        }

        if (cancelled || !mountedRef.current) return;

        transportRef.current = createAcpStreamTransport({
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

export default useAcpStream;
