import { useState, useMemo } from "react";
import type { AgentPreset, ProjectAgentSummary } from "../../types";
import { useExecutorDiscovery } from "../executor-selector";

export interface AgentPresetEditorProps {
  presets: AgentPreset[];
  onSave: (presets: AgentPreset[]) => Promise<void>;
  isSaving?: boolean;
}

// ─── MCP Server 条目 ─────────────────────────────────

interface McpServerEntry {
  name: string;
  url: string;
}

function parseMcpServers(cfg: Record<string, unknown>): McpServerEntry[] {
  const raw = cfg.mcp_servers;
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((item): item is Record<string, unknown> => item != null && typeof item === "object")
    .map((item) => ({
      name: String(item.name ?? "").trim(),
      url: String(item.url ?? "").trim(),
    }))
    .filter((item) => item.name || item.url);
}

// ─── Form state ──────────────────────────────────────

interface PresetFormState {
  name: string;
  display_name: string;
  description: string;
  agent_type: string;
  model_id: string;
  permission_policy: string;
  subagent_keys: string[];
  mcp_servers: McpServerEntry[];
}

function presetToForm(preset?: AgentPreset): PresetFormState {
  const cfg = (preset?.config ?? {}) as Record<string, unknown>;
  const rawSubagents = cfg.subagent_keys;
  const subagentKeys = Array.isArray(rawSubagents)
    ? rawSubagents.filter((v): v is string => typeof v === "string")
    : [];
  return {
    name: preset?.name ?? "",
    display_name: String(cfg.display_name ?? ""),
    description: String(cfg.description ?? ""),
    agent_type: preset?.agent_type ?? "",
    model_id: String(cfg.model_id ?? ""),
    permission_policy: String(cfg.permission_policy ?? ""),
    subagent_keys: subagentKeys,
    mcp_servers: parseMcpServers(cfg),
  };
}

function formToPreset(form: PresetFormState): AgentPreset {
  const config: Record<string, unknown> = {};
  if (form.display_name.trim()) config.display_name = form.display_name.trim();
  if (form.description.trim()) config.description = form.description.trim();
  if (form.model_id.trim()) config.model_id = form.model_id.trim();
  if (form.permission_policy.trim()) config.permission_policy = form.permission_policy.trim();
  if (form.subagent_keys.length > 0) config.subagent_keys = form.subagent_keys;
  const validMcp = form.mcp_servers.filter((s) => s.name.trim() && s.url.trim());
  if (validMcp.length > 0) config.mcp_servers = validMcp;
  return {
    name: form.name.trim(),
    agent_type: form.agent_type.trim(),
    config,
  };
}

// ─── Shared form fields ──────────────────────────────

function PresetFormFields({
  form,
  patchForm,
  agentTypeOptions,
  isDiscoveryLoading,
  availableSubagents,
}: {
  form: PresetFormState;
  patchForm: (patch: Partial<PresetFormState>) => void;
  agentTypeOptions: Array<{ value: string; label: string }>;
  isDiscoveryLoading: boolean;
  availableSubagents?: ProjectAgentSummary[];
}) {
  const otherAgents = availableSubagents?.filter(
    (a) => a.key !== `preset:${form.name}` && a.key !== "default",
  );

  return (
    <>
      {/* ── 身份定义 ── */}
      <div>
        <label className="agentdash-form-label">预设名称 (key)</label>
        <input
          value={form.name}
          onChange={(e) => patchForm({ name: e.target.value })}
          placeholder="唯一标识，例如 code-review"
          className="agentdash-form-input"
        />
        <p className="mt-0.5 text-[10px] text-muted-foreground/60">
          内部标识，不直接展示给用户
        </p>
      </div>

      <div>
        <label className="agentdash-form-label">显示名称</label>
        <input
          value={form.display_name}
          onChange={(e) => patchForm({ display_name: e.target.value })}
          placeholder="留空则使用预设名称"
          className="agentdash-form-input"
        />
      </div>

      <div className="sm:col-span-2">
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={form.description}
          onChange={(e) => patchForm({ description: e.target.value })}
          rows={2}
          placeholder="这个 Agent 的职责和用途"
          className="agentdash-form-textarea"
        />
      </div>

      {/* ── 执行器 ── */}
      <div className="sm:col-span-2">
        <label className="agentdash-form-label">Agent 类型</label>
        <select
          value={form.agent_type}
          onChange={(e) => patchForm({ agent_type: e.target.value })}
          className="agentdash-form-select"
        >
          <option value="">
            {isDiscoveryLoading ? "加载执行器列表..." : "选择 Agent 类型"}
          </option>
          {agentTypeOptions.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
          {form.agent_type && !agentTypeOptions.some((o) => o.value === form.agent_type) && (
            <option value={form.agent_type}>{form.agent_type} (当前值)</option>
          )}
        </select>
      </div>

      {/* ── 运行配置 ── */}
      <div>
        <label className="agentdash-form-label">Model ID</label>
        <input
          value={form.model_id}
          onChange={(e) => patchForm({ model_id: e.target.value })}
          placeholder="可选，覆盖默认模型"
          className="agentdash-form-input"
        />
      </div>
      <div>
        <label className="agentdash-form-label">Permission Policy</label>
        <select
          value={form.permission_policy}
          onChange={(e) => patchForm({ permission_policy: e.target.value })}
          className="agentdash-form-select"
        >
          <option value="">默认</option>
          <option value="AUTO">AUTO</option>
          <option value="SUPERVISED">SUPERVISED</option>
          <option value="PLAN">PLAN</option>
        </select>
      </div>

      {/* ── Subagent 引用 ── */}
      {otherAgents && otherAgents.length > 0 && (
        <div className="sm:col-span-2">
          <label className="agentdash-form-label">可调度的 Subagent</label>
          <p className="mb-1.5 text-[10px] text-muted-foreground/60">
            勾选后，该 Agent 可通过 companion dispatch 调度这些 Agent 实例
          </p>
          <div className="max-h-32 space-y-1 overflow-y-auto rounded-[8px] border border-border bg-secondary/20 p-2">
            {otherAgents.map((agent) => (
              <label key={agent.key} className="flex items-center gap-2 text-xs text-foreground">
                <input
                  type="checkbox"
                  checked={form.subagent_keys.includes(agent.key)}
                  onChange={(e) => {
                    const next = e.target.checked
                      ? [...form.subagent_keys, agent.key]
                      : form.subagent_keys.filter((k) => k !== agent.key);
                    patchForm({ subagent_keys: next });
                  }}
                  className="rounded border-border"
                />
                <span>{agent.display_name}</span>
                <span className="text-[10px] text-muted-foreground">({agent.executor.executor})</span>
              </label>
            ))}
          </div>
        </div>
      )}

      {/* ── MCP Servers ── */}
      <div className="sm:col-span-2">
        <label className="agentdash-form-label">自定义 MCP Servers</label>
        <p className="mb-1.5 text-[10px] text-muted-foreground/60">
          会话启动时注入的额外 MCP 端点（需要后端 MCP Client 能力支持）
        </p>
        <div className="space-y-1.5">
          {form.mcp_servers.map((server, idx) => (
            <div key={idx} className="flex gap-1.5">
              <input
                value={server.name}
                onChange={(e) => {
                  const next = [...form.mcp_servers];
                  next[idx] = { ...next[idx], name: e.target.value };
                  patchForm({ mcp_servers: next });
                }}
                placeholder="名称"
                className="agentdash-form-input flex-1"
              />
              <input
                value={server.url}
                onChange={(e) => {
                  const next = [...form.mcp_servers];
                  next[idx] = { ...next[idx], url: e.target.value };
                  patchForm({ mcp_servers: next });
                }}
                placeholder="http://... 或 stdio://..."
                className="agentdash-form-input flex-[2]"
              />
              <button
                type="button"
                onClick={() => patchForm({ mcp_servers: form.mcp_servers.filter((_, i) => i !== idx) })}
                className="shrink-0 rounded-[6px] border border-destructive/30 px-2 text-xs text-destructive hover:bg-destructive/10"
              >
                ×
              </button>
            </div>
          ))}
          <button
            type="button"
            onClick={() => patchForm({ mcp_servers: [...form.mcp_servers, { name: "", url: "" }] })}
            className="text-xs text-muted-foreground transition-colors hover:text-foreground"
          >
            + 添加 MCP Server
          </button>
        </div>
      </div>
    </>
  );
}

// ─── Helpers ─────────────────────────────────────────

function useAgentTypeOptions() {
  const { executors, isLoading } = useExecutorDiscovery();
  const options = useMemo(() => {
    return executors.map((executor) => ({
      value: executor.id,
      label: `${executor.name}${!executor.available ? " (不可用)" : ""}`,
    }));
  }, [executors]);
  return { agentTypeOptions: options, isDiscoveryLoading: isLoading };
}

function validateForm(form: PresetFormState, existingNames: string[], editingName?: string): string | null {
  if (!form.name.trim()) return "预设名称不能为空";
  if (!form.agent_type.trim()) return "Agent 类型不能为空";
  const filtered = editingName
    ? existingNames.filter((n) => n !== editingName)
    : existingNames;
  if (filtered.includes(form.name.trim())) {
    return `预设名称 "${form.name.trim()}" 已存在`;
  }
  for (const server of form.mcp_servers) {
    if (server.name.trim() && !server.url.trim()) return `MCP Server "${server.name}" 缺少 URL`;
    if (!server.name.trim() && server.url.trim()) return "MCP Server 缺少名称";
  }
  return null;
}

function formatPresetSummary(preset: AgentPreset): string {
  const cfg = (preset.config ?? {}) as Record<string, unknown>;
  const displayName = String(cfg.display_name ?? "").trim();
  const parts: string[] = [preset.agent_type];
  if (displayName && displayName !== preset.name) parts.unshift(displayName);
  const desc = String(cfg.description ?? "").trim();
  if (desc) parts.push(desc);
  return parts.join(" · ");
}

// ─── AgentPresetEditor (inline in project config) ────

export function AgentPresetEditor({ presets, onSave, isSaving = false }: AgentPresetEditorProps) {
  const { agentTypeOptions, isDiscoveryLoading } = useAgentTypeOptions();
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [form, setForm] = useState<PresetFormState>(presetToForm());
  const [validationError, setValidationError] = useState<string | null>(null);

  const existingNames = presets.map((p) => p.name);
  const isFormOpen = isCreating || editingIndex !== null;

  const startCreate = () => {
    setForm(presetToForm());
    setEditingIndex(null);
    setIsCreating(true);
    setValidationError(null);
  };

  const startEdit = (index: number) => {
    setForm(presetToForm(presets[index]));
    setEditingIndex(index);
    setIsCreating(false);
    setValidationError(null);
  };

  const cancel = () => {
    setEditingIndex(null);
    setIsCreating(false);
    setValidationError(null);
  };

  const handleSave = async () => {
    const editingName = editingIndex != null ? presets[editingIndex]?.name : undefined;
    const err = validateForm(form, existingNames, editingName);
    if (err) { setValidationError(err); return; }
    const preset = formToPreset(form);
    const next = isCreating
      ? [...presets, preset]
      : presets.map((p, i) => (i === editingIndex ? preset : p));
    await onSave(next);
    cancel();
  };

  const handleDelete = async (index: number) => {
    await onSave(presets.filter((_, i) => i !== index));
  };

  const patchForm = (patch: Partial<PresetFormState>) => {
    setForm((prev) => ({ ...prev, ...patch }));
    setValidationError(null);
  };

  return (
    <div className="space-y-2.5">
      {presets.length === 0 && !isFormOpen && (
        <p className="rounded-[10px] border border-dashed border-border px-3 py-3 text-center text-xs text-muted-foreground">
          暂无 Agent 预设，点击下方按钮添加
        </p>
      )}

      {presets.map((preset, index) => (
        <div
          key={`${preset.name}-${index}`}
          className="flex items-center justify-between rounded-[12px] border border-border bg-secondary/30 px-4 py-3"
        >
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-foreground">{preset.name}</p>
            <p className="mt-0.5 truncate text-xs text-muted-foreground">
              {formatPresetSummary(preset)}
            </p>
          </div>
          <div className="ml-3 flex gap-1.5">
            <button
              type="button"
              onClick={() => startEdit(index)}
              disabled={isSaving || isFormOpen}
              className="rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground disabled:opacity-40"
            >
              编辑
            </button>
            <button
              type="button"
              onClick={() => void handleDelete(index)}
              disabled={isSaving || isFormOpen}
              className="rounded-[8px] border border-destructive/30 bg-background px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-40"
            >
              删除
            </button>
          </div>
        </div>
      ))}

      {isFormOpen && (
        <div className="space-y-3 rounded-[14px] border border-primary/30 bg-background p-4">
          <p className="text-sm font-medium text-foreground">
            {isCreating ? "新建 Agent 预设" : `编辑预设: ${presets[editingIndex!]?.name}`}
          </p>

          <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
            <PresetFormFields
              form={form}
              patchForm={patchForm}
              agentTypeOptions={agentTypeOptions}
              isDiscoveryLoading={isDiscoveryLoading}
            />
          </div>

          {validationError && (
            <p className="text-xs text-destructive">{validationError}</p>
          )}

          <div className="flex justify-end gap-2 border-t border-border pt-3">
            <button type="button" onClick={cancel} disabled={isSaving} className="agentdash-button-secondary">
              取消
            </button>
            <button type="button" onClick={() => void handleSave()} disabled={isSaving} className="agentdash-button-primary">
              {isSaving ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      )}

      {!isFormOpen && (
        <button
          type="button"
          onClick={startCreate}
          disabled={isSaving}
          className="w-full rounded-[12px] border border-dashed border-border py-2.5 text-sm text-muted-foreground transition-colors hover:border-primary/50 hover:text-foreground disabled:opacity-40"
        >
          + 添加 Agent 预设
        </button>
      )}
    </div>
  );
}

// ─── SinglePresetDialog (from Agent Hub) ─────────────

export interface SinglePresetDialogProps {
  open: boolean;
  initialPreset?: AgentPreset;
  existingNames: string[];
  availableSubagents?: ProjectAgentSummary[];
  onSave: (preset: AgentPreset) => Promise<void>;
  onClose: () => void;
  isSaving?: boolean;
}

export function SinglePresetDialog({
  open,
  initialPreset,
  existingNames,
  availableSubagents,
  onSave,
  onClose,
  isSaving = false,
}: SinglePresetDialogProps) {
  const { agentTypeOptions, isDiscoveryLoading } = useAgentTypeOptions();
  const [form, setForm] = useState<PresetFormState>(presetToForm(initialPreset));
  const [validationError, setValidationError] = useState<string | null>(null);
  const isEditing = Boolean(initialPreset);

  if (!open) return null;

  const handleSave = async () => {
    const err = validateForm(form, existingNames, isEditing ? initialPreset?.name : undefined);
    if (err) { setValidationError(err); return; }
    await onSave(formToPreset(form));
  };

  const patchForm = (patch: Partial<PresetFormState>) => {
    setForm((prev) => ({ ...prev, ...patch }));
    setValidationError(null);
  };

  return (
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/18 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-lg max-h-[90vh] overflow-y-auto rounded-[16px] border border-border bg-background shadow-2xl">
          <div className="border-b border-border px-5 py-4">
            <span className="agentdash-panel-header-tag">Agent</span>
            <h4 className="text-base font-semibold text-foreground">
              {isEditing ? `编辑 Agent 预设: ${initialPreset?.name}` : "新建 Agent 预设"}
            </h4>
            <p className="mt-1 text-xs text-muted-foreground">
              配置后将出现在 Agent Hub 卡片列表中
            </p>
          </div>

          <div className="space-y-3 p-5">
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
              <PresetFormFields
                form={form}
                patchForm={patchForm}
                agentTypeOptions={agentTypeOptions}
                isDiscoveryLoading={isDiscoveryLoading}
                availableSubagents={availableSubagents}
              />
            </div>

            {validationError && (
              <p className="text-xs text-destructive">{validationError}</p>
            )}
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
            <button type="button" onClick={onClose} disabled={isSaving} className="agentdash-button-secondary">取消</button>
            <button type="button" onClick={() => void handleSave()} disabled={isSaving} className="agentdash-button-primary">
              {isSaving ? "保存中..." : isEditing ? "保存修改" : "创建预设"}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
