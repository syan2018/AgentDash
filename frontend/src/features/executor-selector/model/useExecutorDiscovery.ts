import { useCallback, useEffect, useRef, useState } from "react";
import { buildApiPath } from "../../../api/origin";
import type {
  ConnectorInfo,
  DiscoveryResponse,
  ExecutorInfo,
  UseExecutorDiscoveryResult,
} from "./types";

/**
 * 获取后端可用的执行器列表和连接器信息
 *
 * 调用 GET /api/agents/discovery 获取当前连接器能力、
 * 所有已注册的执行器及其变体、可用性信息。
 * 支持手动 refetch。
 */
export function useExecutorDiscovery(): UseExecutorDiscoveryResult {
  const [connector, setConnector] = useState<ConnectorInfo | null>(null);
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
      const res = await fetch(buildApiPath("/agents/discovery"), {
        signal: controller.signal,
      });

      if (!res.ok) {
        throw new Error(`Discovery 请求失败: HTTP ${res.status}`);
      }

      const data: DiscoveryResponse = await res.json();
      setConnector(data.connector);
      setExecutors(data.executors);
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      setError(e instanceof Error ? e : new Error(String(e)));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void fetchDiscovery();
    return () => abortRef.current?.abort();
  }, [fetchDiscovery]);

  return { connector, executors, isLoading, error, refetch: fetchDiscovery };
}
