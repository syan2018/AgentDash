/**
 * ACP 会话流管理 Hook
 *
 * 处理 Streaming HTTP（SSE/NDJSON）连接和 SessionNotification 消息流。
 * 采用 entries 数组作为唯一数据源（single source of truth），
 * tool_call / tool_call_update 直接原地合并到 entries 中。
 *
 * 对照 Zed 实现的关键行为：
 * - tool_call: upsert（按 toolCallId 查找，存在则更新，否则新建）
 * - tool_call_update: 合并到已有 entry；若找不到锚点则创建"孤立 update"条目
 * - agent_message_chunk / user_message_chunk / agent_thought_chunk: 合并相邻同类型同 turn 的 chunk
 * - session_info_update / usage_update: 直接添加新条目（不再丢弃）
 * - isPendingApproval: 仅在 status 为 "pending" 且尚未有后续非-pending 状态时保留
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type {
  SessionNotification,
  SessionUpdate,
} from "@agentclientprotocol/sdk";
import { cancelSession } from "../../../services/session";
import type { AcpDisplayEntry, TokenUsageInfo } from "./types";
import type { PromptSessionRequest } from "../../../services/executor";
import { createAcpStreamTransport, type AcpStreamTransport } from "./streamTransport";
import { extractAgentDashMetaFromUpdate } from "./agentdashMeta";

export interface UseAcpStreamOptions {
  sessionId: string;
  /** 设为 false 时跳过连接，返回空的初始状态。默认 true。 */
  enabled?: boolean;
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
  /** 最新的 token 用量信息（累计更新） */
  tokenUsage: TokenUsageInfo | null;
  reconnect: () => void;
  close: () => void;
  sendCancel: () => Promise<void>;
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

  if (incoming.startsWith(previous)) return incoming;
  if (previous.endsWith(incoming)) return previous;

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

function getTurnId(update: SessionUpdate): string | undefined {
  const meta = extractAgentDashMetaFromUpdate(update);
  return meta?.trace?.turnId ?? undefined;
}

function getEntryIndex(update: SessionUpdate): number | undefined {
  const meta = extractAgentDashMetaFromUpdate(update);
  const idx = meta?.trace?.entryIndex;
  return typeof idx === "number" ? idx : undefined;
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

  // ACP 标准字段
  if (typeof u.size === "number") usage.maxTokens = u.size;
  if (typeof u.used === "number") usage.totalTokens = u.used;

  // AgentDash 扩展字段
  if (typeof u.inputTokens === "number") usage.inputTokens = u.inputTokens;
  if (typeof u.outputTokens === "number") usage.outputTokens = u.outputTokens;
  if (typeof u.totalTokens === "number") usage.totalTokens = u.totalTokens;
  if (typeof u.maxTokens === "number") usage.maxTokens = u.maxTokens;
  if (typeof u.cacheReadTokens === "number") usage.cacheReadTokens = u.cacheReadTokens;
  if (typeof u.cacheCreationTokens === "number") usage.cacheCreationTokens = u.cacheCreationTokens;

  return usage;
}

/**
 * 判断 tool call 状态是否属于终态（不可再变更 isPendingApproval）
 */
function isTerminalToolCallStatus(status: unknown): boolean {
  return status === "completed" || status === "failed" || status === "canceled" || status === "rejected";
}

export function useAcpStream(options: UseAcpStreamOptions): UseAcpStreamResult {
  const {
    sessionId,
    enabled = true,
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
  const [tokenUsage, setTokenUsage] = useState<TokenUsageInfo | null>(null);

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
      turnId: getTurnId(u),
      ...extra,
    });

    // ── tool_call ──────────────────────────────────────────────
    // Zed 模式：upsert — 如果已有同 toolCallId 的 entry，则覆盖；否则新建
    if (update.sessionUpdate === "tool_call") {
      const id = getToolCallId(update)!;
      let existingIndex = -1;
      for (let i = prev.length - 1; i >= 0; i -= 1) {
        if (getToolCallId(prev[i]!.update) === id) {
          existingIndex = i;
          break;
        }
      }
      const isPending = update.status === "pending";
      if (existingIndex >= 0) {
        const next = [...prev];
        next[existingIndex] = {
          ...prev[existingIndex]!,
          update,
          turnId: prev[existingIndex]!.turnId ?? getTurnId(update),
          isPendingApproval: isPending,
        };
        return next;
      }
      return [...prev, makeEntry(update, { isPendingApproval: isPending })];
    }

    // ── tool_call_update ───────────────────────────────────────
    // Zed 模式：合并到已有 entry；若找不到锚点则创建孤立 update 条目
    if (update.sessionUpdate === "tool_call_update") {
      const id = getToolCallId(update)!;
      let existingIndex = -1;
      for (let i = prev.length - 1; i >= 0; i -= 1) {
        if (getToolCallId(prev[i]!.update) === id) {
          existingIndex = i;
          break;
        }
      }
      if (existingIndex >= 0) {
        const existingEntry = prev[existingIndex]!;
        const merged = mergeToolCallUpdateIntoEntry(existingEntry.update, update);
        const incomingStatus = (update as Record<string, unknown>).status;
        // isPendingApproval: 终态覆盖为 false；pending 状态设为 true；其余保留
        let nextPendingApproval = existingEntry.isPendingApproval;
        if (isTerminalToolCallStatus(incomingStatus)) {
          nextPendingApproval = false;
        } else if (incomingStatus === "pending") {
          nextPendingApproval = true;
        } else if (incomingStatus === "in_progress") {
          nextPendingApproval = false;
        }

        const next = [...prev];
        next[existingIndex] = {
          ...existingEntry,
          update: merged,
          turnId: existingEntry.turnId ?? getTurnId(update),
          isPendingApproval: nextPendingApproval,
        };
        return next;
      }
      // 孤立 update（找不到锚点 tool_call）：直接作为新条目添加
      return [...prev, makeEntry(update, {
        isPendingApproval: (update as Record<string, unknown>).status === "pending",
      })];
    }

    // ── session_info_update ────────────────────────────────────
    // 不再丢弃，作为条目添加（系统消息、错误、用户反馈等）
    if (update.sessionUpdate === "session_info_update") {
      return [...prev, makeEntry(update)];
    }

    // ── usage_update ───────────────────────────────────────────
    // 不再丢弃，作为条目添加（token 用量信息）
    if (update.sessionUpdate === "usage_update") {
      return [...prev, makeEntry(update)];
    }

    // ── plan ───────────────────────────────────────────────────
    if (update.sessionUpdate === "plan") {
      // 计划：直接添加（可以考虑 upsert，但协议层 plan 可多次发送）
      return [...prev, makeEntry(update)];
    }

    // ── chunk 类型（agent_message / user_message / agent_thought）───
    const isChunkUpdate =
      update.sessionUpdate === "agent_message_chunk" ||
      update.sessionUpdate === "user_message_chunk" ||
      update.sessionUpdate === "agent_thought_chunk";

    if (!isChunkUpdate) {
      return [...prev, makeEntry(update)];
    }

    const incomingTurnId = getTurnId(update);
    const incomingEntryIndex = getEntryIndex(update);
    const newUpdateAny = update as unknown as { content?: { type?: string; text?: string } };
    const incomingText = newUpdateAny.content?.type === "text" ? (newUpdateAny.content.text ?? "") : null;

    // ── entryIndex upsert：按 (turnId, entryIndex, sessionUpdate) 查找同一消息的已有 entry ──
    // MessageEnd 发出的全量快照和之前所有 TextDelta 共享相同的 entryIndex（entry_index 在
    // MessageEnd 之后才递增）。找到同一消息 → 用全量文本直接覆盖，不拼接。
    if (incomingTurnId !== undefined && incomingEntryIndex !== undefined && incomingText !== null) {
      for (let i = prev.length - 1; i >= 0; i -= 1) {
        const candidate = prev[i]!;
        if (candidate.update.sessionUpdate !== update.sessionUpdate) continue;
        if (candidate.turnId !== incomingTurnId) continue;
        const candidateEntryIndex = getEntryIndex(candidate.update);
        if (candidateEntryIndex !== incomingEntryIndex) continue;

        // 找到同一消息的 entry：覆盖文本（全量快照覆盖增量累积版本）
        const overwrittenUpdate: SessionUpdate = {
          ...candidate.update,
          content: { type: "text" as const, text: incomingText },
        } as SessionUpdate;
        const next = [...prev];
        next[i] = { ...candidate, update: overwrittenUpdate };
        return next;
      }
    }

    // ── 相邻合并：同类型 + 同 turn 的相邻 chunk 累积拼接（正常 delta 场景）──
    if (prev.length === 0) {
      return [makeEntry(update)];
    }

    const lastEntry = prev[prev.length - 1]!;
    if (lastEntry.update.sessionUpdate !== update.sessionUpdate) {
      return [...prev, makeEntry(update)];
    }

    if (incomingTurnId && lastEntry.turnId && lastEntry.turnId !== incomingTurnId) {
      return [...prev, makeEntry(update)];
    }

    const targetUpdateAny = lastEntry.update as unknown as { content?: { type?: string; text?: string } };
    if (targetUpdateAny.content?.type !== "text" || newUpdateAny.content?.type !== "text") {
      return [...prev, makeEntry(update)];
    }

    const previousText = targetUpdateAny.content.text ?? "";
    const mergedText = mergeStreamChunk(previousText, incomingText ?? "");

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

  const enqueueNotificationRef = useRef<(n: SessionNotification) => void>(null!);

  const enqueueNotification = useCallback((notification: SessionNotification) => {
    pendingNotificationsRef.current.push(notification);
    markReceiving();

    // 实时更新 token usage（不等 flush）
    const usage = extractTokenUsage(notification.update);
    if (usage) {
      setTokenUsage((prev) => (prev ? { ...prev, ...usage } : usage));
    }

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

  enqueueNotificationRef.current = enqueueNotification;

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
      setEntries([]);
      setIsLoading(false);
      setError(null);
      setIsConnected(false);
      setTokenUsage(null);
      return () => {
        mountedRef.current = false;
      };
    }

    setEntries(initialEntries);
    setIsLoading(true);
    setError(null);
    setIsConnected(false);
    setTokenUsage(null);

    if (transportRef.current) {
      transportRef.current.close();
      transportRef.current = null;
    }

    transportRef.current = createAcpStreamTransport({
      sessionId,
      endpoint,
      onNotification: (notification) => {
        if (!mountedRef.current) return;
        enqueueNotificationRef.current(notification);
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
  }, [sessionId, endpoint, connectKey, enabled]);

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
    setTokenUsage(null);
    setConnectKey((k) => k + 1);
  }, []);

  return {
    entries,
    isConnected,
    isLoading,
    isReceiving,
    error,
    tokenUsage,
    reconnect,
    close,
    sendCancel,
  };
}

export default useAcpStream;
