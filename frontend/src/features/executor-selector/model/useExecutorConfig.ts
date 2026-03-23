import { useCallback, useState } from "react";
import type { PersistedExecutorConfig, RecentExecutorEntry, UseExecutorConfigResult } from "./types";

// v2 格式 key：包含 thinkingLevel 字段（旧 v1 key 包含 reasoningId，自动丢弃不迁移）
const STORAGE_KEY = "agentdash:executor-config-v2";
const RECENT_KEY = "agentdash:recent-executors";
const MAX_RECENT = 8;

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
      || !isOptionalString(record.modelId)
      || !isOptionalString(record.thinkingLevel)
      || !isOptionalString(record.permissionPolicy)
    ) {
      return null;
    }

    return {
      executor: record.executor,
      variant: record.variant,
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
  return loadPersistedConfig()?.[field] ?? "";
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
      (e) => !(e.executor === entry.executor && e.modelId === entry.modelId),
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
  const [executor, setExecutorRaw] = useState(() => loadOrDefault("executor"));
  const [variant, setVariantRaw] = useState(() => loadOrDefault("variant"));
  const [modelId, setModelIdRaw] = useState(() => loadOrDefault("modelId"));
  const [thinkingLevel, setThinkingLevelRaw] = useState(() => loadOrDefault("thinkingLevel"));
  const [permissionPolicy, setPolicyRaw] = useState(() => loadOrDefault("permissionPolicy"));
  const [recentEntries, setRecentEntries] = useState<RecentExecutorEntry[]>(() => loadRecentEntries());

  const save = useCallback(
    (patch: Partial<PersistedExecutorConfig>) => {
      const next: PersistedExecutorConfig = {
        executor: patch.executor ?? executor,
        variant: patch.variant ?? variant,
        modelId: patch.modelId ?? modelId,
        thinkingLevel: patch.thinkingLevel ?? thinkingLevel,
        permissionPolicy: patch.permissionPolicy ?? permissionPolicy,
      };
      persistConfig(next);
    },
    [executor, variant, modelId, thinkingLevel, permissionPolicy],
  );

  const setExecutor = useCallback(
    (v: string) => {
      setExecutorRaw(v);
      setVariantRaw("");
      setModelIdRaw("");
      setThinkingLevelRaw("");
      setPolicyRaw("");
      save({ executor: v, variant: "", modelId: "", thinkingLevel: "", permissionPolicy: "" });
    },
    [save],
  );

  const setVariant = useCallback(
    (v: string) => {
      setVariantRaw(v);
      save({ variant: v });
    },
    [save],
  );

  const setModelId = useCallback(
    (v: string) => {
      setModelIdRaw(v);
      // 变更模型时，默认清空推理级别（由 UI 根据模型可选项重置）
      setThinkingLevelRaw("");
      save({ modelId: v });
    },
    [save],
  );

  const setThinkingLevel = useCallback(
    (v: string) => {
      setThinkingLevelRaw(v);
      save({ thinkingLevel: v });
    },
    [save],
  );

  const setPermissionPolicy = useCallback(
    (v: string) => {
      setPolicyRaw(v);
      save({ permissionPolicy: v });
    },
    [save],
  );

  const recordUsage = useCallback(() => {
    if (!executor) return;
    const entry: RecentExecutorEntry = {
      executor,
      modelId: modelId || undefined,
      timestamp: Date.now(),
    };
    setRecentEntries(persistRecentEntry(entry));
  }, [executor, modelId]);

  const reset = useCallback(() => {
    setExecutorRaw("");
    setVariantRaw("");
    setModelIdRaw("");
    setThinkingLevelRaw("");
    setPolicyRaw("");
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // noop
    }
  }, []);

  return {
    executor,
    variant,
    modelId,
    thinkingLevel,
    permissionPolicy,
    recentEntries,
    setExecutor,
    setVariant,
    setModelId,
    setThinkingLevel,
    setPermissionPolicy,
    recordUsage,
    reset,
  };
}
