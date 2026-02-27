/**
 * ACP 会话流管理 Hook
 *
 * 处理 Streaming HTTP（SSE）连接和 SessionNotification 消息流
 * 支持消息聚合、工具调用状态跟踪、批处理刷新与去重合并
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type {
  SessionNotification,
  SessionUpdate,
  ToolCall,
  ToolCallUpdate,
} from "@agentclientprotocol/sdk";
import { buildApiPath } from "../../../api/origin";
import type { AcpDisplayEntry, AcpToolCallState } from "./types";
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
  toolStates: Map<string, AcpToolCallState>;
  isConnected: boolean;
  isLoading: boolean;
  error: Error | null;
  reconnect: () => void;
  close: () => void;
  sendCancel: () => void;
}

const FLUSH_INTERVAL_MS = 50;

function mergeStreamChunk(previous: string, incoming: string): string {
  if (!incoming) return previous;
  if (!previous) return incoming;
  if (incoming === previous) return previous;

  // 子串/超串保护：一些 provider 会重复发送“相同片段但位置不同”的快照
  if (previous.includes(incoming)) return previous;
  if (incoming.includes(previous)) return incoming;

  // 有些 provider 会发送“完整快照”chunk
  if (incoming.startsWith(previous)) return incoming;
  // 有些 provider 会重复发送尾部 chunk
  if (previous.endsWith(incoming)) return previous;

  // 通过 overlap 合并，避免边界重复
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

export function useAcpStream(options: UseAcpStreamOptions): UseAcpStreamResult {
  const {
    sessionId,
    endpoint,
    initialEntries = [],
    // executeRequest：SSE 为单向流，不支持在同一连接上发送 execute（由上层 HTTP prompt 触发）
    // 保留该字段是为了兼容未来可能的 transport 切换
    onEntry,
    onConnectionChange,
    onError,
  } = options;

  const [entries, setEntries] = useState<AcpDisplayEntry[]>(initialEntries);
  const [toolStates, setToolStates] = useState<Map<string, AcpToolCallState>>(new Map());
  const [isConnected, setIsConnected] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [connectKey, setConnectKey] = useState(0);

  const transportRef = useRef<AcpStreamTransport | null>(null);
  const mountedRef = useRef(true);
  const pendingNotificationsRef = useRef<SessionNotification[]>([]);
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const toolStatesRef = useRef<Map<string, AcpToolCallState>>(new Map());

  const callbackRefs = useRef({ onEntry, onConnectionChange, onError });
  useEffect(() => {
    callbackRefs.current = { onEntry, onConnectionChange, onError };
  }, [onEntry, onConnectionChange, onError]);

  const handleToolCall = useCallback((update: SessionUpdate & { sessionUpdate: "tool_call" }) => {
    const toolCall: ToolCall = {
      toolCallId: update.toolCallId,
      title: update.title,
      kind: update.kind,
      status: update.status,
      content: update.content,
      locations: update.locations,
      rawInput: update.rawInput,
      rawOutput: update.rawOutput,
      _meta: update._meta,
    };

    const next = new Map(toolStatesRef.current);
    next.set(update.toolCallId, {
      toolCallId: update.toolCallId,
      call: toolCall,
      updates: [],
      status: update.status ?? "pending",
    });
    toolStatesRef.current = next;
    setToolStates(next);

    return toolCall;
  }, []);

  const handleToolCallUpdate = useCallback((update: SessionUpdate & { sessionUpdate: "tool_call_update" }) => {
    const toolUpdate: ToolCallUpdate = {
      toolCallId: update.toolCallId,
      title: update.title,
      kind: update.kind,
      status: update.status,
      content: update.content,
      locations: update.locations,
      rawInput: update.rawInput,
      rawOutput: update.rawOutput,
      _meta: update._meta,
    };

    const next = new Map(toolStatesRef.current);
    const existing = next.get(update.toolCallId);
    if (existing) {
      const updatedState: AcpToolCallState = {
        ...existing,
        updates: [...existing.updates, toolUpdate],
        status: update.status ?? existing.status,
      };
      if (update.status === "completed" || update.status === "failed") {
        updatedState.finalResult = update.rawOutput;
      }
      next.set(update.toolCallId, updatedState);
      toolStatesRef.current = next;
      setToolStates(next);
    }

    return toolUpdate;
  }, []);

  const applyNotification = useCallback((prev: AcpDisplayEntry[], notification: SessionNotification) => {
    const { update } = notification;
    const newEntry: AcpDisplayEntry = {
      id: generateId(),
      sessionId: notification.sessionId,
      timestamp: Date.now(),
      update,
      isStreaming: true,
      isPendingApproval: false,
    };

    if (update.sessionUpdate === "tool_call") {
      handleToolCall(update);
      newEntry.isPendingApproval = update.status === "pending";
      return [...prev, newEntry];
    }

    if (update.sessionUpdate === "tool_call_update") {
      handleToolCallUpdate(update);
      // 查找是否已有相同 toolCallId 的条目（tool_call 或 tool_call_update）
      const existingIndex = prev.findIndex(
        (e) =>
          (e.update.sessionUpdate === "tool_call" || e.update.sessionUpdate === "tool_call_update") &&
          (e.update as { toolCallId?: string }).toolCallId === update.toolCallId
      );
      if (existingIndex >= 0) {
        // 不添加新条目，只更新状态
        return prev;
      }
      newEntry.isPendingApproval = update.status === "pending";
      return [...prev, newEntry];
    }

    const isChunkUpdate =
      update.sessionUpdate === "agent_message_chunk" ||
      update.sessionUpdate === "user_message_chunk" ||
      update.sessionUpdate === "agent_thought_chunk";

    if (!isChunkUpdate) {
      return [...prev, newEntry];
    }

    // turn 边界：用最近一次 user_message_chunk 切分，避免跨 turn 合并 agent chunk
    let lastUserIndex = -1;
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      if (prev[i]?.update.sessionUpdate === "user_message_chunk") {
        lastUserIndex = i;
        break;
      }
    }

    let targetIndex = -1;
    const scanStart = prev.length - 1;
    const scanEndExclusive =
      update.sessionUpdate === "agent_message_chunk" || update.sessionUpdate === "agent_thought_chunk"
        ? Math.max(lastUserIndex, -1)
        : -1;

    for (let i = scanStart; i > scanEndExclusive; i -= 1) {
      if (prev[i]?.update.sessionUpdate === update.sessionUpdate) {
        targetIndex = i;
        break;
      }
    }

    if (targetIndex === -1) {
      return [...prev, newEntry];
    }

    const target = prev[targetIndex]!;
    const targetUpdateAny = target.update as unknown as { content?: { type?: string; text?: string } };
    const newUpdateAny = update as unknown as { content?: { type?: string; text?: string } };
    if (targetUpdateAny.content?.type !== "text" || newUpdateAny.content?.type !== "text") {
      return [...prev, newEntry];
    }

    const previousText = targetUpdateAny.content.text ?? "";
    const incomingText = newUpdateAny.content.text ?? "";
    const mergedText = mergeStreamChunk(previousText, incomingText);

    // 重要：如果合并后文本不变，不触发 state 更新，避免“卡死/不停刷新”的表现
    if (mergedText === previousText) {
      return prev;
    }

    const mergedUpdate: SessionUpdate = {
      ...target.update,
      content: { type: "text" as const, text: mergedText },
    } as SessionUpdate;

    const next = [...prev];
    next[targetIndex] = { ...target, update: mergedUpdate, isStreaming: true };
    return next;
  }, [handleToolCall, handleToolCallUpdate]);

  // 使用 ref 存储 applyNotification 避免依赖循环
  const applyNotificationRef = useRef(applyNotification);
  applyNotificationRef.current = applyNotification;

  const enqueueNotification = useCallback((notification: SessionNotification) => {
    pendingNotificationsRef.current.push(notification);
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
  }, []);

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

  /**
   * sessionId/endpoint 变化时：关闭旧连接 → 重置状态 → 建立新连接（transport）
   * transport 默认优先 NDJSON(fetch streaming)，失败后自动降级到 SSE
   */
  useEffect(() => {
    mountedRef.current = true;

    setEntries(initialEntries);
    setToolStates(new Map());
    toolStatesRef.current = new Map();
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
    setToolStates(new Map());
    toolStatesRef.current = new Map();
    setError(null);
    setIsLoading(true);
    setIsConnected(false);
    setConnectKey((k) => k + 1);
  }, []);

  return {
    entries,
    toolStates,
    isConnected,
    isLoading,
    error,
    reconnect,
    close,
    sendCancel,
  };
}

export default useAcpStream;
