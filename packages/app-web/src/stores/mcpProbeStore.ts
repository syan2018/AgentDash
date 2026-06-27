/**
 * MCP probe 结果缓存。
 *
 * 设计动机：MCP Preset 卡片以前在挂载时自动 probe，N 张卡 → N 个临时 rmcp client，
 * 每次切到 Assets/MCP Preset 页都要重跑一遍；切走但请求未结束时会挤占 axum 线程池，
 * 用户感知为"切其他 Tab 卡住"。
 *
 * 缓存策略：
 * - key = `${projectId}::${digest(transport + runtime_binding)}`，以 probe 输入内容为指纹
 * - 默认不自动 probe；UI 进入时只读取缓存，无缓存则显示"尚未探测"
 * - 用户点击"重新检测" / "Test Connection" 才真正发请求并写入缓存
 *
 * 不持久化到 localStorage：探测结果属于运行期状态，跨刷新无意义。
 */
import { create } from "zustand";

import { probeMcpTransport } from "../services/mcpPreset";
import type {
  McpRuntimeBindingConfig,
  McpTransportConfig,
  ProbeMcpPresetResponse,
} from "../types";

interface CacheEntry {
  result: ProbeMcpPresetResponse;
  fetchedAt: number;
}

interface McpProbeState {
  cache: Record<string, CacheEntry>;
  /** 同一 key 的进行中请求；用于去重并发探测。 */
  inflight: Record<string, Promise<ProbeMcpPresetResponse>>;
  getCached: (
    projectId: string,
    transport: McpTransportConfig,
    runtimeBinding?: McpRuntimeBindingConfig | null,
  ) => ProbeMcpPresetResponse | null;
  /** 先读同 transport 缓存；未命中时再触发去重 probe。 */
  getOrRefresh: (
    projectId: string,
    transport: McpTransportConfig,
    runtimeBinding?: McpRuntimeBindingConfig | null,
  ) => Promise<ProbeMcpPresetResponse>;
  /** 触发一次 probe 并把结果写入缓存。同 key 并发请求会去重。 */
  refresh: (
    projectId: string,
    transport: McpTransportConfig,
    runtimeBinding?: McpRuntimeBindingConfig | null,
  ) => Promise<ProbeMcpPresetResponse>;
  invalidate: (projectId: string, transport?: McpTransportConfig) => void;
}

function buildKey(
  projectId: string,
  transport: McpTransportConfig,
  runtimeBinding?: McpRuntimeBindingConfig | null,
): string {
  return `${projectId}::${digestTransport(transport)}::${digestRuntimeBinding(runtimeBinding)}`;
}

/**
 * 按字段顺序稳定地 stringify transport，避免对象字段顺序差异导致的 cache miss。
 *
 * MCP transport 的字段集合是收敛的（type, url/command, headers/args/env），
 * 直接 JSON.stringify 在 ES2020+ 里属性序列化顺序与插入顺序一致，
 * 但保险起见对 array 字段排序后再 stringify。
 */
function digestTransport(transport: McpTransportConfig): string {
  if (transport.type === "http" || transport.type === "sse") {
    const headers = (transport.headers ?? [])
      .slice()
      .sort((a, b) => a.name.localeCompare(b.name))
      .map((h) => `${h.name}=${h.value}`);
    return JSON.stringify({ type: transport.type, url: transport.url, headers });
  }
  // stdio
  const env = (transport.env ?? [])
    .slice()
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((e) => `${e.name}=${e.value}`);
  return JSON.stringify({
    type: "stdio",
    command: transport.command,
    args: transport.args ?? [],
    env,
    cwd: transport.cwd ?? "",
  });
}

function digestRuntimeBinding(runtimeBinding?: McpRuntimeBindingConfig | null): string {
  if (!runtimeBinding || (runtimeBinding.bindings ?? []).length === 0) {
    return "runtime_binding:none";
  }
  return JSON.stringify({
    mount_id: runtimeBinding.mount_id ?? "",
    bindings: runtimeBinding.bindings ?? [],
  });
}

export const useMcpProbeStore = create<McpProbeState>((set, get) => ({
  cache: {},
  inflight: {},

  getCached: (projectId, transport, runtimeBinding) => {
    const key = buildKey(projectId, transport, runtimeBinding);
    return get().cache[key]?.result ?? null;
  },

  getOrRefresh: (projectId, transport, runtimeBinding) => {
    const cached = get().getCached(projectId, transport, runtimeBinding);
    if (cached) return Promise.resolve(cached);
    return get().refresh(projectId, transport, runtimeBinding);
  },

  refresh: (projectId, transport, runtimeBinding) => {
    const key = buildKey(projectId, transport, runtimeBinding);
    const existing = get().inflight[key];
    if (existing) return existing;

    const promise = (async () => {
      try {
        const result = await probeMcpTransport(projectId, transport, runtimeBinding);
        set((state) => ({
          cache: { ...state.cache, [key]: { result, fetchedAt: Date.now() } },
        }));
        return result;
      } catch (err) {
        const errorResult: ProbeMcpPresetResponse = {
          status: "error",
          error: err instanceof Error ? err.message : String(err),
        };
        set((state) => ({
          cache: { ...state.cache, [key]: { result: errorResult, fetchedAt: Date.now() } },
        }));
        return errorResult;
      } finally {
        set((state) => {
          const next = { ...state.inflight };
          delete next[key];
          return { inflight: next };
        });
      }
    })();

    set((state) => ({ inflight: { ...state.inflight, [key]: promise } }));
    return promise;
  },

  invalidate: (projectId, transport) => {
    if (!transport) {
      const prefix = `${projectId}::`;
      set((state) => {
        const nextCache = { ...state.cache };
        for (const key of Object.keys(nextCache)) {
          if (key.startsWith(prefix)) delete nextCache[key];
        }
        return { cache: nextCache };
      });
      return;
    }
    const key = buildKey(projectId, transport);
    set((state) => {
      const nextCache = { ...state.cache };
      delete nextCache[key];
      return { cache: nextCache };
    });
  },
}));
