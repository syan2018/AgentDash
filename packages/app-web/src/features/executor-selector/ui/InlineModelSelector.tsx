/**
 * 紧凑内联模型/推理选择器
 *
 * 底部工具栏 chip 形态，点击展开两级 Popover:
 * 左列 = Reasoning 档位 + Provider 分组入口
 * 右列 = 选中 Provider 下的模型列表
 */

import { useCallback, useMemo, useRef, useState, useEffect } from "react";
import type { ExecutorDiscoveredOptions, ModelInfo } from "../model/types";
import type { UseExecutorConfigResult } from "../model/types";
import { THINKING_LEVEL_OPTIONS } from "../../../types";

export interface InlineModelSelectorProps {
  execConfig: UseExecutorConfigResult;
  discoveredOptions: ExecutorDiscoveredOptions | null;
  isDiscoveredLoading: boolean;
  executorName?: string;
  readonly?: boolean;
  status?: "resolved" | "model_required";
  message?: string;
  onRefresh: () => void;
}

export function InlineModelSelector({
  execConfig,
  discoveredOptions,
  isDiscoveredLoading,
  executorName,
  readonly: isReadonly = false,
  status = "resolved",
  message,
  onRefresh,
}: InlineModelSelectorProps) {
  const [open, setOpen] = useState(false);
  const [hoveredProvider, setHoveredProvider] = useState<string | null>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const chipRef = useRef<HTMLButtonElement>(null);

  // 点击外部关闭
  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (
        popoverRef.current &&
        !popoverRef.current.contains(e.target as Node) &&
        chipRef.current &&
        !chipRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  const modelSelector = discoveredOptions?.model_selector ?? null;

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
      if (m.blocked) continue;
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
    const id = execConfig.modelId.trim();
    if (!id) return null;
    const pid = execConfig.providerId.trim();
    return (modelSelector?.models ?? []).find(
      (m) => m.id === id && (pid ? (m.provider_id ?? "") === pid : true),
    ) ?? null;
  }, [modelSelector, execConfig.modelId, execConfig.providerId]);

  const showThinkingSelector = !selectedModel || selectedModel.reasoning === true;

  // Chip 文案
  const chipLabel = useMemo(() => {
    if (status === "model_required") return "选择模型…";
    if (!execConfig.executor) return "选择模型…";
    const modelName = selectedModel?.name ?? execConfig.modelId.trim();
    const thinkingLabel = THINKING_LEVEL_OPTIONS.find(
      (o) => o.value === execConfig.thinkingLevel,
    )?.label;
    if (modelName && thinkingLabel) return `${modelName} ${thinkingLabel}`;
    if (modelName) return modelName;
    return executorName ?? execConfig.executor;
  }, [execConfig.executor, execConfig.modelId, execConfig.thinkingLevel, executorName, selectedModel, status]);

  const handleSelectModel = useCallback(
    (providerId: string, modelId: string) => {
      execConfig.setProviderId(providerId);
      execConfig.setModelId(modelId);
      setOpen(false);
      setHoveredProvider(null);
    },
    [execConfig],
  );

  const handleSelectThinking = useCallback(
    (value: string) => {
      execConfig.setThinkingLevel(value);
    },
    [execConfig],
  );

  const providerEntries = useMemo(
    () => [...modelsByProvider.entries()].filter(([, models]) => models.length > 0),
    [modelsByProvider],
  );

  // 默认展开第一个 provider 或当前选中的 provider
  const activeProvider = hoveredProvider ?? (execConfig.providerId.trim() || providerEntries[0]?.[0] || null);

  return (
    <div className="relative">
      {/* Chip */}
      <button
        ref={chipRef}
        type="button"
        onClick={() => {
          if (!isReadonly) setOpen((v) => !v);
        }}
        disabled={isReadonly}
        className={`flex items-center gap-1 rounded-[8px] px-2.5 py-1.5 text-xs transition-colors ${
          isReadonly
            ? "cursor-default text-muted-foreground opacity-60"
            : status === "model_required"
              ? "bg-warning/10 text-warning hover:bg-warning/15"
            : open
              ? "bg-secondary text-foreground"
              : "text-muted-foreground hover:bg-secondary hover:text-foreground"
        }`}
        title={status === "model_required" ? message : undefined}
      >
        {isDiscoveredLoading ? (
          <span className="inline-block h-3 w-3 animate-spin rounded-[8px] border border-muted-foreground border-t-transparent" />
        ) : null}
        <span className="max-w-[200px] truncate">{chipLabel}</span>
        {!isReadonly && (
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`}>
            <path d="M4 6L8 10L12 6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        )}
      </button>

      {/* Popover */}
      {open && (
        <div
          ref={popoverRef}
          className="absolute bottom-full right-0 z-50 mb-2 flex max-w-[calc(100vw-2rem)] overflow-hidden rounded-[12px] border border-border bg-popover shadow-lg"
        >
          {/* 左列: Reasoning + Provider 入口 */}
          <div className="w-[180px] max-w-[45vw] shrink-0 border-r border-border p-2">
            {/* Reasoning 档位 */}
            {showThinkingSelector && (
              <>
                <div className="px-2 pb-1 pt-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                  Reasoning
                </div>
                <div className="space-y-0.5">
                  <button
                    type="button"
                    onClick={() => handleSelectThinking("")}
                    className={`flex w-full items-center justify-between rounded-[6px] px-2 py-1.5 text-xs transition-colors ${
                      !execConfig.thinkingLevel
                        ? "bg-primary/10 text-primary"
                        : "text-foreground hover:bg-secondary"
                    }`}
                  >
                    <span>默认</span>
                    {!execConfig.thinkingLevel && <CheckIcon />}
                  </button>
                  {THINKING_LEVEL_OPTIONS.map((opt) => (
                    <button
                      key={opt.value}
                      type="button"
                      onClick={() => handleSelectThinking(opt.value)}
                      className={`flex w-full items-center justify-between rounded-[6px] px-2 py-1.5 text-xs transition-colors ${
                        execConfig.thinkingLevel === opt.value
                          ? "bg-primary/10 text-primary"
                          : "text-foreground hover:bg-secondary"
                      }`}
                    >
                      <span>{opt.label}</span>
                      {execConfig.thinkingLevel === opt.value && <CheckIcon />}
                    </button>
                  ))}
                </div>
                <div className="my-2 border-t border-border" />
              </>
            )}

            {/* Provider 入口 */}
            {providerEntries.length > 0 && (
              <>
                <div className="px-2 pb-1 pt-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                  Provider
                </div>
                <div className="space-y-0.5">
                  {providerEntries.map(([pid]) => (
                    <button
                      key={pid || "__default__"}
                      type="button"
                      onMouseEnter={() => setHoveredProvider(pid)}
                      onClick={() => setHoveredProvider(pid)}
                      className={`flex w-full items-center justify-between rounded-[6px] px-2 py-1.5 text-xs transition-colors ${
                        activeProvider === pid
                          ? "bg-secondary text-foreground"
                          : "text-foreground hover:bg-secondary"
                      }`}
                    >
                      <span className="truncate">{providersById.get(pid) ?? (pid || "Other")}</span>
                      <svg width="10" height="10" viewBox="0 0 16 16" fill="none" className="shrink-0 text-muted-foreground">
                        <path d="M6 4L10 8L6 12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                      </svg>
                    </button>
                  ))}
                </div>
              </>
            )}

            {/* 底部工具 */}
            <div className="mt-2 flex items-center gap-1 border-t border-border pt-2">
              <button
                type="button"
                onClick={onRefresh}
                className="rounded-[6px] px-2 py-1 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground"
              >
                刷新
              </button>
            </div>
          </div>

          {/* 右列: 模型列表 */}
          {activeProvider !== null && modelsByProvider.has(activeProvider) && (
            <div className="w-[200px] max-w-[50vw] p-2">
              <div className="px-2 pb-1 pt-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                {providersById.get(activeProvider) ?? (activeProvider || "模型")}
              </div>
              <div className="max-h-[300px] space-y-0.5 overflow-y-auto">
                {modelsByProvider.get(activeProvider)?.map((model) => {
                  const isSelected =
                    model.id === execConfig.modelId.trim() &&
                    (model.provider_id ?? "") === execConfig.providerId.trim();
                  return (
                    <button
                      key={`${model.provider_id ?? ""}::${model.id}`}
                      type="button"
                      onClick={() => handleSelectModel(model.provider_id ?? "", model.id)}
                      className={`flex w-full items-center justify-between rounded-[6px] px-2 py-1.5 text-xs transition-colors ${
                        isSelected
                          ? "bg-primary/10 text-primary"
                          : "text-foreground hover:bg-secondary"
                      }`}
                    >
                      <span className="truncate">{model.name}</span>
                      {isSelected && <CheckIcon />}
                    </button>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function CheckIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" className="shrink-0 text-primary">
      <path d="M3.5 8.5L6.5 11.5L12.5 4.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
