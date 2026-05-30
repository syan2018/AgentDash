import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useSettingsStore } from "../../../stores/settingsStore";
import { useCoordinatorStore } from "../../../stores/coordinatorStore";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import { useProjectStore } from "../../../stores/projectStore";
import type { SettingUpdate, SettingsScopeRequest } from "../../../api/settings";
import { getStoredToken } from "../../../api/client";
import { API_ORIGIN } from "../../../api/origin";
import { LocalRuntimeView } from "@agentdash/views/local-runtime";
import { getDesktopLocalRuntimeClient, getDesktopBrowseDirectory } from "../../../desktop/localRuntimeBridge";
import { DebugPrefsSection } from "./DebugPrefsSection";
import { UserByokSection } from "./UserByokSection";
import { SectionCard } from "./primitives";
import { LlmProvidersSection } from "./LlmProvidersSection";
import { AgentSection, BackendSection, ExecutorSection } from "./SettingsSystemSections";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** 从 store 中读取某个 key 的显示值 */
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
