import { useMemo, useState } from "react";
import { Badge, Button, Select, StatusDot, Textarea } from "@agentdash/ui";

import type { SettingUpdate } from "../../../api/settings";
import type { BackendConfig, BackendRuntimeSummary } from "../../../types";
import { useExecutorDiscovery } from "../../executor-selector";
import { btnPrimaryCls, Field, SectionCard } from "./primitives";

interface SettingEntry {
  key: string;
  value: unknown;
  masked: boolean;
}

interface SettingsSectionProps {
  settings: SettingEntry[];
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}

type BackendRuntimeHealth = NonNullable<BackendConfig["runtime_health"]>;
type BackendExecutor = NonNullable<BackendConfig["capabilities"]>["executors"][number];

const PI_AGENT_PREFERENCES_KEY = "agent.pi.user_preferences";
const DEFAULT_EXECUTOR_KEY = "executor.default.executor";
const DEFAULT_EXECUTOR_ID = "PI_AGENT";

function readVal(settings: SettingEntry[], key: string, fallback = ""): string {
  const entry = settings.find((setting) => setting.key === key);
  if (entry?.value === null || entry?.value === undefined) return fallback;
  return String(entry.value);
}

function readStringList(settings: SettingEntry[], key: string): string[] {
  const entry = settings.find((setting) => setting.key === key);
  if (!Array.isArray(entry?.value)) return [];
  return entry.value.filter((value): value is string => typeof value === "string" && value.trim() !== "");
}

export function PiAgentPreferencesSection({ settings, saving, onSave }: SettingsSectionProps) {
  const initialPreferences = useMemo(
    () => readStringList(settings, PI_AGENT_PREFERENCES_KEY),
    [settings],
  );

  return (
    <PiAgentPreferencesForm
      key={JSON.stringify(initialPreferences)}
      initialPreferences={initialPreferences}
      saving={saving}
      onSave={onSave}
    />
  );
}

interface PiAgentPreferencesFormProps {
  initialPreferences: string[];
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}

function PiAgentPreferencesForm({
  initialPreferences,
  saving,
  onSave,
}: PiAgentPreferencesFormProps) {
  const [preferences, setPreferences] = useState<string[]>(
    initialPreferences.length > 0 ? initialPreferences : [""],
  );

  const handleChange = (index: number, value: string) => {
    setPreferences((current) => current.map((item, itemIndex) => (itemIndex === index ? value : item)));
  };

  const handleAdd = () => setPreferences((current) => [...current, ""]);

  const handleRemove = (index: number) => {
    setPreferences((current) => {
      const next = current.filter((_, itemIndex) => itemIndex !== index);
      return next.length > 0 ? next : [""];
    });
  };

  const handleSave = () => {
    const cleaned = preferences.map((preference) => preference.trim()).filter((preference) => preference !== "");
    onSave([{ key: PI_AGENT_PREFERENCES_KEY, value: cleaned }]);
  };

  return (
    <SectionCard title="Pi Agent">
      <Field label="User Preferences" desc="用户偏好提示（每条独立生效，会附加到系统提示末尾）">
        <div className="flex flex-col gap-2">
          {preferences.map((preference, index) => (
            <PreferenceInputRow
              key={index}
              index={index}
              value={preference}
              onChange={handleChange}
              onRemove={handleRemove}
            />
          ))}
          <Button className="self-start" size="sm" onClick={handleAdd}>
            + 添加偏好
          </Button>
        </div>
      </Field>

      <div className="flex justify-end pt-1">
        <button type="button" disabled={saving} className={btnPrimaryCls} onClick={handleSave}>
          {saving ? "保存中..." : "保存"}
        </button>
      </div>
    </SectionCard>
  );
}

interface PreferenceInputRowProps {
  index: number;
  value: string;
  onChange: (index: number, value: string) => void;
  onRemove: (index: number) => void;
}

function PreferenceInputRow({ index, value, onChange, onRemove }: PreferenceInputRowProps) {
  return (
    <div className="flex items-start gap-2">
      <Textarea
        className="min-h-[60px] flex-1"
        value={value}
        onChange={(event) => onChange(index, event.target.value)}
        rows={2}
        placeholder={`偏好 ${index + 1}，例如"请用中文回复"或"优先使用函数式风格"`}
      />
      <Button
        aria-label="删除此条偏好"
        className="mt-1"
        size="sm"
        variant="danger"
        onClick={() => onRemove(index)}
      >
        ×
      </Button>
    </div>
  );
}

export function ExecutorSection({ settings, saving, onSave }: SettingsSectionProps) {
  const { executors, isLoading } = useExecutorDiscovery();
  const currentExecutor = readVal(settings, DEFAULT_EXECUTOR_KEY) || DEFAULT_EXECUTOR_ID;

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

interface ExecutorSectionFormProps {
  executors: Array<{ id: string; name: string; available: boolean }>;
  isLoading: boolean;
  currentExecutor: string;
  saving: boolean;
  onSave: (updates: SettingUpdate[]) => void;
}

function ExecutorSectionForm({
  executors,
  isLoading,
  currentExecutor,
  saving,
  onSave,
}: ExecutorSectionFormProps) {
  const [executor, setExecutor] = useState(currentExecutor);
  const availableExecutors = useMemo(
    () => executors.filter((candidate) => candidate.available),
    [executors],
  );

  const handleSave = () => {
    onSave([{ key: DEFAULT_EXECUTOR_KEY, value: executor }]);
  };

  return (
    <SectionCard title="默认 Executor">
      <Field label="执行器" desc="选择默认使用的执行器（新会话或没有显式绑定时使用）">
        <Select
          value={executor}
          onChange={(event) => setExecutor(event.target.value)}
          disabled={isLoading}
        >
          <option value="">{isLoading ? "加载中..." : "选择执行器..."}</option>
          {availableExecutors.map((info) => (
            <option key={info.id} value={info.id}>
              {info.name}
            </option>
          ))}
        </Select>
      </Field>

      <div className="flex justify-end pt-1">
        <button type="button" disabled={saving} className={btnPrimaryCls} onClick={handleSave}>
          {saving ? "保存中..." : "保存"}
        </button>
      </div>
    </SectionCard>
  );
}

interface BackendSectionProps {
  backends: BackendConfig[];
  runtimeSummaries: BackendRuntimeSummary[];
  onRemove: (id: string) => void;
}

interface BackendViewModel {
  backend: BackendConfig;
  runtimeSummary: BackendRuntimeSummary | undefined;
  runtimeHealth: BackendConfig["runtime_health"];
  executors: BackendExecutor[];
  availableExecutors: BackendExecutor[];
  roots: string[];
  machineLabel: string;
  scopeLabel: string;
  typeLabel: string;
  summaryText: string;
}

export function BackendSection({ backends, runtimeSummaries, onRemove }: BackendSectionProps) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const summaryByBackend = useMemo(
    () => new Map(runtimeSummaries.map((summary) => [summary.backend_id, summary])),
    [runtimeSummaries],
  );
  const backendViews = useMemo(
    () => backends.map((backend) => createBackendViewModel(backend, summaryByBackend.get(backend.id))),
    [backends, summaryByBackend],
  );
  const onlineCount = useMemo(
    () => backends.filter((backend) => backend.online).length,
    [backends],
  );

  const toggleBackend = (id: string) => {
    setExpandedId((current) => (current === id ? null : id));
  };

  return (
    <SectionCard title="后端管理">
      <p className="text-xs text-muted-foreground">
        共 {backends.length} 个后端，{onlineCount} 个在线
      </p>

      {backends.length === 0 ? (
        <p className="rounded-[8px] border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
          暂无已注册后端
        </p>
      ) : (
        <div className="space-y-2">
          {backendViews.map((view) => (
            <BackendListItem
              key={view.backend.id}
              view={view}
              expanded={expandedId === view.backend.id}
              onToggle={toggleBackend}
              onRemove={onRemove}
            />
          ))}
        </div>
      )}
    </SectionCard>
  );
}

interface BackendListItemProps {
  view: BackendViewModel;
  expanded: boolean;
  onToggle: (id: string) => void;
  onRemove: (id: string) => void;
}

function BackendListItem({ view, expanded, onToggle, onRemove }: BackendListItemProps) {
  const { backend } = view;

  return (
    <div className="rounded-[8px] border border-border bg-background/80">
      <button
        type="button"
        className="flex w-full items-center gap-3 px-4 py-3 text-left"
        onClick={() => onToggle(backend.id)}
      >
        <StatusDot tone={backend.online ? "success" : "muted"} size="md" />
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-foreground">{backend.name}</p>
          <p className="text-xs text-muted-foreground">{view.summaryText}</p>
        </div>
        <Badge variant={backend.backend_type === "local" ? "primary" : "neutral"}>{view.typeLabel}</Badge>
        <ChevronIcon expanded={expanded} />
      </button>

      {expanded && <BackendDetails view={view} onRemove={onRemove} />}
    </div>
  );
}

function ChevronIcon({ expanded }: { expanded: boolean }) {
  return (
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
      aria-hidden="true"
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function BackendDetails({
  view,
  onRemove,
}: {
  view: BackendViewModel;
  onRemove: (id: string) => void;
}) {
  const { backend, runtimeHealth } = view;

  return (
    <div className="space-y-3 border-t border-border px-4 pb-4 pt-3">
      <BackendDetailGrid view={view} />

      {runtimeHealth?.disconnect_reason && !backend.online && (
        <p className="rounded-[8px] border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          断开原因：{runtimeHealth.disconnect_reason}
        </p>
      )}

      <ExecutorBadgeList executors={view.executors} availableExecutors={view.availableExecutors} />
      <WorkspaceRootList roots={view.roots} />

      {!backend.online && (
        <div className="flex justify-end pt-1">
          <Button variant="danger" size="sm" onClick={() => onRemove(backend.id)}>
            移除
          </Button>
        </div>
      )}
    </div>
  );
}

function BackendDetailGrid({ view }: { view: BackendViewModel }) {
  const { backend, runtimeSummary, runtimeHealth } = view;

  return (
    <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
      <DetailRow label="ID" value={backend.id} mono title={backend.id} />
      <DetailRow label="状态" value={backend.online ? "在线" : "离线"} tone={backend.online ? "success" : "muted"} />
      <DetailRow label="类型" value={view.typeLabel} />
      {runtimeSummary && (
        <>
          <DetailRow label="执行占用" value={`${runtimeSummary.active_session_count} 个活跃会话`} />
          <DetailRow
            label="自动分配"
            value={runtimeSummary.allocatable ? "可分配" : "不可分配"}
            tone={runtimeSummary.allocatable ? "success" : "muted"}
          />
        </>
      )}
      {backend.backend_type === "local" && (
        <>
          <DetailRow label="机器" value={view.machineLabel} title={backend.machine_id ?? undefined} />
          <DetailRow label="Scope" value={view.scopeLabel} />
          <DetailRow label="能力槽" value={backend.capability_slot || "default"} mono />
        </>
      )}
      {runtimeHealth && (
        <>
          <DetailRow label="Runtime" value={runtimeStatusLabel(runtimeHealth.status)} />
          <DetailRow label="版本" value={runtimeHealth.version ?? "-"} mono title={runtimeHealth.version ?? undefined} />
          <DetailRow label="Last seen" value={formatRuntimeTimestamp(runtimeHealth.last_seen_at)} />
        </>
      )}
    </div>
  );
}

interface DetailRowProps {
  label: string;
  value: string;
  mono?: boolean;
  title?: string;
  tone?: "success" | "muted";
}

function DetailRow({ label, value, mono = false, title, tone }: DetailRowProps) {
  const valueClass = tone === "success"
    ? "text-success"
    : tone === "muted"
      ? "text-muted-foreground"
      : "text-foreground";

  return (
    <>
      <span className="text-muted-foreground">{label}</span>
      <span className={`truncate ${mono ? "font-mono" : ""} ${valueClass}`} title={title}>
        {value}
      </span>
    </>
  );
}

function ExecutorBadgeList({
  executors,
  availableExecutors,
}: {
  executors: BackendExecutor[];
  availableExecutors: BackendExecutor[];
}) {
  if (executors.length === 0) return null;

  return (
    <div>
      <p className="mb-1.5 text-xs font-medium text-muted-foreground">
        执行器 ({availableExecutors.length}/{executors.length} 可用)
      </p>
      <div className="flex flex-wrap gap-1.5">
        {executors.map((executor) => (
          <Badge
            key={executor.id}
            variant={executor.available ? "success" : "neutral"}
            className={executor.available ? undefined : "line-through"}
          >
            {executor.name}
          </Badge>
        ))}
      </div>
    </div>
  );
}

function WorkspaceRootList({ roots }: { roots: string[] }) {
  if (roots.length === 0) return null;

  return (
    <div>
      <p className="mb-1 text-xs font-medium text-muted-foreground">可访问路径</p>
      {roots.map((root) => {
        const displayRoot = root.replace(/^\\\\\?\\/, "");
        return (
          <p key={root} className="truncate text-xs text-foreground" title={root}>
            {displayRoot}
          </p>
        );
      })}
    </div>
  );
}

function createBackendViewModel(
  backend: BackendConfig,
  runtimeSummary: BackendRuntimeSummary | undefined,
): BackendViewModel {
  const executors = backend.capabilities?.executors ?? [];
  const availableExecutors = executors.filter((executor) => executor.available);
  const runtimeHealth = backend.runtime_health;
  const roots = backend.workspace_roots ?? runtimeHealth?.workspace_roots ?? [];
  const machineLabel = backend.machine_label || machineLabelFromDevice(backend.device) || backend.name;
  const scopeLabel = formatBackendScope(backend);
  const typeLabel = backend.backend_type === "local" ? "本机" : "远程";

  return {
    backend,
    runtimeSummary,
    runtimeHealth,
    executors,
    availableExecutors,
    roots,
    machineLabel,
    scopeLabel,
    typeLabel,
    summaryText: backendSummaryText({
      backend,
      runtimeSummary,
      availableExecutorCount: availableExecutors.length,
      machineLabel,
      scopeLabel,
    }),
  };
}

function backendSummaryText({
  backend,
  runtimeSummary,
  availableExecutorCount,
  machineLabel,
  scopeLabel,
}: {
  backend: BackendConfig;
  runtimeSummary: BackendRuntimeSummary | undefined;
  availableExecutorCount: number;
  machineLabel: string;
  scopeLabel: string;
}) {
  if (backend.backend_type === "local") {
    return `${machineLabel} · ${scopeLabel}`;
  }
  if (runtimeSummary) {
    return `${runtimeSummary.active_session_count} 个活跃会话 · ${runtimeSummary.allocatable ? "可分配" : "不可分配"}`;
  }
  if (backend.online) {
    return `${availableExecutorCount} 个执行器可用`;
  }
  return "远程 · 离线";
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

function runtimeStatusLabel(status: BackendRuntimeHealth["status"]) {
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
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}
