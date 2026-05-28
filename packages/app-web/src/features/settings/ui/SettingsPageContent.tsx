import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useSettingsStore } from "../../../stores/settingsStore";
import { useLlmProviderStore } from "../../../stores/llmProviderStore";
import { useCoordinatorStore } from "../../../stores/coordinatorStore";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import { useProjectStore } from "../../../stores/projectStore";
import { useExecutorDiscovery, useExecutorDiscoveredOptions } from "../../executor-selector";
import type { ModelInfo } from "../../executor-selector/model/types";
import type { SettingUpdate, SettingsScopeRequest } from "../../../api/settings";
import { getStoredToken } from "../../../api/client";
import { API_ORIGIN } from "../../../api/origin";
import { llmProvidersApi } from "../../../api/llmProviders";
import type { JsonValue, LlmProvider, UpdateLlmProviderRequest, ProbeModelEntry } from "../../../api/llmProviders";
import type { BackendConfig, BackendRuntimeSummary } from "../../../types";
import { LocalRuntimeView } from "@agentdash/views/local-runtime";
import { ConfirmDialog } from "@agentdash/ui";
import { getDesktopLocalRuntimeClient, getDesktopBrowseDirectory } from "../../../desktop/localRuntimeBridge";
import { DebugPrefsSection } from "./DebugPrefsSection";
import { OAuthLoginWizard } from "./OAuthLoginWizard";
import { UserByokSection } from "./UserByokSection";
import { btnPrimaryCls, Field, inputCls, SectionCard } from "./primitives";

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
      reasoning: record.reasoning !== false,
      supports_image: record.supports_image !== false,
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

function modelConfigsToJsonValue(models: ModelConfig[]): JsonValue {
  return models.map((model): JsonValue => ({
    id: model.id,
    name: model.name,
    context_window: model.context_window,
    reasoning: model.reasoning,
    supports_image: model.supports_image,
  }));
}

type SettingsScopeKind = SettingsScopeRequest["scope"];
type SettingsPanel = SettingsScopeKind | "local-runtime";

interface SettingsNavigationState {
  return_to?: string;
}

const SETTINGS_SCOPE_LABELS: Record<SettingsScopeKind, string> = {
  system: "系统",
  user: "用户设置",
  project: "当前项目",
};

const SETTINGS_PANEL_LABELS: Record<SettingsPanel, string> = {
  "local-runtime": "本机运行时",
  ...SETTINGS_SCOPE_LABELS,
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
    <div className="fixed bottom-6 right-6 z-50 animate-fade-in rounded-[8px] border border-border bg-background px-4 py-2.5 text-sm text-foreground shadow-lg">
      {message}
    </div>
  );
}

function DesktopLocalRuntimePanel() {
  const client = useMemo(() => getDesktopLocalRuntimeClient(), []);
  const browseDirectory = useMemo(() => getDesktopBrowseDirectory(), []);
  if (!client) return null;

  return (
    <LocalRuntimeView
      client={client}
      onBrowseDirectory={browseDirectory}
      defaultAccessToken={getStoredToken() ?? ""}
      defaultServerUrl={API_ORIGIN || "http://127.0.0.1:3001"}
    />
  );
}

// ---------------------------------------------------------------------------
// Section components
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// LLM Providers 配置 (data-driven from DB)
// ---------------------------------------------------------------------------

type ProviderProtocol = "anthropic" | "gemini" | "openai_compatible" | "openai_codex";

interface ProviderPreset {
  name: string;
  slug: string;
  protocol: ProviderProtocol;
  base_url: string;
  env_api_key: string;
  default_model?: string;
  models?: ModelConfig[];
}

const PROVIDER_PRESETS: ProviderPreset[] = [
  { name: "Anthropic Claude", slug: "anthropic", protocol: "anthropic", base_url: "", env_api_key: "ANTHROPIC_API_KEY" },
  { name: "Google Gemini", slug: "gemini", protocol: "gemini", base_url: "", env_api_key: "GEMINI_API_KEY" },
  { name: "OpenAI", slug: "openai", protocol: "openai_compatible", base_url: "https://api.openai.com/v1", env_api_key: "OPENAI_API_KEY" },
  {
    name: "ChatGPT Codex",
    slug: "openai-codex",
    protocol: "openai_codex",
    base_url: "",
    env_api_key: "OPENAI_CODEX_OAUTH",
    default_model: "gpt-5.5",
    models: [
      { id: "gpt-5.5", name: "GPT-5.5", context_window: 272000, reasoning: true, supports_image: true },
      { id: "gpt-5.4", name: "GPT-5.4", context_window: 272000, reasoning: true, supports_image: true },
      { id: "gpt-5.4-mini", name: "GPT-5.4 Mini", context_window: 272000, reasoning: true, supports_image: true },
      { id: "gpt-5.3-codex", name: "GPT-5.3 Codex", context_window: 272000, reasoning: true, supports_image: true },
    ],
  },
  { name: "DeepSeek", slug: "deepseek", protocol: "openai_compatible", base_url: "https://api.deepseek.com/v1", env_api_key: "DEEPSEEK_API_KEY" },
  { name: "Groq", slug: "groq", protocol: "openai_compatible", base_url: "https://api.groq.com/openai/v1", env_api_key: "GROQ_API_KEY" },
  { name: "xAI (Grok)", slug: "xai", protocol: "openai_compatible", base_url: "https://api.x.ai/v1", env_api_key: "XAI_API_KEY" },
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
  supports_image: boolean;
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
  const [createProtocol, setCreateProtocol] = useState<ProviderProtocol>("openai_compatible");
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

  const startCreateCustom = (protocol: ProviderProtocol) => {
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
        models: createPreset.models ? modelConfigsToJsonValue(createPreset.models) : undefined,
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
              onProviderChanged={async () => {
                await fetchProviders();
                onRefreshModels();
              }}
            />
          ))}
        </div>
      )}

      {/* Add Provider */}
      <div className="mt-2">
        {createStep === "pick" && (
          <div className="space-y-1 rounded-[8px] border border-border bg-background/80 p-3">
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
          <div className="rounded-[8px] border border-border bg-background/80 p-3 space-y-3">
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
              <p className="text-xs text-destructive">{createError}</p>
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
            className="flex w-full items-center justify-center gap-1.5 rounded-[8px] border border-dashed border-border px-4 py-2.5 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/30"
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
  onProviderChanged,
}: {
  provider: LlmProvider;
  discoveredModels: ModelInfo[];
  isLoadingModels: boolean;
  onRefreshModels: () => void;
  saving: boolean;
  onSave: (req: UpdateLlmProviderRequest) => void;
  onDelete: () => void;
  onProviderChanged: () => Promise<void> | void;
}) {
  const [expanded, setExpanded] = useState(false);
  const configured = provider.global_api_key_configured;

  return (
    <div className="rounded-[8px] border border-border bg-background/80">
      <button
        type="button"
        className="flex w-full items-center gap-3 px-4 py-3 text-left"
        onClick={() => setExpanded((p) => !p)}
      >
        <span
          className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${configured ? "bg-success" : "bg-muted-foreground/30"}`}
        />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-foreground">{provider.name}</p>
          <p className="text-xs text-muted-foreground">{provider.slug} · {provider.protocol}{provider.base_url ? ` · ${provider.base_url}` : ""}</p>
        </div>
        {!provider.enabled && (
          <span className="rounded-[6px] border border-warning/30 bg-warning/10 px-2 py-0.5 text-[11px] text-warning">
            已禁用
          </span>
        )}
        {configured && provider.enabled && (
          <span className="rounded-[6px] border border-success/30 bg-success/10 px-2 py-0.5 text-[11px] text-success">
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
          onProviderChanged={onProviderChanged}
        />
      )}
    </div>
  );
}

function LlmProviderForm({
  provider,
  discoveredModels,
  isLoadingModels,
  onRefreshModels: _onRefreshModels,
  saving,
  onSave,
  onDelete,
  onProviderChanged,
}: {
  provider: LlmProvider;
  discoveredModels: ModelInfo[];
  isLoadingModels: boolean;
  onRefreshModels: () => void;
  saving: boolean;
  onSave: (req: UpdateLlmProviderRequest) => void;
  onDelete: () => void;
  onProviderChanged: () => Promise<void> | void;
}) {
  const [name, setName] = useState(provider.name);
  const [apiKey, setApiKey] = useState(provider.global_api_key_preview ?? "");
  const [apiKeyTouched, setApiKeyTouched] = useState(false);
  const [baseUrl, setBaseUrl] = useState(provider.base_url);
  const [defaultModel, setDefaultModel] = useState(provider.default_model);
  const [wireApi, setWireApi] = useState(provider.wire_api || (provider.protocol === "openai_compatible" ? defaultOpenAiWireApi(provider.base_url) : ""));
  const [credentialMode, setCredentialMode] = useState(provider.credential_mode);
  const [enabled, setEnabled] = useState(provider.enabled);
  const [models, setModels] = useState<ModelConfig[]>(parseModelConfigs(provider.models));
  const [modelsTouched, setModelsTouched] = useState(false);
  const [blockedModels, setBlockedModels] = useState<string[]>(parseStringList(provider.blocked_models));
  const [blockedModelsTouched, setBlockedModelsTouched] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);

  // 实时探测状态：用当前表单 credentials 探测到的模型列表
  const [probedModels, setProbedModels] = useState<ProbeModelEntry[] | null>(null);
  const [isProbing, setIsProbing] = useState(false);
  const [probeError, setProbeError] = useState<string | null>(null);

  const showApiKey = provider.protocol !== "openai_codex";
  const showBaseUrl = provider.protocol === "openai_compatible" || provider.protocol === "anthropic";
  const showWireApi = provider.protocol === "openai_compatible";
  const showDefaultModel = true; // all protocols support default model

  // 合并来源：probe 结果优先（当存在时替代 discovery 结果），否则用全局 discovery
  const effectiveDiscoveredModels: ModelInfo[] = useMemo(() => {
    if (probedModels !== null) {
      return probedModels.map((m) => ({
        id: m.id,
        name: m.name,
        provider_id: provider.slug,
        reasoning: true,
        supports_image: true,
        context_window: 200_000,
        blocked: false,
        discovered: true,
      }));
    }
    return discoveredModels;
  }, [probedModels, discoveredModels, provider.slug]);

  // 默认模型候选：所有未被屏蔽的模型（discovered + custom 合并去重）
  const defaultModelOptions = useMemo(() => {
    const allIds = new Set<string>();
    const options: { id: string; name: string }[] = [];

    // discovered 中未屏蔽的
    for (const c of effectiveDiscoveredModels) {
      if (!c.id.trim() || blockedModels.includes(c.id)) continue;
      if (allIds.has(c.id)) continue;
      allIds.add(c.id);
      options.push({ id: c.id, name: c.name.trim() || c.id });
    }

    // custom 中未屏蔽的（包含 discovered override 和纯自定义）
    for (const c of models) {
      if (!c.id.trim() || blockedModels.includes(c.id)) continue;
      if (allIds.has(c.id)) continue;
      allIds.add(c.id);
      options.push({ id: c.id, name: c.name.trim() || c.id });
    }

    if (defaultModel.trim().length > 0 && !allIds.has(defaultModel)) {
      options.unshift({ id: defaultModel, name: `${defaultModel}（当前值）` });
    }

    return options;
  }, [effectiveDiscoveredModels, defaultModel, models, blockedModels]);

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
    if (credentialMode !== provider.credential_mode) req.credential_mode = credentialMode;
    if (apiKeyTouched) req.global_api_key = apiKey;
    if (baseUrl !== provider.base_url) req.base_url = baseUrl;
    if (defaultModel !== provider.default_model) req.default_model = defaultModel;
    if (showWireApi && wireApi !== provider.wire_api) req.wire_api = wireApi;
    if (modelsTouched) req.models = modelConfigsToJsonValue(models);
    if (blockedModelsTouched) req.blocked_models = blockedModels;

    if (Object.keys(req).length > 0) {
      onSave(req);
      setApiKeyTouched(false);
      setModelsTouched(false);
      setBlockedModelsTouched(false);
    }
  };

  // 用当前表单 credentials 实时探测模型，不保存、不折叠
  const handleProbeModels = useCallback(async () => {
    setIsProbing(true);
    setProbeError(null);
    try {
      const result = await llmProvidersApi.probeModels({
        protocol: provider.protocol,
        api_key: apiKeyTouched ? apiKey : undefined,
        base_url: baseUrl || undefined,
        discovery_url: undefined,
        env_api_key: provider.env_api_key || undefined,
        provider_id: provider.id,
      });
      setProbedModels(result);
    } catch (e) {
      setProbeError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsProbing(false);
    }
  }, [provider.id, provider.protocol, provider.env_api_key, apiKey, apiKeyTouched, baseUrl]);

  const handleCodexLoginCompleted = useCallback(async () => {
    setApiKeyTouched(false);
    await onProviderChanged();
  }, [onProviderChanged]);


  const handleAddModel = (initial?: ModelConfig) => {
    const newModel: ModelConfig = initial ?? { id: "", name: "", context_window: 200000, reasoning: true, supports_image: true };
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
    <>
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

      <Field label="凭据策略" desc="控制平台全局 Key 与用户 BYOK 的生效方式">
        <select
          className={`${inputCls} h-10 appearance-none`}
          value={credentialMode}
          onChange={(e) => {
            if (e.target.value === "global_only" || e.target.value === "global_or_user" || e.target.value === "user_required") {
              setCredentialMode(e.target.value);
            }
          }}
        >
          <option value="global_only">仅平台全局 Key</option>
          <option value="global_or_user">平台全局 Key 或用户 BYOK</option>
          <option value="user_required">必须用户 BYOK</option>
        </select>
      </Field>

      {/* API Key */}
      {showApiKey && (
        <Field label="全局 API Key" desc="平台级服务密钥，保存后以掩码形式显示">
          <input
            type="password"
            className={inputCls}
            value={apiKey}
            placeholder="输入 API Key"
            onChange={(e) => { setApiKey(e.target.value); setApiKeyTouched(true); }}
          />
        </Field>
      )}

      {provider.protocol === "openai_codex" && (
        <div className="rounded-[8px] border border-border bg-muted/20 px-3 py-3">
          <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <p className="text-sm font-medium text-foreground">ChatGPT OAuth</p>
                {provider.global_api_key_configured && (
                  <span className="rounded-[6px] border border-success/40 bg-success/10 px-2 py-0.5 text-[11px] font-medium text-success">
                    全局已验证
                  </span>
                )}
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                {provider.global_api_key_configured
                  ? `${provider.global_api_key_preview ?? "ChatGPT OAuth"} · 平台全局生效`
                : "未验证"}
              </p>
            </div>
            <OAuthLoginWizard
              start={() => llmProvidersApi.startCodexOAuth(provider.id)}
              getStatus={llmProvidersApi.getCodexOAuthStatus}
              cancel={llmProvidersApi.cancelCodexOAuth}
              onCompleted={handleCodexLoginCompleted}
              idleLabel={provider.global_api_key_configured ? "重新验证 ChatGPT" : "通过 ChatGPT 登录"}
              authLinkLabel="打开 ChatGPT 授权页"
              openedMessage="已在外部浏览器打开 ChatGPT 授权页，等待授权完成…"
              manualMessage="请打开 ChatGPT 授权页并完成登录，完成后这里会自动更新状态。"
              completedMessage="Codex 登录已完成"
              failedMessage="Codex 登录失败"
              surface="inline"
              className="shrink-0"
            />
          </div>
        </div>
      )}

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
              placeholder="保存配置后自动发现，或手动输入"
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
        discoveredModels={effectiveDiscoveredModels}
        customModels={models}
        blockedModels={blockedModels}
        isLoadingModels={isProbing || isLoadingModels}
        onRefreshModels={handleProbeModels}
        onToggleBlocked={toggleBlockedModel}
        onAddModel={handleAddModel}
        onRemoveModel={handleRemoveModel}
        onUpdateModel={handleUpdateModel}
        probeError={probeError}
      />

      <div className="flex justify-between pt-1">
        <button
          type="button"
          className="text-xs text-destructive hover:text-destructive/80"
          onClick={() => setDeleteConfirmOpen(true)}
        >
          删除此 Provider
        </button>
        <button type="button" disabled={saving} className={btnPrimaryCls} onClick={handleSave}>
          {saving ? "保存中…" : "保存"}
        </button>
      </div>
      </div>
      <ConfirmDialog
        open={deleteConfirmOpen}
        title="删除 Provider"
        description={`确定删除 Provider「${provider.name}」？`}
        confirmLabel="删除"
        tone="danger"
        disabled={saving}
        isConfirming={saving}
        onClose={() => setDeleteConfirmOpen(false)}
        onConfirm={() => {
          setDeleteConfirmOpen(false);
          onDelete();
        }}
      />
    </>
  );
}// ---------------------------------------------------------------------------
// 统一模型管理
// ---------------------------------------------------------------------------

function buildModelTooltip(model: ModelInfo): string {
  const lines = [model.id];
  if (model.name && model.name !== model.id) lines.push(`名称: ${model.name}`);
  lines.push(`上下文窗口: ${(model.context_window / 1000).toFixed(0)}k tokens`);
  if (model.reasoning) lines.push("支持推理 (extended thinking)");
  if (model.supports_image) lines.push("支持图像");
  return lines.join("\n");
}

function buildCustomModelTooltip(model: ModelConfig): string {
  const lines = [model.id, "自定义模型"];
  if (model.name && model.name !== model.id) lines.push(`名称: ${model.name}`);
  lines.push(`上下文窗口: ${(model.context_window / 1000).toFixed(0)}k tokens`);
  if (model.reasoning) lines.push("支持推理");
  if (model.supports_image) lines.push("支持图像");
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
  probeError,
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
  probeError?: string | null;
}) {
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [editingDiscoveredId, setEditingDiscoveredId] = useState<string | null>(null);

  const dragRef = useRef<{ action: "block" | "unblock"; touched: Set<string> } | null>(null);

  const handleDragStart = (modelId: string) => {
    const isBlocked = blockedModels.includes(modelId);
    const action = isBlocked ? "unblock" : "block";
    dragRef.current = { action, touched: new Set([modelId]) };
    onToggleBlocked(modelId);
  };

  const handleDragEnter = (modelId: string) => {
    const drag = dragRef.current;
    if (!drag || drag.touched.has(modelId)) return;
    drag.touched.add(modelId);
    const isBlocked = blockedModels.includes(modelId);
    if ((drag.action === "block" && !isBlocked) || (drag.action === "unblock" && isBlocked)) {
      onToggleBlocked(modelId);
    }
  };

  const handleDragEnd = () => { dragRef.current = null; };

  // 找到 discovered model 对应的 override config index（如果存在于 customModels 中）
  const findOverrideIndex = (modelId: string) => customModels.findIndex((m) => m.id === modelId);

  // 对 discovered model 设置/更新 override
  const handleDiscoveredOverride = (model: ModelInfo, field: keyof ModelConfig, value: string | number | boolean) => {
    const existingIdx = findOverrideIndex(model.id);
    if (existingIdx >= 0) {
      onUpdateModel(existingIdx, field, value);
    } else {
      // 首次 override：基于 discovered 属性创建一条配置
      const newConfig: ModelConfig = {
        id: model.id,
        name: model.name,
        context_window: model.context_window,
        reasoning: model.reasoning,
        supports_image: model.supports_image,
        [field]: value,
      };
      onAddModel(newConfig);
    }
  };

  const handleRemoveDiscoveredOverride = (modelId: string) => {
    const idx = findOverrideIndex(modelId);
    if (idx >= 0) {
      onRemoveModel(idx);
    }
    setEditingDiscoveredId(null);
  };

  // 用后端返回的 discovered 字段区分真正的 API 发现模型 vs 纯配置模型
  const trueDiscoveredModels = discoveredModels.filter((m) => m.discovered !== false);
  const trueDiscoveredIds = new Set(trueDiscoveredModels.map((m) => m.id));

  // pureCustom: customModels 中不属于"真正 discovered"的条目
  const pureCustomEntries = customModels
    .map((m, i) => ({ model: m, originalIndex: i }))
    .filter((e) => !trueDiscoveredIds.has(e.model.id));

  const hasAny = trueDiscoveredModels.length > 0 || pureCustomEntries.length > 0;
  const totalCount = trueDiscoveredModels.length + pureCustomEntries.length;
  const enabledCount =
    trueDiscoveredModels.filter((m) => !blockedModels.includes(m.id)).length +
    pureCustomEntries.filter((e) => !blockedModels.includes(e.model.id)).length;

  return (
    <div className="space-y-2">
      {/* Header */}
      <div className="flex items-center justify-between gap-2">
        <div className="space-y-0.5">
          <span className="text-sm font-medium text-foreground">模型管理</span>
          <p className="text-xs text-muted-foreground">
            {hasAny
              ? `共 ${totalCount} 个模型（${enabledCount} 个启用），拖拽批量切换 · 点击标签编辑属性`
              : "暂无模型，点击「探测」发现可用模型"}
          </p>
        </div>
        <button
          type="button"
          onClick={onRefreshModels}
          disabled={isLoadingModels}
          className="inline-flex shrink-0 items-center gap-1.5 rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
          title="用当前表单配置实时探测可用模型（无需先保存）"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={isLoadingModels ? "animate-spin" : ""}>
            <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
            <path d="M21 3v5h-5" />
          </svg>
          {isLoadingModels ? "探测中…" : "探测"}
        </button>
      </div>

      {probeError && (
        <p className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          探测失败: {probeError}
        </p>
      )}

      {/* Model chips */}
      <div
        className="flex flex-wrap gap-1.5 select-none"
        onPointerUp={handleDragEnd}
        onPointerLeave={handleDragEnd}
      >
        {/* Discovered models — 整体是一个统一的 chip */}
        {trueDiscoveredModels.map((model) => {
          const enabled = !blockedModels.includes(model.id);
          const hasOverride = findOverrideIndex(model.id) >= 0;
          const overrideConfig = hasOverride ? customModels[findOverrideIndex(model.id)] : null;
          const isEditing = editingDiscoveredId === model.id;
          const effectiveTooltip = buildModelTooltip({
            ...model,
            ...(overrideConfig ? {
              reasoning: overrideConfig.reasoning,
              supports_image: overrideConfig.supports_image,
              context_window: overrideConfig.context_window,
            } : {}),
          });

          return (
            <span
              key={`d-${model.id}`}
              className={`group relative inline-flex touch-none items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-xs transition-all ${
                isEditing
                  ? "border-primary/40 bg-primary/8 text-primary ring-1 ring-primary/20"
                  : enabled
                    ? "border-success/30 bg-success/10 text-success hover:bg-success/15"
                    : "border-border bg-muted/40 text-muted-foreground hover:bg-muted/60"
              }`}
              title={effectiveTooltip}
              onPointerDown={(e) => { e.preventDefault(); handleDragStart(model.id); }}
              onPointerEnter={() => handleDragEnter(model.id)}
            >
              <span className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full transition-colors ${
                isEditing ? "bg-primary" : enabled ? "bg-success" : "bg-muted-foreground/30"
              }`} />
              <span className={enabled ? "" : "line-through opacity-60"}>
                {(overrideConfig?.name || model.name || model.id)}
              </span>
              {hasOverride && (
                // eslint-disable-next-line no-restricted-syntax -- 1px 圆点指示
                <span className="inline-block h-1 w-1 rounded-full bg-warning" title="已自定义属性" />
              )}
              {/* Hover 浮现的编辑按钮 — 不会破坏标签轮廓 */}
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  setEditingDiscoveredId(isEditing ? null : model.id);
                }}
                className="ml-0.5 hidden rounded p-0.5 transition-colors group-hover:inline-flex hover:bg-black/10 dark:hover:bg-white/10"
                onPointerDown={(e) => e.stopPropagation()}
                title="编辑属性"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="m12 20 9-11-4-4-9 11 1 5 3-1Z" />
                </svg>
              </button>
            </span>
          );
        })}

        {/* Pure custom models — 支持屏蔽和拖拽 */}
        {pureCustomEntries.map(({ model: m, originalIndex }) => {
          const isEditing = editingIndex === originalIndex;
          const enabled = !blockedModels.includes(m.id);
          return (
            <span
              key={`c-${originalIndex}`}
              className={`group relative inline-flex touch-none items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-xs transition-all ${
                isEditing
                  ? "border-primary/40 bg-primary/8 text-primary ring-1 ring-primary/20"
                  : enabled
                    ? "border-blue-500/30 bg-blue-500/8 text-blue-700 hover:bg-blue-500/15 dark:text-blue-300"
                    : "border-border bg-muted/40 text-muted-foreground hover:bg-muted/60"
              }`}
              title={buildCustomModelTooltip(m)}
              onPointerDown={(e) => { if (m.id.trim()) { e.preventDefault(); handleDragStart(m.id); } }}
              onPointerEnter={() => { if (m.id.trim()) handleDragEnter(m.id); }}
            >
              <span className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full transition-colors ${
                isEditing ? "bg-primary" : enabled ? "bg-blue-500" : "bg-muted-foreground/30"
              }`} />
              <span className={enabled ? "" : "line-through opacity-60"}>
                {m.name || m.id || "（未命名）"}
              </span>
              <button
                type="button"
                onClick={(e) => { e.stopPropagation(); setEditingIndex(isEditing ? null : originalIndex); }}
                onPointerDown={(e) => e.stopPropagation()}
                className="ml-0.5 hidden rounded p-0.5 transition-colors group-hover:inline-flex hover:bg-black/10 dark:hover:bg-white/10"
                title="编辑"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="m12 20 9-11-4-4-9 11 1 5 3-1Z" />
                </svg>
              </button>
              <button
                type="button"
                onClick={(e) => { e.stopPropagation(); onRemoveModel(originalIndex); }}
                onPointerDown={(e) => e.stopPropagation()}
                className="hidden rounded p-0.5 transition-colors group-hover:inline-flex hover:bg-destructive/15 hover:text-destructive"
                title="删除"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M18 6 6 18" /><path d="m6 6 12 12" />
                </svg>
              </button>
            </span>
          );
        })}

        {/* Add custom */}
        {!showAddForm && (
          <button
            type="button"
            onClick={() => setShowAddForm(true)}
            className="inline-flex items-center gap-1 rounded-[8px] border border-dashed border-border px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:border-foreground/20 hover:bg-secondary/50 hover:text-foreground"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 5v14" /><path d="M5 12h14" /></svg>
            自定义
          </button>
        )}
      </div>

      {/* Inline editor — discovered model override */}
      {editingDiscoveredId && (() => {
        const model = trueDiscoveredModels.find((m) => m.id === editingDiscoveredId);
        if (!model) return null;
        const overrideIdx = findOverrideIndex(model.id);
        const overrideConfig = overrideIdx >= 0 ? customModels[overrideIdx] : null;
        return (
          <DiscoveredModelEditRow
            model={model}
            override={overrideConfig}
            onOverride={(field, value) => handleDiscoveredOverride(model, field, value)}
            onResetOverride={() => handleRemoveDiscoveredOverride(model.id)}
            onDone={() => setEditingDiscoveredId(null)}
          />
        );
      })()}

      {/* Inline editor — custom model */}
      {editingIndex !== null && (() => {
        const entry = pureCustomEntries.find((e) => e.originalIndex === editingIndex);
        if (!entry) return null;
        return (
          <CustomModelEditRow
            model={entry.model}
            isDiscovered={false}
            onUpdate={(field, value) => onUpdateModel(entry.originalIndex, field, value)}
            onDone={() => setEditingIndex(null)}
            onRemove={() => { onRemoveModel(entry.originalIndex); setEditingIndex(null); }}
          />
        );
      })()}

      {/* New custom model form */}
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
  isDiscovered,
  onUpdate,
  onDone,
  onRemove,
}: {
  model: ModelConfig;
  isDiscovered: boolean;
  onUpdate: (field: keyof ModelConfig, value: string | number | boolean) => void;
  onDone: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="rounded-[8px] border border-border bg-background/80 p-3">
      {/* ID + Name 行 */}
      <div className="flex flex-wrap items-center gap-2 mb-3">
        {/* eslint-disable-next-line no-restricted-syntax -- 状态指示圆点 */}
        <span className="inline-block h-2 w-2 rounded-full bg-info" />
        <input
          type="text"
          className={`${inputCls} !w-40 !py-1 !px-2 ${isDiscovered ? "opacity-50 cursor-not-allowed" : ""}`}
          value={model.id}
          placeholder="模型 ID"
          onChange={(e) => onUpdate("id", e.target.value)}
          disabled={isDiscovered}
          autoFocus={!isDiscovered}
        />
        <input
          type="text"
          className={`${inputCls} !w-32 !py-1 !px-2 ${isDiscovered ? "opacity-50 cursor-not-allowed" : ""}`}
          value={model.name}
          placeholder="显示名称"
          onChange={(e) => onUpdate("name", e.target.value)}
          disabled={isDiscovered}
        />
      </div>
      {/* 属性行 */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          上下文
          <input
            type="number"
            className={`${inputCls} !w-[72px] !py-1 !px-2 text-center`}
            value={Math.round(model.context_window / 1000)}
            placeholder="200"
            onChange={(e) => onUpdate("context_window", (parseInt(e.target.value) || 0) * 1000)}
          />
          <span>k</span>
        </label>

        <TogglePill label="推理" checked={model.reasoning} onChange={(v) => onUpdate("reasoning", v)} />
        <TogglePill label="图像" checked={model.supports_image} onChange={(v) => onUpdate("supports_image", v)} />

        <div className="flex items-center gap-1.5 ml-auto">
          <button
            type="button"
            onClick={onRemove}
            className="rounded-[6px] border border-destructive/30 px-2 py-1 text-[11px] text-destructive transition-colors hover:bg-destructive/10"
          >
            删除
          </button>
          <button
            type="button"
            onClick={onDone}
            className="rounded-[6px] bg-primary px-3 py-1 text-[11px] text-primary-foreground transition-colors hover:opacity-90"
          >
            完成
          </button>
        </div>
      </div>
    </div>
  );
}

/** Discovered 模型属性编辑行 — 模型 ID 和显示名称锁定 */
function DiscoveredModelEditRow({
  model,
  override,
  onOverride,
  onResetOverride,
  onDone,
}: {
  model: ModelInfo;
  override: ModelConfig | null;
  onOverride: (field: keyof ModelConfig, value: string | number | boolean) => void;
  onResetOverride: () => void;
  onDone: () => void;
}) {
  const effectiveContextK = Math.round((override?.context_window ?? model.context_window) / 1000);
  const effectiveReasoning = override?.reasoning ?? model.reasoning;
  const effectiveImage = override?.supports_image ?? model.supports_image;

  return (
    <div className="rounded-[8px] border border-border bg-background/80 p-3">
      {/* 标题行 */}
      <div className="flex items-center gap-2 mb-3">
        {/* eslint-disable-next-line no-restricted-syntax -- 状态指示圆点 */}
        <span className="inline-block h-2 w-2 rounded-full bg-success" />
        <span className="text-xs font-medium text-foreground truncate">{model.name || model.id}</span>
        <code className="ml-auto text-[10px] text-muted-foreground/60 font-mono truncate max-w-[180px]">{model.id}</code>
      </div>
      {/* 属性行 */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          上下文
          <input
            type="number"
            className={`${inputCls} !w-[72px] !py-1 !px-2 text-center`}
            value={effectiveContextK}
            onChange={(e) => onOverride("context_window", (parseInt(e.target.value) || 0) * 1000)}
          />
          <span>k</span>
        </label>

        <TogglePill label="推理" checked={effectiveReasoning} onChange={(v) => onOverride("reasoning", v)} />
        <TogglePill label="图像" checked={effectiveImage} onChange={(v) => onOverride("supports_image", v)} />

        <div className="flex items-center gap-1.5 ml-auto">
          {override && (
            <button
              type="button"
              onClick={onResetOverride}
              className="rounded-[6px] border border-border px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            >
              重置
            </button>
          )}
          <button
            type="button"
            onClick={onDone}
            className="rounded-[6px] bg-primary px-3 py-1 text-[11px] text-primary-foreground transition-colors hover:opacity-90"
          >
            完成
          </button>
        </div>
      </div>
    </div>
  );
}

/** 小型开关药丸 — 替代裸 checkbox，视觉更统一 */
function TogglePill({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      onClick={() => onChange(!checked)}
      className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] font-medium transition-all ${
        checked
          ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
          : "border-border bg-muted/40 text-muted-foreground"
      }`}
    >
      <span className={`inline-block h-1.5 w-1.5 rounded-full transition-colors ${
        checked ? "bg-emerald-500" : "bg-muted-foreground/30"
      }`} />
      {label}
    </button>
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
  const [supportsImage, setSupportsImage] = useState(true);

  const handleSubmit = () => {
    const trimmedId = id.trim();
    if (!trimmedId) return;
    onAdd({
      id: trimmedId,
      name: name.trim(),
      context_window: contextWindowK * 1000,
      reasoning,
      supports_image: supportsImage,
    });
  };

  return (
    <div className="rounded-[8px] border border-border bg-background/80 p-3">
      <p className="text-xs font-medium text-foreground mb-3">添加自定义模型</p>
      {/* ID + Name */}
      <div className="flex flex-wrap items-center gap-2 mb-3">
        {/* eslint-disable-next-line no-restricted-syntax -- 状态指示圆点 */}
        <span className="inline-block h-2 w-2 rounded-full bg-info/50" />
        <input type="text" className={`${inputCls} !w-40 !py-1 !px-2`} value={id} placeholder="模型 ID（必填）" onChange={(e) => setId(e.target.value)} autoFocus />
        <input type="text" className={`${inputCls} !w-32 !py-1 !px-2`} value={name} placeholder="显示名称" onChange={(e) => setName(e.target.value)} />
      </div>
      {/* 属性行 */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          上下文
          <input type="number" className={`${inputCls} !w-[72px] !py-1 !px-2 text-center`} value={contextWindowK} placeholder="200" onChange={(e) => setContextWindowK(parseInt(e.target.value) || 200)} />
          <span>k</span>
        </label>

        <TogglePill label="推理" checked={reasoning} onChange={setReasoning} />
        <TogglePill label="图像" checked={supportsImage} onChange={setSupportsImage} />

        <div className="flex items-center gap-1.5 ml-auto">
          <button type="button" onClick={onCancel} className="rounded-[6px] border border-border px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary">取消</button>
          <button type="button" onClick={handleSubmit} disabled={!id.trim()} className="rounded-[6px] bg-primary px-3 py-1 text-[11px] text-primary-foreground transition-colors hover:opacity-90 disabled:opacity-50">添加</button>
        </div>
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
  const entry = settings.find((s) => s.key === "agent.pi.user_preferences");
  let initialPrefs: string[] = [];
  if (entry && Array.isArray(entry.value)) {
    initialPrefs = entry.value.filter((v): v is string => typeof v === "string" && v.trim() !== "");
  }

  return (
    <AgentSectionForm
      key={JSON.stringify(initialPrefs)}
      initialPreferences={initialPrefs}
      saving={saving}
      onSave={onSave}
    />
  );
}

function AgentSectionForm({
  initialPreferences,
  saving,
  onSave,
}: {
  initialPreferences: string[];
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const [preferences, setPreferences] = useState<string[]>(
    initialPreferences.length > 0 ? initialPreferences : [""],
  );

  const handleChange = (index: number, value: string) => {
    const next = [...preferences];
    next[index] = value;
    setPreferences(next);
  };

  const handleAdd = () => setPreferences([...preferences, ""]);

  const handleRemove = (index: number) => {
    const next = preferences.filter((_, i) => i !== index);
    setPreferences(next.length > 0 ? next : [""]);
  };

  const handleSave = () => {
    const cleaned = preferences.map((p) => p.trim()).filter((p) => p !== "");
    onSave([{ key: "agent.pi.user_preferences", value: cleaned }]);
  };

  return (
    <SectionCard title="Pi Agent">
      <Field label="User Preferences" desc="用户偏好提示（每条独立生效，会附加到系统提示末尾）">
        <div className="flex flex-col gap-2">
          {preferences.map((pref, i) => (
            <div key={i} className="flex items-start gap-2">
              <textarea
                className={`${inputCls} min-h-[60px] flex-1 resize-y`}
                value={pref}
                onChange={(e) => handleChange(i, e.target.value)}
                rows={2}
                placeholder={`偏好 ${i + 1}，例如"请用中文回复"或"优先使用函数式风格"`}
              />
              <button
                type="button"
                className="mt-1 rounded-[6px] border border-border px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-destructive hover:text-destructive-foreground"
                onClick={() => handleRemove(i)}
                title="删除此条"
              >
                ×
              </button>
            </div>
          ))}
          <button
            type="button"
            className="self-start rounded-[6px] border border-border px-3 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary"
            onClick={handleAdd}
          >
            + 添加偏好
          </button>
        </div>
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
            className="h-10 w-full appearance-none rounded-[8px] border border-border bg-background pl-3.5 pr-9 text-sm text-foreground outline-none transition-colors ring-ring focus:border-primary/30 focus:ring-1 focus:ring-ring/40 disabled:opacity-50"
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

function BackendSection({
  backends,
  runtimeSummaries,
  onRemove,
}: {
  backends: BackendConfig[];
  runtimeSummaries: BackendRuntimeSummary[];
  onRemove: (id: string) => void;
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const toggle = (id: string) => setExpandedId((prev) => (prev === id ? null : id));

  const onlineCount = backends.filter((b) => b.online).length;
  const summaryByBackend = useMemo(
    () => new Map(runtimeSummaries.map((summary) => [summary.backend_id, summary])),
    [runtimeSummaries],
  );

  return (
    <SectionCard title="后端管理">
      <p className="text-xs text-muted-foreground">
        共 {backends.length} 个后端，{onlineCount} 个在线
      </p>

      {backends.length === 0 && (
        <p className="rounded-[8px] border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
          暂无已注册后端
        </p>
      )}

      <div className="space-y-2">
        {backends.map((backend) => {
          const isExpanded = expandedId === backend.id;
          const runtimeSummary = summaryByBackend.get(backend.id);
          const executors = backend.capabilities?.executors ?? [];
          const availableExecs = executors.filter((e) => e.available);
          const runtimeHealth = backend.runtime_health;
          const roots = backend.workspace_roots ?? runtimeHealth?.workspace_roots ?? [];
          const machineLabel = backend.machine_label || machineLabelFromDevice(backend.device) || backend.name;
          const scopeLabel = formatBackendScope(backend);

          return (
            <div key={backend.id} className="rounded-[8px] border border-border bg-background/80">
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
                    {backend.backend_type === "local"
                      ? `${machineLabel} · ${scopeLabel}`
                      : runtimeSummary
                        ? `${runtimeSummary.active_session_count} 个活跃会话 · ${runtimeSummary.allocatable ? "可分配" : "不可分配"}`
                        : backend.online
                          ? `${availableExecs.length} 个执行器可用`
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
                    {runtimeSummary && (
                      <>
                        <span className="text-muted-foreground">执行占用</span>
                        <span className="text-foreground">{runtimeSummary.active_session_count} 个活跃会话</span>
                        <span className="text-muted-foreground">自动分配</span>
                        <span className={runtimeSummary.allocatable ? "text-emerald-600 dark:text-emerald-400" : "text-muted-foreground"}>
                          {runtimeSummary.allocatable ? "可分配" : "不可分配"}
                        </span>
                      </>
                    )}
                    {backend.backend_type === "local" && (
                      <>
                        <span className="text-muted-foreground">机器</span>
                        <span className="truncate text-foreground" title={backend.machine_id ?? undefined}>
                          {machineLabel}
                        </span>
                        <span className="text-muted-foreground">Scope</span>
                        <span className="text-foreground">{scopeLabel}</span>
                        <span className="text-muted-foreground">能力槽</span>
                        <span className="font-mono text-foreground">{backend.capability_slot || "default"}</span>
                      </>
                    )}
                    {runtimeHealth && (
                      <>
                        <span className="text-muted-foreground">Runtime</span>
                        <span className="text-foreground">{runtimeStatusLabel(runtimeHealth.status)}</span>
                        <span className="text-muted-foreground">版本</span>
                        <span className="truncate font-mono text-foreground" title={runtimeHealth.version ?? undefined}>
                          {runtimeHealth.version ?? "—"}
                        </span>
                        <span className="text-muted-foreground">Last seen</span>
                        <span className="text-foreground">{formatRuntimeTimestamp(runtimeHealth.last_seen_at)}</span>
                      </>
                    )}
                  </div>

                  {runtimeHealth?.disconnect_reason && !backend.online && (
                    <p className="rounded-[8px] border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
                      断开原因：{runtimeHealth.disconnect_reason}
                    </p>
                  )}

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

function machineLabelFromDevice(device: BackendConfig["device"]) {
  const hostname = device?.hostname;
  return typeof hostname === "string" && hostname.trim() ? hostname.trim() : null;
}

function formatBackendScope(backend: BackendConfig) {
  const kind = backend.share_scope_kind ?? "user";
  const visibility = backend.visibility ?? "private";
  if (kind === "user") return `Personal / ${visibility}`;
  if (kind === "project") return `Project shared / ${visibility}`;
  return `System shared / ${visibility}`;
}

function runtimeStatusLabel(status: NonNullable<BackendConfig["runtime_health"]>["status"]) {
  switch (status) {
    case "online":
      return "在线";
    case "offline":
      return "离线";
    case "starting":
      return "启动中";
    case "degraded":
      return "降级";
    case "stopping":
      return "停止中";
    case "error":
      return "错误";
  }
}

function formatRuntimeTimestamp(value: string | null | undefined) {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function ScopeTabs({
  activePanel,
  includeLocalRuntime,
  onChange,
}: {
  activePanel: SettingsPanel;
  includeLocalRuntime: boolean;
  onChange: (scope: SettingsPanel) => void;
}) {
  const panels: SettingsPanel[] = includeLocalRuntime
    ? ["system", "user", "project", "local-runtime"]
    : ["system", "user", "project"];
  return (
    <div className="flex flex-wrap gap-2">
      {panels.map((scope) => (
        <button
          key={scope}
          type="button"
          onClick={() => onChange(scope)}
          className={`rounded-full border px-3 py-1.5 text-sm transition-colors ${
            activePanel === scope
              ? "border-primary/30 bg-primary/10 text-foreground"
              : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground"
          }`}
        >
          {SETTINGS_PANEL_LABELS[scope]}
        </button>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export function SettingsPage() {
  const navigate = useNavigate();
  const location = useLocation();
  const { settings, loading, saving, error, fetchSettings, updateSettings } = useSettingsStore();
  const {
    backends,
    backendRuntimeSummaries,
    fetchBackends,
    fetchBackendRuntimeSummaries,
    removeBackend,
  } = useCoordinatorStore();
  const { currentUser } = useCurrentUserStore();
  const { currentProjectId, projects } = useProjectStore();
  const [activePanel, setActivePanel] = useState<SettingsPanel>("system");
  const [toast, setToast] = useState<string | null>(null);
  const [llmDiscoveryRefreshKey, setLlmDiscoveryRefreshKey] = useState(0);
  const routeState = (location.state as SettingsNavigationState | null) ?? null;
  const returnTarget = routeState?.return_to?.trim() || "/dashboard/agent";
  const includeLocalRuntime = !!getDesktopLocalRuntimeClient();

  const currentProject = projects.find((project) => project.id === currentProjectId) ?? null;
  const canManageSystemScope = currentUser?.auth_mode === "personal" || currentUser?.is_admin === true;
  const scopeRequest = useMemo<SettingsScopeRequest | null>(() => {
    if (activePanel === "local-runtime") {
      return null;
    }
    if (activePanel === "system") {
      return canManageSystemScope ? { scope: "system" } : null;
    }
    if (activePanel === "user") {
      return { scope: "user" };
    }
    if (!currentProjectId) {
      return null;
    }
    return { scope: "project", project_id: currentProjectId };
  }, [activePanel, canManageSystemScope, currentProjectId]);

  useEffect(() => {
    void fetchBackends();
    void fetchBackendRuntimeSummaries();
  }, [fetchBackends, fetchBackendRuntimeSummaries]);

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

  const handleBack = useCallback(() => {
    navigate(returnTarget);
  }, [navigate, returnTarget]);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          {/* eslint-disable-next-line no-restricted-syntax -- 圆形 spinner 必须 rounded-full 才能正确旋转 */}
          <div className="mx-auto h-7 w-7 animate-spin rounded-full border-2 border-primary border-t-transparent" />
          <p className="mt-3 text-sm text-muted-foreground">正在加载设置…</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-6xl space-y-6 px-6 py-8">
        <div className="space-y-3">
          <button
            type="button"
            onClick={handleBack}
            className="inline-flex items-center gap-2 rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground transition-colors hover:bg-secondary"
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
              管理系统级配置、用户偏好和项目设置。
            </p>
          </div>
        </div>

        <SectionCard title="Scope">
          <ScopeTabs activePanel={activePanel} includeLocalRuntime={includeLocalRuntime} onChange={setActivePanel} />
          <div className="space-y-1 text-xs text-muted-foreground">
            <p>当前 scope：{SETTINGS_PANEL_LABELS[activePanel]}</p>
            {activePanel === "local-runtime" && (
              <p>本机运行时是 desktop-only scope，只管理当前桌面端与目标 server 的本机连接、根目录、能力和诊断日志。</p>
            )}
            {activePanel === "system" && (
              <p>system scope 仅 personal 模式或管理员可访问，适合放全局执行器、LLM Provider 和系统级 Agent 配置。</p>
            )}
            {activePanel === "user" && (
              <p>用户设置绑定当前登录用户，包含个人偏好和本地调试选项，不会影响其他用户。</p>
            )}
            {activePanel === "project" && (
              <p>
                project scope 绑定当前选中 Project。
                {currentProject ? ` 当前项目：${currentProject.name}` : " 请先在侧边栏选择一个 Project。"}
              </p>
            )}
          </div>
        </SectionCard>

        {error && (
          <div className="rounded-[8px] border border-destructive/50 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}

        {activePanel === "local-runtime" && <DesktopLocalRuntimePanel />}

        {activePanel === "system" && !canManageSystemScope && (
          <div className="rounded-[8px] border border-warning/30 bg-warning/10 px-4 py-3 text-sm text-warning">
            当前企业身份不是管理员，system scope 设置已被收口。你仍然可以查看和维护 user / project scope。
          </div>
        )}

        {activePanel === "system" && canManageSystemScope && (
          <>
            <BackendSection
              backends={backends}
              runtimeSummaries={backendRuntimeSummaries}
              onRemove={(id) => void removeBackend(id)}
            />
            <LlmProvidersSection
              discoveryRefreshKey={llmDiscoveryRefreshKey}
              onRefreshModels={() => setLlmDiscoveryRefreshKey((k) => k + 1)}
            />
            <AgentSection settings={settings} saving={saving} onSave={handleSave} />
            <ExecutorSection settings={settings} saving={saving} onSave={handleSave} />
          </>
        )}

        {activePanel === "user" && scopeRequest && (
          <>
            <UserByokSection onRefreshModels={() => setLlmDiscoveryRefreshKey((k) => k + 1)} />
            <DebugPrefsSection />
          </>
        )}

        {activePanel === "project" && !scopeRequest && (
          <div className="rounded-[8px] border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
            还没有选中的 Project，暂时无法进入 project scope。
          </div>
        )}

        {activePanel === "project" && scopeRequest && currentProject && (
          <SectionCard title={`项目设置 · ${currentProject.name}`}>
            <p className="text-xs text-muted-foreground">
              项目级配置请前往{" "}
              <button
                type="button"
                className="text-primary underline underline-offset-2 hover:text-primary/80"
                onClick={() => navigate(`/projects/${currentProject.id}/settings`)}
              >
                项目设置页
              </button>
              {" "}管理。
            </p>
          </SectionCard>
        )}

      </div>

      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
