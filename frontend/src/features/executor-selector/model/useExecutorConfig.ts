import { useCallback, useRef, useState } from "react";
import type { PersistedExecutorConfig, RecentExecutorEntry, UseExecutorConfigResult } from "./types";

// v2 格式 key：包含 thinkingLevel 字段（旧 v1 key 包含 reasoningId，自动丢弃不迁移）
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
    // 检测是否为 v1 格式（含 reasoningId 字段），若是则丢弃返回 null
    if ("reasoningId" in record) return null;
    if (typeof record.executor !== "string") return null;
    if (
      !isOptionalString(record.variant)
      || !isOptionalString(record.providerId)
      || !isOptionalString(record.modelId)
      || !isOptionalString(record.thinkingLevel)
      || !isOptionalString(record.permissionPolicy)
    ) {
      return null;
    }

    return {
      executor: record.executor,
      variant: record.variant,
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

/**
 * 管理执行器选择配置，并自动持久化到 localStorage（v2 格式）。
 *
 * v2 格式变更：reasoning_id 字段替换为 thinkingLevel（ThinkingLevel 枚举）。
 * 组件挂载时自动恢复上次保存的配置（通过 useState 初始化器）。
 * 切换 executor 时自动清除 variant / modelId。
 * 支持最近使用记录追踪（LRU，最多 MAX_RECENT 条）。
 */
export function useExecutorConfig(): UseExecutorConfigResult {
  const initialConfig = {
    executor: loadOrDefault("executor"),
    variant: loadOrDefault("variant"),
    providerId: loadOrDefault("providerId"),
    modelId: loadOrDefault("modelId"),
    thinkingLevel: loadOrDefault("thinkingLevel"),
    permissionPolicy: loadOrDefault("permissionPolicy"),
  };
  const persistedStateRef = useRef<PersistedExecutorConfig>({ ...initialConfig });

  const [executor, setExecutorRaw] = useState(initialConfig.executor);
  const [variant, setVariantRaw] = useState(initialConfig.variant ?? "");
  const [providerId, setProviderIdRaw] = useState(initialConfig.providerId ?? "");
  const [modelId, setModelIdRaw] = useState(initialConfig.modelId ?? "");
  const [thinkingLevel, setThinkingLevelRaw] = useState(initialConfig.thinkingLevel ?? "");
  const [permissionPolicy, setPolicyRaw] = useState(initialConfig.permissionPolicy ?? "");
  const [recentEntries, setRecentEntries] = useState<RecentExecutorEntry[]>(() => loadRecentEntries());

  const persistPatch = useCallback(
    (patch: Partial<PersistedExecutorConfig>) => {
      const next: PersistedExecutorConfig = {
        executor: patch.executor ?? persistedStateRef.current.executor,
        variant: patch.variant ?? persistedStateRef.current.variant,
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
      setVariantRaw("");
      setProviderIdRaw("");
      setModelIdRaw("");
      setThinkingLevelRaw(DEFAULT_THINKING_LEVEL);
      setPolicyRaw("");
      persistPatch({
        executor: v,
        variant: "",
        providerId: "",
        modelId: "",
        thinkingLevel: DEFAULT_THINKING_LEVEL,
        permissionPolicy: "",
      });
    },
    [persistPatch],
  );

  const setVariant = useCallback(
    (v: string) => {
      setVariantRaw(v);
      persistPatch({ variant: v });
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
    setVariantRaw("");
    setProviderIdRaw("");
    setModelIdRaw("");
    setThinkingLevelRaw(DEFAULT_THINKING_LEVEL);
    setPolicyRaw("");
    persistedStateRef.current = {
      executor: DEFAULT_EXECUTOR,
      variant: "",
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

  return {
    executor,
    variant,
    providerId,
    modelId,
    thinkingLevel,
    permissionPolicy,
    recentEntries,
    setExecutor,
    setVariant,
    setProviderId,
    setModelId,
    setThinkingLevel,
    setPermissionPolicy,
    recordUsage,
    reset,
  };
}
