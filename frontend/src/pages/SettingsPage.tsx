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

function isMasked(settings: { key: string; masked: boolean }[], key: string): boolean {
  return settings.find((s) => s.key === key)?.masked ?? false;
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

function LlmSection({
  settings,
  saving,
  onSave,
}: {
  settings: { key: string; value: unknown; masked: boolean }[];
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const [apiKey, setApiKey] = useState("");
  const [apiKeyTouched, setApiKeyTouched] = useState(false);
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [model, setModel] = useState("gpt-4o");
  const [wireApi, setWireApi] = useState<"responses" | "completions">("responses");

  // 从 store 同步初始值
  useEffect(() => {
    const maskedKey = isMasked(settings, "llm.openai.api_key");
    if (!apiKeyTouched) {
      setApiKey(maskedKey ? readVal(settings, "llm.openai.api_key") : readVal(settings, "llm.openai.api_key"));
    }
    setBaseUrl(readVal(settings, "llm.openai.base_url", "https://api.openai.com/v1"));
    setModel(readVal(settings, "llm.openai.default_model", "gpt-4o"));
    const wire = readVal(settings, "llm.openai.wire_api", "responses");
    if (wire === "completions" || wire === "responses") setWireApi(wire);
  }, [settings, apiKeyTouched]);

  const handleSave = () => {
    const updates: SettingUpdate[] = [
      { key: "llm.openai.base_url", value: baseUrl },
      { key: "llm.openai.default_model", value: model },
      { key: "llm.openai.wire_api", value: wireApi },
    ];
    // 仅在用户实际编辑过 api_key 时才提交
    if (apiKeyTouched) {
      updates.push({ key: "llm.openai.api_key", value: apiKey });
    }
    onSave(updates);
    setApiKeyTouched(false);
  };

  return (
    <SectionCard title="LLM 服务">
      <Field label="API Key" desc="OpenAI 兼容接口的密钥">
        <input
          type="password"
          className={inputCls}
          value={apiKey}
          placeholder="sk-..."
          onChange={(e) => {
            setApiKey(e.target.value);
            setApiKeyTouched(true);
          }}
        />
      </Field>

      <Field label="Base URL" desc="API 端点地址">
        <input
          type="text"
          className={inputCls}
          value={baseUrl}
          placeholder="https://api.openai.com/v1"
          onChange={(e) => setBaseUrl(e.target.value)}
        />
      </Field>

      <Field label="默认模型">
        <input
          type="text"
          className={inputCls}
          value={model}
          placeholder="gpt-4o"
          onChange={(e) => setModel(e.target.value)}
        />
      </Field>

      <Field label="Wire API" desc="请求协议，responses 为新版 Responses API">
        <div className="flex gap-4">
          {(["responses", "completions"] as const).map((opt) => (
            <label key={opt} className="flex items-center gap-1.5 text-sm text-foreground">
              <input
                type="radio"
                name="wire_api"
                checked={wireApi === opt}
                onChange={() => setWireApi(opt)}
                className="accent-primary"
              />
              {opt}
            </label>
          ))}
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

function AgentSection({
  settings,
  saving,
  onSave,
}: {
  settings: { key: string; value: unknown; masked: boolean }[];
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}) {
  const [systemPrompt, setSystemPrompt] = useState("");
  const [temperature, setTemperature] = useState("0.7");
  const [maxTurns, setMaxTurns] = useState("25");

  useEffect(() => {
    setSystemPrompt(readVal(settings, "agent.pi.system_prompt"));
    setTemperature(readVal(settings, "agent.pi.temperature", "0.7"));
    setMaxTurns(readVal(settings, "agent.pi.max_turns", "25"));
  }, [settings]);

  const handleSave = () => {
    onSave([
      { key: "agent.pi.system_prompt", value: systemPrompt },
      { key: "agent.pi.temperature", value: Number(temperature) },
      { key: "agent.pi.max_turns", value: Number(maxTurns) },
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

      <div className="grid grid-cols-2 gap-4">
        <Field label="Temperature" desc="生成随机性 (0–1)">
          <input
            type="number"
            className={inputCls}
            value={temperature}
            min={0}
            max={1}
            step={0.1}
            onChange={(e) => setTemperature(e.target.value)}
          />
        </Field>

        <Field label="Max Turns" desc="单次会话最大轮数">
          <input
            type="number"
            className={inputCls}
            value={maxTurns}
            min={1}
            onChange={(e) => setMaxTurns(e.target.value)}
          />
        </Field>
      </div>

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
  const [executor, setExecutor] = useState("");

  useEffect(() => {
    setExecutor(readVal(settings, "executor.default.executor"));
  }, [settings]);

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
        <LlmSection settings={settings} saving={saving} onSave={handleSave} />
        <AgentSection settings={settings} saving={saving} onSave={handleSave} />
        <ExecutorSection settings={settings} saving={saving} onSave={handleSave} />
      </div>

      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
