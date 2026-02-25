/**
 * ACP 会话流管理 Hook
 *
 * 处理 WebSocket 连接和 SessionNotification 消息流
 * 支持消息聚合和工具调用状态跟踪
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type {
  SessionNotification,
  SessionUpdate,
  ToolCall,
  ToolCallUpdate,
} from "@agentclientprotocol/sdk";
import type { AcpDisplayEntry, AcpToolCallState } from "./types";

export interface UseAcpStreamOptions {
  sessionId: string;
  endpoint?: string;
  initialEntries?: AcpDisplayEntry[];
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
}

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

export function useAcpStream(options: UseAcpStreamOptions): UseAcpStreamResult {
  const { sessionId, endpoint, initialEntries = [], onEntry, onConnectionChange, onError } = options;

  const [entries, setEntries] = useState<AcpDisplayEntry[]>(initialEntries);
  const [toolStates, setToolStates] = useState<Map<string, AcpToolCallState>>(new Map());
  const [isConnected, setIsConnected] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const wsRef = useRef<WebSocket | null>(null);
  const entriesRef = useRef<AcpDisplayEntry[]>(initialEntries);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    entriesRef.current = entries;
  }, [entries]);

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

    setToolStates((prev) => {
      const next = new Map(prev);
      next.set(update.toolCallId, {
        toolCallId: update.toolCallId,
        call: toolCall,
        updates: [],
        status: update.status ?? "pending",
      });
      return next;
    });

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

    setToolStates((prev) => {
      const next = new Map(prev);
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
      }
      return next;
    });

    return toolUpdate;
  }, []);

  const processUpdate = useCallback((notification: SessionNotification) => {
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
    }

    if (update.sessionUpdate === "tool_call_update") {
      handleToolCallUpdate(update);
      newEntry.isPendingApproval = update.status === "pending";
    }

    const lastEntry = entriesRef.current[entriesRef.current.length - 1];
    const canMergeChunks = lastEntry && (
      (lastEntry.update.sessionUpdate === "agent_message_chunk" && update.sessionUpdate === "agent_message_chunk") ||
      (lastEntry.update.sessionUpdate === "user_message_chunk" && update.sessionUpdate === "user_message_chunk") ||
      (lastEntry.update.sessionUpdate === "agent_thought_chunk" && update.sessionUpdate === "agent_thought_chunk")
    );

    if (canMergeChunks) {
      setEntries((prev) => {
        const next = [...prev];
        const last = next[next.length - 1];
        if (last) {
          const lastContent = last.update as { content: { type: string; text?: string } };
          const newContent = update as { content: { type: string; text?: string } };
          if (lastContent.content.type === "text" && newContent.content.type === "text") {
            last.update = {
              ...last.update,
              content: {
                type: "text" as const,
                text: (lastContent.content.text ?? "") + (newContent.content.text ?? ""),
              },
            } as SessionUpdate;
          }
        }
        return next;
      });
    } else {
      setEntries((prev) => [...prev, newEntry]);
      onEntry?.(newEntry);
    }
  }, [handleToolCall, handleToolCallUpdate, onEntry]);

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      return;
    }

    setIsLoading(true);
    setError(null);

    const wsUrl = endpoint ?? `/api/acp/sessions/${sessionId}/stream`;
    const url = wsUrl.replace(/^http/, "ws");

    try {
      const ws = new WebSocket(url);
      wsRef.current = ws;

      ws.addEventListener("open", () => {
        setIsConnected(true);
        setIsLoading(false);
        onConnectionChange?.(true);
      });

      ws.addEventListener("message", (event) => {
        try {
          const notification: SessionNotification = JSON.parse(event.data);
          processUpdate(notification);
        } catch (err) {
          console.error("Failed to parse ACP message:", err);
          onError?.(new Error("Failed to parse ACP message"));
        }
      });

      ws.addEventListener("close", () => {
        setIsConnected(false);
        onConnectionChange?.(false);
      });

      ws.addEventListener("error", () => {
        const wsError = new Error("WebSocket error");
        setError(wsError);
        onError?.(wsError);
      });
    } catch (err) {
      const connectError = err instanceof Error ? err : new Error("Failed to connect");
      setError(connectError);
      setIsLoading(false);
      onError?.(connectError);
    }
  }, [sessionId, endpoint, processUpdate, onConnectionChange, onError]);

  const close = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }

    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }

    setIsConnected(false);
  }, []);

  const reconnect = useCallback(() => {
    close();
    setEntries([]);
    setToolStates(new Map());
    connect();
  }, [close, connect]);

  const hasConnectedRef = useRef(false);

  useEffect(() => {
    if (!hasConnectedRef.current) {
      hasConnectedRef.current = true;
      const frameId = requestAnimationFrame(() => {
        connect();
      });

      return () => {
        cancelAnimationFrame(frameId);
        close();
      };
    }

    return () => {
      close();
    };
  }, [connect, close]);

  return {
    entries,
    toolStates,
    isConnected,
    isLoading,
    error,
    reconnect,
    close,
  };
}

export default useAcpStream;
