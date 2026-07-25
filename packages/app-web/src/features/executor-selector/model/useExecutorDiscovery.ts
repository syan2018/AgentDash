import { useCallback, useEffect, useRef, useState } from "react";
import { buildApiPath } from "../../../api/origin";
import { authenticatedFetch } from "../../../api/client";
import type {
  DiscoveryResponse,
  ExecutorInfo,
  UseExecutorDiscoveryResult,
} from "./types";

/**
 * 获取后端可用的执行器列表。
 *
 * 调用 GET /api/agents/discovery 获取所有已注册的执行器及其可用性信息。
 * 支持手动 refetch。
 */
export function useExecutorDiscovery(): UseExecutorDiscoveryResult {
  const [executors, setExecutors] = useState<ExecutorInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const fetchDiscovery = useCallback(async () => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    setIsLoading(true);
    setError(null);

    try {
      const res = await authenticatedFetch(buildApiPath("/agents/discovery"), {
        signal: controller.signal,
      });

      if (!res.ok) {
        throw new Error(`Discovery 请求失败: HTTP ${res.status}`);
      }

      const data: DiscoveryResponse = await res.json();
      setExecutors(data.executors);
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      setError(e instanceof Error ? e : new Error(String(e)));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    const timeoutId = window.setTimeout(() => { void fetchDiscovery(); }, 0);
    return () => {
      window.clearTimeout(timeoutId);
      abortRef.current?.abort();
    };
  }, [fetchDiscovery]);

  return { executors, isLoading, error, refetch: fetchDiscovery };
}
