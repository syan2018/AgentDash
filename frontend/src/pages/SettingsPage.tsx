import { useEffect, useState, useCallback, useMemo } from "react";
import { useSettingsStore } from "../stores/settingsStore";
import { useCoordinatorStore } from "../stores/coordinatorStore";
import { useCurrentUserStore } from "../stores/currentUserStore";
import { useProjectStore } from "../stores/projectStore";
import { useExecutorDiscovery, useExecutorDiscoveredOptions } from "../features/executor-selector";
import type { SettingEntry, SettingUpdate, SettingsScopeRequest } from "../api/settings";
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
      context_window: Number(record.context_window ?? 128000) || 128000,
      reasoning: record.reasoning === true,
      max_tokens: typeof record.max_tokens === "number" ? record.max_tokens : undefined,
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
// LLM Providers 配置
// ---------------------------------------------------------------------------

interface LlmProviderDef {
  id: string;
  name: string;
  description: string;
  apiKeySettingKey?: string;
  baseUrlSettingKey?: string;
  defaultModelSettingKey?: string;
  modelsSettingKey?: string;
  blockedModelsSettingKey?: string;
  wireApiSettingKey?: string;
  supportsBaseUrl: boolean;
  /** 无需 API Key（如本地 Ollama） */
  noApiKey?: boolean;
  apiKeyPlaceholder?: string;
  baseUrlPlaceholder?: string;
}

/** 模型配置 */
interface ModelConfig {
  id: string;
  name: string;
  context_window: number;
  reasoning: boolean;
  max_tokens?: number;
}

const LLM_PROVIDERS: LlmProviderDef[] = [
  {
    id: "anthropic",
    name: "Anthropic Claude",
    description: "Claude Opus、Sonnet 等模型",
    apiKeySettingKey: "llm.anthropic.api_key",
    modelsSettingKey: "llm.anthropic.models",
    blockedModelsSettingKey: "llm.anthropic.blocked_models",
    supportsBaseUrl: false,
    apiKeyPlaceholder: "sk-ant-...",
  },
  {
    id: "gemini",
    name: "Google Gemini",
    description: "Gemini 2.5 Pro、Flash 等模型",
    apiKeySettingKey: "llm.gemini.api_key",
    modelsSettingKey: "llm.gemini.models",
    blockedModelsSettingKey: "llm.gemini.blocked_models",
    supportsBaseUrl: false,
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    description: "DeepSeek Chat、DeepSeek Reasoner (R1)",
    apiKeySettingKey: "llm.deepseek.api_key",
    modelsSettingKey: "llm.deepseek.models",
    blockedModelsSettingKey: "llm.deepseek.blocked_models",
    supportsBaseUrl: false,
    apiKeyPlaceholder: "sk-...",
  },
  {
    id: "openai",
    name: "OpenAI",
    description: "GPT-4o、o3 等模型，支持兼容端点",
    apiKeySettingKey: "llm.openai.api_key",
    baseUrlSettingKey: "llm.openai.base_url",
    defaultModelSettingKey: "llm.openai.default_model",
    modelsSettingKey: "llm.openai.models",
    blockedModelsSettingKey: "llm.openai.blocked_models",
    wireApiSettingKey: "llm.openai.wire_api",
    supportsBaseUrl: true,
    apiKeyPlaceholder: "sk-...",
    baseUrlPlaceholder: "https://api.openai.com/v1",
  },
  {
    id: "groq",
    name: "Groq",
    description: "Llama、QwQ 等模型（高速推理）",
    apiKeySettingKey: "llm.groq.api_key",
    modelsSettingKey: "llm.groq.models",
    blockedModelsSettingKey: "llm.groq.blocked_models",
    supportsBaseUrl: false,
    apiKeyPlaceholder: "gsk_...",
  },
  {
    id: "xai",
    name: "xAI (Grok)",
    description: "Grok 3、Grok 3 Mini 等模型",
    apiKeySettingKey: "llm.xai.api_key",
    modelsSettingKey: "llm.xai.models",
    blockedModelsSettingKey: "llm.xai.blocked_models",
    supportsBaseUrl: false,
    apiKeyPlaceholder: "xai-...",
  },
];

/** 判断 setting value 是否已配置（非空） */
function isConfigured(val: string): boolean {
  return val.length > 0;
}

function LlmProvidersSection({
  settings,
  saving,
  onSave,
}: {
  settings: { key: string; value: unknown; masked: boolean }[];
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const discovered = useExecutorDiscoveredOptions("PI_AGENT", "");
  const discoveredModels = discovered.options?.model_selector.models ?? [];

  return (
    <SectionCard title="LLM Providers">
      <p className="text-xs text-muted-foreground -mt-2 mb-1">
        配置各 LLM 服务商的 API 密钥和端点，按需开启
      </p>
      <div className="space-y-2">
        {LLM_PROVIDERS.map((provider) => (
          <LlmProviderRow
            key={provider.id}
            provider={provider}
            settings={settings}
            discoveredModels={discoveredModels}
            saving={saving}
            onSave={onSave}
          />
        ))}
      </div>
    </SectionCard>
  );
}

function LlmProviderRow({
  provider,
  settings,
  discoveredModels,
  saving,
  onSave,
}: {
  provider: LlmProviderDef;
  settings: { key: string; value: unknown; masked: boolean }[];
  discoveredModels: Array<{ id: string; name: string; provider_id?: string | null; blocked?: boolean }>;
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const [expanded, setExpanded] = useState(false);

  // 读取当前已保存的值
  const savedApiKey = provider.apiKeySettingKey ? readVal(settings, provider.apiKeySettingKey) : "";
  const savedBaseUrl = provider.baseUrlSettingKey ? readVal(settings, provider.baseUrlSettingKey) : "";
  const savedModel = provider.defaultModelSettingKey ? readVal(settings, provider.defaultModelSettingKey) : "";
  const savedWireApi = provider.wireApiSettingKey ? readVal(settings, provider.wireApiSettingKey, "responses") : "";

  // 读取模型列表配置
  const savedModelsRaw = provider.modelsSettingKey
    ? settings.find((s) => s.key === provider.modelsSettingKey)?.value
    : undefined;
  const savedModels = parseModelConfigs(savedModelsRaw);
  const savedBlockedModelsRaw = provider.blockedModelsSettingKey
    ? settings.find((s) => s.key === provider.blockedModelsSettingKey)?.value
    : undefined;
  const savedBlockedModels = parseStringList(savedBlockedModelsRaw);

  // 对于有 API Key 的 provider，以 API Key 存在为判断依据；对于 noApiKey 的 provider，以 baseUrl 为判断依据
  const configured = provider.noApiKey
    ? isConfigured(savedBaseUrl)
    : isConfigured(savedApiKey);

  return (
    <div className="rounded-[10px] border border-border bg-background/80">
      <button
        type="button"
        className="flex w-full items-center gap-3 px-4 py-3 text-left"
        onClick={() => setExpanded((p) => !p)}
      >
        {/* 配置状态指示 */}
        <span
          className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${configured ? "bg-emerald-500" : "bg-muted-foreground/30"}`}
        />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-foreground">{provider.name}</p>
          <p className="text-xs text-muted-foreground">{provider.description}</p>
        </div>
        {configured && (
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
          key={`${provider.id}-${savedApiKey}-${savedBaseUrl}-${savedModel}-${JSON.stringify(savedModels)}-${savedBlockedModels.join(",")}`}
          provider={provider}
          initialApiKey={savedApiKey}
          initialBaseUrl={savedBaseUrl}
          initialModel={savedModel}
          initialWireApi={savedWireApi}
          initialModels={savedModels}
          initialBlockedModels={savedBlockedModels}
          discoveredModels={discoveredModels.filter((model) => (model.provider_id ?? "") === provider.id)}
          saving={saving}
          onSave={onSave}
        />
      )}
    </div>
  );
}

function LlmProviderForm({
  provider,
  initialApiKey,
  initialBaseUrl,
  initialModel,
  initialWireApi,
  initialModels,
  initialBlockedModels,
  discoveredModels,
  saving,
  onSave,
}: {
  provider: LlmProviderDef;
  initialApiKey: string;
  initialBaseUrl: string;
  initialModel: string;
  initialWireApi: string;
  initialModels: ModelConfig[];
  initialBlockedModels: string[];
  discoveredModels: Array<{ id: string; name: string; blocked?: boolean }>;
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const [apiKey, setApiKey] = useState(initialApiKey);
  const [apiKeyTouched, setApiKeyTouched] = useState(false);
  const [baseUrl, setBaseUrl] = useState(initialBaseUrl);
  const [model, setModel] = useState(initialModel);
  const [wireApi, setWireApi] = useState(initialWireApi || "responses");
  const [models, setModels] = useState<ModelConfig[]>(initialModels);
  const [modelsTouched, setModelsTouched] = useState(false);
  const [blockedModels, setBlockedModels] = useState<string[]>(initialBlockedModels);
  const [blockedModelsTouched, setBlockedModelsTouched] = useState(false);
  const toggleBlockedModel = (modelId: string) => {
    setBlockedModels((current) => {
      const exists = current.includes(modelId);
      const next = exists
        ? current.filter((item) => item !== modelId)
        : [...current, modelId];
      return next;
    });
    setBlockedModelsTouched(true);
  };

  const handleSave = () => {
    const updates: SettingUpdate[] = [];

    // API Key（仅当用户编辑过才提交，避免覆盖掩码值）
    if (!provider.noApiKey && provider.apiKeySettingKey && apiKeyTouched) {
      updates.push({ key: provider.apiKeySettingKey, value: apiKey });
    }

    // Base URL
    if (provider.supportsBaseUrl && provider.baseUrlSettingKey) {
      updates.push({ key: provider.baseUrlSettingKey, value: baseUrl });
    }

    // 默认模型
    if (provider.defaultModelSettingKey) {
      updates.push({ key: provider.defaultModelSettingKey, value: model });
    }

    // Wire API（仅 OpenAI）
    if (provider.wireApiSettingKey) {
      updates.push({ key: provider.wireApiSettingKey, value: wireApi });
    }

    // 模型列表
    if (modelsTouched && provider.modelsSettingKey) {
      updates.push({ key: provider.modelsSettingKey, value: models });
    }

    if (blockedModelsTouched && provider.blockedModelsSettingKey) {
      updates.push({
        key: provider.blockedModelsSettingKey,
        value: blockedModels,
      });
    }

    if (updates.length > 0) {
      onSave(updates);
      setApiKeyTouched(false);
      setModelsTouched(false);
      setBlockedModelsTouched(false);
    }
  };

  const handleAddModel = () => {
    const newModel: ModelConfig = {
      id: "",
      name: "",
      context_window: 128000,
      reasoning: false,
    };
    setModels([...models, newModel]);
    setModelsTouched(true);
  };

  const handleRemoveModel = (index: number) => {
    const newModels = models.filter((_, i) => i !== index);
    setModels(newModels);
    setModelsTouched(true);
  };

  const handleUpdateModel = (index: number, field: keyof ModelConfig, value: string | number | boolean) => {
    const newModels = models.map((m, i) => {
      if (i !== index) return m;
      return { ...m, [field]: value };
    });
    setModels(newModels);
    setModelsTouched(true);
  };

  return (
    <div className="space-y-3 border-t border-border px-4 pb-4 pt-3">
      {/* API Key 输入（非 noApiKey provider） */}
      {!provider.noApiKey && (
        <Field label="API Key" desc="服务密钥，保存后以掩码形式显示">
          <input
            type="password"
            className={inputCls}
            value={apiKey}
            placeholder={provider.apiKeyPlaceholder ?? "输入 API Key"}
            onChange={(e) => {
              setApiKey(e.target.value);
              setApiKeyTouched(true);
            }}
          />
        </Field>
      )}

      {/* Base URL 输入（supportsBaseUrl provider） */}
      {provider.supportsBaseUrl && (
        <Field label="Base URL" desc="API 端点地址">
          <input
            type="text"
            className={inputCls}
            value={baseUrl}
            placeholder={provider.baseUrlPlaceholder ?? "https://..."}
            onChange={(e) => setBaseUrl(e.target.value)}
          />
        </Field>
      )}

      {/* 默认模型 */}
      {provider.defaultModelSettingKey && (
        <Field label="默认模型">
          <input
            type="text"
            className={inputCls}
            value={model}
            placeholder="例如 gpt-4o"
            onChange={(e) => setModel(e.target.value)}
          />
        </Field>
      )}

      {/* Wire API（仅 OpenAI） */}
      {provider.wireApiSettingKey && (
        <Field label="Wire API" desc="请求协议，responses 为新版 Responses API">
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

      {/* 模型列表编辑器 */}
      {provider.modelsSettingKey && (
        <Field label="可用模型" desc="配置此 Provider 的可用模型列表（可选，留空则自动获取）">
          <div className="space-y-2">
            {models.length === 0 ? (
              <p className="text-xs text-muted-foreground py-2">未配置自定义模型，将自动从 Provider API 获取</p>
            ) : (
              <div className="space-y-2">
                {models.map((m, index) => (
                  <div key={index} className="rounded-[8px] border border-border bg-secondary/30 p-3 space-y-2">
                    <div className="grid grid-cols-2 gap-2">
                      <input
                        type="text"
                        className={inputCls}
                        value={m.id}
                        placeholder="模型 ID"
                        onChange={(e) => handleUpdateModel(index, "id", e.target.value)}
                      />
                      <input
                        type="text"
                        className={inputCls}
                        value={m.name}
                        placeholder="显示名称"
                        onChange={(e) => handleUpdateModel(index, "name", e.target.value)}
                      />
                    </div>
                    <div className="grid grid-cols-2 gap-2">
                      <input
                        type="number"
                        className={inputCls}
                        value={m.context_window}
                        placeholder="Context Window"
                        onChange={(e) => handleUpdateModel(index, "context_window", parseInt(e.target.value) || 0)}
                      />
                      <label className="flex items-center gap-2 text-sm text-foreground">
                        <input
                          type="checkbox"
                          checked={m.reasoning}
                          onChange={(e) => handleUpdateModel(index, "reasoning", e.target.checked)}
                          className="accent-primary"
                        />
                        支持推理
                      </label>
                    </div>
                    <button
                      type="button"
                      onClick={() => handleRemoveModel(index)}
                      className="text-xs text-destructive hover:underline"
                    >
                      删除此模型
                    </button>
                  </div>
                ))}
              </div>
            )}
            <button
              type="button"
              onClick={handleAddModel}
              className="rounded-[8px] border border-border bg-secondary px-3 py-1.5 text-xs text-foreground transition-colors hover:bg-secondary/80"
            >
              + 添加模型
            </button>
          </div>
        </Field>
      )}

      {provider.blockedModelsSettingKey && (
        <Field label="屏蔽模型" desc="点击自动发现的模型开关。关闭后，该模型会从会话选择器中隐藏。">
          <div className="space-y-2">
            {discoveredModels.length === 0 ? (
              <p className="rounded-[8px] border border-dashed border-border px-3 py-2 text-xs text-muted-foreground">
                暂无自动发现模型。请先保存 Provider 配置，然后回到这里调整可见模型。
              </p>
            ) : (
              <div className="flex flex-wrap gap-2">
                {discoveredModels.map((model) => {
                  const enabled = !blockedModels.includes(model.id);
                  return (
                    <button
                      key={model.id}
                      type="button"
                      onClick={() => toggleBlockedModel(model.id)}
                      className={`rounded-full border px-3 py-1.5 text-xs transition-colors ${
                        enabled
                          ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
                          : "border-border bg-muted/60 text-muted-foreground line-through"
                      }`}
                    >
                      {model.name || model.id}
                    </button>
                  );
                })}
              </div>
            )}
            {blockedModels.length > 0 && (
              <p className="text-xs text-muted-foreground">
                已屏蔽：{blockedModels.join("，")}
              </p>
            )}
          </div>
        </Field>
      )}

      <div className="flex justify-end pt-1">
        <button type="button" disabled={saving} className={btnPrimaryCls} onClick={handleSave}>
          {saving ? "保存中…" : "保存"}
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
// Page
// ---------------------------------------------------------------------------

export function SettingsPage() {
  const { settings, loading, saving, error, fetchSettings, updateSettings, deleteSetting } = useSettingsStore();
  const { backends, fetchBackends, removeBackend } = useCoordinatorStore();
  const { currentUser } = useCurrentUserStore();
  const { currentProjectId, projects } = useProjectStore();
  const [activeScope, setActiveScope] = useState<SettingsScopeKind>("system");
  const [toast, setToast] = useState<string | null>(null);

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
        <div>
          <h1 className="text-xl font-semibold text-foreground">设置</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            管理 system / user / project 三层 scope 设置。system 更偏宿主级配置，user 和 project 用来承接企业化后的个体偏好与项目策略。
          </p>
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
            <LlmProvidersSection settings={settings} saving={saving} onSave={handleSave} />
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
      </div>

      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
