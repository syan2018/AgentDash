import { useMemo, useState } from "react";
import type { ExecutorDiscoveredOptions, ExecutorInfo, ModelInfo } from "../model/types";

export interface ExecutorSelectorProps {
  executors: ExecutorInfo[];
  isLoading: boolean;
  error: Error | null;

  discoveredOptions: ExecutorDiscoveredOptions | null;
  discoveredError: Error | null;
  isDiscoveredLoading: boolean;
  onDiscoveredReconnect: () => void;

  executor: string;
  variant: string;
  modelId: string;
  reasoningId: string;
  permissionPolicy: string;

  onExecutorChange: (executor: string) => void;
  onVariantChange: (variant: string) => void;
  onModelIdChange: (modelId: string) => void;
  onReasoningIdChange: (reasoningId: string) => void;
  onPermissionPolicyChange: (policy: string) => void;
  onReset: () => void;
  onRefetch: () => void;
}

function ChevronDown({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path d="M4 6L8 10L12 6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function StatusDot({ available }: { available: boolean }) {
  return (
    <span
      className={`inline-block h-2 w-2 shrink-0 rounded-full ${available ? "bg-emerald-500" : "bg-zinc-300 dark:bg-zinc-600"}`}
    />
  );
}

/**
 * 执行器选择器组件
 *
 * 提供执行器、变体、模型ID、权限策略的选择/输入界面。
 * 替代原来的纯文本输入框，提供下拉选择 + 手动输入的混合模式。
 */
export function ExecutorSelector({
  executors,
  isLoading,
  error,
  discoveredOptions,
  discoveredError,
  isDiscoveredLoading,
  onDiscoveredReconnect,
  executor,
  variant,
  modelId,
  reasoningId,
  permissionPolicy,
  onExecutorChange,
  onVariantChange,
  onModelIdChange,
  onReasoningIdChange,
  onPermissionPolicyChange,
  onReset,
  onRefetch,
}: ExecutorSelectorProps) {
  const [showAdvanced, setShowAdvanced] = useState(false);

  const errorBanner = error ? (
    <div className="flex items-center gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      <span>无法加载执行器列表: {error.message}</span>
      <button
        type="button"
        onClick={onRefetch}
        className="rounded-md bg-destructive/20 px-2 py-0.5 text-xs hover:bg-destructive/30"
      >
        重试
      </button>
    </div>
  ) : discoveredError ? (
    <div className="flex items-center gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      <span>无法加载模型/模式选项: {discoveredError.message}</span>
      <button
        type="button"
        onClick={onDiscoveredReconnect}
        className="rounded-md bg-destructive/20 px-2 py-0.5 text-xs hover:bg-destructive/30"
      >
        重试
      </button>
    </div>
  ) : null;

  const currentExecutorInfo = useMemo(
    () => executors.find((e) => e.id === executor),
    [executors, executor],
  );

  const variantOptions = useMemo(() => {
    if (!currentExecutorInfo) return [];
    return currentExecutorInfo.variants.filter((v) => v !== "DEFAULT");
  }, [currentExecutorInfo]);

  const displayLabel = useMemo(() => {
    if (!executor) return "选择执行器…";
    const info = executors.find((e) => e.id === executor);
    const name = info?.name ?? executor;
    if (variant && variant !== "DEFAULT") return `${name} (${variant})`;
    return name;
  }, [executor, variant, executors]);

  const modelSelector = discoveredOptions?.model_selector ?? null;
  const permissions = modelSelector?.permissions ?? [];

  const providersById = useMemo(() => {
    const map = new Map<string, string>();
    for (const p of modelSelector?.providers ?? []) {
      map.set(p.id, p.name);
    }
    return map;
  }, [modelSelector]);

  const modelsByProvider = useMemo(() => {
    const out = new Map<string, ModelInfo[]>();
    for (const m of modelSelector?.models ?? []) {
      const pid = m.provider_id ?? "";
      const list = out.get(pid) ?? [];
      list.push(m);
      out.set(pid, list);
    }
    for (const list of out.values()) {
      list.sort((a, b) => a.name.localeCompare(b.name));
    }
    return out;
  }, [modelSelector]);

  const selectedModel = useMemo(() => {
    const id = modelId.trim();
    if (!id) return null;
    return (modelSelector?.models ?? []).find((m) => m.id === id) ?? null;
  }, [modelSelector, modelId]);

  const reasoningOptions = selectedModel?.reasoning_options ?? [];

  return (
    <div className="space-y-3">
      {errorBanner}
      {/* 主选择行 */}
      <div className="flex flex-wrap items-end gap-3">
        {/* 执行器下拉 */}
        <div className="min-w-[180px] flex-1">
          <span className="mb-1 block text-xs font-medium text-muted-foreground">执行器</span>
          <div className="relative">
            <select
              value={executor}
              onChange={(e) => onExecutorChange(e.target.value)}
              disabled={isLoading}
              className="h-9 w-full appearance-none rounded-md border border-border bg-background pl-3 pr-8 text-sm outline-none ring-ring focus:ring-1 disabled:opacity-50"
            >
              <option value="">
                {isLoading ? "加载中…" : "选择执行器…"}
              </option>
              {executors.map((info) => (
                <option key={info.id} value={info.id}>
                  {info.name}{info.available ? "" : " (不可用)"}
                </option>
              ))}
            </select>
            <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
          </div>
        </div>

        {/* 变体下拉（仅当选中的执行器有多个变体时显示） */}
        {variantOptions.length > 0 && (
          <div className="min-w-[140px]">
            <span className="mb-1 block text-xs font-medium text-muted-foreground">变体（Variant）</span>
            <div className="relative">
              <select
                value={variant}
                onChange={(e) => onVariantChange(e.target.value)}
                className="h-9 w-full appearance-none rounded-md border border-border bg-background pl-3 pr-8 text-sm outline-none ring-ring focus:ring-1"
              >
                <option value="">Default</option>
                {variantOptions.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
              <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
            </div>
          </div>
        )}

        {/* 模型选择（来自 discovered-options；无硬编码） */}
        <div className="min-w-[220px] flex-1">
          <span className="mb-1 block text-xs font-medium text-muted-foreground">模型</span>
          <div className="relative">
            <select
              value={modelId}
              onChange={(e) => onModelIdChange(e.target.value)}
              disabled={!executor || isDiscoveredLoading || (modelSelector?.models?.length ?? 0) === 0}
              className="h-9 w-full appearance-none rounded-md border border-border bg-background pl-3 pr-8 text-sm outline-none ring-ring focus:ring-1 disabled:opacity-50"
            >
              <option value="">
                {!executor
                  ? "先选择执行器…"
                  : isDiscoveredLoading
                    ? "加载模型中…"
                    : (modelSelector?.models?.length ?? 0) === 0
                      ? "暂无模型选项"
                      : "选择模型…"}
              </option>
              {[...modelsByProvider.entries()].map(([providerId, models]) => {
                const label =
                  providerId && providersById.get(providerId)
                    ? providersById.get(providerId)
                    : providerId || "Other";
                return (
                  <optgroup key={providerId || "default"} label={label}>
                    {models.map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.name}
                      </option>
                    ))}
                  </optgroup>
                );
              })}
            </select>
            <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
          </div>
        </div>

        {/* 高级选项切换 + 重置 */}
        <div className="flex items-center gap-1.5 self-end pb-0.5">
          <button
            type="button"
            onClick={() => setShowAdvanced((p) => !p)}
            className="rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground hover:bg-secondary"
          >
            {showAdvanced ? "收起" : "高级"}
          </button>
          <button
            type="button"
            onClick={onReset}
            className="rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground hover:bg-secondary"
            title="重置为默认值"
          >
            重置
          </button>
        </div>
      </div>

      {/* 当前选择概览（紧凑标签） */}
      {executor && (
        <div className="flex flex-wrap items-center gap-1.5">
          <ConfigTag
            label={displayLabel}
            available={currentExecutorInfo?.available}
          />
          {modelId && <ConfigTag label={`model: ${modelId}`} />}
          {reasoningId && <ConfigTag label={`mode: ${reasoningId}`} />}
          {permissionPolicy && (
            <ConfigTag label={`policy: ${permissionPolicy}`} />
          )}
        </div>
      )}

      {/* 高级选项面板 */}
      {showAdvanced && (
        <div className="rounded-md border border-border bg-background/50 p-3">
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            {/* Mode / Reasoning */}
            <div>
              <span className="mb-1 block text-xs font-medium text-muted-foreground">模式（Mode / Reasoning）</span>
              <div className="relative">
                <select
                  value={reasoningId}
                  onChange={(e) => onReasoningIdChange(e.target.value)}
                  disabled={(reasoningOptions?.length ?? 0) === 0}
                  className="h-9 w-full appearance-none rounded-md border border-border bg-background pl-3 pr-8 text-sm outline-none ring-ring focus:ring-1 disabled:opacity-50"
                >
                  <option value="">
                    {(reasoningOptions?.length ?? 0) === 0 ? "当前模型不支持模式选择" : "默认"}
                  </option>
                  {reasoningOptions.map((o) => (
                    <option key={o.id} value={o.id}>
                      {o.label}{o.is_default ? " (默认)" : ""}
                    </option>
                  ))}
                </select>
                <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
              </div>
            </div>

            <div>
              <span className="mb-1 block text-xs font-medium text-muted-foreground">权限策略（Permission Policy）</span>
              <div className="relative">
                <select
                  value={permissionPolicy}
                  onChange={(e) => onPermissionPolicyChange(e.target.value)}
                  className="h-9 w-full appearance-none rounded-md border border-border bg-background pl-3 pr-8 text-sm outline-none ring-ring focus:ring-1"
                >
                  <option value="">默认（Auto）</option>
                  {permissions.map((p) => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))}
                </select>
                <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function ConfigTag({
  label,
  available,
}: {
  label: string;
  available?: boolean;
}) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-secondary/60 px-2.5 py-0.5 text-xs text-foreground">
      {available !== undefined && <StatusDot available={available} />}
      {label}
    </span>
  );
}
