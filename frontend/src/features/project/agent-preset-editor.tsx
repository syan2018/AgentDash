import { useState, useEffect, useMemo } from "react";
import type { AgentPreset, McpEnvVar, McpHttpHeader, McpServerDecl, SystemPromptMode, ThinkingLevel, ToolCluster } from "../../types";
import { THINKING_LEVEL_OPTIONS, TOOL_CLUSTER_OPTIONS, isThinkingLevel } from "../../types";
import { useExecutorDiscovery, useExecutorDiscoveredOptions } from "../executor-selector";
import type { ModelInfo, PermissionPolicy } from "../executor-selector";
import { AddressSpaceBrowser } from "../address-space";

export interface AgentPresetEditorProps {
  presets: AgentPreset[];
  onSave: (presets: AgentPreset[]) => Promise<void>;
  isSaving?: boolean;
}

export interface PresetFormState {
  name: string;
  display_name: string;
  description: string;
  agent_type: string;
  provider_id: string;
  model_id: string;
  agent_id: string;
  thinking_level: ThinkingLevel | "";
  permission_policy: string;
  system_prompt: string;
  system_prompt_mode: SystemPromptMode | "";
  mcp_servers: McpServerDecl[];
  tool_clusters: ToolCluster[];
  allowed_companions: string[];
}

export function presetToForm(preset?: AgentPreset): PresetFormState {
  const cfg = preset?.config ?? {};
  const rawMcps = Array.isArray(cfg.mcp_servers) ? (cfg.mcp_servers as McpServerDecl[]) : [];
  const rawClusters = Array.isArray(cfg.tool_clusters) ? (cfg.tool_clusters as ToolCluster[]) : [];
  const rawCompanions = Array.isArray(cfg.allowed_companions) ? (cfg.allowed_companions as string[]) : [];
  return {
    name: preset?.name ?? "",
    display_name: String(cfg.display_name ?? ""),
    description: String(cfg.description ?? ""),
    agent_type: preset?.agent_type ?? "",
    provider_id: String(cfg.provider_id ?? ""),
    model_id: String(cfg.model_id ?? ""),
    agent_id: String(cfg.agent_id ?? ""),
    thinking_level: isThinkingLevel(cfg.thinking_level) ? cfg.thinking_level : "",
    permission_policy: String(cfg.permission_policy ?? ""),
    system_prompt: String(cfg.system_prompt ?? ""),
    system_prompt_mode: (cfg.system_prompt_mode === "override" || cfg.system_prompt_mode === "append") ? cfg.system_prompt_mode : "",
    mcp_servers: rawMcps,
    tool_clusters: rawClusters,
    allowed_companions: rawCompanions,
  };
}

export function formToPreset(form: PresetFormState): AgentPreset {
  const config: Record<string, unknown> = {};
  if (form.display_name.trim()) config.display_name = form.display_name.trim();
  if (form.description.trim()) config.description = form.description.trim();
  if (form.provider_id.trim()) config.provider_id = form.provider_id.trim();
  if (form.model_id.trim()) config.model_id = form.model_id.trim();
  if (form.agent_id.trim()) config.agent_id = form.agent_id.trim();
  if (form.thinking_level) config.thinking_level = form.thinking_level;
  if (form.permission_policy.trim()) config.permission_policy = form.permission_policy.trim();
  if (form.system_prompt.trim()) config.system_prompt = form.system_prompt.trim();
  if (form.system_prompt.trim() && form.system_prompt_mode) config.system_prompt_mode = form.system_prompt_mode;
  if (form.mcp_servers.length > 0) config.mcp_servers = form.mcp_servers;
  if (form.tool_clusters.length > 0) config.tool_clusters = form.tool_clusters;
  if (form.allowed_companions.length > 0) config.allowed_companions = form.allowed_companions;
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
        <label className="flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground">
          <input
            type="checkbox"
            checked={server.relay ?? (server.type === "stdio")}
            onChange={(e) => onChange({ ...server, relay: e.target.checked })}
            className="h-3 w-3"
          />
          Relay
        </label>
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

// ─── Tool Capabilities ──────────────────────────────────

function ToolCapabilitiesField({
  clusters,
  onChange,
}: {
  clusters: ToolCluster[];
  onChange: (next: ToolCluster[]) => void;
}) {
  // empty = all enabled (no restriction)
  const isAll = clusters.length === 0;
  const has = (v: ToolCluster) => isAll || clusters.includes(v);

  const toggle = (v: ToolCluster) => {
    if (isAll) {
      // entering custom mode: enable everything except the toggled one
      onChange(TOOL_CLUSTER_OPTIONS.map((o) => o.value).filter((c) => c !== v));
      return;
    }
    const next = clusters.includes(v)
      ? clusters.filter((c) => c !== v)
      : [...clusters, v];
    // if all re-selected, collapse back to []
    onChange(next.length >= TOOL_CLUSTER_OPTIONS.length ? [] : next);
  };

  const basicOpts = TOOL_CLUSTER_OPTIONS.filter((o) => o.group === "basic");
  const extOpts = TOOL_CLUSTER_OPTIONS.filter((o) => o.group === "extended");

  return (
    <div className="space-y-3">
      {/* ── basic: horizontal pill toggles ── */}
      <div>
        <label className="agentdash-form-label">基础能力</label>
        <div className="flex flex-wrap gap-1.5">
          {basicOpts.map((opt) => {
            const on = has(opt.value);
            return (
              <button
                key={opt.value}
                type="button"
                onClick={() => toggle(opt.value)}
                className={`rounded-[8px] border px-3 py-1.5 text-xs font-medium transition-all duration-160 ${
                  on
                    ? "border-primary/30 bg-primary/8 text-primary"
                    : "border-border bg-secondary/30 text-muted-foreground hover:border-primary/20 hover:text-foreground"
                }`}
                title={opt.description}
              >
                {opt.label}
              </button>
            );
          })}
        </div>
      </div>

      {/* ── extended: vertical rows with toggle switches ── */}
      <div>
        <label className="agentdash-form-label">扩展能力</label>
        <div className="rounded-[10px] border border-border bg-secondary/20 p-2.5 space-y-0.5">
          {extOpts.map((opt) => {
            const on = has(opt.value);
            return (
              <label
                key={opt.value}
                className={`flex cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-[7px] transition-all duration-160 ${
                  on
                    ? "bg-primary/6"
                    : "opacity-45 hover:opacity-70"
                }`}
              >
                <span className="relative inline-flex h-[18px] w-[32px] shrink-0">
                  <input
                    type="checkbox"
                    checked={on}
                    onChange={() => toggle(opt.value)}
                    className="peer sr-only"
                  />
                  <span className="absolute inset-0 rounded-full bg-border transition-colors duration-160 peer-checked:bg-primary" />
                  <span className="absolute left-[3px] top-[3px] h-3 w-3 rounded-full bg-background shadow-sm transition-transform duration-160 peer-checked:translate-x-[14px]" />
                </span>
                <span className="text-xs font-medium text-foreground">{opt.label}</span>
                <span className="text-[10px] text-muted-foreground">{opt.description}</span>
              </label>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ─── Form Section ───────────────────────────────────────

function FormSection({
  title,
  badge,
  defaultOpen = true,
  children,
}: {
  title: string;
  badge?: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="rounded-[12px] border border-border/70 bg-secondary/15">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-4 py-2.5 text-left transition-colors hover:bg-secondary/25"
      >
        <svg
          className={`h-3 w-3 shrink-0 text-muted-foreground transition-transform duration-150 ${open ? "rotate-90" : ""}`}
          viewBox="0 0 16 16"
          fill="currentColor"
        >
          <path d="M6 3l5 5-5 5V3z" />
        </svg>
        <span className="text-xs font-semibold uppercase tracking-[0.1em] text-foreground/80">{title}</span>
        {badge && (
          <span className="ml-auto rounded-full bg-secondary/60 px-2 py-0.5 text-[10px] text-muted-foreground">
            {badge}
          </span>
        )}
      </button>
      {open && <div className="space-y-3 px-4 pb-4 pt-1">{children}</div>}
    </div>
  );
}

// ─── Preset Form ─────────────────────────────────────────

export function PresetFormFields({
  form,
  patchForm,
  agentTypeOptions,
  isDiscoveryLoading,
  siblingAgents,
}: {
  form: PresetFormState;
  patchForm: (patch: Partial<PresetFormState>) => void;
  agentTypeOptions: Array<{ value: string; label: string }>;
  isDiscoveryLoading: boolean;
  siblingAgents?: Array<{ name: string; display_name: string }>;
}) {
  const discovered = useExecutorDiscoveredOptions(form.agent_type);
  const modelSelector = discovered.options?.model_selector ?? null;
  const isModelLoading = !discovered.isInitialized || (discovered.options?.loading_models ?? false);

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

  const selectedModelOptionValue = useMemo(() => {
    const trimmedModelId = form.model_id.trim();
    if (!trimmedModelId) return "";
    return `${form.provider_id.trim()}::${trimmedModelId}`;
  }, [form.model_id, form.provider_id]);

  const hasModelInDiscovery = useMemo(() => {
    if (!selectedModelOptionValue) return false;
    return [...modelsByProvider.values()].flat().some(
      (m) => `${m.provider_id ?? ""}::${m.id}` === selectedModelOptionValue,
    );
  }, [modelsByProvider, selectedModelOptionValue]);

  const selectedModel = useMemo(() => {
    const id = form.model_id.trim();
    if (!id) return null;
    const pid = form.provider_id.trim();
    return (modelSelector?.models ?? []).find(
      (m) => m.id === id && (pid ? (m.provider_id ?? "") === pid : true),
    ) ?? null;
  }, [modelSelector, form.model_id, form.provider_id]);

  const showThinkingSelector = !selectedModel || selectedModel.reasoning === true;
  const agents = modelSelector?.agents ?? [];
  const permissions = modelSelector?.permissions ?? [];

  const handleAgentTypeChange = (newType: string) => {
    patchForm({
      agent_type: newType,
      provider_id: "",
      model_id: "",
      agent_id: "",
    });
  };

  const handleModelChange = (value: string) => {
    if (!value) {
      patchForm({ provider_id: "", model_id: "" });
      return;
    }
    const sep = value.indexOf("::");
    const nextProviderId = sep >= 0 ? value.slice(0, sep) : "";
    const nextModelId = sep >= 0 ? value.slice(sep + 2) : value;
    patchForm({ provider_id: nextProviderId, model_id: nextModelId });
  };

  const companionCount = siblingAgents?.filter((a) => a.name !== form.name).length ?? 0;

  return (
    <div className="space-y-2.5">
      {/* ── Section 1: 基本信息 ── */}
      <FormSection title="基本信息">
        <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
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
        </div>
        <div>
          <label className="agentdash-form-label">描述</label>
          <textarea
            value={form.description}
            onChange={(e) => patchForm({ description: e.target.value })}
            rows={2}
            placeholder="这个 Agent 的职责和用途"
            className="agentdash-form-textarea"
          />
        </div>
      </FormSection>

      {/* ── Section 2: System Prompt ── */}
      <FormSection title="System Prompt">
        <div>
          <textarea
            value={form.system_prompt}
            onChange={(e) => patchForm({ system_prompt: e.target.value })}
            rows={3}
            placeholder="留空则仅使用全局 System Prompt"
            className="agentdash-form-textarea"
          />
        </div>
        {form.system_prompt.trim() && (
          <div className="flex items-center gap-1.5">
            <span className="text-[10px] text-muted-foreground">注入模式</span>
            {(["append", "override"] as const).map((mode) => {
              const active = (form.system_prompt_mode || "append") === mode;
              return (
                <button
                  key={mode}
                  type="button"
                  onClick={() => patchForm({ system_prompt_mode: mode })}
                  className={`rounded-[8px] border px-2.5 py-1 text-[11px] font-medium transition-all duration-160 ${
                    active
                      ? "border-primary/30 bg-primary/8 text-primary"
                      : "border-border bg-secondary/30 text-muted-foreground hover:border-primary/20 hover:text-foreground"
                  }`}
                >
                  {mode === "append" ? "追加" : "覆盖"}
                </button>
              );
            })}
            <span className="text-[10px] text-muted-foreground/60">
              {(form.system_prompt_mode || "append") === "append"
                ? "在全局 prompt 之后追加"
                : "完全替换全局 prompt"}
            </span>
          </div>
        )}
      </FormSection>

      {/* ── Section 3: 执行器 & 模型 ── */}
      <FormSection title="执行器 & 模型">
        <div>
          <label className="agentdash-form-label">Agent 类型</label>
          <select
            value={form.agent_type}
            onChange={(e) => handleAgentTypeChange(e.target.value)}
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

        <div className="grid grid-cols-[1fr_auto] gap-2">
          <div>
            <label className="agentdash-form-label">模型</label>
            <select
              value={selectedModelOptionValue}
              onChange={(e) => handleModelChange(e.target.value)}
              disabled={!form.agent_type || (isModelLoading && [...modelsByProvider.values()].flat().length === 0)}
              className="agentdash-form-select"
            >
              <option value="">
                {!form.agent_type
                  ? "先选择 Agent 类型"
                  : isModelLoading && [...modelsByProvider.values()].flat().length === 0
                    ? "加载模型中..."
                    : "不指定模型"}
              </option>
              {[...modelsByProvider.entries()].map(([pid, models]) => {
                const label = pid && providersById.get(pid)
                  ? providersById.get(pid)
                  : pid || "Other";
                return (
                  <optgroup key={pid || "default"} label={label}>
                    {models.map((m) => (
                      <option key={`${pid || "default"}::${m.id}`} value={`${pid}::${m.id}`}>
                        {m.name}
                      </option>
                    ))}
                  </optgroup>
                );
              })}
              {selectedModelOptionValue && !hasModelInDiscovery && (
                <option value={selectedModelOptionValue}>
                  {form.model_id} (当前值)
                </option>
              )}
            </select>
          </div>
          {showThinkingSelector && (
            <div className="w-[130px]">
              <label className="agentdash-form-label">推理级别</label>
              <select
                value={form.thinking_level}
                onChange={(e) => patchForm({ thinking_level: (e.target.value as ThinkingLevel) || "" })}
                className="agentdash-form-select"
              >
                <option value="">不设置</option>
                {THINKING_LEVEL_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </div>
          )}
        </div>

        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {(agents.length > 0 || form.agent_id) && (
            <div>
              <label className="agentdash-form-label">Agent</label>
              {agents.length > 0 ? (
                <select
                  value={form.agent_id}
                  onChange={(e) => patchForm({ agent_id: e.target.value })}
                  className="agentdash-form-select"
                >
                  <option value="">默认</option>
                  {agents.map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.label}{a.is_default ? " (默认)" : ""}
                    </option>
                  ))}
                  {form.agent_id && !agents.some((a) => a.id === form.agent_id) && (
                    <option value={form.agent_id}>{form.agent_id} (当前值)</option>
                  )}
                </select>
              ) : (
                <input
                  value={form.agent_id}
                  onChange={(e) => patchForm({ agent_id: e.target.value })}
                  placeholder="可选"
                  className="agentdash-form-input"
                />
              )}
            </div>
          )}
          <div>
            <label className="agentdash-form-label">权限策略</label>
            <select
              value={form.permission_policy}
              onChange={(e) => patchForm({ permission_policy: e.target.value })}
              className="agentdash-form-select"
            >
              <option value="">默认</option>
              {permissions.length > 0
                ? permissions.map((p) => (
                    <option key={p} value={p}>{p}</option>
                  ))
                : (
                  <>
                    <option value="AUTO">AUTO</option>
                    <option value="SUPERVISED">SUPERVISED</option>
                    <option value="PLAN">PLAN</option>
                  </>
                )
              }
              {form.permission_policy && permissions.length > 0 &&
                !permissions.includes(form.permission_policy as PermissionPolicy) && (
                <option value={form.permission_policy}>{form.permission_policy} (当前值)</option>
              )}
            </select>
          </div>
        </div>
      </FormSection>

      {/* ── Section 4: 运行限制 ── */}
      {/* ── Section 5: 工具 & 协作 ── */}
      <FormSection
        title="工具 & 协作"
        defaultOpen={false}
        badge={
          form.tool_clusters.length > 0
            ? `${form.tool_clusters.length}/${TOOL_CLUSTER_OPTIONS.length}`
            : companionCount > 0 && form.allowed_companions.length > 0
              ? `${form.allowed_companions.length} companion`
              : undefined
        }
      >
        <ToolCapabilitiesField clusters={form.tool_clusters} onChange={(v) => patchForm({ tool_clusters: v })} />

        {companionCount > 0 && (
          <div className="space-y-1 border-t border-border/50 pt-3">
            <label className="agentdash-form-label">
              可用 Companion Agents {form.allowed_companions.length > 0
                ? `(已选 ${form.allowed_companions.length}/${companionCount})`
                : `(全部 ${companionCount} 个)`}
            </label>
            <p className="text-[10px] text-muted-foreground/60">
              勾选此 Agent 可调用的 companion，不选则默认可调用全部项目 Agent
            </p>
            <div className="rounded-[10px] border border-border bg-secondary/20 p-2.5 space-y-0.5">
              {siblingAgents!.filter((a) => a.name !== form.name).map((agent) => {
                const checked = form.allowed_companions.includes(agent.name);
                return (
                  <label
                    key={agent.name}
                    className={`flex cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-[7px] transition-all duration-160 ${
                      checked ? "bg-violet-500/6" : "opacity-50 hover:opacity-70"
                    }`}
                  >
                    <span className="relative inline-flex h-[18px] w-[32px] shrink-0">
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => {
                          const next = checked
                            ? form.allowed_companions.filter((c) => c !== agent.name)
                            : [...form.allowed_companions, agent.name];
                          patchForm({ allowed_companions: next });
                        }}
                        className="peer sr-only"
                      />
                      <span className="absolute inset-0 rounded-full bg-border transition-colors duration-160 peer-checked:bg-violet-500" />
                      <span className="absolute left-[3px] top-[3px] h-3 w-3 rounded-full bg-background shadow-sm transition-transform duration-160 peer-checked:translate-x-[14px]" />
                    </span>
                    <span className="text-xs font-medium text-foreground">{agent.name}</span>
                    {agent.display_name && agent.display_name !== agent.name && (
                      <span className="text-[10px] text-muted-foreground">{agent.display_name}</span>
                    )}
                  </label>
                );
              })}
            </div>
          </div>
        )}
      </FormSection>

      {/* ── Section 6: MCP Servers ── */}
      <FormSection
        title="MCP Servers"
        defaultOpen={false}
        badge={form.mcp_servers.length > 0 ? `${form.mcp_servers.length} 个` : undefined}
      >
        <McpServersEditor
          servers={form.mcp_servers}
          onChange={(mcp_servers) => patchForm({ mcp_servers })}
        />
      </FormSection>
    </div>
  );
}

export function useAgentTypeOptions() {
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

          <PresetFormFields
            form={form}
            patchForm={patchForm}
            agentTypeOptions={agentTypeOptions}
            isDiscoveryLoading={isDiscoveryLoading}
          />

          {validationError && (
            <p className="mt-2 text-xs text-destructive">{validationError}</p>
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

// ─── Agent 知识库区域 ─────────────────────────────────────

function KnowledgeSection({
  enabled,
  onToggle,
  projectId,
  agentId,
  linkId,
}: {
  enabled: boolean;
  onToggle: (next: boolean) => void;
  projectId?: string;
  agentId?: string;
  linkId?: string;
}) {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className="mt-4 rounded-[12px] border border-border/70 bg-secondary/15">
      {/* ── 开关行 ── */}
      <div className="flex items-center gap-3 px-4 py-3">
        <label className="flex cursor-pointer items-center gap-2.5">
          <span className="relative inline-flex h-[18px] w-[32px] shrink-0">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => onToggle(e.target.checked)}
              className="peer sr-only"
            />
            <span className="absolute inset-0 rounded-full bg-border transition-colors duration-160 peer-checked:bg-primary" />
            <span className="absolute left-[3px] top-[3px] h-3 w-3 rounded-full bg-background shadow-sm transition-transform duration-160 peer-checked:translate-x-[14px]" />
          </span>
          <span className="text-xs font-medium text-foreground">启用知识库</span>
        </label>
        <span className="text-[10px] text-muted-foreground">
          {enabled ? "Agent 会在 session 间积累知识" : "Agent 无状态运行（默认）"}
        </span>
        {enabled && projectId && agentId && (
          <button
            type="button"
            onClick={() => setIsOpen((v) => !v)}
            className="ml-auto text-[10px] text-muted-foreground hover:text-foreground"
          >
            {isOpen ? "收起" : "浏览知识库"}
          </button>
        )}
      </div>

      {/* ── Address Space 浏览器 ── */}
      {enabled && isOpen && projectId && agentId && linkId && (
        <div className="border-t border-border px-2 py-3">
          <AddressSpaceBrowser
            source={{
              source_type: "project_agent_knowledge",
              project_id: projectId,
              agent_id: agentId,
              link_id: linkId,
            }}
            visibleMountIds={["agent-knowledge"]}
            initialMountId="agent-knowledge"
          />
        </div>
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
  siblingAgents?: Array<{ name: string; display_name: string }>;
  /** 是否启用 Agent 知识库 */
  knowledgeEnabled?: boolean;
  /** 切换知识库开关 */
  onToggleKnowledge?: (enabled: boolean) => void;
  /** 用于加载知识库文件的 project/agent ID */
  knowledgeProjectId?: string;
  knowledgeAgentId?: string;
  knowledgeLinkId?: string;
}

export function SinglePresetDialog({
  open,
  initialPreset,
  existingNames,
  onSave,
  onClose,
  isSaving = false,
  siblingAgents,
  knowledgeEnabled,
  onToggleKnowledge,
  knowledgeProjectId,
  knowledgeAgentId,
  knowledgeLinkId,
}: SinglePresetDialogProps) {
  const { agentTypeOptions, isDiscoveryLoading } = useAgentTypeOptions();
  const [form, setForm] = useState<PresetFormState>(presetToForm(initialPreset));
  const [validationError, setValidationError] = useState<string | null>(null);
  const isEditing = Boolean(initialPreset);

  // 当 initialPreset 变化时（打开不同的编辑目标），重新填充表单
  useEffect(() => {
    setForm(presetToForm(initialPreset));
    setValidationError(null);
  }, [initialPreset]);

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
        <div className="w-full max-w-2xl rounded-[16px] border border-border bg-background shadow-2xl">
          <div className="border-b border-border px-5 py-4">
            <span className="agentdash-panel-header-tag">Agent</span>
            <h4 className="text-base font-semibold text-foreground">
              {isEditing ? `编辑 Agent 预设: ${initialPreset?.name}` : "新建 Agent 预设"}
            </h4>
            <p className="mt-1 text-xs text-muted-foreground">
              配置后将出现在 Agent Hub 卡片列表中
            </p>
          </div>

          <div className="max-h-[70vh] overflow-y-auto p-5">
            <PresetFormFields
              form={form}
              patchForm={patchForm}
              agentTypeOptions={agentTypeOptions}
              isDiscoveryLoading={isDiscoveryLoading}
              siblingAgents={siblingAgents}
            />

            {/* ── 知识库 ── */}
            {knowledgeEnabled !== undefined && onToggleKnowledge && (
              <KnowledgeSection
                enabled={knowledgeEnabled}
                onToggle={onToggleKnowledge}
                projectId={knowledgeProjectId}
                agentId={knowledgeAgentId}
                linkId={knowledgeLinkId}
              />
            )}

            {validationError && (
              <p className="mt-2 text-xs text-destructive">{validationError}</p>
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
