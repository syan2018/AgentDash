/**
 * Capability Panel —— tool_directives 编辑。
 *
 * 操作 `capability_config.tool_directives` —— 扁平的 Add / Remove 指令序列。
 * UI 分为两区：
 *   1. 基线能力（auto_granted baseline） —— 按 target_kinds 计算，可直接「屏蔽此能力」/ 展开屏蔽单个工具
 *   2. 工作流追加能力 —— 非 baseline 的显式 Add（如 workflow_management、mcp:*）
 *
 * 每个按钮动作对应一条独立 Directive，与后端 slot 归约契约一一映射。
 * 本 panel 为受控组件：projectId / targetKinds / directives 入参 + onChange 出参。
 * MCP preset 拉取 / tool catalog 拉取等副作用封装在 panel 内部（属于 panel 自包含的懒加载行为）。
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import type {
  CapabilityCatalogEntryDto,
  CapabilityDirective,
  McpPresetDto,
  ToolDescriptor,
  WorkflowTargetKind,
} from "../../../../types";
import {
  addDirective,
  capabilityBlockedByWorkflow,
  listDeclaredCapabilityKeys,
  makeAddCapability,
  makeRemoveCapability,
  makeRemoveTool,
  removeDirective,
  toolBlockedByWorkflow,
} from "../../capability-directive-ops";
import { fetchProjectMcpPresets } from "../../../../services/mcpPreset";
import { fetchCapabilityCatalog, fetchToolCatalog } from "../../../../services/workflow";
import { useMcpProbeStore } from "../../../../stores/mcpProbeStore";
import { formatTargetKinds } from "../../shared-labels";
import {
  mapMcpProbeToToolDescriptors,
  mcpProbePlaceholderDescriptor,
} from "../../../mcp-shared/probeViewModel";
import {
  capabilityAutoGrantedForTargetKind,
  capabilityVisibleForTargetKind,
  extractMcpPresetName,
} from "./shared";

/** 工具行 — 展示单个工具，带「屏蔽此工具」/「恢复」按钮。 */
function ToolRow({
  capKey,
  tool,
  isBlocked,
  onToggleBlock,
}: {
  capKey: string;
  tool: ToolDescriptor;
  isBlocked: boolean;
  onToggleBlock: (capKey: string, toolName: string) => void;
}) {
  const scopeLabel =
    tool.source.type === "platform_mcp"
      ? tool.source.scope
      : tool.source.type === "mcp"
        ? tool.source.server_name
        : tool.source.cluster;
  return (
    <div
      className={`flex items-center gap-2 rounded-md border px-2 py-1 text-[11px] transition-colors ${
        isBlocked
          ? "border-destructive/30 bg-destructive/5 text-destructive line-through"
          : "border-border bg-background text-foreground"
      }`}
      title={`${tool.display_name}: ${tool.description}`}
    >
      <code className="font-mono">{tool.name}</code>
      <span className="rounded bg-secondary/60 px-1 py-0.5 text-[9px] text-muted-foreground">
        {scopeLabel}
      </span>
      {isBlocked && <span className="text-[9px]">(屏蔽)</span>}
      <button
        type="button"
        onClick={() => onToggleBlock(capKey, tool.name)}
        className={`ml-auto rounded px-1.5 py-0.5 text-[10px] transition-colors ${
          isBlocked
            ? "text-primary hover:bg-primary/10"
            : "text-destructive hover:bg-destructive/10"
        }`}
      >
        {isBlocked ? "恢复" : "屏蔽此工具"}
      </button>
    </div>
  );
}

/** 工具列表面板 — 展开一个 capability 后按 directive 序列判定每个工具的屏蔽状态。 */
function ToolListPanel({
  capKey,
  tools,
  directives,
  onToggleToolBlock,
}: {
  capKey: string;
  tools: ToolDescriptor[];
  directives: CapabilityDirective[];
  onToggleToolBlock: (capKey: string, toolName: string) => void;
}) {
  if (tools.length === 0) {
    return <p className="pl-4 py-1 text-[11px] text-muted-foreground">此能力无下属平台工具</p>;
  }
  return (
    <div className="pl-4 mt-1 flex flex-col gap-1">
      {tools.map((tool) => (
        <ToolRow
          key={tool.name}
          capKey={capKey}
          tool={tool}
          isBlocked={toolBlockedByWorkflow(directives, capKey, tool.name)}
          onToggleBlock={onToggleToolBlock}
        />
      ))}
    </div>
  );
}

/** Capability 行 — 基线/追加两区共用的渲染单元。 */
function CapabilityRow({
  capKey,
  label,
  description,
  isBaseline,
  isBlocked,
  isExpanded,
  tools,
  directives,
  onToggleBlock,
  onRemoveAdd,
  onToggleExpand,
  onToggleToolBlock,
  extraBadge,
}: {
  capKey: string;
  label: string;
  description: string;
  isBaseline: boolean;
  isBlocked: boolean;
  isExpanded: boolean;
  tools: ToolDescriptor[];
  directives: CapabilityDirective[];
  onToggleBlock?: () => void;
  onRemoveAdd?: () => void;
  onToggleExpand: () => void;
  onToggleToolBlock: (capKey: string, toolName: string) => void;
  extraBadge?: React.ReactNode;
}) {
  return (
    <div
      className={`rounded-[8px] border px-3 py-2 transition-colors ${
        isBlocked ? "border-destructive/30 bg-destructive/5" : "border-border bg-secondary/20"
      }`}
    >
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span
              className={`text-xs font-medium ${
                isBlocked ? "text-destructive line-through" : "text-foreground"
              }`}
            >
              {label}
            </span>
            {extraBadge}
            {isBlocked && (
              <span className="rounded bg-destructive/10 px-1.5 py-0.5 text-[9px] text-destructive">
                已屏蔽
              </span>
            )}
            {!isBaseline && !isBlocked && (
              <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[9px] text-primary/70">
                追加
              </span>
            )}
          </div>
          <p className="mt-0.5 text-[11px] text-muted-foreground leading-[1.35]">
            {description}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={onToggleExpand}
            className="rounded p-0.5 text-muted-foreground hover:text-foreground transition-colors"
            title={isExpanded ? "收起工具列表" : "展开工具列表"}
          >
            <svg
              className={`h-3.5 w-3.5 transition-transform ${isExpanded ? "rotate-90" : ""}`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
            </svg>
          </button>
          {isBaseline && onToggleBlock && (
            <button
              type="button"
              onClick={onToggleBlock}
              className={`rounded-[6px] border px-2 py-0.5 text-[11px] transition-colors ${
                isBlocked
                  ? "border-primary/30 text-primary hover:bg-primary/5"
                  : "border-destructive/30 text-destructive hover:bg-destructive/5"
              }`}
            >
              {isBlocked ? "恢复此能力" : "屏蔽此能力"}
            </button>
          )}
          {!isBaseline && onRemoveAdd && (
            <button
              type="button"
              onClick={onRemoveAdd}
              className="rounded-[6px] border border-destructive/30 px-2 py-0.5 text-[11px] text-destructive hover:bg-destructive/5"
              title="移除此追加能力"
            >
              移除
            </button>
          )}
        </div>
      </div>
      {isExpanded && (
        <ToolListPanel
          capKey={capKey}
          tools={tools}
          directives={directives}
          onToggleToolBlock={onToggleToolBlock}
        />
      )}
    </div>
  );
}

// ─── CapabilitiesEditor 主体 ───────────────────────────

interface CapabilitiesEditorProps {
  projectId: string;
  targetKinds: WorkflowTargetKind[];
  directives: CapabilityDirective[];
  onChange: (next: CapabilityDirective[]) => void;
}

function CapabilitiesEditor({
  projectId,
  targetKinds,
  directives,
  onChange,
}: CapabilitiesEditorProps) {
  const [presets, setPresets] = useState<McpPresetDto[]>([]);
  const [presetsLoading, setPresetsLoading] = useState(false);
  const [presetsError, setPresetsError] = useState<string | null>(null);
  const [capabilityCatalog, setCapabilityCatalog] = useState<CapabilityCatalogEntryDto[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const getOrRefreshProbe = useMcpProbeStore((state) => state.getOrRefresh);

  // 已展开工具面板的 capability key 集合
  const [expandedCaps, setExpandedCaps] = useState<Set<string>>(new Set());
  // 工具目录缓存：capability key → ToolDescriptor[]
  const [toolCatalogCache, setToolCatalogCache] = useState<Record<string, ToolDescriptor[]>>({});
  // 「+ 添加能力」picker 是否展开
  const [showPicker, setShowPicker] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      setCatalogLoading(true);
      setCatalogError(null);
      try {
        const catalog = await fetchCapabilityCatalog();
        if (!cancelled) setCapabilityCatalog(catalog.capabilities);
      } catch (err) {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : String(err);
          setCatalogError(message);
          setCapabilityCatalog([]);
        }
      } finally {
        if (!cancelled) setCatalogLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!projectId) return;
    let cancelled = false;
    void (async () => {
      setPresetsLoading(true);
      setPresetsError(null);
      try {
        const items = await fetchProjectMcpPresets(projectId);
        if (!cancelled) setPresets(items);
      } catch (err) {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : String(err);
          setPresetsError(message);
          setPresets([]);
        }
      } finally {
        if (!cancelled) setPresetsLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  const catalogByKey = useMemo(() => {
    const entries = new Map<string, CapabilityCatalogEntryDto>();
    for (const entry of capabilityCatalog) {
      entries.set(entry.key, entry);
    }
    return entries;
  }, [capabilityCatalog]);

  const baselineKeys = useMemo(() => {
    const keys: string[] = [];
    for (const targetKind of targetKinds) {
      for (const entry of capabilityCatalog) {
        if (capabilityAutoGrantedForTargetKind(entry, targetKind) && !keys.includes(entry.key)) {
          keys.push(entry.key);
        }
      }
    }
    return keys;
  }, [capabilityCatalog, targetKinds]);
  const baselineSet = useMemo(() => new Set<string>(baselineKeys), [baselineKeys]);

  // 当前所有显式 Add 的 capability key（短 path + 长 path 合并）
  const declaredAddKeys = useMemo(
    () => new Set(listDeclaredCapabilityKeys(directives)),
    [directives],
  );

  // 「追加能力」区展示的 key 列表：显式 Add 中不属于 baseline 的部分
  const extraKeys = useMemo(() => {
    const keys: string[] = [];
    for (const key of declaredAddKeys) {
      if (!baselineSet.has(key)) keys.push(key);
    }
    return keys;
  }, [declaredAddKeys, baselineSet]);

  // 展开/收起 capability 工具面板 —— 按需拉取 tool catalog
  //
  // 对于 mcp:<key> 类型的 capability，tool-catalog API 仅返回占位符。
  // 这里改为调用 probe 端点拿到真实工具列表（实时 tools/list），
  // 并将结果映射为 ToolDescriptor 格式供现有 ToolListPanel 消费。
  // 失败 / unsupported 时回填带说明的占位符，避免面板空白。
  const toggleExpand = useCallback(
    async (key: string) => {
      setExpandedCaps((prev) => {
        const next = new Set(prev);
        if (next.has(key)) {
          next.delete(key);
        } else {
          next.add(key);
        }
        return next;
      });
      if (toolCatalogCache[key]) return;

      const mcpServerName = key.startsWith("mcp:") ? key.slice(4) : null;
      const catalogEntry = catalogByKey.get(key);
      if (mcpServerName === null && catalogEntry) {
        setToolCatalogCache((prev) => ({ ...prev, [key]: catalogEntry.tools }));
        return;
      }

      if (mcpServerName !== null) {
        const preset = presets.find((p) => p.key === mcpServerName);
        if (!preset) {
          setToolCatalogCache((prev) => ({
            ...prev,
            [key]: [
              mcpProbePlaceholderDescriptor(
                key,
                mcpServerName,
                `未找到 MCP Preset "${mcpServerName}"`,
              ),
            ],
          }));
          return;
        }
        try {
          const result = await getOrRefreshProbe(preset.project_id, preset.transport);
          setToolCatalogCache((prev) => ({
            ...prev,
            [key]: mapMcpProbeToToolDescriptors({
              capabilityKey: key,
              serverName: mcpServerName,
              result,
            }),
          }));
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          setToolCatalogCache((prev) => ({
            ...prev,
            [key]: [mcpProbePlaceholderDescriptor(key, mcpServerName, `探测失败：${msg}`)],
          }));
        }
        return;
      }

      try {
        const tools = await fetchToolCatalog([key]);
        setToolCatalogCache((prev) => ({ ...prev, [key]: tools }));
      } catch {
        setToolCatalogCache((prev) => ({ ...prev, [key]: [] }));
      }
    },
    [toolCatalogCache, presets, getOrRefreshProbe, catalogByKey],
  );

  // 切换 baseline 能力的屏蔽状态：发出 Remove(cap) / 撤销该 Remove
  const toggleBaselineBlock = useCallback(
    (key: string) => {
      const target = makeRemoveCapability(key);
      if (capabilityBlockedByWorkflow(directives, key)) {
        onChange(removeDirective(directives, target));
      } else {
        onChange(addDirective(directives, target));
      }
    },
    [directives, onChange],
  );

  // 切换单个工具的屏蔽状态：发出 Remove(cap::tool) / 撤销该 Remove
  const toggleToolBlock = useCallback(
    (capKey: string, toolName: string) => {
      const target = makeRemoveTool(capKey, toolName);
      if (toolBlockedByWorkflow(directives, capKey, toolName)) {
        onChange(removeDirective(directives, target));
      } else {
        onChange(addDirective(directives, target));
      }
    },
    [directives, onChange],
  );

  // 追加区：添加一条新的能力级 Add
  const addExtraCapability = useCallback(
    (key: string) => {
      onChange(addDirective(directives, makeAddCapability(key)));
      setShowPicker(false);
    },
    [directives, onChange],
  );

  // 追加区：移除某能力的所有相关 Add directive（能力级 + 该能力下的工具级 Add）
  const removeExtraCapability = useCallback(
    (key: string) => {
      const next = directives.filter((d) => {
        if (!("add" in d)) return true;
        const qualified = d.add;
        // 匹配 "<key>" 或 "<key>::<tool>"
        if (qualified === key) return false;
        if (qualified.startsWith(`${key}::`)) return false;
        return true;
      });
      onChange(next);
      setExpandedCaps((prev) => {
        const nextSet = new Set(prev);
        nextSet.delete(key);
        return nextSet;
      });
    },
    [directives, onChange],
  );

  // 可追加选项：catalog 中未被 baseline 覆盖也未被显式 Add 的能力。
  const catalogAddable = useMemo(() => {
    return capabilityCatalog.filter(
      (entry) =>
        !baselineSet.has(entry.key) &&
        !declaredAddKeys.has(entry.key) &&
        targetKinds.some((targetKind) => capabilityVisibleForTargetKind(entry, targetKind)),
    );
  }, [baselineSet, capabilityCatalog, declaredAddKeys, targetKinds]);

  // 可追加的 MCP preset：当前 project 已注册且未被显式 Add 的
  const mcpAddable = useMemo(() => {
    return presets.filter((p) => !declaredAddKeys.has(`mcp:${p.key}`));
  }, [presets, declaredAddKeys]);

  return (
    <div className="space-y-5">
      {/* 基线能力 */}
      <div>
        <label className="agentdash-form-label">
          基线能力（{formatTargetKinds(targetKinds)}）
        </label>
        <p className="mb-2 text-[11px] text-muted-foreground">
          根据挂载类型自动授予的能力基线（<code className="rounded bg-secondary/50 px-1">auto_granted</code>）。
          每条能力可单独屏蔽，或展开后屏蔽下属某个工具。
        </p>
        {catalogLoading && (
          <p className="py-2 text-center text-xs text-muted-foreground">Capability catalog 加载中...</p>
        )}
        {catalogError && (
          <p className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-2 py-1 text-[11px] text-destructive">
            加载 capability catalog 失败：{catalogError}
          </p>
        )}
        <div className="space-y-1.5">
          {baselineKeys.map((key) => {
            const entry = catalogByKey.get(key);
            const isBlocked = capabilityBlockedByWorkflow(directives, key);
            const isExpanded = expandedCaps.has(key);
            const tools = toolCatalogCache[key] ?? [];
            return (
              <CapabilityRow
                key={key}
                capKey={key}
                label={entry?.label ?? key}
                description={entry?.description ?? ""}
                isBaseline
                isBlocked={isBlocked}
                isExpanded={isExpanded}
                tools={tools}
                directives={directives}
                onToggleBlock={() => toggleBaselineBlock(key)}
                onToggleExpand={() => void toggleExpand(key)}
                onToggleToolBlock={toggleToolBlock}
              />
            );
          })}
        </div>
      </div>

      {/* 追加能力 */}
      <div>
        <label className="agentdash-form-label">工作流追加能力</label>
        <p className="mb-2 text-[11px] text-muted-foreground">
          基线之外的能力 —— 例如 <code className="rounded bg-secondary/50 px-1">workflow_management</code>、
          <code className="rounded bg-secondary/50 px-1">mcp:&lt;preset&gt;</code>。每条以 <code className="rounded bg-secondary/50 px-1">Add</code>{" "}
          指令写入 contract。
        </p>
        {extraKeys.length === 0 && !showPicker && (
          <p className="py-2 text-center text-xs text-muted-foreground">暂无追加能力</p>
        )}
        <div className="space-y-1.5">
          {extraKeys.map((key) => {
            const catalogEntry = catalogByKey.get(key);
            const mcpName = extractMcpPresetName(key);
            const label = catalogEntry
              ? catalogEntry.label
              : mcpName
                ? `MCP · ${mcpName}`
                : key;
            const description = catalogEntry
              ? catalogEntry.description
              : mcpName
                ? `用户自定义 MCP Preset 引用。由后端按 preset key 展开为运行时 MCP server。`
                : "未识别的 capability key —— 建议清理。";
            const isBlocked = capabilityBlockedByWorkflow(directives, key);
            // 追加能力不需要 baseline 的「屏蔽」语义（移除 Add 即可），
            // 但若用户同时声明了 Add + Remove，仍以 Remove 为真
            const isExpanded = expandedCaps.has(key);
            const tools = toolCatalogCache[key] ?? [];
            const badge = mcpName ? (
              <span className="rounded bg-warning/15 px-1.5 py-0.5 text-[9px] font-mono text-warning">
                mcp
              </span>
            ) : !catalogEntry ? (
              <span className="rounded bg-destructive/10 px-1.5 py-0.5 text-[9px] text-destructive">
                未知
              </span>
            ) : null;
            return (
              <CapabilityRow
                key={key}
                capKey={key}
                label={label}
                description={description}
                isBaseline={false}
                isBlocked={isBlocked}
                isExpanded={isExpanded}
                tools={tools}
                directives={directives}
                onRemoveAdd={() => removeExtraCapability(key)}
                onToggleExpand={() => void toggleExpand(key)}
                onToggleToolBlock={toggleToolBlock}
                extraBadge={badge}
              />
            );
          })}
        </div>

        {/* Picker */}
        {showPicker ? (
          <div className="mt-2 rounded-[8px] border-2 border-dashed border-primary/30 bg-primary/5 p-3 space-y-3">
            {presetsError && (
              <p className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-2 py-1 text-[11px] text-destructive">
                加载 MCP Preset 失败：{presetsError}
              </p>
            )}

            {/* Well-known 可追加 */}
            <div>
              <p className="mb-1 text-[11px] font-medium text-muted-foreground">Well-known 能力</p>
              {catalogAddable.length === 0 ? (
                <p className="py-1 text-[11px] text-muted-foreground">
                  所有 well-known 能力已在基线或已追加
                </p>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {catalogAddable.map((entry) => (
                    <button
                      key={entry.key}
                      type="button"
                      onClick={() => addExtraCapability(entry.key)}
                      className="rounded-[8px] border border-border bg-background px-3 py-1 text-xs text-foreground hover:border-primary/30 hover:bg-primary/5"
                      title={entry.description}
                    >
                      {entry.label}
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* MCP Preset 可追加 */}
            <div>
              <p className="mb-1 text-[11px] font-medium text-muted-foreground">MCP Preset 引用</p>
              {presetsLoading ? (
                <p className="py-1 text-[11px] text-muted-foreground">加载中…</p>
              ) : mcpAddable.length === 0 ? (
                <p className="py-1 text-[11px] text-muted-foreground">
                  无可追加的 MCP Preset（当前 project 未注册或均已追加）
                </p>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {mcpAddable.map((preset) => {
                    const sourceLabel = preset.source === "builtin" ? "builtin" : "user";
                    return (
                      <button
                        key={preset.id}
                        type="button"
                        onClick={() => addExtraCapability(`mcp:${preset.key}`)}
                        className="flex items-center gap-1.5 rounded-[8px] border border-border bg-background px-3 py-1 text-xs text-foreground hover:border-primary/30 hover:bg-primary/5"
                        title={preset.description ?? preset.display_name}
                      >
                        <span>{preset.display_name}</span>
                        <span
                          className={`rounded px-1 py-0.5 text-[9px] font-mono ${
                            preset.source === "builtin"
                              ? "bg-warning/15 text-warning"
                              : "bg-secondary text-muted-foreground"
                          }`}
                        >
                          {sourceLabel}
                        </span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>

            <div className="flex justify-end">
              <button
                type="button"
                onClick={() => setShowPicker(false)}
                className="agentdash-button-secondary text-xs px-3 py-1"
              >
                关闭
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setShowPicker(true)}
            className="mt-2 w-full rounded-[8px] border-2 border-dashed border-border/60 py-2 text-sm text-muted-foreground hover:border-primary/40 hover:text-primary/70 transition-colors"
          >
            + 添加能力
          </button>
        )}
      </div>
    </div>
  );
}

// ─── Panel 外壳 ───────────────────────────────────────

export interface CapabilityPanelProps {
  projectId: string;
  targetKinds: WorkflowTargetKind[];
  directives: CapabilityDirective[];
  onDirectivesChange: (next: CapabilityDirective[]) => void;
}

export function CapabilityPanel({
  projectId,
  targetKinds,
  directives,
  onDirectivesChange,
}: CapabilityPanelProps) {
  return (
    <section className="space-y-2">
      <label className="agentdash-form-label">Agent 工具能力 ({directives.length})</label>
      <CapabilitiesEditor
        projectId={projectId}
        targetKinds={targetKinds}
        directives={directives}
        onChange={onDirectivesChange}
      />
    </section>
  );
}
