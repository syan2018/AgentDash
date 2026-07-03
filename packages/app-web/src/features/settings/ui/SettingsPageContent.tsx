import { useEffect, useState, useCallback, useMemo } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useSettingsStore } from "../../../stores/settingsStore";
import { useCoordinatorStore } from "../../../stores/coordinatorStore";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import { useEventStore, type EventConnectionState } from "../../../stores/eventStore";
import type { SettingUpdate, SettingsScopeRequest } from "../../../api/settings";
import { getStoredToken } from "../../../api/client";
import { API_ORIGIN } from "../../../api/origin";
import { LocalRuntimeView } from "@agentdash/views/local-runtime";
import { getDesktopAppBridge, getDesktopLocalRuntimeClient, getDesktopBrowseDirectory } from "../../../desktop/localRuntimeBridge";
import {
  ensureDesktopDefaultsLoaded,
  resolveDefaultLocalRuntimeServerUrl,
  subscribeDesktopDefaults,
} from "../../../desktop/defaults";
import { DebugPrefsSection } from "./DebugPrefsSection";
import { UserByokSection } from "./UserByokSection";
import { SectionCard } from "./primitives";
import { LlmProvidersSection } from "./LlmProvidersSection";
import { BackendSection, ExecutorSection, PiAgentPreferencesSection } from "./SettingsSystemSections";
import {
  backendDiagnosticsFacts,
  createCloudApiDiagnosticsInput,
  runtimeSummaryDiagnosticsFacts,
} from "../model/runtimeDiagnostics";
import type { BackendConfig, BackendRuntimeSummary } from "../../../types";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type SettingsScopeKind = SettingsScopeRequest["scope"];
type SettingsPanel = Exclude<SettingsScopeKind, "project"> | "local-runtime";

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

function DesktopLocalRuntimePanel({
  backends,
  runtimeSummaries,
  cloudApiError,
  cloudApiChecking,
  eventConnectionState,
}: {
  backends: BackendConfig[];
  runtimeSummaries: BackendRuntimeSummary[];
  cloudApiError: string | null;
  cloudApiChecking: boolean;
  eventConnectionState: EventConnectionState;
}) {
  const client = useMemo(() => getDesktopLocalRuntimeClient(), []);
  const browseDirectory = useMemo(() => getDesktopBrowseDirectory(), []);
  const desktopApp = useMemo(() => getDesktopAppBridge(), []);
  const [defaultServerUrl, setDefaultServerUrl] = useState(() => resolveDefaultLocalRuntimeServerUrl());

  useEffect(() => {
    let alive = true;
    const refresh = () => setDefaultServerUrl(resolveDefaultLocalRuntimeServerUrl());
    const unsubscribe = subscribeDesktopDefaults(refresh);
    ensureDesktopDefaultsLoaded()
      .then(() => {
        if (alive) refresh();
      })
      .catch(() => {
        if (alive) refresh();
      });
    return () => {
      alive = false;
      unsubscribe();
    };
  }, []);

  if (!client) return null;

  return (
    <LocalRuntimeView
      client={client}
      onBrowseDirectory={browseDirectory}
      desktopApp={desktopApp ?? undefined}
      diagnosticsContext={{
        cloud_api: createCloudApiDiagnosticsInput({
          apiError: cloudApiError,
          isChecking: cloudApiChecking,
          target: API_ORIGIN || "http://127.0.0.1:17301",
          eventConnectionState,
        }),
        backends: backendDiagnosticsFacts(backends),
        runtime_summaries: runtimeSummaryDiagnosticsFacts(runtimeSummaries),
      }}
      defaultAccessToken={getStoredToken() ?? ""}
      defaultServerUrl={defaultServerUrl}
    />
  );
}

/** 从 store 中读取某个 key 的显示值 */
function ScopeTabs({
  activePanel,
  includeLocalRuntime,
  includeSystemScope,
  onChange,
}: {
  activePanel: SettingsPanel;
  includeLocalRuntime: boolean;
  includeSystemScope: boolean;
  onChange: (scope: SettingsPanel) => void;
}) {
  const panels: SettingsPanel[] = [];
  if (includeSystemScope) panels.push("system");
  panels.push("user");
  if (includeLocalRuntime) panels.push("local-runtime");
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
    isLoading: coordinatorLoading,
    error: coordinatorError,
    fetchBackends,
    fetchBackendRuntimeSummaries,
    removeBackend,
  } = useCoordinatorStore();
  const { currentUser } = useCurrentUserStore();
  const eventConnectionState = useEventStore((state) => state.connectionState);
  const [activePanel, setActivePanel] = useState<SettingsPanel>("user");
  const [toast, setToast] = useState<string | null>(null);
  const [llmDiscoveryRefreshKey, setLlmDiscoveryRefreshKey] = useState(0);
  const routeState = (location.state as SettingsNavigationState | null) ?? null;
  const returnTarget = routeState?.return_to?.trim() || "/dashboard/agent";
  const includeLocalRuntime = !!getDesktopLocalRuntimeClient();

  const canManageSystemScope = currentUser?.auth_mode === "personal" || currentUser?.is_admin === true;
  const effectiveActivePanel = activePanel === "system" && !canManageSystemScope ? "user" : activePanel;
  const scopeRequest = useMemo<SettingsScopeRequest | null>(() => {
    if (effectiveActivePanel === "local-runtime") {
      return null;
    }
    if (effectiveActivePanel === "system") {
      return canManageSystemScope ? { scope: "system" } : null;
    }
    if (effectiveActivePanel === "user") {
      return { scope: "user" };
    }
    return null;
  }, [effectiveActivePanel, canManageSystemScope]);

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
              管理当前账号可用的配置和偏好。
            </p>
          </div>
        </div>

        <SectionCard title="Scope">
          <ScopeTabs
            activePanel={effectiveActivePanel}
            includeLocalRuntime={includeLocalRuntime}
            includeSystemScope={canManageSystemScope}
            onChange={setActivePanel}
          />
          <div className="space-y-1 text-xs text-muted-foreground">
            <p>当前 scope：{SETTINGS_PANEL_LABELS[effectiveActivePanel]}</p>
            {effectiveActivePanel === "local-runtime" && (
              <p>本机运行时是 desktop-only scope，只管理当前桌面端与目标 server 的本机连接、根目录、能力和诊断日志。</p>
            )}
            {effectiveActivePanel === "system" && canManageSystemScope && (
              <p>system scope 仅 personal 模式或管理员可访问，适合放全局执行器、LLM Provider 和系统级 Agent 配置。</p>
            )}
            {effectiveActivePanel === "user" && (
              <p>用户设置绑定当前登录用户，包含个人偏好和本地调试选项，不会影响其他用户。</p>
            )}
          </div>
        </SectionCard>

        {error && (
          <div className="rounded-[8px] border border-destructive/50 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}

        {effectiveActivePanel === "local-runtime" && (
          <DesktopLocalRuntimePanel
            backends={backends}
            runtimeSummaries={backendRuntimeSummaries}
            cloudApiError={coordinatorError ?? error}
            cloudApiChecking={coordinatorLoading || loading}
            eventConnectionState={eventConnectionState}
          />
        )}

        {effectiveActivePanel === "system" && canManageSystemScope && (
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
            <ExecutorSection settings={settings} saving={saving} onSave={handleSave} />
          </>
        )}

        {effectiveActivePanel === "user" && scopeRequest && (
          <>
            <PiAgentPreferencesSection settings={settings} saving={saving} onSave={handleSave} />
            <UserByokSection onRefreshModels={() => setLlmDiscoveryRefreshKey((k) => k + 1)} />
            <DebugPrefsSection />
          </>
        )}

      </div>

      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
