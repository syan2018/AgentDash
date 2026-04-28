import { useCallback, useEffect, useRef, useState } from "react";
import type {
  ExecutorConfigSource,
  PersistedExecutorConfig,
  RecentExecutorEntry,
  UseExecutorConfigResult,
} from "./types";

// 当前唯一受支持的本地持久化格式。
const STORAGE_KEY = "agentdash:executor-config-v2";
const RECENT_KEY = "agentdash:recent-executors";
const MAX_RECENT = 8;
const DEFAULT_THINKING_LEVEL = "medium";

// 默认执行器标识
const DEFAULT_EXECUTOR = "PI_AGENT";

function isOptionalString(value: unknown): value is string | undefined {
  return value === undefined || typeof value === "string";
}

function loadPersistedConfig(): PersistedExecutorConfig | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
    const record = parsed as Record<string, unknown>;
    if (typeof record.executor !== "string") return null;
    if (
      !isOptionalString(record.providerId)
      || !isOptionalString(record.modelId)
      || !isOptionalString(record.thinkingLevel)
      || !isOptionalString(record.permissionPolicy)
    ) {
      return null;
    }

    return {
      executor: record.executor,
      providerId: record.providerId,
      modelId: record.modelId,
      thinkingLevel: record.thinkingLevel,
      permissionPolicy: record.permissionPolicy,
    };
  } catch {
    return null;
  }
}

function persistConfig(config: PersistedExecutorConfig): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
  } catch {
    // localStorage 不可用时静默失败
  }
}

function loadOrDefault(field: keyof PersistedExecutorConfig): string {
  const persisted = loadPersistedConfig()?.[field];
  if (persisted) return persisted;
  // 如果没有本地存储配置，返回默认值
  if (field === "executor") return DEFAULT_EXECUTOR;
  if (field === "thinkingLevel") return DEFAULT_THINKING_LEVEL;
  return "";
}

function loadRecentEntries(): RecentExecutorEntry[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    if (!raw) return [];
    return JSON.parse(raw) as RecentExecutorEntry[];
  } catch {
    return [];
  }
}

function persistRecentEntry(entry: RecentExecutorEntry): RecentExecutorEntry[] {
  try {
    const existing = loadRecentEntries().filter(
      (e) => !(
        e.executor === entry.executor
        && e.providerId === entry.providerId
        && e.modelId === entry.modelId
      ),
    );
    const updated = [entry, ...existing].slice(0, MAX_RECENT);
    localStorage.setItem(RECENT_KEY, JSON.stringify(updated));
    return updated;
  } catch {
    return [];
  }
}

function normalizeSource(source: ExecutorConfigSource | null | undefined): Partial<PersistedExecutorConfig> {
  if (!source) return {};
  const out: Partial<PersistedExecutorConfig> = {};
  if (typeof source.executor === "string" && source.executor.trim()) out.executor = source.executor.trim();
  if (typeof source.providerId === "string" && source.providerId.trim()) out.providerId = source.providerId.trim();
  if (typeof source.modelId === "string" && source.modelId.trim()) out.modelId = source.modelId.trim();
  if (typeof source.thinkingLevel === "string" && source.thinkingLevel.trim()) out.thinkingLevel = source.thinkingLevel.trim();
  if (typeof source.permissionPolicy === "string" && source.permissionPolicy.trim()) out.permissionPolicy = source.permissionPolicy.trim();
  return out;
}

export interface UseExecutorConfigOptions {
  /** 外部提供的初始配置（优先级高于 localStorage；仅挂载时生效） */
  initialSource?: ExecutorConfigSource | null;
}

/**
 * 管理执行器选择配置，并自动持久化到 localStorage（v2 格式）。
 *
 * v2 格式变更：reasoning_id 字段替换为 thinkingLevel（ThinkingLevel 枚举）。
 * 组件挂载时自动恢复上次保存的配置（通过 useState 初始化器）。
 * 切换 executor 时自动清除 modelId。
 * 支持最近使用记录追踪（LRU，最多 MAX_RECENT 条）。
 *
 * 初始优先级：`initialSource`（非空字段）> localStorage > 硬编码默认值。
 * 后续可用 `hydrate(source)` 在 session 切换时重新注入外部默认。
 */
export function useExecutorConfig(options?: UseExecutorConfigOptions): UseExecutorConfigResult {
  // useState 的 lazy init 只在首次挂载时运行一次，这里既读 options.initialSource
  // 也读 localStorage 并合并，避免在 render 期访问 ref。
  const [executor, setExecutorRaw] = useState(() => {
    const src = normalizeSource(options?.initialSource);
    return src.executor ?? loadOrDefault("executor");
  });
  const [providerId, setProviderIdRaw] = useState(() => {
    const src = normalizeSource(options?.initialSource);
    return src.providerId ?? loadOrDefault("providerId");
  });
  const [modelId, setModelIdRaw] = useState(() => {
    const src = normalizeSource(options?.initialSource);
    return src.modelId ?? loadOrDefault("modelId");
  });
  const [thinkingLevel, setThinkingLevelRaw] = useState(() => {
    const src = normalizeSource(options?.initialSource);
    return src.thinkingLevel ?? loadOrDefault("thinkingLevel");
  });
  const [permissionPolicy, setPolicyRaw] = useState(() => {
    const src = normalizeSource(options?.initialSource);
    return src.permissionPolicy ?? loadOrDefault("permissionPolicy");
  });
  const [recentEntries, setRecentEntries] = useState<RecentExecutorEntry[]>(() => loadRecentEntries());

  // persistedStateRef 跟踪"已持久化"的快照，仅在 effect / event handler 中访问。
  // 初值用已 useState 出来的局部变量构造（ref 初值仅在首次挂载有效）。
  const persistedStateRef = useRef<PersistedExecutorConfig>({
    executor,
    providerId,
    modelId,
    thinkingLevel,
    permissionPolicy,
  });
  // 初始 source 是否非空：仅挂载时一次性快照给 effect 使用
  const hasInitialSourceRef = useRef<boolean>(
    Object.keys(normalizeSource(options?.initialSource)).length > 0,
  );

  // 挂载时如来自外部 source，立即持久化一次，让下次刷新仍能命中
  useEffect(() => {
    if (hasInitialSourceRef.current) {
      persistConfig(persistedStateRef.current);
    }
    // 仅挂载时触发一次（ref 不需要进依赖）
  }, []);

  const persistPatch = useCallback(
    (patch: Partial<PersistedExecutorConfig>) => {
      const next: PersistedExecutorConfig = {
        executor: patch.executor ?? persistedStateRef.current.executor,
        providerId: patch.providerId ?? persistedStateRef.current.providerId,
        modelId: patch.modelId ?? persistedStateRef.current.modelId,
        thinkingLevel: patch.thinkingLevel ?? persistedStateRef.current.thinkingLevel,
        permissionPolicy: patch.permissionPolicy ?? persistedStateRef.current.permissionPolicy,
      };
      persistedStateRef.current = next;
      persistConfig(next);
    },
    [],
  );

  const setExecutor = useCallback(
    (v: string) => {
      setExecutorRaw(v);
      setProviderIdRaw("");
      setModelIdRaw("");
      setThinkingLevelRaw(DEFAULT_THINKING_LEVEL);
      setPolicyRaw("");
      persistPatch({
        executor: v,
        providerId: "",
        modelId: "",
        thinkingLevel: DEFAULT_THINKING_LEVEL,
        permissionPolicy: "",
      });
    },
    [persistPatch],
  );

  const setProviderId = useCallback(
    (v: string) => {
      setProviderIdRaw(v);
      persistPatch({ providerId: v });
    },
    [persistPatch],
  );

  const setModelId = useCallback(
    (v: string) => {
      setModelIdRaw(v);
      // 变更模型时，默认清空推理级别（由 UI 根据模型可选项重置）
      setThinkingLevelRaw("");
      persistPatch({ modelId: v, thinkingLevel: "" });
    },
    [persistPatch],
  );

  const setThinkingLevel = useCallback(
    (v: string) => {
      setThinkingLevelRaw(v);
      persistPatch({ thinkingLevel: v });
    },
    [persistPatch],
  );

  const setPermissionPolicy = useCallback(
    (v: string) => {
      setPolicyRaw(v);
      persistPatch({ permissionPolicy: v });
    },
    [persistPatch],
  );

  const recordUsage = useCallback(() => {
    if (!executor) return;
    const entry: RecentExecutorEntry = {
      executor,
      providerId: providerId || undefined,
      modelId: modelId || undefined,
      timestamp: Date.now(),
    };
    setRecentEntries(persistRecentEntry(entry));
  }, [executor, providerId, modelId]);

  const reset = useCallback(() => {
    setExecutorRaw(DEFAULT_EXECUTOR);
    setProviderIdRaw("");
    setModelIdRaw("");
    setThinkingLevelRaw(DEFAULT_THINKING_LEVEL);
    setPolicyRaw("");
    persistedStateRef.current = {
      executor: DEFAULT_EXECUTOR,
      providerId: "",
      modelId: "",
      thinkingLevel: DEFAULT_THINKING_LEVEL,
      permissionPolicy: "",
    };
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // noop
    }
  }, []);

  const hydrate = useCallback(
    (source: ExecutorConfigSource | null | undefined) => {
      const normalized = normalizeSource(source);
      if (Object.keys(normalized).length === 0) return;

      // 原子写入：跳过 setExecutor 的副作用，仅覆盖非空字段
      if (normalized.executor !== undefined) setExecutorRaw(normalized.executor);
      if (normalized.providerId !== undefined) setProviderIdRaw(normalized.providerId);
      if (normalized.modelId !== undefined) setModelIdRaw(normalized.modelId);
      if (normalized.thinkingLevel !== undefined) setThinkingLevelRaw(normalized.thinkingLevel);
      if (normalized.permissionPolicy !== undefined) setPolicyRaw(normalized.permissionPolicy);

      persistPatch(normalized);
    },
    [persistPatch],
  );

  return {
    executor,
    providerId,
    modelId,
    thinkingLevel,
    permissionPolicy,
    recentEntries,
    setExecutor,
    setProviderId,
    setModelId,
    setThinkingLevel,
    setPermissionPolicy,
    recordUsage,
    reset,
    hydrate,
  };
}
