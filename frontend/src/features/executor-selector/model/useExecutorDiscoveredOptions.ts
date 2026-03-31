import { useEffect, useRef, useState } from "react";
import { applyPatch } from "fast-json-patch";
import type { Operation } from "fast-json-patch";
import { buildApiPath } from "../../../api/origin";
import { authenticatedFetch } from "../../../api/client";
import type {
  ExecutorDiscoveryStreamState,
  ExecutorDiscoveredOptions,
  UseExecutorDiscoveredOptionsResult,
} from "./types";

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

/**
 * 通过 NDJSON over HTTP 接收 JsonPatch 增量更新 discovered options。
 * 消息格式与原 WebSocket 端点保持一致：Ready / JsonPatch / finished / Error。
 */
export function useExecutorDiscoveredOptions(
  executor: string,
  variant: string,
  refreshKey = 0,
): UseExecutorDiscoveredOptionsResult {
  const [options, setOptions] = useState<ExecutorDiscoveryStreamState["options"]>(null);
  const [isConnected, setIsConnected] = useState(false);
  const [isInitialized, setIsInitialized] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [reconnectNonce, setReconnectNonce] = useState(0);

  const abortRef = useRef<AbortController | null>(null);
  const stateRef = useRef<ExecutorDiscoveryStreamState>({ ...INITIAL_STATE });

  useEffect(() => {
    const trimmed = executor.trim();
    if (!trimmed) return;

    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    stateRef.current = { ...INITIAL_STATE };
    setOptions(stateRef.current.options);
    setIsConnected(false);
    setIsInitialized(false);
    setError(null);

    const base = buildApiPath("/agents/discovered-options/stream");
    const url = new URL(base, window.location.href);
    url.searchParams.set("executor", trimmed);
    const v = variant.trim();
    if (v) url.searchParams.set("variant", v);

    void (async () => {
      try {
        const response = await authenticatedFetch(url.toString(), {
          method: "GET",
          headers: { Accept: "application/x-ndjson", "Cache-Control": "no-cache" },
          signal: controller.signal,
          cache: "no-store",
        });

        if (!response.ok || !response.body) {
          setError(new Error(`发现选项请求失败: HTTP ${response.status}`));
          return;
        }

        setIsConnected(true);

        const decoder = new TextDecoder();
        const reader = response.body.getReader();
        let buffer = "";

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          buffer += decoder.decode(value, { stream: true });
          let newlineIdx = buffer.indexOf("\n");
          while (newlineIdx >= 0) {
            const line = buffer.slice(0, newlineIdx).trim();
            buffer = buffer.slice(newlineIdx + 1);
            if (line) handleLine(line);
            newlineIdx = buffer.indexOf("\n");
          }
        }

        const trailing = buffer.trim();
        if (trailing) handleLine(trailing);

      } catch (e) {
        if (e instanceof DOMException && e.name === "AbortError") return;
        setError(e instanceof Error ? e : new Error(String(e)));
      } finally {
        setIsConnected(false);
      }
    })();

    function handleLine(line: string) {
      let msg: ServerMessage;
      try {
        msg = JSON.parse(line) as ServerMessage;
      } catch {
        return;
      }

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
        if (!Array.isArray(msg.JsonPatch)) return;
        const patch = msg.JsonPatch as Operation[];
        const next = structuredClone(stateRef.current);
        const result = applyPatch(next, patch, true, false);
        const updated = result.newDocument as ExecutorDiscoveryStreamState;
        stateRef.current = updated;
        setOptions(updated.options);
      }
    }

    return () => {
      controller.abort();
    };
  }, [executor, variant, reconnectNonce, refreshKey]);

  return {
    options,
    isConnected,
    isInitialized,
    error,
    reconnect: () => setReconnectNonce((n) => n + 1),
  };
}
