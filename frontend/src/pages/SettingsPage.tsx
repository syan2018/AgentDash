import { useEffect, useState, useCallback, useMemo } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useSettingsStore } from "../stores/settingsStore";
import { useDebugPrefs } from "../hooks/use-debug-prefs";
import { useLlmProviderStore } from "../stores/llmProviderStore";
import { useCoordinatorStore } from "../stores/coordinatorStore";
import { useCurrentUserStore } from "../stores/currentUserStore";
import { useProjectStore } from "../stores/projectStore";
import { useExecutorDiscovery, useExecutorDiscoveredOptions } from "../features/executor-selector";
import type { ModelInfo } from "../features/executor-selector/model/types";
import type { SettingEntry, SettingUpdate, SettingsScopeRequest } from "../api/settings";
import type { LlmProvider, UpdateLlmProviderRequest } from "../api/llmProviders";
import type { BackendConfig } from "../types";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** 从 store 中读取某个 key 的显示值 */
function readVal(settings: { key: string; value: unknown }[], key: string, fallback = ""): string {
  const entry = settings.find((s) => s.key === key);
  if (entry === undefined || entry.value === null || entry.value === undefined) return fallback;
  return String(entry.value);
}

function parseModelConfigs(value: unknown): ModelConfig[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const record = item as Record<string, unknown>;
    const id = String(record.id ?? "").trim();
    if (!id) return [];
    return [{
      id,
      name: String(record.name ?? "").trim(),
      context_window: Number(record.context_window ?? 200000) || 200000,
      reasoning: record.reasoning === true,
    }];
  });
}

function parseStringList(value: unknown): string[] {
  if (Array.isArray(value)) {
    return value
      .map((item) => String(item).trim())
      .filter((item) => item.length > 0);
  }
  if (typeof value === "string") {
    return value
      .split(/\r?\n|,/)
      .map((item) => item.trim())
      .filter((item) => item.length > 0);
  }
  return [];
}

type SettingsScopeKind = SettingsScopeRequest["scope"];

interface SettingsNavigationState {
  return_to?: string;
}

const SETTINGS_SCOPE_LABELS: Record<SettingsScopeKind, string> = {
  system: "系统",
  user: "我的设置",
  project: "当前项目",
};

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

function Toast({ message, onDone }: { message: string; onDone: () => void }) {
  useEffect(() => {
    const t = setTimeout(onDone, 2400);
    return () => clearTimeout(t);
  }, [onDone]);

  return (
    <div className="fixed bottom-6 right-6 z-50 animate-fade-in rounded-[10px] border border-border bg-background px-4 py-2.5 text-sm text-foreground shadow-lg">
      {message}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Shared UI atoms
// ---------------------------------------------------------------------------

const inputCls =
  "w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring";

const btnPrimaryCls =
  "rounded-[8px] border border-border bg-primary text-primary-foreground px-4 py-2 text-sm font-medium transition-colors hover:opacity-90 disabled:opacity-50";

function SectionCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-[12px] border border-border bg-secondary/35 p-5">
      <h2 className="text-base font-semibold text-foreground">{title}</h2>
      <div className="mt-4 space-y-4">{children}</div>
    </section>
  );
}

function Field({
  label,
  desc,
  children,
}: {
  label: string;
  desc?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block space-y-1.5">
      <span className="text-sm font-medium text-foreground">{label}</span>
      {desc && <p className="text-xs text-muted-foreground">{desc}</p>}
      {children}
    </label>
  );
}

// ---------------------------------------------------------------------------
// Section components
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// LLM Providers 配置 (data-driven from DB)
// ---------------------------------------------------------------------------

interface ProviderPreset {
  name: string;
  slug: string;
  protocol: "anthropic" | "gemini" | "openai_compatible";
  base_url: string;
  env_api_key: string;
  default_model: string;
}

const PROVIDER_PRESETS: ProviderPreset[] = [
  { name: "Anthropic Claude", slug: "anthropic", protocol: "anthropic", base_url: "", env_api_key: "ANTHROPIC_API_KEY", default_model: "claude-sonnet-4-6-20250514" },
  { name: "Google Gemini", slug: "gemini", protocol: "gemini", base_url: "", env_api_key: "GEMINI_API_KEY", default_model: "gemini-2.5-flash" },
  { name: "OpenAI", slug: "openai", protocol: "openai_compatible", base_url: "https://api.openai.com/v1", env_api_key: "OPENAI_API_KEY", default_model: "gpt-5.4" },
  { name: "DeepSeek", slug: "deepseek", protocol: "openai_compatible", base_url: "https://api.deepseek.com/v1", env_api_key: "DEEPSEEK_API_KEY", default_model: "deepseek-chat" },
  { name: "Groq", slug: "groq", protocol: "openai_compatible", base_url: "https://api.groq.com/openai/v1", env_api_key: "GROQ_API_KEY", default_model: "llama-3.3-70b-versatile" },
  { name: "xAI (Grok)", slug: "xai", protocol: "openai_compatible", base_url: "https://api.x.ai/v1", env_api_key: "XAI_API_KEY", default_model: "grok-3" },
];

function defaultOpenAiWireApi(baseUrl: string): "responses" | "completions" {
  const normalized = baseUrl.trim().replace(/\/+$/, "").toLowerCase();
  if (!normalized || normalized === "https://api.openai.com/v1" || normalized === "https://api.openai.com") {
    return "responses";
  }
  return "completions";
}

/** 模型配置 */
interface ModelConfig {
  id: string;
  name: string;
  context_window: number;
  reasoning: boolean;
}

function LlmProvidersSection({
  discoveryRefreshKey,
  onRefreshModels,
}: {
  discoveryRefreshKey: number;
  onRefreshModels: () => void;
}) {
  const { providers, loading, saving, fetchProviders, createProvider, updateProvider, deleteProvider } = useLlmProviderStore();
  const discovered = useExecutorDiscoveredOptions("PI_AGENT", discoveryRefreshKey);
  const discoveredModels = discovered.options?.model_selector.models ?? [];
  const isLoadingModels = discovered.options?.loading_models ?? true;

  // 创建流程: null = 未开始, ProviderPreset|null = 选中的模板(null=自定义)
  const [createStep, setCreateStep] = useState<"idle" | "pick" | "form">("idle");
  const [createPreset, setCreatePreset] = useState<ProviderPreset | null>(null);
  const [createName, setCreateName] = useState("");
  const [createSlug, setCreateSlug] = useState("");
  const [createProtocol, setCreateProtocol] = useState<"anthropic" | "gemini" | "openai_compatible">("openai_compatible");
  const [createError, setCreateError] = useState("");

  useEffect(() => {
    fetchProviders();
  }, [fetchProviders]);

  const startCreateFromPreset = (preset: ProviderPreset) => {
    setCreatePreset(preset);
    setCreateName(preset.name);
    setCreateSlug(preset.slug);
    setCreateProtocol(preset.protocol);
    setCreateError("");
    setCreateStep("form");
  };

  const startCreateCustom = (protocol: "anthropic" | "gemini" | "openai_compatible") => {
    setCreatePreset(null);
    setCreateName("");
    setCreateSlug("");
    setCreateProtocol(protocol);
    setCreateError("");
    setCreateStep("form");
  };

  const cancelCreate = () => {
    setCreateStep("idle");
    setCreatePreset(null);
    setCreateName("");
    setCreateSlug("");
    setCreateError("");
  };

  const submitCreate = async () => {
    const name = createName.trim();
    const slug = createSlug.trim().toLowerCase();
    if (!name) { setCreateError("名称不能为空"); return; }
    if (!slug) { setCreateError("唯一标识不能为空"); return; }
    if (!/^[a-z0-9][a-z0-9_-]*$/.test(slug)) { setCreateError("唯一标识仅允许小写字母、数字、- 和 _，且不能以符号开头"); return; }
    if (providers.some((p) => p.slug === slug)) { setCreateError(`标识 "${slug}" 已被占用`); return; }
    setCreateError("");

    const result = await createProvider({
      name,
      slug,
      protocol: createProtocol,
      ...(createPreset ? {
        base_url: createPreset.base_url,
        env_api_key: createPreset.env_api_key,
        default_model: createPreset.default_model,
      } : {}),
    });
    if (result) {
      cancelCreate();
      onRefreshModels();
    }
  };

  return (
    <SectionCard title="LLM Providers">
      <p className="text-xs text-muted-foreground -mt-2 mb-1">
        配置各 LLM 服务商的 API 密钥和端点，支持同一协议的多个实例
      </p>
      {loading ? (
        <p className="text-xs text-muted-foreground py-2">加载中…</p>
      ) : (
        <div className="space-y-2">
          {providers.map((provider) => (
            <LlmProviderRow
              key={provider.id}
              provider={provider}
              discoveredModels={discoveredModels.filter((m) => (m.provider_id ?? "") === provider.slug)}
              isLoadingModels={isLoadingModels}
              onRefreshModels={onRefreshModels}
              saving={saving}
              onSave={async (req) => {
                await updateProvider(provider.id, req);
                onRefreshModels();
              }}
              onDelete={async () => {
                await deleteProvider(provider.id);
                onRefreshModels();
              }}
            />
          ))}
        </div>
      )}

      {/* Add Provider */}
      <div className="mt-2">
        {createStep === "pick" && (
          <div className="space-y-1 rounded-[10px] border border-border bg-background/80 p-3">
            <p className="text-xs font-medium text-foreground mb-2">选择预设模板</p>
            {PROVIDER_PRESETS.map((preset) => (
              <button
                key={preset.slug}
                type="button"
                className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-muted/50"
                onClick={() => startCreateFromPreset(preset)}
              >
                <span className="font-medium">{preset.name}</span>
                <span className="text-xs text-muted-foreground">({preset.protocol})</span>
              </button>
            ))}
            <div className="border-t border-border mt-1 pt-2">
              <p className="text-xs text-muted-foreground mb-1.5 px-3">自定义端点</p>
              {(["openai_compatible", "anthropic", "gemini"] as const).map((proto) => (
                <button
                  key={proto}
                  type="button"
                  className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-muted/50"
                  onClick={() => startCreateCustom(proto)}
                >
                  <span className="font-medium">{proto}</span>
                </button>
              ))}
            </div>
            <div className="flex justify-end pt-1">
              <button type="button" className="text-xs text-muted-foreground hover:text-foreground" onClick={cancelCreate}>
                取消
              </button>
            </div>
          </div>
        )}

        {createStep === "form" && (
          <div className="rounded-[10px] border border-border bg-background/80 p-3 space-y-3">
            <p className="text-xs font-medium text-foreground">
              创建 Provider{createPreset ? ` — ${createPreset.name}` : ` — ${createProtocol}`}
            </p>
            <div className="space-y-2">
              <div>
                <label className="block text-xs text-muted-foreground mb-1">名称</label>
                <input
                  type="text"
                  className={inputCls}
                  value={createName}
                  placeholder="例如 My Azure Proxy"
                  onChange={(e) => setCreateName(e.target.value)}
                  autoFocus
                />
              </div>
              <div>
                <label className="block text-xs text-muted-foreground mb-1">
                  唯一标识 (slug)
                  <span className="text-muted-foreground/60 ml-1">— 小写字母、数字、-、_</span>
                </label>
                <input
                  type="text"
                  className={inputCls}
                  value={createSlug}
                  placeholder="例如 my-azure-proxy"
                  onChange={(e) => {
                    setCreateSlug(e.target.value.replace(/[^a-zA-Z0-9_-]/g, "").toLowerCase());
                    setCreateError("");
                  }}
                />
              </div>
            </div>
            {createError && (
              <p className="text-xs text-red-500">{createError}</p>
            )}
            <div className="flex justify-end gap-2 pt-1">
              <button type="button" className="text-xs text-muted-foreground hover:text-foreground" onClick={cancelCreate}>
                取消
              </button>
              <button type="button" disabled={saving} className={btnPrimaryCls} onClick={submitCreate}>
                {saving ? "创建中…" : "创建"}
              </button>
            </div>
          </div>
        )}

        {createStep === "idle" && (
          <button
            type="button"
            className="flex w-full items-center justify-center gap-1.5 rounded-[10px] border border-dashed border-border px-4 py-2.5 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/30"
            onClick={() => setCreateStep("pick")}
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 5v14"/><path d="M5 12h14"/></svg>
            添加 Provider
          </button>
        )}
      </div>
    </SectionCard>
  );
}

function LlmProviderRow({
  provider,
  discoveredModels,
  isLoadingModels,
  onRefreshModels,
  saving,
  onSave,
  onDelete,
}: {
  provider: LlmProvider;
  discoveredModels: ModelInfo[];
  isLoadingModels: boolean;
  onRefreshModels: () => void;
  saving: boolean;
  onSave: (req: UpdateLlmProviderRequest) => void;
  onDelete: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const configured = provider.api_key_configured;

  return (
    <div className="rounded-[10px] border border-border bg-background/80">
      <button
        type="button"
        className="flex w-full items-center gap-3 px-4 py-3 text-left"
        onClick={() => setExpanded((p) => !p)}
      >
        <span
          className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${configured ? "bg-emerald-500" : "bg-muted-foreground/30"}`}
        />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-foreground">{provider.name}</p>
          <p className="text-xs text-muted-foreground">{provider.slug} · {provider.protocol}{provider.base_url ? ` · ${provider.base_url}` : ""}</p>
        </div>
        {!provider.enabled && (
          <span className="rounded-[6px] border border-yellow-500/30 bg-yellow-500/10 px-2 py-0.5 text-[11px] text-yellow-700 dark:text-yellow-400">
            已禁用
          </span>
        )}
        {configured && provider.enabled && (
          <span className="rounded-[6px] border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-700 dark:text-emerald-400">
            已配置
          </span>
        )}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={`shrink-0 text-muted-foreground transition-transform ${expanded ? "rotate-180" : ""}`}
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>

      {expanded && (
        <LlmProviderForm
          key={`${provider.id}-${provider.updated_at}`}
          provider={provider}
          discoveredModels={discoveredModels}
          isLoadingModels={isLoadingModels}
          onRefreshModels={onRefreshModels}
          saving={saving}
          onSave={onSave}
          onDelete={onDelete}
        />
      )}
    </div>
  );
}

function LlmProviderForm({
  provider,
  discoveredModels,
  isLoadingModels,
  onRefreshModels,
  saving,
  onSave,
  onDelete,
}: {
  provider: LlmProvider;
  discoveredModels: ModelInfo[];
  isLoadingModels: boolean;
  onRefreshModels: () => void;
  saving: boolean;
  onSave: (req: UpdateLlmProviderRequest) => void;
  onDelete: () => void;
}) {
  const [name, setName] = useState(provider.name);
  const [apiKey, setApiKey] = useState(provider.api_key);
  const [apiKeyTouched, setApiKeyTouched] = useState(false);
  const [baseUrl, setBaseUrl] = useState(provider.base_url);
  const [defaultModel, setDefaultModel] = useState(provider.default_model);
  const [wireApi, setWireApi] = useState(provider.wire_api || (provider.protocol === "openai_compatible" ? defaultOpenAiWireApi(provider.base_url) : ""));
  const [enabled, setEnabled] = useState(provider.enabled);
  const [models, setModels] = useState<ModelConfig[]>(parseModelConfigs(provider.models));
  const [modelsTouched, setModelsTouched] = useState(false);
  const [blockedModels, setBlockedModels] = useState<string[]>(parseStringList(provider.blocked_models));
  const [blockedModelsTouched, setBlockedModelsTouched] = useState(false);

  const showBaseUrl = provider.protocol === "openai_compatible" || provider.protocol === "anthropic";
  const showWireApi = provider.protocol === "openai_compatible";
  const showDefaultModel = true; // all protocols support default model

  const defaultModelOptions = useMemo(() => {
    const fromDiscovered = discoveredModels
      .filter((c) => c.id.trim().length > 0 && c.blocked !== true)
      .map((c) => ({ id: c.id, name: c.name.trim() || c.id }));
    const fromCustom = models
      .filter((c) => c.id.trim().length > 0)
      .map((c) => ({ id: c.id, name: c.name.trim() || c.id }));

    const options = [...fromDiscovered, ...fromCustom];

    if (defaultModel.trim().length > 0 && !options.some((c) => c.id === defaultModel)) {
      options.unshift({ id: defaultModel, name: `${defaultModel}（当前值）` });
    }

    return options.filter((c, i, list) => list.findIndex((x) => x.id === c.id) === i);
  }, [discoveredModels, defaultModel, models]);

  const toggleBlockedModel = (modelId: string) => {
    setBlockedModels((current) => {
      return current.includes(modelId)
        ? current.filter((item) => item !== modelId)
        : [...current, modelId];
    });
    setBlockedModelsTouched(true);
  };

  const handleSave = () => {
    const req: UpdateLlmProviderRequest = {};
    if (name !== provider.name) req.name = name;
    if (enabled !== provider.enabled) req.enabled = enabled;
    if (apiKeyTouched) req.api_key = apiKey;
    if (baseUrl !== provider.base_url) req.base_url = baseUrl;
    if (defaultModel !== provider.default_model) req.default_model = defaultModel;
    if (showWireApi && wireApi !== provider.wire_api) req.wire_api = wireApi;
    if (modelsTouched) req.models = models;
    if (blockedModelsTouched) req.blocked_models = blockedModels;

    if (Object.keys(req).length > 0) {
      onSave(req);
      setApiKeyTouched(false);
      setModelsTouched(false);
      setBlockedModelsTouched(false);
    }
  };

  const handleAddModel = (initial?: ModelConfig) => {
    const newModel: ModelConfig = initial ?? { id: "", name: "", context_window: 200000, reasoning: true };
    setModels([...models, newModel]);
    setModelsTouched(true);
  };

  const handleRemoveModel = (index: number) => {
    setModels(models.filter((_, i) => i !== index));
    setModelsTouched(true);
  };

  const handleUpdateModel = (index: number, field: keyof ModelConfig, value: string | number | boolean) => {
    setModels(models.map((m, i) => i !== index ? m : { ...m, [field]: value }));
    setModelsTouched(true);
  };

  return (
    <div className="space-y-3 border-t border-border px-4 pb-4 pt-3">
      {/* Name */}
      <Field label="名称" desc="Provider 显示名称">
        <input type="text" className={inputCls} value={name} onChange={(e) => setName(e.target.value)} />
      </Field>

      {/* Enabled toggle */}
      <Field label="启用" desc="禁用后不会出现在模型选择中">
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
            className="accent-primary"
          />
          <span className="text-sm">{enabled ? "已启用" : "已禁用"}</span>
        </label>
      </Field>

      {/* API Key */}
      <Field label="API Key" desc="服务密钥，保存后以掩码形式显示">
        <input
          type="password"
          className={inputCls}
          value={apiKey}
          placeholder="输入 API Key"
          onChange={(e) => { setApiKey(e.target.value); setApiKeyTouched(true); }}
        />
      </Field>

      {/* Base URL */}
      {showBaseUrl && (
        <Field label="Base URL" desc="API 端点地址（留空使用默认值）">
          <input
            type="text"
            className={inputCls}
            value={baseUrl}
            placeholder="https://..."
            onChange={(e) => setBaseUrl(e.target.value)}
          />
        </Field>
      )}

      {/* Default Model */}
      {showDefaultModel && (
        <Field
          label="默认模型"
          desc={defaultModelOptions.length > 0 ? "从当前候选模型中选择默认值" : "先保存配置后会自动出现下拉"}
        >
          {defaultModelOptions.length > 0 ? (
            <select
              className={`${inputCls} h-10 appearance-none`}
              value={defaultModel}
              onChange={(e) => setDefaultModel(e.target.value)}
            >
              <option value="">选择默认模型…</option>
              {defaultModelOptions.map((c) => (
                <option key={c.id} value={c.id}>{c.name}</option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              className={inputCls}
              value={defaultModel}
              placeholder="例如 gpt-5.4"
              onChange={(e) => setDefaultModel(e.target.value)}
            />
          )}
        </Field>
      )}

      {/* Wire API (OpenAI-compatible only) */}
      {showWireApi && (
        <Field
          label="Wire API"
          desc="官方 OpenAI 默认用 responses；自定义兼容端点默认更适合 completions"
        >
          <div className="flex gap-4">
            {(["responses", "completions"] as const).map((opt) => (
              <label key={opt} className="flex items-center gap-1.5 text-sm text-foreground">
                <input
                  type="radio"
                  name={`wire_api_${provider.id}`}
                  checked={wireApi === opt}
                  onChange={() => setWireApi(opt)}
                  className="accent-primary"
                />
                {opt}
              </label>
            ))}
          </div>
        </Field>
      )}

      {/* Model Management */}
      <ModelManagementSection
        discoveredModels={discoveredModels}
        customModels={models}
        blockedModels={blockedModels}
        isLoadingModels={isLoadingModels}
        onRefreshModels={onRefreshModels}
        onToggleBlocked={toggleBlockedModel}
        onAddModel={handleAddModel}
        onRemoveModel={handleRemoveModel}
        onUpdateModel={handleUpdateModel}
      />

      <div className="flex justify-between pt-1">
        <button
          type="button"
          className="text-xs text-red-500 hover:text-red-600"
          onClick={() => { if (window.confirm(`删除 Provider「${provider.name}」？`)) onDelete(); }}
        >
          删除此 Provider
        </button>
        <button type="button" disabled={saving} className={btnPrimaryCls} onClick={handleSave}>
          {saving ? "保存中…" : "保存"}
        </button>
      </div>
    </div>
  );
}// ---------------------------------------------------------------------------
// 统一模型管理
// ---------------------------------------------------------------------------

/** 构建 discovered model 的 tooltip 文本 */
function buildModelTooltip(model: ModelInfo): string {
  const lines = [model.id];
  if (model.name && model.name !== model.id) lines.push(`名称: ${model.name}`);
  lines.push(`上下文窗口: ${(model.context_window / 1000).toFixed(0)}k tokens`);
  if (model.reasoning) lines.push("支持推理 (extended thinking)");
  return lines.join("\n");
}

function buildCustomModelTooltip(model: ModelConfig): string {
  const lines = [model.id, "自定义模型"];
  if (model.name && model.name !== model.id) lines.push(`名称: ${model.name}`);
  lines.push(`上下文窗口: ${(model.context_window / 1000).toFixed(0)}k tokens`);
  if (model.reasoning) lines.push("支持推理");
  return lines.join("\n");
}

function ModelManagementSection({
  discoveredModels,
  customModels,
  blockedModels,
  isLoadingModels,
  onRefreshModels,
  onToggleBlocked,
  onAddModel,
  onRemoveModel,
  onUpdateModel,
}: {
  discoveredModels: ModelInfo[];
  customModels: ModelConfig[];
  blockedModels: string[];
  isLoadingModels: boolean;
  onRefreshModels: () => void;
  onToggleBlocked: (modelId: string) => void;
  onAddModel: (initial?: ModelConfig) => void;
  onRemoveModel: (index: number) => void;
  onUpdateModel: (index: number, field: keyof ModelConfig, value: string | number | boolean) => void;
}) {
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);

  const hasAny = discoveredModels.length > 0 || customModels.length > 0;
  const totalCount = discoveredModels.length + customModels.length;
  const enabledCount = discoveredModels.filter((m) => !blockedModels.includes(m.id)).length + customModels.length;

  return (
    <div className="space-y-1.5">
      {/* 标题行：标签 + 统计 + 刷新按钮 */}
      <div className="flex items-center justify-between gap-2">
        <div className="space-y-0.5">
          <span className="text-sm font-medium text-foreground">模型管理</span>
          <p className="text-xs text-muted-foreground">
            {hasAny
              ? `共 ${totalCount} 个模型（${enabledCount} 个启用），点击切换启用/屏蔽`
              : "暂无模型，请先保存 Provider 配置后刷新"}
          </p>
        </div>
        <button
          type="button"
          onClick={onRefreshModels}
          disabled={isLoadingModels}
          className="inline-flex shrink-0 items-center gap-1.5 rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
          title="重新发现可用模型"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            className={isLoadingModels ? "animate-spin" : ""}
          >
            <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
            <path d="M21 3v5h-5" />
          </svg>
          {isLoadingModels ? "发现中…" : "刷新"}
        </button>
      </div>

      {/* 统一的模型列表：发现 + 自定义混排 */}
      <div className="flex flex-wrap gap-1.5">
        {/* 自动发现的模型 */}
        {discoveredModels.map((model) => {
          const enabled = !blockedModels.includes(model.id);
          return (
            <button
              key={`d-${model.id}`}
              type="button"
              onClick={() => onToggleBlocked(model.id)}
              title={buildModelTooltip(model)}
              className={`group inline-flex items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-xs transition-all ${
                enabled
                  ? "border-emerald-500/30 bg-emerald-500/8 text-emerald-700 hover:bg-emerald-500/15 dark:text-emerald-300"
                  : "border-border bg-muted/40 text-muted-foreground hover:bg-muted/60"
              }`}
            >
              <span className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full transition-colors ${
                enabled ? "bg-emerald-500" : "bg-muted-foreground/30"
              }`} />
              <span className={enabled ? "" : "line-through opacity-60"}>
                {model.name || model.id}
              </span>
            </button>
          );
        })}

        {/* 自定义模型（追加在发现模型之后） */}
        {customModels.map((m, index) => (
          editingIndex === index ? (
            <div key={`c-${index}`} className="w-full">
              <CustomModelEditRow
                model={m}
                onUpdate={(field, value) => onUpdateModel(index, field, value)}
                onDone={() => setEditingIndex(null)}
                onRemove={() => { onRemoveModel(index); setEditingIndex(null); }}
              />
            </div>
          ) : (
            <span
              key={`c-${index}`}
              className="group inline-flex items-center gap-1.5 rounded-[8px] border border-blue-500/30 bg-blue-500/8 px-2.5 py-1.5 text-xs text-blue-700 dark:text-blue-300"
              title={buildCustomModelTooltip(m)}
            >
              <span className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-blue-500" />
              {m.name || m.id || "（未命名）"}
              <button
                type="button"
                onClick={() => setEditingIndex(index)}
                className="ml-0.5 rounded p-0.5 text-blue-600/60 hover:text-blue-700 hover:bg-blue-500/10 dark:text-blue-400/60 dark:hover:text-blue-300 transition-colors"
                title="编辑此模型"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                </svg>
              </button>
              <button
                type="button"
                onClick={() => onRemoveModel(index)}
                className="rounded p-0.5 text-blue-600/60 hover:text-destructive hover:bg-destructive/10 dark:text-blue-400/60 dark:hover:text-destructive transition-colors"
                title="删除此模型"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M18 6 6 18" /><path d="m6 6 12 12" />
                </svg>
              </button>
            </span>
          )
        ))}

        {/* 添加自定义模型入口（在列表末尾） */}
        {showAddForm ? null : (
          <button
            type="button"
            onClick={() => setShowAddForm(true)}
            className="inline-flex items-center gap-1 rounded-[8px] border border-dashed border-border px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:border-foreground/20 hover:bg-secondary/50 hover:text-foreground"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 5v14" /><path d="M5 12h14" />
            </svg>
            自定义
          </button>
        )}
      </div>

      {/* 添加自定义模型表单 */}
      {showAddForm && (
        <NewCustomModelForm
          onAdd={(newModel) => {
            onAddModel(newModel);
            setShowAddForm(false);
          }}
          onCancel={() => setShowAddForm(false)}
        />
      )}
    </div>
  );
}

/** 内联编辑某个自定义模型的行 */
function CustomModelEditRow({
  model,
  onUpdate,
  onDone,
  onRemove,
}: {
  model: ModelConfig;
  onUpdate: (field: keyof ModelConfig, value: string | number | boolean) => void;
  onDone: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 rounded-[8px] border border-blue-500/20 bg-blue-500/5 px-3 py-2">
      <input
        type="text"
        className={`${inputCls} !w-36`}
        value={model.id}
        placeholder="模型 ID"
        onChange={(e) => onUpdate("id", e.target.value)}
        autoFocus
      />
      <input
        type="text"
        className={`${inputCls} !w-28`}
        value={model.name}
        placeholder="显示名称"
        onChange={(e) => onUpdate("name", e.target.value)}
      />
      <div className="flex items-center gap-1">
        <input
          type="number"
          className={`${inputCls} !w-20`}
          value={Math.round(model.context_window / 1000)}
          placeholder="200"
          onChange={(e) => onUpdate("context_window", (parseInt(e.target.value) || 0) * 1000)}
        />
        <span className="text-xs text-muted-foreground">k</span>
      </div>
      <label className="flex items-center gap-1 text-xs text-foreground whitespace-nowrap">
        <input
          type="checkbox"
          checked={model.reasoning}
          onChange={(e) => onUpdate("reasoning", e.target.checked)}
          className="accent-primary"
        />
        推理
      </label>
      <div className="flex items-center gap-1.5 ml-auto">
        <button
          type="button"
          onClick={onDone}
          className="rounded-[6px] bg-primary px-2.5 py-1 text-[11px] text-primary-foreground transition-colors hover:opacity-90"
        >
          完成
        </button>
        <button
          type="button"
          onClick={onRemove}
          className="rounded-[6px] border border-destructive/30 px-2.5 py-1 text-[11px] text-destructive transition-colors hover:bg-destructive/10"
        >
          删除
        </button>
      </div>
    </div>
  );
}

/** 新建自定义模型的内联表单 */
function NewCustomModelForm({
  onAdd,
  onCancel,
}: {
  onAdd: (model: ModelConfig) => void;
  onCancel: () => void;
}) {
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [contextWindowK, setContextWindowK] = useState(200);
  const [reasoning, setReasoning] = useState(true);

  const handleSubmit = () => {
    const trimmedId = id.trim();
    if (!trimmedId) return;
    onAdd({
      id: trimmedId,
      name: name.trim(),
      context_window: contextWindowK * 1000,
      reasoning,
    });
  };

  return (
    <div className="rounded-[8px] border border-primary/20 bg-primary/5 p-3 space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <input
          type="text"
          className={`${inputCls} !w-40`}
          value={id}
          placeholder="模型 ID（必填）"
          onChange={(e) => setId(e.target.value)}
          autoFocus
        />
        <input
          type="text"
          className={`${inputCls} !w-28`}
          value={name}
          placeholder="显示名称"
          onChange={(e) => setName(e.target.value)}
        />
        <div className="flex items-center gap-1">
          <input
            type="number"
            className={`${inputCls} !w-20`}
            value={contextWindowK}
            placeholder="200"
            onChange={(e) => setContextWindowK(parseInt(e.target.value) || 200)}
          />
          <span className="text-xs text-muted-foreground">k</span>
        </div>
        <label className="flex items-center gap-1 text-xs text-foreground whitespace-nowrap">
          <input
            type="checkbox"
            checked={reasoning}
            onChange={(e) => setReasoning(e.target.checked)}
            className="accent-primary"
          />
          推理
        </label>
      </div>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={handleSubmit}
          disabled={!id.trim()}
          className="rounded-[6px] bg-primary px-3 py-1 text-xs text-primary-foreground transition-colors hover:opacity-90 disabled:opacity-50"
        >
          添加
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-[6px] border border-border px-3 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary"
        >
          取消
        </button>
      </div>
    </div>
  );
}

function AgentSection({
  settings,
  saving,
  onSave,
}: {
  settings: { key: string; value: unknown; masked: boolean }[];
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const seed = {
    systemPrompt: readVal(settings, "agent.pi.system_prompt"),
  };

  return (
    <AgentSectionForm
      key={JSON.stringify(seed)}
      initialSystemPrompt={seed.systemPrompt}
      saving={saving}
      onSave={onSave}
    />
  );
}

function AgentSectionForm({
  initialSystemPrompt,
  saving,
  onSave,
}: {
  initialSystemPrompt: string;
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const [systemPrompt, setSystemPrompt] = useState(initialSystemPrompt);

  const handleSave = () => {
    onSave([
      { key: "agent.pi.system_prompt", value: systemPrompt },
    ]);
  };

  return (
    <SectionCard title="Pi Agent">
      <Field label="System Prompt" desc="Agent 的系统提示词">
        <textarea
          className={`${inputCls} min-h-[100px] resize-y`}
          value={systemPrompt}
          onChange={(e) => setSystemPrompt(e.target.value)}
          rows={4}
        />
      </Field>

      <div className="flex justify-end pt-1">
        <button type="button" disabled={saving} className={btnPrimaryCls} onClick={handleSave}>
          {saving ? "保存中…" : "保存"}
        </button>
      </div>
    </SectionCard>
  );
}

function ExecutorSection({
  settings,
  saving,
  onSave,
}: {
  settings: { key: string; value: unknown; masked: boolean }[];
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const { executors, isLoading } = useExecutorDiscovery();
  const currentExecutor = readVal(settings, "executor.default.executor") || "PI_AGENT";

  return (
    <ExecutorSectionForm
      key={currentExecutor}
      executors={executors}
      isLoading={isLoading}
      currentExecutor={currentExecutor}
      saving={saving}
      onSave={onSave}
    />
  );
}

function ExecutorSectionForm({
  executors,
  isLoading,
  currentExecutor,
  saving,
  onSave,
}: {
  executors: Array<{ id: string; name: string; available: boolean }>;
  isLoading: boolean;
  currentExecutor: string;
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const [executor, setExecutor] = useState(currentExecutor);

  // 只显示可用的执行器
  const availableExecutors = executors.filter((e) => e.available);

  const handleSave = () => {
    onSave([{ key: "executor.default.executor", value: executor }]);
  };

  return (
    <SectionCard title="默认 Executor">
      <label className="block space-y-1.5">
        <span className="text-sm font-medium text-foreground">执行器</span>
        <p className="text-xs text-muted-foreground">选择默认使用的执行器（新会话或没有显式绑定时使用）</p>
        <div className="relative">
          <select
            value={executor}
            onChange={(e) => setExecutor(e.target.value)}
            disabled={isLoading}
            className="h-10 w-full appearance-none rounded-[10px] border border-border bg-background pl-3.5 pr-9 text-sm text-foreground outline-none transition-colors ring-ring focus:border-primary/30 focus:ring-1 focus:ring-ring/40 disabled:opacity-50"
          >
            <option value="">
              {isLoading ? "加载中…" : "选择执行器…"}
            </option>
            {availableExecutors.map((info) => (
              <option key={info.id} value={info.id}>
                {info.name}
              </option>
            ))}
          </select>
          <svg
            className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
          >
            <path d="M4 6L8 10L12 6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </div>
      </label>

      <div className="flex justify-end pt-1">
        <button type="button" disabled={saving} className={btnPrimaryCls} onClick={handleSave}>
          {saving ? "保存中…" : "保存"}
        </button>
      </div>
    </SectionCard>
  );
}

function BackendSection({ backends, onRemove }: { backends: BackendConfig[]; onRemove: (id: string) => void }) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const toggle = (id: string) => setExpandedId((prev) => (prev === id ? null : id));

  const onlineCount = backends.filter((b) => b.online).length;

  return (
    <SectionCard title="后端管理">
      <p className="text-xs text-muted-foreground">
        共 {backends.length} 个后端，{onlineCount} 个在线
      </p>

      {backends.length === 0 && (
        <p className="rounded-[10px] border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
          暂无已注册后端
        </p>
      )}

      <div className="space-y-2">
        {backends.map((backend) => {
          const isExpanded = expandedId === backend.id;
          const executors = backend.capabilities?.executors ?? [];
          const availableExecs = executors.filter((e) => e.available);
          const roots = backend.accessible_roots ?? [];

          return (
            <div key={backend.id} className="rounded-[10px] border border-border bg-background/80">
              <button
                type="button"
                className="flex w-full items-center gap-3 px-4 py-3 text-left"
                onClick={() => toggle(backend.id)}
              >
                <span
                  className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${backend.online ? "bg-emerald-500" : "bg-muted-foreground/30"}`}
                />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium text-foreground">{backend.name}</p>
                  <p className="text-xs text-muted-foreground">
                    {backend.online
                      ? `${availableExecs.length} 个执行器可用`
                      : backend.backend_type === "local"
                        ? "本机 · 离线"
                        : "远程 · 离线"}
                  </p>
                </div>
                <span className="rounded-[6px] border border-border bg-muted/50 px-2 py-0.5 text-[11px] text-muted-foreground">
                  {backend.backend_type === "local" ? "本机" : "远程"}
                </span>
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  className={`shrink-0 text-muted-foreground transition-transform ${isExpanded ? "rotate-180" : ""}`}
                >
                  <path d="m6 9 6 6 6-6" />
                </svg>
              </button>

              {isExpanded && (
                <div className="space-y-3 border-t border-border px-4 pb-4 pt-3">
                  <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
                    <span className="text-muted-foreground">ID</span>
                    <span className="truncate font-mono text-foreground" title={backend.id}>
                      {backend.id}
                    </span>
                    <span className="text-muted-foreground">状态</span>
                    <span className={backend.online ? "text-emerald-600 dark:text-emerald-400" : "text-muted-foreground"}>
                      {backend.online ? "在线" : "离线"}
                    </span>
                    <span className="text-muted-foreground">类型</span>
                    <span className="text-foreground">{backend.backend_type === "local" ? "本机" : "远程"}</span>
                  </div>

                  {executors.length > 0 && (
                    <div>
                      <p className="mb-1.5 text-xs font-medium text-muted-foreground">执行器 ({availableExecs.length}/{executors.length} 可用)</p>
                      <div className="flex flex-wrap gap-1.5">
                        {executors.map((ex) => (
                          <span
                            key={ex.id}
                            className={`inline-block rounded-[6px] border px-2 py-0.5 text-[11px] ${
                              ex.available
                                ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                                : "border-border bg-muted/50 text-muted-foreground line-through"
                            }`}
                          >
                            {ex.name}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}

                  {roots.length > 0 && (
                    <div>
                      <p className="mb-1 text-xs font-medium text-muted-foreground">可访问路径</p>
                      {roots.map((root) => (
                        <p key={root} className="truncate text-xs text-foreground" title={root}>
                          {root.replace(/^\\\\\?\\/, "")}
                        </p>
                      ))}
                    </div>
                  )}

                  {!backend.online && (
                    <div className="flex justify-end pt-1">
                      <button
                        type="button"
                        className="rounded-[8px] border border-destructive/30 px-3 py-1.5 text-xs text-destructive transition-colors hover:bg-destructive/10"
                        onClick={() => onRemove(backend.id)}
                      >
                        移除
                      </button>
                    </div>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </SectionCard>
  );
}

function ScopeTabs({
  activeScope,
  onChange,
}: {
  activeScope: SettingsScopeKind;
  onChange: (scope: SettingsScopeKind) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {(Object.keys(SETTINGS_SCOPE_LABELS) as SettingsScopeKind[]).map((scope) => (
        <button
          key={scope}
          type="button"
          onClick={() => onChange(scope)}
          className={`rounded-full border px-3 py-1.5 text-sm transition-colors ${
            activeScope === scope
              ? "border-primary/30 bg-primary/10 text-foreground"
              : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground"
          }`}
        >
          {SETTINGS_SCOPE_LABELS[scope]}
        </button>
      ))}
    </div>
  );
}

function RawScopedSettingsSection({
  title,
  description,
  settings,
  saving,
  onSave,
  onDelete,
}: {
  title: string;
  description: string;
  settings: SettingEntry[];
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
  onDelete: (key: string) => void;
}) {
  const [editingKey, setEditingKey] = useState("");
  const [editingValue, setEditingValue] = useState("{}");
  const [localError, setLocalError] = useState<string | null>(null);

  const handleSubmit = () => {
    const trimmedKey = editingKey.trim();
    if (!trimmedKey) {
      setLocalError("key 不能为空");
      return;
    }

    try {
      const parsed = JSON.parse(editingValue);
      setLocalError(null);
      onSave([{ key: trimmedKey, value: parsed }]);
    } catch (error) {
      setLocalError(`JSON 解析失败：${(error as Error).message}`);
    }
  };

  const loadEntry = (entry: SettingEntry) => {
    setEditingKey(entry.key);
    setEditingValue(JSON.stringify(entry.value, null, 2));
    setLocalError(null);
  };

  return (
    <SectionCard title={title}>
      <p className="text-xs text-muted-foreground -mt-2 mb-1">{description}</p>

      <div className="space-y-3 rounded-[10px] border border-border bg-background/70 p-4">
        <Field label="Key">
          <input
            className={inputCls}
            value={editingKey}
            placeholder="例如 ui.dashboard.layout"
            onChange={(e) => setEditingKey(e.target.value)}
          />
        </Field>
        <Field label="Value (JSON)" desc="这里要求填写合法 JSON，例如字符串请写成 &quot;hello&quot;">
          <textarea
            className={`${inputCls} min-h-[140px] resize-y font-mono text-xs`}
            value={editingValue}
            onChange={(e) => setEditingValue(e.target.value)}
          />
        </Field>
        {localError && (
          <div className="rounded-[8px] border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {localError}
          </div>
        )}
        <div className="flex justify-end">
          <button type="button" disabled={saving} className={btnPrimaryCls} onClick={handleSubmit}>
            {saving ? "保存中…" : "保存此项"}
          </button>
        </div>
      </div>

      <div className="space-y-2">
        {settings.length === 0 ? (
          <p className="rounded-[10px] border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
            当前 scope 还没有设置项
          </p>
        ) : (
          settings.map((entry) => (
            <div key={entry.key} className="rounded-[10px] border border-border bg-background/80 p-4">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-sm text-foreground">{entry.key}</p>
                  <p className="mt-1 text-[11px] text-muted-foreground">updated_at: {entry.updated_at}</p>
                </div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => loadEntry(entry)}
                    className="rounded-[8px] border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                  >
                    编辑
                  </button>
                  <button
                    type="button"
                    onClick={() => onDelete(entry.key)}
                    className="rounded-[8px] border border-destructive/30 px-3 py-1.5 text-xs text-destructive transition-colors hover:bg-destructive/10"
                  >
                    删除
                  </button>
                </div>
              </div>
              <pre className="mt-3 overflow-x-auto rounded-[8px] border border-border bg-secondary/25 px-3 py-2 text-xs leading-5 text-foreground/85">
                {JSON.stringify(entry.value, null, 2)}
              </pre>
            </div>
          ))
        )}
      </div>
    </SectionCard>
  );
}

// ---------------------------------------------------------------------------
// Debug Preferences (localStorage, not server-side)
// ---------------------------------------------------------------------------

function DebugPrefsSection() {
  const { prefs, setHookVerbose } = useDebugPrefs();
  return (
    <SectionCard title="开发者">
      <div className="space-y-1 text-xs text-muted-foreground">
        <p>本地调试偏好（仅存储在当前浏览器，不影响其他用户）。</p>
      </div>
      <label className="flex items-center gap-3 cursor-pointer">
        <input
          type="checkbox"
          checked={prefs.hookVerbose}
          onChange={(e) => setHookVerbose(e.target.checked)}
          className="h-4 w-4 rounded border-border accent-primary"
        />
        <div>
          <span className="text-sm text-foreground">Hook Verbose 模式</span>
          <p className="text-xs text-muted-foreground">
            开启后，会话事件流中将显示所有 Hook 决策（包括 noop、allow、dispatched 等通常被过滤的静默事件），便于调试 Hook 规则链路。
          </p>
        </div>
      </label>
    </SectionCard>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export function SettingsPage() {
  const navigate = useNavigate();
  const location = useLocation();
  const { settings, loading, saving, error, fetchSettings, updateSettings, deleteSetting } = useSettingsStore();
  const { backends, fetchBackends, removeBackend } = useCoordinatorStore();
  const { currentUser } = useCurrentUserStore();
  const { currentProjectId, projects } = useProjectStore();
  const [activeScope, setActiveScope] = useState<SettingsScopeKind>("system");
  const [toast, setToast] = useState<string | null>(null);
  const [llmDiscoveryRefreshKey, setLlmDiscoveryRefreshKey] = useState(0);
  const routeState = (location.state as SettingsNavigationState | null) ?? null;
  const returnTarget = routeState?.return_to?.trim() || "/dashboard/agent";

  const currentProject = projects.find((project) => project.id === currentProjectId) ?? null;
  const canManageSystemScope = currentUser?.auth_mode === "personal" || currentUser?.is_admin === true;
  const scopeRequest = useMemo<SettingsScopeRequest | null>(() => {
    if (activeScope === "system") {
      return canManageSystemScope ? { scope: "system" } : null;
    }
    if (activeScope === "user") {
      return { scope: "user" };
    }
    if (!currentProjectId) {
      return null;
    }
    return { scope: "project", project_id: currentProjectId };
  }, [activeScope, canManageSystemScope, currentProjectId]);

  useEffect(() => {
    void fetchBackends();
  }, [fetchBackends]);

  useEffect(() => {
    if (!scopeRequest) return;
    void fetchSettings(scopeRequest);
  }, [fetchSettings, scopeRequest]);

  const handleSave = useCallback(
    async (updates: SettingUpdate[]) => {
      if (!scopeRequest) return;
      const updated = await updateSettings(scopeRequest, updates);
      if (updated.length > 0) {
        if (updated.some((key) => key.startsWith("llm."))) {
          setLlmDiscoveryRefreshKey((current) => current + 1);
        }
        setToast("设置已保存");
      }
    },
    [scopeRequest, updateSettings],
  );

  const handleDelete = useCallback(
    async (key: string) => {
      if (!scopeRequest) return;
      await deleteSetting(scopeRequest, key);
      setToast("设置已删除");
    },
    [deleteSetting, scopeRequest],
  );

  const handleBack = useCallback(() => {
    navigate(returnTarget);
  }, [navigate, returnTarget]);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <div className="mx-auto h-7 w-7 animate-spin rounded-full border-2 border-primary border-t-transparent" />
          <p className="mt-3 text-sm text-muted-foreground">正在加载设置…</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-2xl space-y-6 px-6 py-8">
        <div className="space-y-3">
          <button
            type="button"
            onClick={handleBack}
            className="inline-flex items-center gap-2 rounded-[10px] border border-border bg-background px-3 py-2 text-sm text-foreground transition-colors hover:bg-secondary"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="m15 18-6-6 6-6" />
            </svg>
            返回
          </button>
          <div>
            <h1 className="text-xl font-semibold text-foreground">设置</h1>
            <p className="mt-1 text-sm text-muted-foreground">
              管理 system / user / project 三层 scope 设置。system 更偏宿主级配置，user 和 project 用来承接企业化后的个体偏好与项目策略。
            </p>
          </div>
        </div>

        <SectionCard title="Scope">
          <ScopeTabs activeScope={activeScope} onChange={setActiveScope} />
          <div className="space-y-1 text-xs text-muted-foreground">
            <p>当前 scope：{SETTINGS_SCOPE_LABELS[activeScope]}</p>
            {activeScope === "system" && (
              <p>system scope 仅 personal 模式或管理员可访问，适合放全局执行器、LLM Provider 和系统级 Agent 配置。</p>
            )}
            {activeScope === "user" && (
              <p>user scope 绑定当前登录用户，适合放个人偏好、个体协作策略和不会影响他人的私有配置。</p>
            )}
            {activeScope === "project" && (
              <p>
                project scope 绑定当前选中 Project。
                {currentProject ? ` 当前项目：${currentProject.name}` : " 请先在侧边栏选择一个 Project。"}
              </p>
            )}
          </div>
        </SectionCard>

        {error && (
          <div className="rounded-[10px] border border-destructive/50 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}

        {activeScope === "system" && !canManageSystemScope && (
          <div className="rounded-[10px] border border-amber-300/50 bg-amber-50 px-4 py-3 text-sm text-amber-800">
            当前企业身份不是管理员，system scope 设置已被收口。你仍然可以查看和维护 user / project scope。
          </div>
        )}

        {activeScope === "system" && canManageSystemScope && (
          <>
            <BackendSection backends={backends} onRemove={(id) => void removeBackend(id)} />
            <LlmProvidersSection
              discoveryRefreshKey={llmDiscoveryRefreshKey}
              onRefreshModels={() => setLlmDiscoveryRefreshKey((k) => k + 1)}
            />
            <AgentSection settings={settings} saving={saving} onSave={handleSave} />
            <ExecutorSection settings={settings} saving={saving} onSave={handleSave} />
          </>
        )}

        {activeScope === "user" && scopeRequest && (
          <RawScopedSettingsSection
            title="我的设置"
            description="这里是当前用户自己的设置层。它不会影响其他用户，也不应该承担 system 级或 Project 级共享配置。"
            settings={settings}
            saving={saving}
            onSave={handleSave}
            onDelete={(key) => void handleDelete(key)}
          />
        )}

        {activeScope === "project" && !scopeRequest && (
          <div className="rounded-[10px] border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
            还没有选中的 Project，暂时无法进入 project scope。
          </div>
        )}

        {activeScope === "project" && scopeRequest && currentProject && (
          <RawScopedSettingsSection
            title={`项目设置 · ${currentProject.name}`}
            description="project scope 适合放某个 Project 自己的协作策略或局部配置。写入时会受当前用户对该 Project 的编辑权限约束。"
            settings={settings}
            saving={saving}
            onSave={handleSave}
            onDelete={(key) => void handleDelete(key)}
          />
        )}

        <DebugPrefsSection />
      </div>

      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
