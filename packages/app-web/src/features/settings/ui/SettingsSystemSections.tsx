import { useMemo, useState } from "react";

import type { SettingUpdate } from "../../../api/settings";
import type { BackendConfig, BackendRuntimeSummary } from "../../../types";
import { useExecutorDiscovery } from "../../executor-selector";
import { btnPrimaryCls, Field, inputCls, SectionCard } from "./primitives";

function readVal(settings: { key: string; value: unknown }[], key: string, fallback = ""): string {
  const entry = settings.find((s) => s.key === key);
  if (entry === undefined || entry.value === null || entry.value === undefined) return fallback;
  return String(entry.value);
}

export function PiAgentPreferencesSection({
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

export function ExecutorSection({
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

export function BackendSection({
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
  if (device == null || typeof device !== "object" || Array.isArray(device)) return null;
  const hostname = device.hostname;
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
