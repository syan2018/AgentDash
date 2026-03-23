import { useEffect, useState, useCallback } from "react";
import { useSettingsStore } from "../stores/settingsStore";
import { useCoordinatorStore } from "../stores/coordinatorStore";
import type { SettingUpdate } from "../api/settings";
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
  wireApiSettingKey?: string;
  supportsBaseUrl: boolean;
  /** 无需 API Key（如本地 Ollama） */
  noApiKey?: boolean;
  apiKeyPlaceholder?: string;
  baseUrlPlaceholder?: string;
}

const LLM_PROVIDERS: LlmProviderDef[] = [
  {
    id: "anthropic",
    name: "Anthropic Claude",
    description: "Claude Opus、Sonnet 等模型",
    apiKeySettingKey: "llm.anthropic.api_key",
    supportsBaseUrl: false,
    apiKeyPlaceholder: "sk-ant-...",
  },
  {
    id: "gemini",
    name: "Google Gemini",
    description: "Gemini 2.5 Pro、Flash 等模型",
    apiKeySettingKey: "llm.gemini.api_key",
    supportsBaseUrl: false,
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    description: "DeepSeek Chat、DeepSeek Reasoner (R1)",
    apiKeySettingKey: "llm.deepseek.api_key",
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
    supportsBaseUrl: false,
    apiKeyPlaceholder: "gsk_...",
  },
  {
    id: "xai",
    name: "xAI (Grok)",
    description: "Grok 3、Grok 3 Mini 等模型",
    apiKeySettingKey: "llm.xai.api_key",
    supportsBaseUrl: false,
    apiKeyPlaceholder: "xai-...",
  },
  {
    id: "ollama",
    name: "Ollama（本地）",
    description: "本地部署的开源模型，无需 API Key",
    baseUrlSettingKey: "llm.ollama.base_url",
    supportsBaseUrl: true,
    noApiKey: true,
    baseUrlPlaceholder: "http://localhost:11434",
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
  saving,
  onSave,
}: {
  provider: LlmProviderDef;
  settings: { key: string; value: unknown; masked: boolean }[];
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const [expanded, setExpanded] = useState(false);

  // 读取当前已保存的值
  const savedApiKey = provider.apiKeySettingKey ? readVal(settings, provider.apiKeySettingKey) : "";
  const savedBaseUrl = provider.baseUrlSettingKey ? readVal(settings, provider.baseUrlSettingKey) : "";
  const savedModel = provider.defaultModelSettingKey ? readVal(settings, provider.defaultModelSettingKey) : "";
  const savedWireApi = provider.wireApiSettingKey ? readVal(settings, provider.wireApiSettingKey, "responses") : "";

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
          key={`${provider.id}-${savedApiKey}-${savedBaseUrl}`}
          provider={provider}
          initialApiKey={savedApiKey}
          initialBaseUrl={savedBaseUrl}
          initialModel={savedModel}
          initialWireApi={savedWireApi}
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
  saving,
  onSave,
}: {
  provider: LlmProviderDef;
  initialApiKey: string;
  initialBaseUrl: string;
  initialModel: string;
  initialWireApi: string;
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const [apiKey, setApiKey] = useState(initialApiKey);
  const [apiKeyTouched, setApiKeyTouched] = useState(false);
  const [baseUrl, setBaseUrl] = useState(initialBaseUrl);
  const [model, setModel] = useState(initialModel);
  const [wireApi, setWireApi] = useState(initialWireApi || "responses");

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

    // 默认模型（仅 OpenAI 类 provider 有此选项）
    if (provider.defaultModelSettingKey) {
      updates.push({ key: provider.defaultModelSettingKey, value: model });
    }

    // Wire API（仅 OpenAI 有此选项）
    if (provider.wireApiSettingKey) {
      updates.push({ key: provider.wireApiSettingKey, value: wireApi });
    }

    if (updates.length > 0) {
      onSave(updates);
      setApiKeyTouched(false);
    }
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

      {/* 默认模型（仅有 defaultModelSettingKey 的 provider） */}
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
  return (
    <ExecutorSectionForm
      key={readVal(settings, "executor.default.executor")}
      initialExecutor={readVal(settings, "executor.default.executor")}
      saving={saving}
      onSave={onSave}
    />
  );
}

function ExecutorSectionForm({
  initialExecutor,
  saving,
  onSave,
}: {
  initialExecutor: string;
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const [executor, setExecutor] = useState(initialExecutor);

  const handleSave = () => {
    onSave([{ key: "executor.default.executor", value: executor }]);
  };

  return (
    <SectionCard title="默认 Executor">
      <Field label="Executor" desc="默认执行器标识">
        <input
          type="text"
          className={inputCls}
          value={executor}
          placeholder="e.g. local-docker"
          onChange={(e) => setExecutor(e.target.value)}
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

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export function SettingsPage() {
  const { settings, loading, saving, error, fetchSettings, updateSettings } = useSettingsStore();
  const { backends, fetchBackends, removeBackend } = useCoordinatorStore();
  const [toast, setToast] = useState<string | null>(null);

  useEffect(() => {
    void fetchSettings();
    void fetchBackends();
  }, [fetchSettings, fetchBackends]);

  const handleSave = useCallback(
    async (updates: SettingUpdate[]) => {
      const updated = await updateSettings(updates);
      if (updated.length > 0) {
        setToast("设置已保存");
      }
    },
    [updateSettings],
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
          <p className="mt-1 text-sm text-muted-foreground">管理 LLM 服务、Agent 参数与执行器配置</p>
        </div>

        {error && (
          <div className="rounded-[10px] border border-destructive/50 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}

        <BackendSection backends={backends} onRemove={(id) => void removeBackend(id)} />
        <LlmProvidersSection settings={settings} saving={saving} onSave={handleSave} />
        <AgentSection settings={settings} saving={saving} onSave={handleSave} />
        <ExecutorSection settings={settings} saving={saving} onSave={handleSave} />
      </div>

      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
