import { useEffect, useRef, useState } from "react";
import { applyPatch } from "fast-json-patch";
import { buildApiPath } from "../../../api/origin";
import type {
  ExecutorDiscoveryStreamState,
  ExecutorDiscoveredOptions,
  UseExecutorDiscoveredOptionsResult,
} from "./types";

function toWsUrl(httpUrl: string): string {
  if (httpUrl.startsWith("https://")) return httpUrl.replace("https://", "wss://");
  if (httpUrl.startsWith("http://")) return httpUrl.replace("http://", "ws://");
  // relative（这里不做转换，调用方应先用 URL() 解析为绝对 http(s) 再转 ws）
  return httpUrl;
}

const INITIAL_STATE: ExecutorDiscoveryStreamState = {
  options: {
    model_selector: {
      providers: [],
      models: [],
      default_model: null,
      agents: [],
      permissions: [],
    },
    slash_commands: [],
    loading_models: true,
    loading_agents: true,
    loading_slash_commands: true,
    error: null,
  } satisfies ExecutorDiscoveredOptions,
  commands: [],
  discovering: false,
  error: null,
};

type ServerMessage =
  | { Ready: boolean }
  | { JsonPatch: unknown }
  | { finished: boolean }
  | { Error: string };

type JsonPatchOp = {
  op: "add" | "remove" | "replace" | "move" | "copy" | "test";
  path: string;
  value?: unknown;
  from?: string;
};

/**
 * 对齐 vibe-kanban：通过 WebSocket 接收 JsonPatch 增量更新 discovered options。
 */
export function useExecutorDiscoveredOptions(
  executor: string,
  variant: string,
): UseExecutorDiscoveredOptionsResult {
  const [options, setOptions] = useState<ExecutorDiscoveryStreamState["options"]>(null);
  const [isConnected, setIsConnected] = useState(false);
  const [isInitialized, setIsInitialized] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [reconnectNonce, setReconnectNonce] = useState(0);

  const wsRef = useRef<WebSocket | null>(null);
  const stateRef = useRef<ExecutorDiscoveryStreamState>({ ...INITIAL_STATE });

  useEffect(() => {
    const trimmed = executor.trim();
    if (!trimmed) return;

    wsRef.current?.close();

    const http = new URL(buildApiPath("/agents/discovered-options/ws"), window.location.href).toString();
    const url = new URL(toWsUrl(http));
    url.searchParams.set("executor", trimmed);
    const v = variant.trim();
    if (v) url.searchParams.set("variant", v);

    const ws = new WebSocket(url.toString());
    wsRef.current = ws;

    ws.onopen = () => {
      stateRef.current = { ...INITIAL_STATE };
      setOptions(stateRef.current.options);
      setIsConnected(true);
      setIsInitialized(false);
      setError(null);
    };
    ws.onerror = () => setError(new Error("WebSocket 连接失败"));
    ws.onclose = () => setIsConnected(false);
    ws.onmessage = (evt) => {
      try {
        const msg: ServerMessage = JSON.parse(String(evt.data));
        if ("Ready" in msg) {
          setIsInitialized(Boolean(msg.Ready));
          return;
        }
        if ("Error" in msg) {
          setError(new Error(msg.Error));
          return;
        }
        if ("finished" in msg) {
          return;
        }
        if ("JsonPatch" in msg) {
          const patch = msg.JsonPatch as JsonPatchOp[];
          const next = structuredClone(stateRef.current);
          // 注意：fast-json-patch 在 mutateDocument=false 时不会修改入参，需要使用返回的 newDocument
          const result = applyPatch(next, patch, true, false);
          const updated = result.newDocument as ExecutorDiscoveryStreamState;
          stateRef.current = updated;
          setOptions(updated.options);
        }
      } catch (e) {
        setError(e instanceof Error ? e : new Error(String(e)));
      }
    };

    return () => ws.close();
  }, [executor, variant, reconnectNonce]);

  return {
    options,
    isConnected,
    isInitialized,
    error,
    reconnect: () => setReconnectNonce((n) => n + 1),
  };
}

