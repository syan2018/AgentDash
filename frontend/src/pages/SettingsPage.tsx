import { useEffect, useState, useCallback } from "react";
import { useSettingsStore } from "../stores/settingsStore";
import type { SettingUpdate } from "../api/settings";

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

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export function SettingsPage() {
  const { settings, loading, saving, error, fetchSettings, updateSettings } = useSettingsStore();
  const [toast, setToast] = useState<string | null>(null);

  useEffect(() => {
    void fetchSettings();
  }, [fetchSettings]);

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

        <LlmSection settings={settings} saving={saving} onSave={handleSave} />
        <AgentSection settings={settings} saving={saving} onSave={handleSave} />
        <ExecutorSection settings={settings} saving={saving} onSave={handleSave} />
      </div>

      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
