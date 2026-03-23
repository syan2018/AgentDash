import { useState, useMemo } from "react";
import type { AgentPreset, McpEnvVar, McpHttpHeader, McpServerDecl } from "../../types";
import { useExecutorDiscovery } from "../executor-selector";

export interface AgentPresetEditorProps {
  presets: AgentPreset[];
  onSave: (presets: AgentPreset[]) => Promise<void>;
  isSaving?: boolean;
}

interface PresetFormState {
  name: string;
  display_name: string;
  description: string;
  agent_type: string;
  variant: string;
  model_id: string;
  agent_id: string;
  reasoning_id: string;
  permission_policy: string;
  mcp_servers: McpServerDecl[];
}

function presetToForm(preset?: AgentPreset): PresetFormState {
  const cfg = preset?.config ?? {};
  const rawMcps = Array.isArray(cfg.mcp_servers) ? (cfg.mcp_servers as McpServerDecl[]) : [];
  return {
    name: preset?.name ?? "",
    display_name: String(cfg.display_name ?? ""),
    description: String(cfg.description ?? ""),
    agent_type: preset?.agent_type ?? "",
    variant: String(cfg.variant ?? ""),
    model_id: String(cfg.model_id ?? ""),
    agent_id: String(cfg.agent_id ?? ""),
    reasoning_id: String(cfg.reasoning_id ?? ""),
    permission_policy: String(cfg.permission_policy ?? ""),
    mcp_servers: rawMcps,
  };
}

function formToPreset(form: PresetFormState): AgentPreset {
  const config: Record<string, unknown> = {};
  if (form.display_name.trim()) config.display_name = form.display_name.trim();
  if (form.description.trim()) config.description = form.description.trim();
  if (form.variant.trim()) config.variant = form.variant.trim();
  if (form.model_id.trim()) config.model_id = form.model_id.trim();
  if (form.agent_id.trim()) config.agent_id = form.agent_id.trim();
  if (form.reasoning_id.trim()) config.reasoning_id = form.reasoning_id.trim();
  if (form.permission_policy.trim()) config.permission_policy = form.permission_policy.trim();
  if (form.mcp_servers.length > 0) config.mcp_servers = form.mcp_servers;
  return {
    name: form.name.trim(),
    agent_type: form.agent_type.trim(),
    config,
  };
}

// ─── MCP Servers Editor ──────────────────────────────────

function KeyValueList({
  items,
  onChange,
  keyPlaceholder,
  valuePlaceholder,
}: {
  items: McpHttpHeader[] | McpEnvVar[];
  onChange: (items: McpHttpHeader[]) => void;
  keyPlaceholder: string;
  valuePlaceholder: string;
}) {
  return (
    <div className="space-y-1">
      {items.map((item, i) => (
        <div key={i} className="flex gap-1.5">
          <input
            value={item.name}
            onChange={(e) => {
              const next = [...items] as McpHttpHeader[];
              next[i] = { ...next[i], name: e.target.value };
              onChange(next);
            }}
            placeholder={keyPlaceholder}
            className="agentdash-form-input flex-1"
          />
          <input
            value={item.value}
            onChange={(e) => {
              const next = [...items] as McpHttpHeader[];
              next[i] = { ...next[i], value: e.target.value };
              onChange(next);
            }}
            placeholder={valuePlaceholder}
            className="agentdash-form-input flex-1"
          />
          <button
            type="button"
            onClick={() => {
              const next = items.filter((_, j) => j !== i) as McpHttpHeader[];
              onChange(next);
            }}
            className="shrink-0 rounded-[6px] border border-destructive/30 px-2 text-xs text-destructive hover:bg-destructive/10"
          >
            ×
          </button>
        </div>
      ))}
      <button
        type="button"
        onClick={() => onChange([...items as McpHttpHeader[], { name: "", value: "" }])}
        className="text-[10px] text-muted-foreground hover:text-foreground"
      >
        + 添加
      </button>
    </div>
  );
}

function StringList({
  items,
  onChange,
  placeholder,
}: {
  items: string[];
  onChange: (items: string[]) => void;
  placeholder: string;
}) {
  return (
    <div className="space-y-1">
      {items.map((item, i) => (
        <div key={i} className="flex gap-1.5">
          <input
            value={item}
            onChange={(e) => {
              const next = [...items];
              next[i] = e.target.value;
              onChange(next);
            }}
            placeholder={placeholder}
            className="agentdash-form-input flex-1"
          />
          <button
            type="button"
            onClick={() => onChange(items.filter((_, j) => j !== i))}
            className="shrink-0 rounded-[6px] border border-destructive/30 px-2 text-xs text-destructive hover:bg-destructive/10"
          >
            ×
          </button>
        </div>
      ))}
      <button
        type="button"
        onClick={() => onChange([...items, ""])}
        className="text-[10px] text-muted-foreground hover:text-foreground"
      >
        + 添加
      </button>
    </div>
  );
}

function McpServerEntry({
  server,
  onChange,
  onRemove,
}: {
  server: McpServerDecl;
  onChange: (s: McpServerDecl) => void;
  onRemove: () => void;
}) {
  return (
    <div className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3">
      <div className="flex items-center gap-2">
        <select
          value={server.type}
          onChange={(e) => {
            const t = e.target.value as McpServerDecl["type"];
            if (t === "stdio") {
              onChange({ type: "stdio", name: server.name, command: "", args: [], env: [] });
            } else {
              onChange({ type: t, name: server.name, url: "", headers: [] });
            }
          }}
          className="agentdash-form-select w-24"
        >
          <option value="http">HTTP</option>
          <option value="sse">SSE</option>
          <option value="stdio">Stdio</option>
        </select>
        <input
          value={server.name}
          onChange={(e) => onChange({ ...server, name: e.target.value })}
          placeholder="服务名称"
          className="agentdash-form-input flex-1"
        />
        <button
          type="button"
          onClick={onRemove}
          className="shrink-0 rounded-[6px] border border-destructive/30 px-2 py-1 text-xs text-destructive hover:bg-destructive/10"
        >
          删除
        </button>
      </div>

      {(server.type === "http" || server.type === "sse") && (
        <>
          <div>
            <label className="agentdash-form-label">URL</label>
            <input
              value={server.url}
              onChange={(e) => onChange({ ...server, url: e.target.value })}
              placeholder="https://example.com/mcp"
              className="agentdash-form-input"
            />
          </div>
          <div>
            <label className="agentdash-form-label">Headers</label>
            <KeyValueList
              items={server.headers ?? []}
              onChange={(h) => onChange({ ...server, headers: h })}
              keyPlaceholder="Header 名称"
              valuePlaceholder="值"
            />
          </div>
        </>
      )}

      {server.type === "stdio" && (
        <>
          <div>
            <label className="agentdash-form-label">Command</label>
            <input
              value={server.command}
              onChange={(e) => onChange({ ...server, command: e.target.value })}
              placeholder="npx / python / /path/to/binary"
              className="agentdash-form-input"
            />
          </div>
          <div>
            <label className="agentdash-form-label">Args</label>
            <StringList
              items={server.args ?? []}
              onChange={(a) => onChange({ ...server, args: a })}
              placeholder="参数"
            />
          </div>
          <div>
            <label className="agentdash-form-label">Env</label>
            <KeyValueList
              items={(server.env ?? []) as McpHttpHeader[]}
              onChange={(e) => onChange({ ...server, env: e as McpEnvVar[] })}
              keyPlaceholder="变量名"
              valuePlaceholder="值"
            />
          </div>
        </>
      )}
    </div>
  );
}

function McpServersEditor({
  servers,
  onChange,
}: {
  servers: McpServerDecl[];
  onChange: (servers: McpServerDecl[]) => void;
}) {
  const addServer = () => {
    onChange([...servers, { type: "http", name: "", url: "", headers: [] }]);
  };

  return (
    <div className="space-y-2">
      {servers.length === 0 && (
        <p className="rounded-[8px] border border-dashed border-border px-2 py-2 text-center text-[10px] text-muted-foreground">
          暂无 MCP Server，点击下方按钮添加
        </p>
      )}
      {servers.map((server, i) => (
        <McpServerEntry
          key={i}
          server={server}
          index={i}
          onChange={(s) => {
            const next = [...servers];
            next[i] = s;
            onChange(next);
          }}
          onRemove={() => onChange(servers.filter((_, j) => j !== i))}
        />
      ))}
      <button
        type="button"
        onClick={addServer}
        className="w-full rounded-[8px] border border-dashed border-border py-1.5 text-[10px] text-muted-foreground hover:border-primary/50 hover:text-foreground"
      >
        + 添加 MCP Server
      </button>
    </div>
  );
}

// ─── Preset Form ─────────────────────────────────────────

function PresetFormFields({
  form,
  patchForm,
  agentTypeOptions,
  isDiscoveryLoading,
}: {
  form: PresetFormState;
  patchForm: (patch: Partial<PresetFormState>) => void;
  agentTypeOptions: Array<{ value: string; label: string }>;
  isDiscoveryLoading: boolean;
}) {
  return (
    <>
      <div>
        <label className="agentdash-form-label">预设名称 (key)</label>
        <input
          value={form.name}
          onChange={(e) => patchForm({ name: e.target.value })}
          placeholder="唯一标识，例如 code-review"
          className="agentdash-form-input"
        />
        <p className="mt-0.5 text-[10px] text-muted-foreground/60">
          用作内部标识，不会直接展示给用户
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

      <details className="sm:col-span-2">
        <summary className="cursor-pointer text-xs text-muted-foreground transition-colors hover:text-foreground">
          执行器高级配置
        </summary>
        <div className="mt-2 grid grid-cols-1 gap-2 sm:grid-cols-2">
          <div>
            <label className="agentdash-form-label">Variant</label>
            <input value={form.variant} onChange={(e) => patchForm({ variant: e.target.value })} placeholder="可选" className="agentdash-form-input" />
          </div>
          <div>
            <label className="agentdash-form-label">Model ID</label>
            <input value={form.model_id} onChange={(e) => patchForm({ model_id: e.target.value })} placeholder="可选" className="agentdash-form-input" />
          </div>
          <div>
            <label className="agentdash-form-label">Agent ID</label>
            <input value={form.agent_id} onChange={(e) => patchForm({ agent_id: e.target.value })} placeholder="可选" className="agentdash-form-input" />
          </div>
          <div>
            <label className="agentdash-form-label">Reasoning ID</label>
            <input value={form.reasoning_id} onChange={(e) => patchForm({ reasoning_id: e.target.value })} placeholder="可选" className="agentdash-form-input" />
          </div>
          <div className="sm:col-span-2">
            <label className="agentdash-form-label">Permission Policy</label>
            <select value={form.permission_policy} onChange={(e) => patchForm({ permission_policy: e.target.value })} className="agentdash-form-select">
              <option value="">默认</option>
              <option value="AUTO">AUTO</option>
              <option value="SUPERVISED">SUPERVISED</option>
              <option value="PLAN">PLAN</option>
            </select>
          </div>
        </div>
      </details>

      <details className="sm:col-span-2">
        <summary className="cursor-pointer text-xs text-muted-foreground transition-colors hover:text-foreground">
          MCP Servers 配置 ({form.mcp_servers.length} 个)
        </summary>
        <div className="mt-2">
          <McpServersEditor
            servers={form.mcp_servers}
            onChange={(mcp_servers) => patchForm({ mcp_servers })}
          />
        </div>
      </details>
    </>
  );
}

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
  return null;
}

function formatPresetSummary(preset: AgentPreset): string {
  const cfg = preset.config ?? {};
  const displayName = String(cfg.display_name ?? "").trim();
  const parts: string[] = [preset.agent_type];
  if (displayName && displayName !== preset.name) parts.unshift(displayName);
  const desc = String(cfg.description ?? "").trim();
  if (desc) parts.push(desc);
  const mcps = Array.isArray(cfg.mcp_servers) ? (cfg.mcp_servers as McpServerDecl[]) : [];
  if (mcps.length > 0) parts.push(`${mcps.length} MCP`);
  return parts.join(" · ");
}

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

export interface SinglePresetDialogProps {
  open: boolean;
  initialPreset?: AgentPreset;
  existingNames: string[];
  onSave: (preset: AgentPreset) => Promise<void>;
  onClose: () => void;
  isSaving?: boolean;
}

export function SinglePresetDialog({
  open,
  initialPreset,
  existingNames,
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
        <div className="w-full max-w-lg rounded-[16px] border border-border bg-background shadow-2xl">
          <div className="border-b border-border px-5 py-4">
            <span className="agentdash-panel-header-tag">Agent</span>
            <h4 className="text-base font-semibold text-foreground">
              {isEditing ? `编辑 Agent 预设: ${initialPreset?.name}` : "新建 Agent 预设"}
            </h4>
            <p className="mt-1 text-xs text-muted-foreground">
              配置后将出现在 Agent Hub 卡片列表中
            </p>
          </div>

          <div className="max-h-[70vh] space-y-3 overflow-y-auto p-5">
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
