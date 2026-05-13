import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import type {
  Routine,
  RoutineExecutionStatus,
  RoutineSessionMode,
  RoutineTriggerType,
  ProjectAgentLink,
} from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useRoutineStore } from "../../stores/routineStore";
import { DetailPanel, DetailMenu, DangerConfirmDialog } from "../../components/ui/detail-panel";

// ─── 通用工具 ───

function formatRelativeTime(iso: string | null): string {
  if (!iso) return "从未触发";
  const diffMs = Date.now() - new Date(iso).getTime();
  if (diffMs < 0) return "刚刚";
  const seconds = Math.floor(diffMs / 1000);
  if (seconds < 60) return "刚刚";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  return `${days} 天前`;
}

// ─── Cron 分段选择器（复用 project-agent-view 的逻辑） ───

type CronFrequency = "none" | "every_n_min" | "every_n_hour" | "daily" | "weekday";

const CRON_FREQ_OPTIONS: Array<{ value: CronFrequency; label: string }> = [
  { value: "none", label: "不启用" },
  { value: "every_n_min", label: "每隔 N 分钟" },
  { value: "every_n_hour", label: "每隔 N 小时" },
  { value: "daily", label: "每天指定时间" },
  { value: "weekday", label: "工作日指定时间" },
];

function cronToSegments(cron: string): { freq: CronFrequency; interval: number; hour: number; minute: number } {
  const parts = cron.trim().split(/\s+/);
  if (parts.length !== 5) return { freq: "none", interval: 10, hour: 9, minute: 0 };
  const [mm, hh, , , dow] = parts;
  if (mm.startsWith("*/") && hh === "*") {
    const n = Number(mm.slice(2));
    if (Number.isFinite(n) && n > 0) return { freq: "every_n_min", interval: n, hour: 9, minute: 0 };
  }
  if (hh.startsWith("*/") && /^\d+$/.test(mm)) {
    const n = Number(hh.slice(2));
    if (Number.isFinite(n) && n > 0) return { freq: "every_n_hour", interval: n, hour: 9, minute: Number(mm) };
  }
  if (/^\d+$/.test(mm) && /^\d+$/.test(hh)) {
    const h = Number(hh);
    const m = Number(mm);
    if (dow === "1-5") return { freq: "weekday", interval: 10, hour: h, minute: m };
    if (dow === "*") return { freq: "daily", interval: 10, hour: h, minute: m };
  }
  return { freq: "none", interval: 10, hour: 9, minute: 0 };
}

function segmentsToCron(freq: CronFrequency, interval: number, hour: number, minute: number): string {
  switch (freq) {
    case "every_n_min": return `*/${Math.max(1, interval)} * * * *`;
    case "every_n_hour": return `${minute} */${Math.max(1, interval)} * * *`;
    case "daily": return `${minute} ${hour} * * *`;
    case "weekday": return `${minute} ${hour} * * 1-5`;
    default: return "";
  }
}

function describeCron(freq: CronFrequency, interval: number, hour: number, minute: number): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  switch (freq) {
    case "every_n_min": return `每 ${interval} 分钟执行一次`;
    case "every_n_hour": return `每 ${interval} 小时执行一次（在第 ${minute} 分钟）`;
    case "daily": return `每天 ${pad(hour)}:${pad(minute)} 执行`;
    case "weekday": return `工作日 ${pad(hour)}:${pad(minute)} 执行`;
    default: return "";
  }
}

function CronScheduleSelector({ value, onChange }: { value: string; onChange: (cron: string) => void }) {
  const parsed = useMemo(() => cronToSegments(value), [value]);
  const isCustom = value.trim() !== "" && parsed.freq === "none";
  const [freq, setFreq] = useState<CronFrequency>(parsed.freq);
  const [interval, setIntervalVal] = useState(parsed.interval);
  const [hour, setHour] = useState(parsed.hour);
  const [minute, setMinute] = useState(parsed.minute);
  const [showRaw, setShowRaw] = useState(isCustom);

  const handleFreqChange = (f: CronFrequency) => {
    setFreq(f);
    setShowRaw(false);
    onChange(segmentsToCron(f, interval, hour, minute));
  };

  const handleParamChange = (newInterval: number, newHour: number, newMinute: number) => {
    setIntervalVal(newInterval);
    setHour(newHour);
    setMinute(newMinute);
    onChange(segmentsToCron(freq, newInterval, newHour, newMinute));
  };

  const generatedCron = segmentsToCron(freq, interval, hour, minute);

  return (
    <div className="space-y-2.5">
      <div>
        <label className="agentdash-form-label">定时频率</label>
        <select value={showRaw ? "none" : freq} onChange={(e) => handleFreqChange(e.target.value as CronFrequency)} className="agentdash-form-select">
          {CRON_FREQ_OPTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
        </select>
      </div>
      {freq === "every_n_min" && !showRaw && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">每隔</span>
          <input type="number" value={interval} onChange={(e) => handleParamChange(Math.max(1, Number(e.target.value) || 1), hour, minute)} min={1} max={59} className="agentdash-form-input w-20" />
          <span className="text-xs text-muted-foreground">分钟</span>
        </div>
      )}
      {freq === "every_n_hour" && !showRaw && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">每隔</span>
          <input type="number" value={interval} onChange={(e) => handleParamChange(Math.max(1, Number(e.target.value) || 1), hour, minute)} min={1} max={23} className="agentdash-form-input w-20" />
          <span className="text-xs text-muted-foreground">小时</span>
        </div>
      )}
      {(freq === "daily" || freq === "weekday") && !showRaw && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">时间</span>
          <input type="number" value={hour} onChange={(e) => handleParamChange(interval, Math.min(23, Math.max(0, Number(e.target.value) || 0)), minute)} min={0} max={23} className="agentdash-form-input w-16" />
          <span className="text-xs text-muted-foreground">:</span>
          <input type="number" value={minute} onChange={(e) => handleParamChange(interval, hour, Math.min(59, Math.max(0, Number(e.target.value) || 0)))} min={0} max={59} className="agentdash-form-input w-16" />
        </div>
      )}
      {showRaw && (
        <div>
          <label className="agentdash-form-label">Cron 表达式</label>
          <input value={value} onChange={(e) => onChange(e.target.value)} placeholder="* * * * *" className="agentdash-form-input font-mono" />
        </div>
      )}
      {freq !== "none" && !showRaw && (
        <div className="flex items-center gap-2">
          <code className="rounded-[6px] bg-secondary/50 px-2 py-0.5 font-mono text-[10px] text-muted-foreground">{generatedCron}</code>
          <span className="text-[10px] text-muted-foreground/70">{describeCron(freq, interval, hour, minute)}</span>
        </div>
      )}
      {isCustom && !showRaw && (
        <p className="text-[10px] text-amber-600 dark:text-amber-400">
          当前为自定义表达式：<code className="font-mono">{value}</code>
          <button type="button" onClick={() => setShowRaw(true)} className="ml-1 underline hover:no-underline">手动编辑</button>
        </p>
      )}
    </div>
  );
}

// ─── 触发类型 badge ───

const TRIGGER_TYPE_BADGE: Record<RoutineTriggerType, { label: string; className: string }> = {
  scheduled: { label: "定时", className: "border-blue-500/30 bg-blue-500/10 text-blue-700 dark:text-blue-300" },
  webhook: { label: "Webhook", className: "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300" },
  plugin: { label: "Plugin", className: "border-purple-500/30 bg-purple-500/10 text-purple-700 dark:text-purple-300" },
};

const EXEC_STATUS_STYLE: Record<RoutineExecutionStatus, string> = {
  pending: "border-border bg-secondary/50 text-muted-foreground",
  running: "border-blue-500/30 bg-blue-500/10 text-blue-700 dark:text-blue-300",
  completed: "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
  failed: "border-destructive/30 bg-destructive/10 text-destructive",
  skipped: "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300",
};

// ─── 表单状态 ───

interface RoutineFormState {
  name: string;
  prompt_template: string;
  agent_id: string;
  trigger_type: RoutineTriggerType;
  cron_expression: string;
  provider_key: string;
  provider_config_json: string;
  session_mode: RoutineSessionMode;
  entity_key_path: string;
}

const INITIAL_FORM: RoutineFormState = {
  name: "",
  prompt_template: "",
  agent_id: "",
  trigger_type: "scheduled",
  cron_expression: "*/10 * * * *",
  provider_key: "",
  provider_config_json: "{}",
  session_mode: "fresh",
  entity_key_path: "",
};

function routineToForm(r: Routine): RoutineFormState {
  return {
    name: r.name,
    prompt_template: r.prompt_template,
    agent_id: r.agent_id,
    trigger_type: r.trigger_config.type,
    cron_expression: r.trigger_config.cron_expression ?? "*/10 * * * *",
    provider_key: r.trigger_config.provider_key ?? "",
    provider_config_json: r.trigger_config.provider_config ? JSON.stringify(r.trigger_config.provider_config, null, 2) : "{}",
    session_mode: r.session_strategy.mode,
    entity_key_path: r.session_strategy.entity_key_path ?? "",
  };
}

function formToPayload(form: RoutineFormState): {
  name: string;
  prompt_template: string;
  agent_id: string;
  trigger_config: Record<string, unknown>;
  session_strategy: Record<string, unknown>;
} {
  let trigger_config: Record<string, unknown>;
  switch (form.trigger_type) {
    case "scheduled":
      trigger_config = { type: "scheduled", cron_expression: form.cron_expression };
      break;
    case "webhook":
      trigger_config = { type: "webhook" };
      break;
    case "plugin":
      trigger_config = {
        type: "plugin",
        provider_key: form.provider_key,
        provider_config: JSON.parse(form.provider_config_json || "{}"),
      };
      break;
  }

  const session_strategy: Record<string, unknown> = { mode: form.session_mode };
  if (form.session_mode === "per_entity" && form.entity_key_path.trim()) {
    session_strategy.entity_key_path = form.entity_key_path.trim();
  }

  return {
    name: form.name,
    prompt_template: form.prompt_template,
    agent_id: form.agent_id,
    trigger_config,
    session_strategy,
  };
}

function validateForm(form: RoutineFormState): string | null {
  if (!form.name.trim()) return "名称不能为空";
  if (!form.prompt_template.trim()) return "Prompt 模板不能为空";
  if (!form.agent_id) return "请选择执行 Agent";
  if (form.trigger_type === "scheduled" && !form.cron_expression.trim()) return "请配置定时表达式";
  if (form.trigger_type === "plugin" && !form.provider_key.trim()) return "请输入 provider_key";
  if (form.session_mode === "per_entity" && !form.entity_key_path.trim()) return "Per-Entity 模式需要指定 entity_key_path";
  return null;
}

// ─── RoutineCard ───

function RoutineCard({
  routine,
  agentLinks,
  onEdit,
  onToggleEnable,
  onViewHistory,
  onDelete,
}: {
  routine: Routine;
  agentLinks: ProjectAgentLink[];
  onEdit: () => void;
  onToggleEnable: () => void;
  onViewHistory: () => void;
  onDelete: () => void;
}) {
  const badge = TRIGGER_TYPE_BADGE[routine.trigger_config.type];
  const agentLink = agentLinks.find((l) => l.agent_id === routine.agent_id);
  const agentName = agentLink?.agent_name || routine.agent_id;

  const triggerDetail = (() => {
    switch (routine.trigger_config.type) {
      case "scheduled":
        return routine.trigger_config.cron_expression ?? "";
      case "webhook":
        return routine.trigger_config.endpoint_id ?? "";
      case "plugin":
        return routine.trigger_config.provider_key ?? "";
    }
  })();

  return (
    <article className="group rounded-[12px] border border-border bg-background/75 p-4 transition-colors hover:bg-background">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className={`inline-block h-2 w-2 shrink-0 rounded-full ${routine.enabled ? "bg-emerald-500" : "bg-muted-foreground/30"}`} />
            <h3 className="truncate text-sm font-medium text-foreground">{routine.name}</h3>
          </div>

          <div className="mt-2 flex flex-wrap items-center gap-1.5">
            <span className={`inline-block rounded-[6px] border px-2 py-0.5 text-[10px] ${badge.className}`}>
              {badge.label}
            </span>
            {triggerDetail && (
              <span className="rounded-[6px] border border-border bg-secondary/50 px-2 py-0.5 font-mono text-[10px] text-muted-foreground">
                {triggerDetail}
              </span>
            )}
            <span className="rounded-[6px] border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
              {agentName}
            </span>
          </div>

          <div className="mt-2 flex items-center gap-3 text-[11px] text-muted-foreground">
            <span>最近触发: {formatRelativeTime(routine.last_fired_at)}</span>
            <span>·</span>
            <span>{routine.enabled ? "已启用" : "已禁用"}</span>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          <button
            type="button"
            onClick={onToggleEnable}
            className={`rounded-[8px] border px-2.5 py-1 text-[11px] transition-colors ${
              routine.enabled
                ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 hover:bg-emerald-500/20 dark:text-emerald-300"
                : "border-border bg-secondary text-muted-foreground hover:bg-secondary/80"
            }`}
          >
            {routine.enabled ? "启用中" : "已禁用"}
          </button>
          <DetailMenu
            items={[
              { key: "edit", label: "编辑", onSelect: onEdit },
              { key: "history", label: "执行历史", onSelect: onViewHistory },
              { key: "delete", label: "删除", onSelect: onDelete, danger: true },
            ]}
          />
        </div>
      </div>
    </article>
  );
}

// ─── Webhook Token 一次性展示 ───

function WebhookTokenAlert({
  token,
  endpointId,
  routineName,
  onClose,
}: {
  token: string;
  endpointId: string;
  routineName: string;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const triggerUrl = `/api/routine-triggers/${endpointId}/fire`;

  const handleCopy = async () => {
    await navigator.clipboard.writeText(token);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/24 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-lg rounded-[16px] border border-amber-500/30 bg-background shadow-2xl">
          <div className="border-b border-amber-500/20 bg-amber-500/5 px-5 py-4">
            <span className="inline-block rounded-[6px] border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-amber-700 dark:text-amber-300">
              Webhook Token
            </span>
            <h4 className="mt-1 text-base font-semibold text-foreground">
              {routineName} — Token 仅此一次可见
            </h4>
          </div>
          <div className="space-y-4 p-5">
            <div>
              <p className="text-xs font-medium text-muted-foreground">触发端点</p>
              <code className="mt-1 block rounded-[8px] border border-border bg-secondary/50 px-3 py-2 font-mono text-xs text-foreground break-all">
                POST {triggerUrl}
              </code>
            </div>
            <div>
              <p className="text-xs font-medium text-muted-foreground">Bearer Token</p>
              <div className="mt-1 flex items-center gap-2">
                <code className="flex-1 rounded-[8px] border border-border bg-secondary/50 px-3 py-2 font-mono text-xs text-foreground break-all">
                  {token}
                </code>
                <button
                  type="button"
                  onClick={() => void handleCopy()}
                  className="agentdash-button-secondary shrink-0 text-xs"
                >
                  {copied ? "已复制" : "复制"}
                </button>
              </div>
            </div>
            <p className="rounded-[8px] border border-amber-500/20 bg-amber-500/5 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
              此 Token 不会再次展示，请立即复制并安全保管。
            </p>
          </div>
          <div className="flex items-center justify-end border-t border-border px-5 py-4">
            <button type="button" onClick={onClose} className="agentdash-button-primary">
              我已复制，关闭
            </button>
          </div>
        </div>
      </div>
    </>
  );
}

// ─── Create / Edit Dialog ───

function RoutineDialog({
  mode,
  initial,
  agentLinks,
  editingRoutine,
  onSave,
  onClose,
}: {
  mode: "create" | "edit";
  initial: RoutineFormState;
  agentLinks: ProjectAgentLink[];
  editingRoutine?: Routine;
  onSave: (payload: ReturnType<typeof formToPayload>) => Promise<void>;
  onClose: () => void;
}) {
  const [form, setForm] = useState<RoutineFormState>(initial);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [regeneratedToken, setRegeneratedToken] = useState<{ endpoint_id: string; token: string } | null>(null);
  const { regenerateToken } = useRoutineStore();

  const patchForm = (patch: Partial<RoutineFormState>) => setForm((prev) => ({ ...prev, ...patch }));

  const handleSubmit = async () => {
    const validationError = validateForm(form);
    if (validationError) {
      setError(validationError);
      return;
    }
    setError(null);
    setSaving(true);
    try {
      await onSave(formToPayload(form));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const handleRegenerate = async () => {
    if (!editingRoutine) return;
    const result = await regenerateToken(editingRoutine.id);
    if (result) {
      setRegeneratedToken({ endpoint_id: result.endpoint_id, token: result.webhook_token });
    }
  };

  const isWebhookEdit = mode === "edit" && editingRoutine?.trigger_config.type === "webhook";

  return (
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/18 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-2xl rounded-[16px] border border-border bg-background shadow-2xl">
          {/* Header */}
          <div className="border-b border-border px-5 py-4">
            <span className="agentdash-panel-header-tag">
              {mode === "create" ? "Create" : "Edit"}
            </span>
            <h4 className="text-base font-semibold text-foreground">
              {mode === "create" ? "创建 Routine" : "编辑 Routine"}
            </h4>
            <p className="mt-1 text-sm text-muted-foreground">
              配置触发规则，定时或通过 Webhook 自动启动 Agent 执行。
            </p>
          </div>

          {/* Body */}
          <div className="max-h-[70vh] space-y-5 overflow-y-auto p-5">
            {error && (
              <div className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
                {error}
              </div>
            )}

            {/* 名称 */}
            <div>
              <label className="agentdash-form-label">名称</label>
              <input
                value={form.name}
                onChange={(e) => patchForm({ name: e.target.value })}
                placeholder="如: daily-code-review"
                className="agentdash-form-input"
              />
            </div>

            {/* Agent 选择 */}
            <div>
              <label className="agentdash-form-label">执行 Agent</label>
              <select
                value={form.agent_id}
                onChange={(e) => patchForm({ agent_id: e.target.value })}
                className="agentdash-form-select"
              >
                <option value="">请选择 Agent</option>
                {agentLinks.map((link) => (
                  <option key={link.agent_id} value={link.agent_id}>
                    {link.agent_name}
                  </option>
                ))}
              </select>
            </div>

            {/* Prompt 模板 */}
            <div>
              <label className="agentdash-form-label">Prompt 模板</label>
              <textarea
                value={form.prompt_template}
                onChange={(e) => patchForm({ prompt_template: e.target.value })}
                placeholder={"支持 Tera/Jinja2 语法变量：\n{{ trigger.source }}、{{ trigger.payload.xxx }}、{{ routine.name }}"}
                rows={5}
                className="agentdash-form-textarea font-mono text-xs"
              />
              <p className="mt-1 text-[10px] text-muted-foreground">
                {"可用变量: {{ trigger.source }}, {{ trigger.timestamp }}, {{ trigger.payload.* }}, {{ routine.name }}, {{ routine.project_id }}"}
              </p>
            </div>

            {/* 触发类型 */}
            <div>
              <label className="agentdash-form-label">触发类型</label>
              <select
                value={form.trigger_type}
                onChange={(e) => patchForm({ trigger_type: e.target.value as RoutineTriggerType })}
                className="agentdash-form-select"
                disabled={mode === "edit"}
              >
                <option value="scheduled">定时 (Scheduled)</option>
                <option value="webhook">Webhook</option>
                <option value="plugin">Plugin</option>
              </select>
              {mode === "edit" && (
                <p className="mt-1 text-[10px] text-muted-foreground">触发类型创建后不可更改</p>
              )}
            </div>

            {/* 触发类型条件字段 */}
            {form.trigger_type === "scheduled" && (
              <div className="rounded-[10px] border border-border bg-secondary/20 p-4">
                <CronScheduleSelector
                  value={form.cron_expression}
                  onChange={(cron) => patchForm({ cron_expression: cron })}
                />
              </div>
            )}

            {form.trigger_type === "webhook" && mode === "create" && (
              <div className="rounded-[10px] border border-blue-500/20 bg-blue-500/5 p-4">
                <p className="text-xs text-muted-foreground">
                  Endpoint ID 和 Auth Token 将在创建时自动生成，Token 仅在创建成功后展示一次。
                </p>
              </div>
            )}

            {isWebhookEdit && editingRoutine && (
              <div className="rounded-[10px] border border-border bg-secondary/20 p-4 space-y-3">
                <div>
                  <p className="text-xs font-medium text-muted-foreground">触发端点</p>
                  <code className="mt-1 block font-mono text-xs text-foreground break-all">
                    POST /api/routine-triggers/{editingRoutine.trigger_config.endpoint_id}/fire
                  </code>
                </div>
                <button
                  type="button"
                  onClick={() => void handleRegenerate()}
                  className="agentdash-button-secondary text-xs"
                >
                  重新生成 Token
                </button>
                {regeneratedToken && (
                  <div className="rounded-[8px] border border-amber-500/20 bg-amber-500/5 p-3 space-y-1">
                    <p className="text-xs font-medium text-amber-700 dark:text-amber-300">新 Token（仅此一次可见）</p>
                    <code className="block font-mono text-xs text-foreground break-all">{regeneratedToken.token}</code>
                  </div>
                )}
              </div>
            )}

            {form.trigger_type === "plugin" && (
              <div className="space-y-3 rounded-[10px] border border-border bg-secondary/20 p-4">
                <div>
                  <label className="agentdash-form-label">Provider Key</label>
                  <input
                    value={form.provider_key}
                    onChange={(e) => patchForm({ provider_key: e.target.value })}
                    placeholder="如: github:pull_request"
                    className="agentdash-form-input font-mono"
                  />
                </div>
                <div>
                  <label className="agentdash-form-label">Provider Config (JSON)</label>
                  <textarea
                    value={form.provider_config_json}
                    onChange={(e) => patchForm({ provider_config_json: e.target.value })}
                    rows={4}
                    className="agentdash-form-textarea font-mono text-xs"
                  />
                </div>
              </div>
            )}

            {/* Session 策略 */}
            <div>
              <label className="agentdash-form-label">Session 策略</label>
              <select
                value={form.session_mode}
                onChange={(e) => patchForm({ session_mode: e.target.value as RoutineSessionMode })}
                className="agentdash-form-select"
              >
                <option value="fresh">每次新建 (Fresh)</option>
                <option value="reuse">复用已有 (Reuse)</option>
                <option value="per_entity">按实体分配 (Per Entity)</option>
              </select>
            </div>

            {form.session_mode === "per_entity" && (
              <div>
                <label className="agentdash-form-label">Entity Key Path</label>
                <input
                  value={form.entity_key_path}
                  onChange={(e) => patchForm({ entity_key_path: e.target.value })}
                  placeholder="如: pull_request.number"
                  className="agentdash-form-input font-mono"
                />
                <p className="mt-1 text-[10px] text-muted-foreground">
                  从 trigger payload 中按此 JSON 路径提取 entity key，相同 key 复用同一 session
                </p>
              </div>
            )}
          </div>

          {/* Footer */}
          <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
            <button type="button" onClick={onClose} className="agentdash-button-secondary">
              取消
            </button>
            <button
              type="button"
              onClick={() => void handleSubmit()}
              disabled={saving}
              className="agentdash-button-primary"
            >
              {saving ? "保存中..." : mode === "create" ? "创建" : "保存"}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}

// ─── 执行历史面板 ───

function ExecutionHistoryContent({ routineId }: { routineId: string }) {
  const navigate = useNavigate();
  const { executionsByRoutineId, fetchExecutions } = useRoutineStore();
  const executions = executionsByRoutineId[routineId] ?? [];
  const [loading, setLoading] = useState(false);

  // 切换 routine 时重新加载执行列表；setLoading 是 fetch 的配套副作用。
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setLoading(true);
    void fetchExecutions(routineId, 20, 0).finally(() => setLoading(false));
  }, [routineId, fetchExecutions]);

  const loadMore = () => {
    void fetchExecutions(routineId, 20, executions.length);
  };

  if (loading && executions.length === 0) {
    return <p className="py-8 text-center text-sm text-muted-foreground">加载中...</p>;
  }

  if (executions.length === 0) {
    return <p className="py-8 text-center text-sm text-muted-foreground">暂无执行记录</p>;
  }

  return (
    <div className="space-y-2 p-4">
      {executions.map((exec) => (
        <div key={exec.id} className="rounded-[10px] border border-border bg-background/75 p-3">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className={`inline-block rounded-[6px] border px-2 py-0.5 text-[10px] ${EXEC_STATUS_STYLE[exec.status]}`}>
                {exec.status}
              </span>
              <span className="text-xs text-muted-foreground">{exec.trigger_source}</span>
            </div>
            <span className="text-[10px] text-muted-foreground">
              {new Date(exec.started_at).toLocaleString()}
            </span>
          </div>
          {exec.error && (
            <p className="mt-2 rounded-[6px] bg-destructive/5 px-2 py-1 text-xs text-destructive">{exec.error}</p>
          )}
          {exec.session_id && (
            <button
              type="button"
              onClick={() => navigate(`/session/${exec.session_id}`)}
              className="mt-2 text-xs text-primary underline hover:no-underline"
            >
              查看 Session
            </button>
          )}
        </div>
      ))}
      <button
        type="button"
        onClick={loadMore}
        className="w-full rounded-[8px] border border-border py-2 text-xs text-muted-foreground transition-colors hover:bg-secondary"
      >
        加载更多
      </button>
    </div>
  );
}

// ─── RoutineTabView ───

export function RoutineTabView() {
  const { currentProjectId, agentLinksByProjectId, fetchProjectAgentLinks } = useProjectStore();
  const { routinesByProjectId, fetchRoutines, createRoutine, updateRoutine, deleteRoutine, enableRoutine } =
    useRoutineStore();

  const [showCreate, setShowCreate] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [historyId, setHistoryId] = useState<string | null>(null);
  const [tokenAlert, setTokenAlert] = useState<{ token: string; endpointId: string; name: string } | null>(null);

  const routines = currentProjectId ? routinesByProjectId[currentProjectId] ?? [] : [];
  const agentLinks: ProjectAgentLink[] = currentProjectId
    ? agentLinksByProjectId[currentProjectId] ?? []
    : [];
  const editingRoutine = editingId ? routines.find((r) => r.id === editingId) : undefined;
  const deletingRoutine = deletingId ? routines.find((r) => r.id === deletingId) : undefined;
  const historyRoutine = historyId ? routines.find((r) => r.id === historyId) : undefined;

  useEffect(() => {
    if (currentProjectId) {
      void fetchRoutines(currentProjectId);
      void fetchProjectAgentLinks(currentProjectId);
    }
  }, [currentProjectId, fetchRoutines, fetchProjectAgentLinks]);

  const handleCreate = useCallback(
    async (payload: ReturnType<typeof formToPayload>) => {
      if (!currentProjectId) return;
      const result = await createRoutine(currentProjectId, payload as Parameters<typeof createRoutine>[1]);
      if (result) {
        setShowCreate(false);
        // Webhook 创建后展示 token
        if (result.webhook_token && result.trigger_config?.endpoint_id) {
          setTokenAlert({
            token: result.webhook_token,
            endpointId: result.trigger_config.endpoint_id,
            name: result.name,
          });
        }
      }
    },
    [currentProjectId, createRoutine],
  );

  const handleUpdate = useCallback(
    async (payload: ReturnType<typeof formToPayload>) => {
      if (!editingId) return;
      const result = await updateRoutine(editingId, payload);
      if (result) setEditingId(null);
    },
    [editingId, updateRoutine],
  );

  const handleDelete = useCallback(async () => {
    if (!deletingId || !currentProjectId) return;
    const ok = await deleteRoutine(deletingId, currentProjectId);
    if (ok) setDeletingId(null);
  }, [deletingId, currentProjectId, deleteRoutine]);

  const handleToggleEnable = useCallback(
    async (routine: Routine) => {
      if (!currentProjectId) return;
      await enableRoutine(routine.id, !routine.enabled, currentProjectId);
    },
    [currentProjectId, enableRoutine],
  );

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-sm text-muted-foreground">请先选择项目</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <header className="flex items-center justify-between border-b border-border px-6 py-4">
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-semibold text-foreground">Routine</h2>
          {routines.length > 0 && (
            <span className="rounded-[6px] border border-border bg-secondary px-2 py-0.5 text-[10px] text-muted-foreground">
              {routines.length}
            </span>
          )}
        </div>
        <button
          type="button"
          onClick={() => setShowCreate(true)}
          className="agentdash-button-primary text-sm"
        >
          创建 Routine
        </button>
      </header>

      {/* Content */}
      <main className="flex-1 overflow-y-auto">
        {routines.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
            <p className="text-sm text-muted-foreground">暂无 Routine</p>
            <p className="text-xs text-muted-foreground">
              创建 Routine 来定时触发 Agent 或通过 Webhook 接收外部事件
            </p>
            <button
              type="button"
              onClick={() => setShowCreate(true)}
              className="agentdash-button-secondary mt-2 text-sm"
            >
              创建第一个 Routine
            </button>
          </div>
        ) : (
          <div className="space-y-3 p-4">
            {routines.map((routine) => (
              <RoutineCard
                key={routine.id}
                routine={routine}
                agentLinks={agentLinks}
                onEdit={() => setEditingId(routine.id)}
                onToggleEnable={() => void handleToggleEnable(routine)}
                onViewHistory={() => setHistoryId(routine.id)}
                onDelete={() => setDeletingId(routine.id)}
              />
            ))}
          </div>
        )}
      </main>

      {/* Create Dialog */}
      {showCreate && (
        <RoutineDialog
          mode="create"
          initial={{ ...INITIAL_FORM, agent_id: agentLinks[0]?.agent_id ?? "" }}
          agentLinks={agentLinks}
          onSave={handleCreate}
          onClose={() => setShowCreate(false)}
        />
      )}

      {/* Edit Dialog */}
      {editingRoutine && (
        <RoutineDialog
          mode="edit"
          initial={routineToForm(editingRoutine)}
          agentLinks={agentLinks}
          editingRoutine={editingRoutine}
          onSave={handleUpdate}
          onClose={() => setEditingId(null)}
        />
      )}

      {/* Delete Confirm */}
      <DangerConfirmDialog
        open={!!deletingRoutine}
        title={`删除 Routine「${deletingRoutine?.name ?? ""}」`}
        description="删除后不可恢复，关联的执行记录也将被清除。"
        confirmLabel="确认删除"
        onClose={() => setDeletingId(null)}
        onConfirm={() => void handleDelete()}
      />

      {/* Execution History Panel */}
      <DetailPanel
        open={!!historyRoutine}
        title={`执行历史 — ${historyRoutine?.name ?? ""}`}
        onClose={() => setHistoryId(null)}
      >
        {historyId && <ExecutionHistoryContent routineId={historyId} />}
      </DetailPanel>

      {/* Webhook Token Alert */}
      {tokenAlert && (
        <WebhookTokenAlert
          token={tokenAlert.token}
          endpointId={tokenAlert.endpointId}
          routineName={tokenAlert.name}
          onClose={() => setTokenAlert(null)}
        />
      )}
    </div>
  );
}
