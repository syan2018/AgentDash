import { useEffect, useRef, useState } from "react";

import { DetailPanel } from "@agentdash/ui";

import type { LlmProviderModelConfig } from "../model/llmProviderModels";
import type { ModelInfo } from "../../executor-selector/model/types";
import { inputCls } from "./primitives";

export interface BulkManageModelEntry {
  key: string;
  id: string;
  name: string;
  source: "discovered" | "custom";
  enabled: boolean;
  context_window: number;
  reasoning: boolean;
  supports_image: boolean;
  provider_prefixed: boolean;
  has_override: boolean;
  discovered_model?: ModelInfo;
  custom_index?: number;
}

type BulkModelFilter = "enabled" | "disabled" | "prefixed" | "all";

interface BulkActionMenuItem {
  key: string;
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  danger?: boolean;
}

interface BulkModelManagementPanelProps {
  open: boolean;
  entries: BulkManageModelEntry[];
  customModels: LlmProviderModelConfig[];
  trueDiscoveredIds: Set<string>;
  blockedModels: string[];
  saving: boolean;
  onClose: () => void;
  onSetBlockedModels: (modelIds: string[]) => void;
  onUpdateEntry: (
    entry: BulkManageModelEntry,
    field: keyof LlmProviderModelConfig,
    value: string | number | boolean,
  ) => void;
  onReplaceModels: (models: LlmProviderModelConfig[]) => void;
  onSave: () => Promise<void>;
}

export function BulkModelManagementPanel({
  open,
  entries,
  customModels,
  trueDiscoveredIds,
  blockedModels,
  saving,
  onClose,
  onSetBlockedModels,
  onUpdateEntry,
  onReplaceModels,
  onSave,
}: BulkModelManagementPanelProps) {
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const [filter, setFilter] = useState<BulkModelFilter>("enabled");
  const [search, setSearch] = useState("");
  const [bulkContextK, setBulkContextK] = useState(200);

  const normalizedSearch = search.trim().toLowerCase();
  const filteredEntries = entries.filter((entry) => {
    if (filter === "enabled" && !entry.enabled) return false;
    if (filter === "disabled" && entry.enabled) return false;
    if (filter === "prefixed" && !entry.provider_prefixed) return false;
    if (!normalizedSearch) return true;
    return `${entry.name}\n${entry.id}`.toLowerCase().includes(normalizedSearch);
  });
  const visibleIds = filteredEntries.map((entry) => entry.id).filter((id) => id.trim().length > 0);
  const selectedEntries = entries.filter((entry) => entry.id.trim().length > 0 && selectedIds.has(entry.id));
  const allSelected = visibleIds.length > 0 && visibleIds.every((id) => selectedIds.has(id));
  const providerPrefixedIds = entries
    .filter((entry) => entry.provider_prefixed)
    .map((entry) => entry.id)
    .filter((id) => id.trim().length > 0);

  const toggleSelected = (modelId: string) => {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(modelId)) {
        next.delete(modelId);
      } else {
        next.add(modelId);
      }
      return next;
    });
  };

  const selectVisibleEntries = () => {
    setSelectedIds(new Set(visibleIds));
  };

  const clearSelectedEntries = () => {
    setSelectedIds(new Set());
  };

  const setSelectedEnabled = (enabled: boolean) => {
    const selectedIdSet = new Set(selectedEntries.map((entry) => entry.id));
    const nextBlocked = enabled
      ? blockedModels.filter((modelId) => !selectedIdSet.has(modelId))
      : [...blockedModels, ...selectedIdSet];
    onSetBlockedModels(nextBlocked);
  };

  const setVisibleEnabled = (enabled: boolean) => {
    const visibleIdSet = new Set(visibleIds);
    const nextBlocked = enabled
      ? blockedModels.filter((modelId) => !visibleIdSet.has(modelId))
      : [...blockedModels, ...visibleIdSet];
    onSetBlockedModels(nextBlocked);
  };

  const blockProviderPrefixedModels = () => {
    onSetBlockedModels([...blockedModels, ...providerPrefixedIds]);
  };

  const applyContextToSelected = () => {
    const contextWindow = Math.max(1, Math.round(bulkContextK)) * 1000;
    for (const entry of selectedEntries) {
      onUpdateEntry(entry, "context_window", contextWindow);
    }
  };

  const applyBooleanToSelected = (field: "reasoning" | "supports_image", value: boolean) => {
    for (const entry of selectedEntries) {
      onUpdateEntry(entry, field, value);
    }
  };

  const resetDiscoveredOverrides = () => {
    onReplaceModels(customModels.filter((model) => !trueDiscoveredIds.has(model.id)));
  };

  const removeCustomModels = () => {
    onReplaceModels(customModels.filter((model) => trueDiscoveredIds.has(model.id)));
  };

  const filterOptions: Array<{ value: BulkModelFilter; label: string; count: number }> = [
    { value: "enabled", label: "可用", count: entries.filter((entry) => entry.enabled).length },
    { value: "disabled", label: "已禁用", count: entries.filter((entry) => !entry.enabled).length },
    { value: "prefixed", label: "前缀", count: providerPrefixedIds.length },
    { value: "all", label: "全部", count: entries.length },
  ];
  const bulkActionItems: BulkActionMenuItem[] = [
    { key: "select-visible", label: "选择当前结果", disabled: visibleIds.length === 0, onSelect: selectVisibleEntries },
    { key: "clear-selection", label: "清空选择", disabled: selectedIds.size === 0, onSelect: clearSelectedEntries },
    { key: "enable-selected", label: "启用已选", disabled: selectedEntries.length === 0, onSelect: () => setSelectedEnabled(true) },
    { key: "disable-selected", label: "禁用已选", disabled: selectedEntries.length === 0, onSelect: () => setSelectedEnabled(false) },
    { key: "enable-visible", label: "启用当前结果", disabled: visibleIds.length === 0, onSelect: () => setVisibleEnabled(true) },
    { key: "disable-visible", label: "禁用当前结果", disabled: visibleIds.length === 0, onSelect: () => setVisibleEnabled(false) },
    { key: "block-prefixed", label: "屏蔽 provider/model", disabled: providerPrefixedIds.length === 0, onSelect: blockProviderPrefixedModels },
    { key: "apply-context", label: `应用 ${bulkContextK}k 上下文到已选`, disabled: selectedEntries.length === 0, onSelect: applyContextToSelected },
    { key: "reasoning-on", label: "已选模型推理开", disabled: selectedEntries.length === 0, onSelect: () => applyBooleanToSelected("reasoning", true) },
    { key: "reasoning-off", label: "已选模型推理关", disabled: selectedEntries.length === 0, onSelect: () => applyBooleanToSelected("reasoning", false) },
    { key: "image-on", label: "已选模型图像开", disabled: selectedEntries.length === 0, onSelect: () => applyBooleanToSelected("supports_image", true) },
    { key: "image-off", label: "已选模型图像关", disabled: selectedEntries.length === 0, onSelect: () => applyBooleanToSelected("supports_image", false) },
    { key: "reset-overrides", label: "重置发现模型属性", onSelect: resetDiscoveredOverrides },
    { key: "remove-custom", label: "移除自定义模型", danger: true, onSelect: removeCustomModels },
  ];

  return (
    <DetailPanel
      open={open}
      title="批量管理模型"
      subtitle={`${entries.length} 个模型 · ${entries.filter((entry) => entry.enabled).length} 个启用`}
      onClose={onClose}
      widthClassName="max-w-6xl"
    >
      <div className="space-y-4 p-5">
        <section className="space-y-3 rounded-[8px] border border-border bg-secondary/35 p-4">
          <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <div className="flex flex-wrap items-center gap-1">
              {filterOptions.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  onClick={() => setFilter(option.value)}
                  className={`inline-flex items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-xs transition-colors ${
                    filter === option.value
                      ? "border-primary/40 bg-primary/10 text-primary"
                      : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground"
                  }`}
                >
                  {option.label}
                  <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                    {option.count}
                  </span>
                </button>
              ))}
            </div>
            <input
              type="search"
              className={`${inputCls} lg:!w-64`}
              value={search}
              placeholder="搜索模型名称或 ID"
              onChange={(event) => setSearch(event.target.value)}
            />
          </div>

          <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs text-muted-foreground">
                当前结果 {filteredEntries.length} 个 · 已选 {selectedEntries.length} 个
              </span>
              <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                上下文
                <input
                  type="number"
                  min={1}
                  className={`${inputCls} !w-20 !py-1.5 text-center`}
                  value={bulkContextK}
                  onChange={(event) => setBulkContextK(parseInt(event.target.value) || 1)}
                />
                k
              </label>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <BulkActionMenu items={bulkActionItems} />
              <button
                type="button"
                disabled={saving}
                onClick={() => void onSave()}
                className="inline-flex items-center justify-center rounded-[8px] border border-primary/50 bg-primary/10 px-3 py-1.5 text-xs font-medium text-primary transition-colors hover:bg-primary/15 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {saving ? "保存中…" : "保存"}
              </button>
            </div>
          </div>
        </section>

        <div className="overflow-x-auto rounded-[8px] border border-border">
          <table className="min-w-[760px] w-full table-fixed border-collapse text-sm">
            <thead className="bg-secondary/60 text-xs text-muted-foreground">
              <tr>
                <th className="w-10 px-3 py-2 text-left">
                  <input
                    type="checkbox"
                    checked={allSelected}
                    onChange={(event) => setSelectedIds(event.target.checked ? new Set(visibleIds) : new Set())}
                    aria-label="选择当前结果模型"
                    className="accent-primary"
                  />
                </th>
                <th className="w-16 px-3 py-2 text-left">可用</th>
                <th className="px-3 py-2 text-left">模型</th>
                <th className="w-28 px-3 py-2 text-left">上下文</th>
                <th className="w-20 px-3 py-2 text-left">推理</th>
                <th className="w-20 px-3 py-2 text-left">图像</th>
                <th className="w-24 px-3 py-2 text-left">来源</th>
              </tr>
            </thead>
            <tbody>
              {filteredEntries.map((entry) => (
                <tr key={entry.key} className="border-t border-border/70">
                  <td className="px-3 py-2 align-middle">
                    <input
                      type="checkbox"
                      checked={selectedIds.has(entry.id)}
                      disabled={!entry.id.trim()}
                      onChange={() => toggleSelected(entry.id)}
                      aria-label={`选择 ${entry.name}`}
                      className="accent-primary"
                    />
                  </td>
                  <td className="px-3 py-2 align-middle">
                    <input
                      type="checkbox"
                      checked={entry.enabled}
                      disabled={!entry.id.trim()}
                      onChange={(event) => {
                        const nextBlocked = event.target.checked
                          ? blockedModels.filter((modelId) => modelId !== entry.id)
                          : [...blockedModels, entry.id];
                        onSetBlockedModels(nextBlocked);
                      }}
                      aria-label={`${entry.name} 是否可用`}
                      className="accent-primary"
                    />
                  </td>
                  <td className="min-w-0 px-3 py-2 align-middle">
                    <div className="flex min-w-0 items-center gap-2">
                      <span className="truncate text-sm text-foreground">{entry.name}</span>
                      {entry.provider_prefixed && (
                        <span className="shrink-0 rounded-[6px] border border-warning/30 bg-warning/10 px-1.5 py-0.5 text-[10px] text-warning">
                          前缀
                        </span>
                      )}
                      {entry.has_override && (
                        <span className="shrink-0 rounded-[6px] border border-info/30 bg-info/10 px-1.5 py-0.5 text-[10px] text-info">
                          已改
                        </span>
                      )}
                    </div>
                    <code className="block truncate text-[10px] text-muted-foreground">{entry.id}</code>
                  </td>
                  <td className="px-3 py-2 align-middle">
                    <div className="flex items-center gap-1">
                      <input
                        type="number"
                        min={1}
                        className={`${inputCls} !w-20 !py-1.5 text-center`}
                        value={Math.round(entry.context_window / 1000)}
                        onChange={(event) => onUpdateEntry(entry, "context_window", (parseInt(event.target.value) || 1) * 1000)}
                      />
                      <span className="text-xs text-muted-foreground">k</span>
                    </div>
                  </td>
                  <td className="px-3 py-2 align-middle">
                    <input
                      type="checkbox"
                      checked={entry.reasoning}
                      onChange={(event) => onUpdateEntry(entry, "reasoning", event.target.checked)}
                      aria-label={`${entry.name} 推理能力`}
                      className="accent-primary"
                    />
                  </td>
                  <td className="px-3 py-2 align-middle">
                    <input
                      type="checkbox"
                      checked={entry.supports_image}
                      onChange={(event) => onUpdateEntry(entry, "supports_image", event.target.checked)}
                      aria-label={`${entry.name} 图像能力`}
                      className="accent-primary"
                    />
                  </td>
                  <td className="px-3 py-2 align-middle">
                    <span className="rounded-[6px] border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground">
                      {entry.source === "discovered" ? "发现" : "自定义"}
                    </span>
                  </td>
                </tr>
              ))}
              {filteredEntries.length === 0 && (
                <tr>
                  <td colSpan={7} className="px-3 py-8 text-center text-sm text-muted-foreground">
                    当前筛选条件下没有模型
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </DetailPanel>
  );
}

function BulkActionMenu({ items }: { items: BulkActionMenuItem[] }) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const handlePointerDown = (event: PointerEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };

    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  return (
    <div ref={containerRef} className="relative shrink-0">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="inline-flex items-center justify-center rounded-[8px] border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-secondary"
      >
        批量操作
      </button>
      {open && (
        <div className="absolute right-0 top-9 z-[80] grid w-56 gap-1 rounded-[8px] border border-border bg-background p-1.5 shadow-xl">
          {items.map((item) => (
            <button
              key={item.key}
              type="button"
              disabled={item.disabled}
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              className={`rounded-[6px] px-2.5 py-1.5 text-left text-xs transition-colors disabled:cursor-not-allowed disabled:opacity-45 ${
                item.danger
                  ? "text-destructive hover:bg-destructive/10"
                  : "text-foreground hover:bg-secondary"
              }`}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
