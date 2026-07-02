import { useMemo, useState } from "react";
import type { CapabilityKey, ThinkingLevel } from "../../../types";
import {
  THINKING_LEVEL_OPTIONS,
  CAPABILITY_OPTIONS,
  directivePath,
  parseCapabilityPath,
} from "../../../types";
import { useExecutorDiscovery, useExecutorDiscoveredOptions } from "../../executor-selector";
import type { ModelInfo, PermissionPolicy } from "../../executor-selector";
import { CapabilityPicker } from "./capability-picker";
import { KnowledgeSection } from "./knowledge-section";
import { McpPresetPicker } from "./mcp-preset-picker";
import { SkillAssetPicker } from "./skill-asset-picker";
import { ToolCapabilitiesField } from "./tool-capabilities-field";
import {
  selectedMcpPresetKeysFromDirectives,
  type PresetFormState,
} from "./form-state";
import { ProjectVfsMountExposurePicker } from "./project-vfs-mount-exposure-picker";
import { WorkspaceModuleVisibilityPicker } from "./workspace-module-visibility-picker";

const WELL_KNOWN_CAPABILITY_KEYS = new Set<CapabilityKey>(
  CAPABILITY_OPTIONS.map((option) => option.value),
);

// eslint-disable-next-line react-refresh/only-export-components
export function useAgentTypeOptions() {
  const { executors, isLoading } = useExecutorDiscovery();
  const options = useMemo(() => {
    return executors.map((executor) => ({
      value: executor.id,
      label: `${executor.name}${!executor.available ? " (不可用)" : ""}`,
    }));
  }, [executors]);
  return { agentTypeOptions: options, isDiscoveryLoading: isLoading };
}

export function PresetFormFields({
  form,
  patchForm,
  agentTypeOptions,
  isDiscoveryLoading,
  siblingAgents,
  projectId,
  knowledgeEnabled,
  onToggleKnowledge,
  knowledgeAgentId,
}: {
  form: PresetFormState;
  patchForm: (patch: Partial<PresetFormState>) => void;
  agentTypeOptions: Array<{ value: string; label: string }>;
  isDiscoveryLoading: boolean;
  siblingAgents?: Array<{ name: string; display_name: string; default_companion_enabled?: boolean }>;
  projectId?: string;
  knowledgeEnabled?: boolean;
  onToggleKnowledge?: (enabled: boolean) => void;
  knowledgeAgentId?: string;
}) {
  const [activeTab, setActiveTab] = useState<'basic' | 'capability' | 'memory'>('basic');
  const [activeCapability, setActiveCapability] = useState<'tool' | 'mcp' | 'vfs' | 'skill' | 'module' | 'companion'>('tool');
  const discovered = useExecutorDiscoveredOptions(form.agent_type);
  const modelSelector = discovered.options?.model_selector ?? null;
  const isModelLoading = !discovered.isInitialized || (discovered.options?.loading_models ?? false);

  const providersById = useMemo(() => {
    const map = new Map<string, string>();
    for (const p of modelSelector?.providers ?? []) {
      map.set(p.id, p.name);
    }
    return map;
  }, [modelSelector]);

  const modelsByProvider = useMemo(() => {
    const out = new Map<string, ModelInfo[]>();
    for (const m of modelSelector?.models ?? []) {
      if (m.blocked) continue;
      const pid = m.provider_id ?? "";
      const list = out.get(pid) ?? [];
      list.push(m);
      out.set(pid, list);
    }
    for (const list of out.values()) {
      list.sort((a, b) => a.name.localeCompare(b.name));
    }
    return out;
  }, [modelSelector]);
  const visibleModelCount = useMemo(() => {
    return [...modelsByProvider.values()].reduce((total, models) => total + models.length, 0);
  }, [modelsByProvider]);

  const selectedModelOptionValue = useMemo(() => {
    const trimmedModelId = form.model_id.trim();
    if (!trimmedModelId) return "";
    return `${form.provider_id.trim()}::${trimmedModelId}`;
  }, [form.model_id, form.provider_id]);

  const hasModelInDiscovery = useMemo(() => {
    if (!selectedModelOptionValue) return false;
    return [...modelsByProvider.values()].flat().some(
      (m) => `${m.provider_id ?? ""}::${m.id}` === selectedModelOptionValue,
    );
  }, [modelsByProvider, selectedModelOptionValue]);

  const selectedModel = useMemo(() => {
    const id = form.model_id.trim();
    if (!id) return null;
    const pid = form.provider_id.trim();
    return (modelSelector?.models ?? []).find(
      (m) => m.id === id && (pid ? (m.provider_id ?? "") === pid : true),
    ) ?? null;
  }, [modelSelector, form.model_id, form.provider_id]);

  const showThinkingSelector = !selectedModel || selectedModel.reasoning === true;
  const agents = modelSelector?.agents ?? [];
  const permissions = modelSelector?.permissions ?? [];

  const handleAgentTypeChange = (newType: string) => {
    patchForm({
      agent_type: newType,
      provider_id: "",
      model_id: "",
      agent_id: "",
    });
  };

  const handleModelChange = (value: string) => {
    if (!value) {
      patchForm({ provider_id: "", model_id: "" });
      return;
    }
    const sep = value.indexOf("::");
    const nextProviderId = sep >= 0 ? value.slice(0, sep) : "";
    const nextModelId = sep >= 0 ? value.slice(sep + 2) : value;
    patchForm({ provider_id: nextProviderId, model_id: nextModelId });
  };

  const extraCompanionCandidates = useMemo(
    () => (siblingAgents ?? []).filter((a) => a.name !== form.name && a.default_companion_enabled !== true),
    [form.name, siblingAgents],
  );
  const companionCount = extraCompanionCandidates.length;
  const selectedMcpPresetKeys = useMemo(
    () => selectedMcpPresetKeysFromDirectives(form.capability_directives),
    [form.capability_directives],
  );
  const wellKnownCapabilityDirectiveCount = useMemo(() => {
    return form.capability_directives.filter((directive) => {
      try {
        const path = parseCapabilityPath(directivePath(directive));
        return path.tool === null && WELL_KNOWN_CAPABILITY_KEYS.has(path.capability as CapabilityKey);
      } catch {
        return false;
      }
    }).length;
  }, [form.capability_directives]);

  // ── 字段渲染（从 Tab 容器调用） ─────────────────────────────
  const renderIdentityGroup = () => (
    <div className="space-y-3">
      <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
        <div>
          <label className="agentdash-form-label">预设名称 (key)</label>
          <input
            value={form.name}
            onChange={(e) => patchForm({ name: e.target.value })}
            placeholder="唯一标识，例如 code-review"
            className="agentdash-form-input"
          />
          <p className="mt-0.5 text-[10px] text-muted-foreground/60">
            用作内部标识，不会直接展示给用户
          </p>
        </div>
        <div>
          <label className="agentdash-form-label">显示名称</label>
          <input
            value={form.display_name}
            onChange={(e) => patchForm({ display_name: e.target.value })}
            placeholder="留空则使用预设名称"
            className="agentdash-form-input"
          />
        </div>
      </div>
      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={form.description}
          onChange={(e) => patchForm({ description: e.target.value })}
          rows={2}
          placeholder="这个 Agent 的职责和用途"
          className="agentdash-form-textarea"
        />
      </div>
    </div>
  );

  const renderSystemPromptGroup = () => (
    <div className="space-y-2">
      <div>
        <label className="agentdash-form-label">System Prompt</label>
        <textarea
          value={form.system_prompt}
          onChange={(e) => patchForm({ system_prompt: e.target.value })}
          rows={3}
          placeholder="留空则仅使用全局 System Prompt"
          className="agentdash-form-textarea"
        />
      </div>
    </div>
  );

  const renderRuntimeGroup = () => (
    <div className="space-y-3">
      <div>
        <label className="agentdash-form-label">Agent 类型</label>
        <select
          value={form.agent_type}
          onChange={(e) => handleAgentTypeChange(e.target.value)}
          className="agentdash-form-select"
        >
          <option value="">
            {isDiscoveryLoading ? "加载执行器列表..." : "选择 Agent 类型"}
          </option>
          {agentTypeOptions.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
          {form.agent_type && !agentTypeOptions.some((o) => o.value === form.agent_type) && (
            <option value={form.agent_type}>{form.agent_type} (当前值)</option>
          )}
        </select>
      </div>

      <div>
        <label className="agentdash-form-label">运行环境</label>
        <select
          value={form.backend_requirement}
          onChange={(e) =>
            patchForm({ backend_requirement: e.target.value === "optional" ? "optional" : "required" })
          }
          className="agentdash-form-select"
        >
          <option value="required">必须有可用机器</option>
          <option value="optional">机器可选</option>
        </select>
      </div>

      <div className="grid grid-cols-[1fr_auto] gap-2">
        <div>
          <label className="agentdash-form-label">模型</label>
          <select
            value={selectedModelOptionValue}
            onChange={(e) => handleModelChange(e.target.value)}
            disabled={!form.agent_type || (isModelLoading && visibleModelCount === 0)}
            className="agentdash-form-select"
          >
            <option value="">
              {!form.agent_type
                ? "先选择 Agent 类型"
                : isModelLoading && visibleModelCount === 0
                  ? "加载模型中..."
                  : "不指定模型"}
            </option>
            {[...modelsByProvider.entries()].map(([pid, models]) => {
              const label = pid && providersById.get(pid)
                ? providersById.get(pid)
                : pid || "Other";
              return (
                <optgroup key={pid || "default"} label={label}>
                  {models.map((m) => (
                    <option key={`${pid || "default"}::${m.id}`} value={`${pid}::${m.id}`}>
                      {m.name}
                    </option>
                  ))}
                </optgroup>
              );
            })}
            {selectedModelOptionValue && !hasModelInDiscovery && (
              <option value={selectedModelOptionValue}>
                {form.model_id} (当前值)
              </option>
            )}
          </select>
        </div>
        {showThinkingSelector && (
          <div className="w-[130px]">
            <label className="agentdash-form-label">推理级别</label>
            <select
              value={form.thinking_level}
              onChange={(e) => patchForm({ thinking_level: (e.target.value as ThinkingLevel) || "" })}
              className="agentdash-form-select"
            >
              <option value="">不设置</option>
              {THINKING_LEVEL_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        {(agents.length > 0 || form.agent_id) && (
          <div>
            <label className="agentdash-form-label">Agent</label>
            {agents.length > 0 ? (
              <select
                value={form.agent_id}
                onChange={(e) => patchForm({ agent_id: e.target.value })}
                className="agentdash-form-select"
              >
                <option value="">默认</option>
                {agents.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.label}{a.is_default ? " (默认)" : ""}
                  </option>
                ))}
                {form.agent_id && !agents.some((a) => a.id === form.agent_id) && (
                  <option value={form.agent_id}>{form.agent_id} (当前值)</option>
                )}
              </select>
            ) : (
              <input
                value={form.agent_id}
                onChange={(e) => patchForm({ agent_id: e.target.value })}
                placeholder="可选"
                className="agentdash-form-input"
              />
            )}
          </div>
        )}
        <div>
          <label className="agentdash-form-label">权限策略</label>
          <select
            value={form.permission_policy}
            onChange={(e) => patchForm({ permission_policy: e.target.value })}
            className="agentdash-form-select"
          >
            <option value="">默认</option>
            {permissions.length > 0
              ? permissions.map((p) => (
                  <option key={p} value={p}>{p}</option>
                ))
              : (
                <>
                  <option value="AUTO">AUTO</option>
                  <option value="SUPERVISED">SUPERVISED</option>
                  <option value="PLAN">PLAN</option>
                </>
              )
            }
            {form.permission_policy && permissions.length > 0 &&
              !permissions.includes(form.permission_policy as PermissionPolicy) && (
              <option value={form.permission_policy}>{form.permission_policy} (当前值)</option>
            )}
          </select>
        </div>
      </div>
    </div>
  );

  const renderCompanionContent = () => {
    const toggleDefaultCompanion = (
      <label className="flex items-start gap-2 rounded-[8px] border border-border bg-secondary/25 p-3 text-xs">
        <input
          type="checkbox"
          checked={form.default_companion_enabled}
          onChange={(event) => patchForm({ default_companion_enabled: event.target.checked })}
          className="mt-0.5 h-4 w-4 rounded-[4px] border-input"
        />
        <span className="space-y-0.5">
          <span className="block font-medium text-foreground">默认作为协作 Agent 开放</span>
          <span className="block text-muted-foreground/70">
            开启后，同项目其它 Agent 会默认在 companion roster 中看到此 Agent。
          </span>
        </span>
      </label>
    );
    if (companionCount === 0) {
      return (
        <div className="space-y-3">
          {toggleDefaultCompanion}
          <p className="text-xs text-muted-foreground/70">
            其它 Agent 当前都已默认开放，或项目内暂无可额外加入的非默认 companion。
          </p>
        </div>
      );
    }
    const toggleCompanion = (name: string) => {
      const next = form.extra_companions.includes(name)
        ? form.extra_companions.filter((c) => c !== name)
        : [...form.extra_companions, name];
      patchForm({ extra_companions: next });
    };
    return (
      <div className="space-y-3">
        {toggleDefaultCompanion}
        <CapabilityPicker
          hint="额外 companion 只用于加入未默认开放的 sibling Agent；默认开放的 Agent 会自动进入 roster。"
          isLoading={false}
          error={null}
          items={extraCompanionCandidates}
          selectedKeys={form.extra_companions}
          itemKey={(a) => a.name}
          itemToCardProps={(a) => ({
            reactKey: a.name,
            title: a.display_name && a.display_name !== a.name ? a.display_name : a.name,
            subtitle: a.display_name && a.display_name !== a.name ? a.name : undefined,
          })}
          onToggle={toggleCompanion}
          loadingText=""
          emptyAllText=""
          enabledEmptyText="尚未额外加入非默认 companion。"
          availableEmptyText="所有非默认 companion 都已额外加入。"
        />
      </div>
    );
  };

  // ── Tab / 二级 Sidebar 元数据 ────────────────────────────
  const capabilityCount =
    form.capability_directives.length +
    form.project_vfs_mount_exposure_grants.length +
    form.skill_asset_keys.length +
    form.visible_workspace_module_refs.length +
    form.extra_companions.length +
    (form.default_companion_enabled ? 1 : 0);

  const tabs: Array<{
    key: 'basic' | 'capability' | 'memory';
    label: string;
    badge?: string;
    indicator?: boolean;
  }> = [
    { key: 'basic', label: '基础' },
    { key: 'capability', label: '能力', badge: capabilityCount > 0 ? String(capabilityCount) : undefined },
    { key: 'memory', label: '记忆', indicator: knowledgeEnabled === true },
  ];

  const capabilityItems: Array<{
    key: 'tool' | 'mcp' | 'vfs' | 'skill' | 'module' | 'companion';
    label: string;
    badge?: string;
    disabled?: boolean;
  }> = [
    {
      key: 'tool',
      label: '工具能力',
      badge: wellKnownCapabilityDirectiveCount > 0
        ? `${wellKnownCapabilityDirectiveCount}/${CAPABILITY_OPTIONS.length}`
        : '全部',
    },
    {
      key: 'mcp',
      label: 'MCP',
      badge: selectedMcpPresetKeys.length > 0 ? String(selectedMcpPresetKeys.length) : undefined,
    },
    {
      key: 'vfs',
      label: 'Project VFS',
      badge: form.project_vfs_mount_exposure_grants.length > 0
        ? String(form.project_vfs_mount_exposure_grants.length)
        : undefined,
    },
    {
      key: 'skill',
      label: 'Skills',
      badge: form.skill_asset_keys.length > 0 ? String(form.skill_asset_keys.length) : undefined,
    },
    {
      key: 'module',
      label: 'Modules',
      badge: form.visible_workspace_module_refs.length > 0
        ? String(form.visible_workspace_module_refs.length)
        : '全部',
    },
    {
      key: 'companion',
      label: 'Companion',
      badge: companionCount === 0
        ? (form.default_companion_enabled ? '默认' : undefined)
        : form.extra_companions.length > 0
          ? `${form.extra_companions.length}/${companionCount}`
          : (form.default_companion_enabled ? '默认' : undefined),
      disabled: false,
    },
  ];

  return (
    <div className="space-y-3">
      {/* ── 一级 Tab ── */}
      <div className="flex gap-1.5 border-b border-border pb-2">
        {tabs.map((t) => {
          const active = activeTab === t.key;
          return (
            <button
              key={t.key}
              role="tab"
              aria-selected={active}
              type="button"
              onClick={() => setActiveTab(t.key)}
              className={`flex items-center gap-1.5 rounded-[8px] border px-3 py-1.5 text-xs font-medium transition-colors duration-160 ${
                active
                  ? "border-primary/30 bg-primary/10 text-foreground"
                  : "border-transparent text-muted-foreground hover:bg-secondary/40 hover:text-foreground"
              }`}
            >
              <span>{t.label}</span>
              {t.badge && (
                <span className="rounded-[6px] bg-secondary/60 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {t.badge}
                </span>
              )}
              {t.indicator && (
                // eslint-disable-next-line no-restricted-syntax -- 状态点为圆形指示器
                <span className="inline-block h-1.5 w-1.5 rounded-full bg-primary" aria-hidden />
              )}
            </button>
          );
        })}
      </div>

      {/* ── 基础 Tab ── */}
      {activeTab === 'basic' && (
        <div className="space-y-4">
          {renderIdentityGroup()}
          <div className="border-t border-border/50" />
          {renderSystemPromptGroup()}
          <div className="border-t border-border/50" />
          {renderRuntimeGroup()}
        </div>
      )}

      {/* ── 能力 Tab ── */}
      {activeTab === 'capability' && (
        <div className="flex flex-col gap-3 sm:flex-row sm:gap-4">
          {/* 二级导航：宽屏 sidebar */}
          <aside className="hidden sm:block w-[160px] shrink-0 border-r border-border/60 pr-3">
            <ul className="space-y-0.5">
              {capabilityItems.map((item) => {
                const active = activeCapability === item.key;
                return (
                  <li key={item.key}>
                    <button
                      type="button"
                      disabled={item.disabled}
                      onClick={() => !item.disabled && setActiveCapability(item.key)}
                      className={`flex w-full items-center justify-between rounded-[8px] px-2.5 py-1.5 text-xs transition-colors duration-160 ${
                        item.disabled
                          ? "cursor-not-allowed text-muted-foreground/40"
                          : active
                            ? "bg-secondary/60 text-foreground"
                            : "text-muted-foreground hover:bg-secondary/30 hover:text-foreground"
                      }`}
                    >
                      <span>{item.label}</span>
                      {item.badge && (
                        <span className="text-[10px] text-muted-foreground/70">{item.badge}</span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          </aside>

          {/* 二级导航：窄屏 chip 行 */}
          <div className="flex flex-wrap gap-1.5 sm:hidden">
            {capabilityItems.map((item) => {
              const active = activeCapability === item.key;
              return (
                <button
                  key={item.key}
                  type="button"
                  disabled={item.disabled}
                  onClick={() => !item.disabled && setActiveCapability(item.key)}
                  className={`flex items-center gap-1.5 rounded-[8px] border px-2.5 py-1 text-[11px] font-medium transition-colors duration-160 ${
                    item.disabled
                      ? "cursor-not-allowed border-border/40 text-muted-foreground/40"
                      : active
                        ? "border-primary/30 bg-primary/10 text-foreground"
                        : "border-border bg-secondary/30 text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {item.label}
                  {item.badge && (
                    <span className="text-[10px] text-muted-foreground/70">{item.badge}</span>
                  )}
                </button>
              );
            })}
          </div>

          <div className="flex-1 min-w-0">
            {activeCapability === 'tool' && (
              <ToolCapabilitiesField
                directives={form.capability_directives}
                onChange={(v) => patchForm({ capability_directives: v })}
              />
            )}
            {activeCapability === 'mcp' && (
              <McpPresetPicker
                projectId={projectId}
                directives={form.capability_directives}
                onChange={(capability_directives) => patchForm({ capability_directives })}
              />
            )}
            {activeCapability === 'vfs' && (
              <ProjectVfsMountExposurePicker
                projectId={projectId}
                grants={form.project_vfs_mount_exposure_grants}
                onChange={(project_vfs_mount_exposure_grants) =>
                  patchForm({ project_vfs_mount_exposure_grants })
                }
              />
            )}
            {activeCapability === 'skill' && (
              <SkillAssetPicker
                projectId={projectId}
                selectedKeys={form.skill_asset_keys}
                onChange={(skill_asset_keys) => patchForm({ skill_asset_keys })}
              />
            )}
            {activeCapability === 'module' && (
              <WorkspaceModuleVisibilityPicker
                projectId={projectId}
                selectedRefs={form.visible_workspace_module_refs}
                onChange={(visible_workspace_module_refs) =>
                  patchForm({ visible_workspace_module_refs })
                }
              />
            )}
            {activeCapability === 'companion' && renderCompanionContent()}
          </div>
        </div>
      )}

      {/* ── 记忆 Tab ── */}
      {activeTab === 'memory' && (
        <div>
          {knowledgeEnabled !== undefined && onToggleKnowledge ? (
            <KnowledgeSection
              enabled={knowledgeEnabled}
              onToggle={onToggleKnowledge}
              projectId={projectId}
              agentId={knowledgeAgentId}
            />
          ) : (
            <p className="text-xs text-muted-foreground/70">
              该 Agent 尚未保存为项目实例，保存后即可配置知识库。
            </p>
          )}
        </div>
      )}
    </div>
  );
}
