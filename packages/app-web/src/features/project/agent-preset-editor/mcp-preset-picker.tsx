import { useEffect, useMemo, useState } from "react";
import {
  createMcpPreset,
  fetchProjectMcpPresets,
} from "../../../services/mcpPreset";
import { useMcpProbeStore } from "../../../stores/mcpProbeStore";
import type { CapabilityDirective, McpPresetDto, ProbeMcpToolInfo } from "../../../types";
import { McpTransportConfigEditor } from "../../mcp-shared";
import {
  MCP_ROUTE_POLICY_OPTIONS,
  buildCreateMcpPresetRequest,
  buildMcpPresetFormState,
  hasMcpRuntimeBinding,
  readMcpRoutePolicy,
  validateMcpPresetForm,
  type McpPresetFormState,
} from "../../mcp-shared/helpers";
import { buildMcpProbeViewModel } from "../../mcp-shared/probeViewModel";
import { toolBlockedByWorkflow } from "../../workflow/capability-directive-ops";
import { CapabilityPicker } from "./capability-picker";
import {
  addMcpPresetDirective,
  mcpCapabilityKey,
  removeMcpPresetDirective,
  selectedMcpPresetKeysFromDirectives,
  setMcpToolBlockedDirective,
} from "./form-state";

function McpToolBlockList({
  projectId,
  preset,
  directives,
  onChange,
}: {
  projectId?: string;
  preset: McpPresetDto;
  directives: CapabilityDirective[];
  onChange: (directives: CapabilityDirective[]) => void;
}) {
  const [isProbing, setIsProbing] = useState(false);
  useMcpProbeStore((state) => state.cache);
  const getCachedProbe = useMcpProbeStore((state) => state.getCached);
  const refreshProbe = useMcpProbeStore((state) => state.refresh);
  const probeResult = projectId
    ? getCachedProbe(projectId, preset.transport, preset.runtime_binding)
    : null;
  const probeView = useMemo(() => buildMcpProbeViewModel(probeResult), [probeResult]);
  const capabilityKey = mcpCapabilityKey(preset.key);

  const handleProbe = async () => {
    if (!projectId) return;
    setIsProbing(true);
    try {
      await refreshProbe(projectId, preset.transport, preset.runtime_binding);
    } finally {
      setIsProbing(false);
    }
  };

  const toggleTool = (tool: ProbeMcpToolInfo) => {
    const isBlocked = toolBlockedByWorkflow(directives, capabilityKey, tool.name);
    onChange(setMcpToolBlockedDirective(directives, preset.key, tool.name, !isBlocked));
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <span
          className={`text-[10px] ${
            probeView.detailTone === "danger"
              ? "text-destructive"
              : probeView.detailTone === "success"
                ? "text-primary"
                : "text-muted-foreground"
          }`}
          title={probeView.bodyTitle ?? undefined}
        >
          {probeView.detailMessage ?? probeView.bodyMessage}
        </span>
        <button
          type="button"
          onClick={() => void handleProbe()}
          disabled={!projectId || isProbing}
          className="rounded-[6px] border border-border px-2 py-0.5 text-[10px] text-muted-foreground transition-colors hover:border-primary/30 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isProbing ? "探测中..." : "探测工具"}
        </button>
      </div>
      {probeView.showToolGrid && (
        <div className="space-y-1">
          {probeView.tools.map((tool) => {
            const isBlocked = toolBlockedByWorkflow(directives, capabilityKey, tool.name);
            return (
              <div
                key={tool.name}
                className={`flex items-center gap-2 rounded-[7px] border px-2 py-1 text-[11px] ${
                  isBlocked
                    ? "border-destructive/30 bg-destructive/5 text-destructive"
                    : "border-border bg-background text-foreground"
                }`}
              >
                <div className="min-w-0 flex-1">
                  <div className={`truncate font-mono ${isBlocked ? "line-through" : ""}`}>
                    {tool.name}
                  </div>
                  {tool.description && (
                    <div className="truncate text-[10px] text-muted-foreground/75">
                      {tool.description}
                    </div>
                  )}
                </div>
                <button
                  type="button"
                  onClick={() => toggleTool(tool)}
                  className={`shrink-0 rounded-[6px] border px-2 py-0.5 text-[10px] transition-colors ${
                    isBlocked
                      ? "border-primary/30 text-primary hover:bg-primary/5"
                      : "border-destructive/30 text-destructive hover:bg-destructive/5"
                  }`}
                >
                  {isBlocked ? "恢复" : "屏蔽"}
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function McpPresetPicker({
  projectId,
  directives,
  onChange,
}: {
  projectId?: string;
  directives: CapabilityDirective[];
  onChange: (directives: CapabilityDirective[]) => void;
}) {
  const [presets, setPresets] = useState<McpPresetDto[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [createForm, setCreateForm] = useState<McpPresetFormState>(() => buildMcpPresetFormState());
  const [createError, setCreateError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);

  const loadPresets = async () => {
    if (!projectId) return;
    setIsLoading(true);
    setError(null);
    try {
      const next = await fetchProjectMcpPresets(projectId);
      setPresets(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : "加载 MCP Preset 失败");
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void loadPresets();
    // loadPresets 本身只依赖 projectId，把它纳入 deps 会因每次渲染重建函数引用导致无限 fetch。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  const selectedKeys = useMemo(
    () => selectedMcpPresetKeysFromDirectives(directives),
    [directives],
  );

  const toggleKey = (key: string) => {
    if (selectedKeys.includes(key)) {
      onChange(removeMcpPresetDirective(directives, key));
      return;
    }
    onChange(addMcpPresetDirective(directives, key));
  };

  const handleCreate = async () => {
    if (!projectId) return;
    const validationError = validateMcpPresetForm(createForm);
    if (validationError) {
      setCreateError(validationError);
      return;
    }
    setIsCreating(true);
    setCreateError(null);
    try {
      const created = await createMcpPreset(projectId, buildCreateMcpPresetRequest(createForm));
      setPresets((prev) => [...prev, created]);
      onChange(addMcpPresetDirective(directives, created.key));
      setIsCreateOpen(false);
      setCreateForm(buildMcpPresetFormState());
    } catch (e) {
      setCreateError(e instanceof Error ? e.message : "创建 MCP Preset 失败");
    } finally {
      setIsCreating(false);
    }
  };

  const sortedPresets = useMemo(
    () => presets.slice().sort((a, b) => a.display_name.localeCompare(b.display_name, "zh-CN")),
    [presets],
  );

  const createCard = projectId ? (
    <button
      type="button"
      onClick={() => {
        setCreateError(null);
        setCreateForm(buildMcpPresetFormState());
        setIsCreateOpen(true);
      }}
      className="flex min-h-[96px] flex-col items-center justify-center gap-1 rounded-[12px] border border-dashed border-border px-3 py-3 text-[11px] text-muted-foreground transition-colors hover:border-primary/50 hover:bg-secondary/20 hover:text-foreground"
    >
      <span className="text-base leading-none">+</span>
      <span>快速创建 MCP Preset</span>
    </button>
  ) : null;

  return (
    <div className="space-y-4">
      <CapabilityPicker
        hint="Agent 仅引用 project 级 MCP Preset；不再支持内联定义原始 MCP Server。"
        isLoading={isLoading}
        error={error}
        items={sortedPresets}
        selectedKeys={selectedKeys}
        itemKey={(p) => p.key}
        itemToCardProps={(p) => ({
          reactKey: p.id,
          title: p.display_name,
          subtitle: p.key,
          description: p.description?.trim() || undefined,
          chips: [
            { label: p.transport.type },
            { label: p.route_policy },
            ...(hasMcpRuntimeBinding(p.runtime_binding) ? [{ label: "运行时绑定" }] : []),
          ],
          footer: selectedKeys.includes(p.key) ? (
            <McpToolBlockList
              projectId={projectId}
              preset={p}
              directives={directives}
              onChange={onChange}
            />
          ) : undefined,
        })}
        onToggle={toggleKey}
        loadingText="正在加载 MCP Preset…"
        emptyAllText="当前项目还没有 MCP Preset"
        enabledEmptyText="尚未启用任何 MCP Preset，从下方选取或创建。"
        availableEmptyText="所有可用 MCP Preset 都已启用。"
        trailingAvailable={createCard ?? undefined}
      />

      {isCreateOpen && (
        <div className="rounded-[12px] border border-primary/20 bg-background p-3 space-y-3">
          <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
            <div>
              <label className="agentdash-form-label">工具标识</label>
              <input
                value={createForm.key}
                onChange={(e) => {
                  setCreateForm((prev) => ({ ...prev, key: e.target.value }));
                  setCreateError(null);
                }}
                placeholder="例如 filesystem-read"
                className="agentdash-form-input"
              />
            </div>
            <div>
              <label className="agentdash-form-label">显示名称</label>
              <input
                value={createForm.display_name}
                onChange={(e) => {
                  setCreateForm((prev) => ({ ...prev, display_name: e.target.value }));
                  setCreateError(null);
                }}
                placeholder="例如 Filesystem"
                className="agentdash-form-input"
              />
            </div>
          </div>
          <div>
            <label className="agentdash-form-label">描述</label>
            <textarea
              value={createForm.description}
              onChange={(e) => {
                setCreateForm((prev) => ({ ...prev, description: e.target.value }));
                setCreateError(null);
              }}
              rows={2}
              className="agentdash-form-textarea"
            />
          </div>
          <div>
            <label className="agentdash-form-label">路由策略</label>
            <select
              value={createForm.route_policy}
              onChange={(e) => {
                setCreateForm((prev) => ({
                  ...prev,
                  route_policy: readMcpRoutePolicy(e.target.value),
                }));
                setCreateError(null);
              }}
              className="agentdash-form-select"
            >
              {MCP_ROUTE_POLICY_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="agentdash-form-label">Transport 定义</label>
            <McpTransportConfigEditor
              value={createForm.transport}
              onChange={(transport) => {
                setCreateForm((prev) => ({ ...prev, transport }));
                setCreateError(null);
              }}
            />
          </div>
          {createError && (
            <p className="text-xs text-destructive">{createError}</p>
          )}
          <div className="flex justify-end gap-2 border-t border-border pt-3">
            <button
              type="button"
              onClick={() => setIsCreateOpen(false)}
              className="agentdash-button-secondary"
              disabled={isCreating}
            >
              取消
            </button>
            <button
              type="button"
              onClick={() => void handleCreate()}
              className="agentdash-button-primary"
              disabled={isCreating}
            >
              {isCreating ? "创建中..." : "创建并选中"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
