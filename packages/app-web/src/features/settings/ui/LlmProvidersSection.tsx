import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ConfirmDialog } from "@agentdash/ui";

import type { LlmProvider, ProbeModelEntry, UpdateLlmProviderRequest } from "../../../api/llmProviders";
import type { ModelInfo } from "../../executor-selector/model/types";
import { useExecutorDiscoveredOptions } from "../../executor-selector";
import {
  createAdminCodexOAuthActions,
  hasDesktopCodexOAuthBridge,
  probeLlmProviderModels,
} from "../model/llmProviderActions";
import {
  CUSTOM_LLM_PROVIDER_PROTOCOLS,
  LLM_PROVIDER_PRESETS,
  type LlmProviderPreset,
  type LlmProviderProtocol,
} from "../model/llmProviderPresets";
import {
  modelBelongsToProviderSlug,
  parseLlmProviderBlockedModels,
  parseLlmProviderModelConfigs,
  serializeLlmProviderBlockedModels,
  serializeLlmProviderModelConfigs,
  type LlmProviderModelConfig,
} from "../model/llmProviderModels";
import {
  useCreateLlmProviderMutation,
  useDeleteLlmProviderMutation,
  useLlmProvidersQuery,
  useUpdateLlmProviderMutation,
} from "../model/llmProviderQueries";
import {
  BulkModelManagementPanel,
  type BulkManageModelEntry,
} from "./BulkModelManagementPanel";
import { OAuthLoginWizard } from "./OAuthLoginWizard";
import { btnPrimaryCls, Field, inputCls, SectionCard } from "./primitives";

// ---------------------------------------------------------------------------
// LLM Providers 配置 (data-driven from DB)
// ---------------------------------------------------------------------------

function defaultOpenAiWireApi(baseUrl: string): "responses" | "completions" {
  const normalized = baseUrl.trim().replace(/\/+$/, "").toLowerCase();
  if (!normalized || normalized === "https://api.openai.com/v1" || normalized === "https://api.openai.com") {
    return "responses";
  }
  return "completions";
}

function compareModelLabel(
  a: { id: string; name: string },
  b: { id: string; name: string },
): number {
  const nameCompare = a.name.localeCompare(b.name, "zh-Hans", {
    numeric: true,
    sensitivity: "base",
  });
  if (nameCompare !== 0) return nameCompare;

  return a.id.localeCompare(b.id, "zh-Hans", {
    numeric: true,
    sensitivity: "base",
  });
}

export function LlmProvidersSection({
  discoveryRefreshKey,
  onRefreshModels,
}: {
  discoveryRefreshKey: number;
  onRefreshModels: () => void;
}) {
  const providersQuery = useLlmProvidersQuery();
  const createProvider = useCreateLlmProviderMutation();
  const updateProvider = useUpdateLlmProviderMutation();
  const deleteProvider = useDeleteLlmProviderMutation();
  const discovered = useExecutorDiscoveredOptions("PI_AGENT", discoveryRefreshKey);
  const discoveredModels = discovered.options?.model_selector.models ?? [];
  const isLoadingModels = discovered.options?.loading_models ?? true;
  const providers = providersQuery.data ?? [];
  const saving = createProvider.isPending || updateProvider.isPending || deleteProvider.isPending;

  // 创建流程: null = 未开始, ProviderPreset|null = 选中的模板(null=自定义)
  const [createStep, setCreateStep] = useState<"idle" | "pick" | "form">("idle");
  const [createPreset, setCreatePreset] = useState<LlmProviderPreset | null>(null);
  const [createName, setCreateName] = useState("");
  const [createSlug, setCreateSlug] = useState("");
  const [createProtocol, setCreateProtocol] = useState<LlmProviderProtocol>("openai_compatible");
  const [createError, setCreateError] = useState("");

  const startCreateFromPreset = (preset: LlmProviderPreset) => {
    setCreatePreset(preset);
    setCreateName(preset.name);
    setCreateSlug(preset.slug);
    setCreateProtocol(preset.protocol);
    setCreateError("");
    setCreateStep("form");
  };

  const startCreateCustom = (protocol: LlmProviderProtocol) => {
    setCreatePreset(null);
    setCreateName("");
    setCreateSlug("");
    setCreateProtocol(protocol);
    setCreateError("");
    setCreateStep("form");
  };

  const cancelCreate = () => {
    setCreateStep("idle");
    setCreatePreset(null);
    setCreateName("");
    setCreateSlug("");
    setCreateError("");
  };

  const submitCreate = async () => {
    const name = createName.trim();
    const slug = createSlug.trim().toLowerCase();
    if (!name) { setCreateError("名称不能为空"); return; }
    if (!slug) { setCreateError("唯一标识不能为空"); return; }
    if (!/^[a-z0-9][a-z0-9_-]*$/.test(slug)) { setCreateError("唯一标识仅允许小写字母、数字、- 和 _，且不能以符号开头"); return; }
    if (providers.some((p) => p.slug === slug)) { setCreateError(`标识 "${slug}" 已被占用`); return; }
    setCreateError("");

    const result = await createProvider.mutateAsync({
      name,
      slug,
      protocol: createProtocol,
      ...(createPreset ? {
        base_url: createPreset.base_url,
        env_api_key: createPreset.env_api_key,
        default_model: createPreset.default_model,
        models: createPreset.models ? serializeLlmProviderModelConfigs(createPreset.models) : undefined,
      } : {}),
    });
    if (result) {
      cancelCreate();
      onRefreshModels();
    }
  };

  return (
    <SectionCard title="LLM Providers">
      <p className="text-xs text-muted-foreground -mt-2 mb-1">
        配置各 LLM 服务商的 API 密钥和端点，支持同一协议的多个实例
      </p>
      {providersQuery.isPending ? (
        <p className="text-xs text-muted-foreground py-2">加载中…</p>
      ) : (
        <div className="space-y-2">
          {providers.map((provider) => (
            <LlmProviderRow
              key={provider.id}
              provider={provider}
              discoveredModels={discoveredModels.filter((model) => modelBelongsToProviderSlug(model, provider))}
              isLoadingModels={isLoadingModels}
              onRefreshModels={onRefreshModels}
              saving={saving}
              onSave={async (req) => {
                await updateProvider.mutateAsync({ id: provider.id, request: req });
                onRefreshModels();
              }}
              onDelete={async () => {
                await deleteProvider.mutateAsync(provider.id);
                onRefreshModels();
              }}
              onProviderChanged={async () => {
                await providersQuery.refetch();
                onRefreshModels();
              }}
            />
          ))}
        </div>
      )}

      {/* Add Provider */}
      <div className="mt-2">
        {createStep === "pick" && (
          <div className="space-y-1 rounded-[8px] border border-border bg-background/80 p-3">
            <p className="text-xs font-medium text-foreground mb-2">选择预设模板</p>
            {LLM_PROVIDER_PRESETS.map((preset) => (
              <button
                key={preset.slug}
                type="button"
                className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-muted/50"
                onClick={() => startCreateFromPreset(preset)}
              >
                <span className="font-medium">{preset.name}</span>
                <span className="text-xs text-muted-foreground">({preset.protocol})</span>
              </button>
            ))}
            <div className="border-t border-border mt-1 pt-2">
              <p className="text-xs text-muted-foreground mb-1.5 px-3">自定义端点</p>
              {CUSTOM_LLM_PROVIDER_PROTOCOLS.map((proto) => (
                <button
                  key={proto}
                  type="button"
                  className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-muted/50"
                  onClick={() => startCreateCustom(proto)}
                >
                  <span className="font-medium">{proto}</span>
                </button>
              ))}
            </div>
            <div className="flex justify-end pt-1">
              <button type="button" className="text-xs text-muted-foreground hover:text-foreground" onClick={cancelCreate}>
                取消
              </button>
            </div>
          </div>
        )}

        {createStep === "form" && (
          <div className="rounded-[8px] border border-border bg-background/80 p-3 space-y-3">
            <p className="text-xs font-medium text-foreground">
              创建 Provider{createPreset ? ` — ${createPreset.name}` : ` — ${createProtocol}`}
            </p>
            <div className="space-y-2">
              <div>
                <label className="block text-xs text-muted-foreground mb-1">名称</label>
                <input
                  type="text"
                  className={inputCls}
                  value={createName}
                  placeholder="例如 My Azure Proxy"
                  onChange={(e) => setCreateName(e.target.value)}
                  autoFocus
                />
              </div>
              <div>
                <label className="block text-xs text-muted-foreground mb-1">
                  唯一标识 (slug)
                  <span className="text-muted-foreground/60 ml-1">— 小写字母、数字、-、_</span>
                </label>
                <input
                  type="text"
                  className={inputCls}
                  value={createSlug}
                  placeholder="例如 my-azure-proxy"
                  onChange={(e) => {
                    setCreateSlug(e.target.value.replace(/[^a-zA-Z0-9_-]/g, "").toLowerCase());
                    setCreateError("");
                  }}
                />
              </div>
            </div>
            {createError && (
              <p className="text-xs text-destructive">{createError}</p>
            )}
            <div className="flex justify-end gap-2 pt-1">
              <button type="button" className="text-xs text-muted-foreground hover:text-foreground" onClick={cancelCreate}>
                取消
              </button>
              <button type="button" disabled={saving} className={btnPrimaryCls} onClick={submitCreate}>
                {saving ? "创建中…" : "创建"}
              </button>
            </div>
          </div>
        )}

        {createStep === "idle" && (
          <button
            type="button"
            className="flex w-full items-center justify-center gap-1.5 rounded-[8px] border border-dashed border-border px-4 py-2.5 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/30"
            onClick={() => setCreateStep("pick")}
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 5v14"/><path d="M5 12h14"/></svg>
            添加 Provider
          </button>
        )}
      </div>
    </SectionCard>
  );
}

function LlmProviderRow({
  provider,
  discoveredModels,
  isLoadingModels,
  onRefreshModels,
  saving,
  onSave,
  onDelete,
  onProviderChanged,
}: {
  provider: LlmProvider;
  discoveredModels: ModelInfo[];
  isLoadingModels: boolean;
  onRefreshModels: () => void;
  saving: boolean;
  onSave: (req: UpdateLlmProviderRequest) => Promise<void>;
  onDelete: () => void;
  onProviderChanged: () => Promise<void> | void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [saveConfirmed, setSaveConfirmed] = useState(false);
  const saveConfirmedTimerRef = useRef<number | null>(null);
  const configured = provider.global_api_key_configured;

  useEffect(() => {
    return () => {
      if (saveConfirmedTimerRef.current !== null) {
        window.clearTimeout(saveConfirmedTimerRef.current);
      }
    };
  }, []);

  const handleSaved = () => {
    setExpanded(false);
    setSaveConfirmed(true);
    if (saveConfirmedTimerRef.current !== null) {
      window.clearTimeout(saveConfirmedTimerRef.current);
    }
    saveConfirmedTimerRef.current = window.setTimeout(() => {
      setSaveConfirmed(false);
      saveConfirmedTimerRef.current = null;
    }, 2400);
  };

  return (
    <div className="rounded-[8px] border border-border bg-background/80">
      <button
        type="button"
        className="flex w-full items-center gap-3 px-4 py-3 text-left"
        onClick={() => setExpanded((p) => !p)}
      >
        <span
          className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${configured ? "bg-success" : "bg-muted-foreground/30"}`}
        />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-foreground">{provider.name}</p>
          <p className="text-xs text-muted-foreground">{provider.slug} · {provider.protocol}{provider.base_url ? ` · ${provider.base_url}` : ""}</p>
        </div>
        {!provider.enabled && (
          <span className="rounded-[6px] border border-warning/30 bg-warning/10 px-2 py-0.5 text-[11px] text-warning">
            已禁用
          </span>
        )}
        {saveConfirmed && (
          <span className="rounded-[6px] border border-success/30 bg-success/10 px-2 py-0.5 text-[11px] text-success">
            已保存
          </span>
        )}
        {configured && provider.enabled && (
          <span className="rounded-[6px] border border-success/30 bg-success/10 px-2 py-0.5 text-[11px] text-success">
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
          key={`${provider.id}-${provider.updated_at}`}
          provider={provider}
          discoveredModels={discoveredModels}
          isLoadingModels={isLoadingModels}
          onRefreshModels={onRefreshModels}
          saving={saving}
          onSave={onSave}
          onDelete={onDelete}
          onProviderChanged={onProviderChanged}
          onSaved={handleSaved}
        />
      )}
    </div>
  );
}

function LlmProviderForm({
  provider,
  discoveredModels,
  isLoadingModels,
  onRefreshModels: _onRefreshModels,
  saving,
  onSave,
  onDelete,
  onProviderChanged,
  onSaved,
}: {
  provider: LlmProvider;
  discoveredModels: ModelInfo[];
  isLoadingModels: boolean;
  onRefreshModels: () => void;
  saving: boolean;
  onSave: (req: UpdateLlmProviderRequest) => Promise<void>;
  onDelete: () => void;
  onProviderChanged: () => Promise<void> | void;
  onSaved: () => void;
}) {
  const [name, setName] = useState(provider.name);
  const [apiKey, setApiKey] = useState(provider.global_api_key_preview ?? "");
  const [apiKeyTouched, setApiKeyTouched] = useState(false);
  const [baseUrl, setBaseUrl] = useState(provider.base_url);
  const [defaultModel, setDefaultModel] = useState(provider.default_model);
  const [wireApi, setWireApi] = useState(provider.wire_api || (provider.protocol === "openai_compatible" ? defaultOpenAiWireApi(provider.base_url) : ""));
  const [credentialMode, setCredentialMode] = useState(provider.credential_mode);
  const [enabled, setEnabled] = useState(provider.enabled);
  const [models, setModels] = useState<LlmProviderModelConfig[]>(parseLlmProviderModelConfigs(provider.models));
  const [modelsTouched, setModelsTouched] = useState(false);
  const [blockedModels, setBlockedModels] = useState<string[]>(parseLlmProviderBlockedModels(provider.blocked_models));
  const persistedBlockedModels = useMemo(
    () => parseLlmProviderBlockedModels(provider.blocked_models),
    [provider.blocked_models],
  );
  const [blockedModelsTouched, setBlockedModelsTouched] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  // 实时探测状态：用当前表单 credentials 探测到的模型列表
  const [probedModels, setProbedModels] = useState<ProbeModelEntry[] | null>(null);
  const [isProbing, setIsProbing] = useState(false);
  const [probeError, setProbeError] = useState<string | null>(null);

  const showApiKey = provider.protocol !== "openai_codex";
  const showBaseUrl = provider.protocol === "openai_compatible" || provider.protocol === "anthropic";
  const showWireApi = provider.protocol === "openai_compatible";
  const showDefaultModel = true; // all protocols support default model

  // 合并来源：probe 结果优先（当存在时替代 discovery 结果），否则用全局 discovery
  const effectiveDiscoveredModels: ModelInfo[] = useMemo(() => {
    if (probedModels !== null) {
      return probedModels.map((m) => ({
        id: m.id,
        name: m.name,
        provider_id: provider.slug,
        reasoning: true,
        supports_image: true,
        context_window: 200_000,
          blocked: false,
          discovered: true,
          source: "oauth_default",
      }));
    }
    return discoveredModels;
  }, [probedModels, discoveredModels, provider.slug]);

  // 默认模型候选：所有未被屏蔽的模型（discovered + custom 合并去重）
  const defaultModelOptions = useMemo(() => {
    const allIds = new Set<string>();
    const options: { id: string; name: string }[] = [];

    // discovered 中未屏蔽的
    for (const c of effectiveDiscoveredModels) {
      if (!c.id.trim() || blockedModels.includes(c.id)) continue;
      if (allIds.has(c.id)) continue;
      allIds.add(c.id);
      options.push({ id: c.id, name: c.name.trim() || c.id });
    }

    // custom 中未屏蔽的（包含 discovered override 和纯自定义）
    for (const c of models) {
      if (!c.id.trim() || blockedModels.includes(c.id)) continue;
      if (allIds.has(c.id)) continue;
      allIds.add(c.id);
      options.push({ id: c.id, name: c.name.trim() || c.id });
    }

    options.sort(compareModelLabel);

    if (defaultModel.trim().length > 0 && !allIds.has(defaultModel)) {
      options.unshift({ id: defaultModel, name: `${defaultModel}（当前值）` });
    }

    return options;
  }, [effectiveDiscoveredModels, defaultModel, models, blockedModels]);

  const toggleBlockedModel = (modelId: string) => {
    setBlockedModels((current) => {
      return current.includes(modelId)
        ? current.filter((item) => item !== modelId)
        : [...current, modelId];
    });
    setBlockedModelsTouched(true);
  };

  const replaceBlockedModels = (nextBlockedModels: string[]) => {
    setBlockedModels(Array.from(new Set(nextBlockedModels.filter((modelId) => modelId.trim().length > 0))));
    setBlockedModelsTouched(true);
  };

  const replaceModels = (nextModels: LlmProviderModelConfig[]) => {
    setModels(nextModels);
    setModelsTouched(true);
  };

  const handleSave = async () => {
    const req: UpdateLlmProviderRequest = {};
    if (name !== provider.name) req.name = name;
    if (enabled !== provider.enabled) req.enabled = enabled;
    if (credentialMode !== provider.credential_mode) req.credential_mode = credentialMode;
    if (apiKeyTouched) req.global_api_key = apiKey;
    if (baseUrl !== provider.base_url) req.base_url = baseUrl;
    if (defaultModel !== provider.default_model) req.default_model = defaultModel;
    if (showWireApi && wireApi !== provider.wire_api) req.wire_api = wireApi;
    if (modelsTouched) req.models = serializeLlmProviderModelConfigs(models);
    if (blockedModelsTouched) req.blocked_models = serializeLlmProviderBlockedModels(blockedModels);

    setSaveError(null);
    if (Object.keys(req).length === 0) {
      onSaved();
      return;
    }

    try {
      await onSave(req);
      setApiKeyTouched(false);
      setModelsTouched(false);
      setBlockedModelsTouched(false);
      onSaved();
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : String(e));
    }
  };

  // 用当前表单 credentials 实时探测模型，不保存、不折叠
  const handleProbeModels = useCallback(async () => {
    setIsProbing(true);
    setProbeError(null);
    try {
      const result = await probeLlmProviderModels({
        protocol: provider.protocol,
        api_key: apiKeyTouched ? apiKey : undefined,
        base_url: baseUrl || undefined,
        discovery_url: undefined,
        env_api_key: provider.env_api_key || undefined,
        provider_id: provider.id,
      });
      setProbedModels(result);
    } catch (e) {
      setProbeError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsProbing(false);
    }
  }, [provider.id, provider.protocol, provider.env_api_key, apiKey, apiKeyTouched, baseUrl]);

  const handleCodexLoginCompleted = useCallback(async () => {
    setApiKeyTouched(false);
    await onProviderChanged();
  }, [onProviderChanged]);

  const codexOAuthActions = useMemo(() => createAdminCodexOAuthActions(provider.id), [provider.id]);
  const desktopCodexOAuthAvailable = hasDesktopCodexOAuthBridge();

  const handleAddModel = (initial?: LlmProviderModelConfig) => {
    const newModel: LlmProviderModelConfig = initial ?? { id: "", name: "", context_window: 200000, reasoning: true, supports_image: true };
    setModels([...models, newModel]);
    setModelsTouched(true);
  };

  const handleRemoveModel = (index: number) => {
    setModels(models.filter((_, i) => i !== index));
    setModelsTouched(true);
  };

  const handleUpdateModel = (index: number, field: keyof LlmProviderModelConfig, value: string | number | boolean) => {
    setModels(models.map((m, i) => i !== index ? m : { ...m, [field]: value }));
    setModelsTouched(true);
  };

  return (
    <>
      <div className="space-y-3 border-t border-border px-4 pb-4 pt-3">
      {/* Name */}
      <Field label="名称" desc="Provider 显示名称">
        <input type="text" className={inputCls} value={name} onChange={(e) => setName(e.target.value)} />
      </Field>

      {/* Enabled toggle */}
      <Field label="启用" desc="禁用后不会出现在模型选择中">
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
            className="accent-primary"
          />
          <span className="text-sm">{enabled ? "已启用" : "已禁用"}</span>
        </label>
      </Field>

      <Field label="凭据策略" desc="控制平台全局 Key 与用户 BYOK 的生效方式">
        <select
          className={`${inputCls} h-10 appearance-none`}
          value={credentialMode}
          onChange={(e) => {
            if (e.target.value === "global_only" || e.target.value === "global_or_user" || e.target.value === "user_required") {
              setCredentialMode(e.target.value);
            }
          }}
        >
          <option value="global_only">仅平台全局 Key</option>
          <option value="global_or_user">平台全局 Key 或用户 BYOK</option>
          <option value="user_required">必须用户 BYOK</option>
        </select>
      </Field>

      {/* API Key */}
      {showApiKey && (
        <Field label="全局 API Key" desc="平台级服务密钥，保存后以掩码形式显示">
          <input
            type="password"
            className={inputCls}
            value={apiKey}
            placeholder="输入 API Key"
            onChange={(e) => { setApiKey(e.target.value); setApiKeyTouched(true); }}
          />
        </Field>
      )}

      {provider.protocol === "openai_codex" && (
        <div className="rounded-[8px] border border-border bg-muted/20 px-3 py-3">
          <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <p className="text-sm font-medium text-foreground">ChatGPT OAuth</p>
                {provider.global_api_key_configured && (
                  <span className="rounded-[6px] border border-success/40 bg-success/10 px-2 py-0.5 text-[11px] font-medium text-success">
                    全局已验证
                  </span>
                )}
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                {provider.global_api_key_configured
                  ? `${provider.global_api_key_preview ?? "ChatGPT OAuth"} · 平台全局生效`
                : "未验证"}
              </p>
            </div>
            <OAuthLoginWizard
              start={codexOAuthActions.start}
              getStatus={codexOAuthActions.getStatus}
              cancel={codexOAuthActions.cancel}
              onCompleted={handleCodexLoginCompleted}
              idleLabel={provider.global_api_key_configured ? "重新验证 ChatGPT" : "通过 ChatGPT 登录"}
              authLinkLabel="打开 ChatGPT 授权页"
              openedMessage="已在外部浏览器打开 ChatGPT 授权页，等待授权完成…"
              manualMessage="请打开 ChatGPT 授权页并完成登录，完成后这里会自动更新状态。"
              completedMessage="Codex 登录已完成"
              failedMessage="Codex 登录失败"
              disabled={!desktopCodexOAuthAvailable}
              disabledMessage="ChatGPT OAuth 需要在 AgentDash 桌面端完成"
              surface="inline"
              className="shrink-0"
            />
          </div>
        </div>
      )}

      {/* Base URL */}
      {showBaseUrl && (
        <Field label="Base URL" desc="API 端点地址（留空使用默认值）">
          <input
            type="text"
            className={inputCls}
            value={baseUrl}
            placeholder="https://..."
            onChange={(e) => setBaseUrl(e.target.value)}
          />
        </Field>
      )}

      {/* Default Model */}
      {showDefaultModel && (
        <Field
          label="默认模型"
          desc={defaultModelOptions.length > 0 ? "从当前候选模型中选择默认值" : "先保存配置后会自动出现下拉"}
        >
          {defaultModelOptions.length > 0 ? (
            <select
              className={`${inputCls} h-10 appearance-none`}
              value={defaultModel}
              onChange={(e) => setDefaultModel(e.target.value)}
            >
              <option value="">选择默认模型…</option>
              {defaultModelOptions.map((c) => (
                <option key={c.id} value={c.id}>{c.name}</option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              className={inputCls}
              value={defaultModel}
              placeholder="保存配置后自动发现，或手动输入"
              onChange={(e) => setDefaultModel(e.target.value)}
            />
          )}
        </Field>
      )}

      {/* Wire API (OpenAI-compatible only) */}
      {showWireApi && (
        <Field
          label="Wire API"
          desc="官方 OpenAI 默认用 responses；自定义兼容端点默认更适合 completions"
        >
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

      {/* Model Management */}
      <ModelManagementSection
        discoveredModels={effectiveDiscoveredModels}
        customModels={models}
        blockedModels={blockedModels}
        initialBlockedModels={persistedBlockedModels}
        isLoadingModels={isProbing || isLoadingModels}
        saving={saving}
        onRefreshModels={handleProbeModels}
        onToggleBlocked={toggleBlockedModel}
        onSetBlockedModels={replaceBlockedModels}
        onAddModel={handleAddModel}
        onRemoveModel={handleRemoveModel}
        onUpdateModel={handleUpdateModel}
        onReplaceModels={replaceModels}
        onSave={handleSave}
        probeError={probeError}
      />

      {saveError && (
        <p className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          保存失败: {saveError}
        </p>
      )}

      <div className="flex justify-between pt-1">
        <button
          type="button"
          className="text-xs text-destructive hover:text-destructive/80"
          onClick={() => setDeleteConfirmOpen(true)}
        >
          删除此 Provider
        </button>
        <button type="button" disabled={saving} className={btnPrimaryCls} onClick={() => void handleSave()}>
          {saving ? "保存中…" : "保存"}
        </button>
      </div>
      </div>
      <ConfirmDialog
        open={deleteConfirmOpen}
        title="删除 Provider"
        description={`确定删除 Provider「${provider.name}」？`}
        confirmLabel="删除"
        tone="danger"
        disabled={saving}
        isConfirming={saving}
        onClose={() => setDeleteConfirmOpen(false)}
        onConfirm={() => {
          setDeleteConfirmOpen(false);
          onDelete();
        }}
      />
    </>
  );
}// ---------------------------------------------------------------------------
// 统一模型管理
// ---------------------------------------------------------------------------

function buildModelTooltip(model: ModelInfo): string {
  const lines = [model.id];
  if (model.name && model.name !== model.id) lines.push(`名称: ${model.name}`);
  lines.push(`上下文窗口: ${(model.context_window / 1000).toFixed(0)}k tokens`);
  if (model.reasoning) lines.push("支持推理 (extended thinking)");
  if (model.supports_image) lines.push("支持图像");
  return lines.join("\n");
}

function buildCustomModelTooltip(model: LlmProviderModelConfig): string {
  const lines = [model.id, "自定义模型"];
  if (model.name && model.name !== model.id) lines.push(`名称: ${model.name}`);
  lines.push(`上下文窗口: ${(model.context_window / 1000).toFixed(0)}k tokens`);
  if (model.reasoning) lines.push("支持推理");
  if (model.supports_image) lines.push("支持图像");
  return lines.join("\n");
}

function isProviderPrefixedModel(id: string, name: string): boolean {
  const candidates = [id, name].map((value) => value.trim()).filter((value) => value.length > 0);
  return candidates.some((value) => /^[^/\s]+\/[^/]+$/.test(value));
}

function ModelManagementSection({
  discoveredModels,
  customModels,
  blockedModels,
  initialBlockedModels,
  isLoadingModels,
  saving,
  onRefreshModels,
  onToggleBlocked,
  onSetBlockedModels,
  onAddModel,
  onRemoveModel,
  onUpdateModel,
  onReplaceModels,
  onSave,
  probeError,
}: {
  discoveredModels: ModelInfo[];
  customModels: LlmProviderModelConfig[];
  blockedModels: string[];
  initialBlockedModels: string[];
  isLoadingModels: boolean;
  saving: boolean;
  onRefreshModels: () => void;
  onToggleBlocked: (modelId: string) => void;
  onSetBlockedModels: (modelIds: string[]) => void;
  onAddModel: (initial?: LlmProviderModelConfig) => void;
  onRemoveModel: (index: number) => void;
  onUpdateModel: (index: number, field: keyof LlmProviderModelConfig, value: string | number | boolean) => void;
  onReplaceModels: (models: LlmProviderModelConfig[]) => void;
  onSave: () => Promise<void>;
  probeError?: string | null;
}) {
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [editingDiscoveredId, setEditingDiscoveredId] = useState<string | null>(null);
  const [bulkPanelOpen, setBulkPanelOpen] = useState(false);
  const initialBlockedModelIds = useMemo(() => new Set(initialBlockedModels), [initialBlockedModels]);

  const dragRef = useRef<{ action: "block" | "unblock"; touched: Set<string> } | null>(null);

  const handleDragStart = (modelId: string) => {
    const isBlocked = blockedModels.includes(modelId);
    const action = isBlocked ? "unblock" : "block";
    dragRef.current = { action, touched: new Set([modelId]) };
    onToggleBlocked(modelId);
  };

  const handleDragEnter = (modelId: string) => {
    const drag = dragRef.current;
    if (!drag || drag.touched.has(modelId)) return;
    drag.touched.add(modelId);
    const isBlocked = blockedModels.includes(modelId);
    if ((drag.action === "block" && !isBlocked) || (drag.action === "unblock" && isBlocked)) {
      onToggleBlocked(modelId);
    }
  };

  const handleDragEnd = () => { dragRef.current = null; };

  // 找到 discovered model 对应的 override config index（如果存在于 customModels 中）
  const findOverrideIndex = (modelId: string) => customModels.findIndex((m) => m.id === modelId);

  // 对 discovered model 设置/更新 override
  const handleDiscoveredOverride = (model: ModelInfo, field: keyof LlmProviderModelConfig, value: string | number | boolean) => {
    const existingIdx = findOverrideIndex(model.id);
    if (existingIdx >= 0) {
      onUpdateModel(existingIdx, field, value);
    } else {
      // 首次 override：基于 discovered 属性创建一条配置
      const newConfig: LlmProviderModelConfig = {
        id: model.id,
        name: model.name,
        context_window: model.context_window,
        reasoning: model.reasoning,
        supports_image: model.supports_image,
        [field]: value,
      };
      onAddModel(newConfig);
    }
  };

  const handleRemoveDiscoveredOverride = (modelId: string) => {
    const idx = findOverrideIndex(modelId);
    if (idx >= 0) {
      onRemoveModel(idx);
    }
    setEditingDiscoveredId(null);
  };

  // 用后端返回的 discovered 字段区分真正的 API 发现模型 vs 纯配置模型
  const trueDiscoveredModels = discoveredModels.filter((m) => m.discovered !== false);
  const trueDiscoveredIds = new Set(trueDiscoveredModels.map((m) => m.id));

  // pureCustom: customModels 中不属于"真正 discovered"的条目
  const pureCustomEntries = customModels
    .map((m, i) => ({ model: m, originalIndex: i }))
    .filter((e) => !trueDiscoveredIds.has(e.model.id));

  const hasAny = trueDiscoveredModels.length > 0 || pureCustomEntries.length > 0;
  const totalCount = trueDiscoveredModels.length + pureCustomEntries.length;
  const enabledCount =
    trueDiscoveredModels.filter((m) => !blockedModels.includes(m.id)).length +
    pureCustomEntries.filter((e) => !blockedModels.includes(e.model.id)).length;
  const bulkEntries: BulkManageModelEntry[] = [
    ...trueDiscoveredModels.map((model): BulkManageModelEntry => {
      const overrideIndex = findOverrideIndex(model.id);
      const override = overrideIndex >= 0 ? customModels[overrideIndex] : null;
      const name = override?.name || model.name || model.id;
      return {
        key: `d-${model.id}`,
        id: model.id,
        name,
        source: "discovered",
        enabled: !blockedModels.includes(model.id),
        context_window: override?.context_window ?? model.context_window,
        reasoning: override?.reasoning ?? model.reasoning,
        supports_image: override?.supports_image ?? model.supports_image,
        provider_prefixed: isProviderPrefixedModel(model.id, name),
        has_override: overrideIndex >= 0,
        discovered_model: model,
      };
    }),
    ...pureCustomEntries.map(({ model, originalIndex }): BulkManageModelEntry => ({
      key: `c-${originalIndex}`,
      id: model.id,
      name: model.name || model.id || "（未命名）",
      source: "custom",
      enabled: !blockedModels.includes(model.id),
      context_window: model.context_window,
      reasoning: model.reasoning,
      supports_image: model.supports_image,
      provider_prefixed: isProviderPrefixedModel(model.id, model.name),
      has_override: false,
      custom_index: originalIndex,
    })),
  ].sort((a, b) => {
    const aInitiallyEnabled = !initialBlockedModelIds.has(a.id);
    const bInitiallyEnabled = !initialBlockedModelIds.has(b.id);
    if (aInitiallyEnabled !== bInitiallyEnabled) return aInitiallyEnabled ? -1 : 1;
    return compareModelLabel(a, b);
  });
  const hasBulkModels = bulkEntries.some((entry) => entry.id.trim().length > 0);
  const displayEntries = bulkEntries;

  const updateBulkEntry = (
    entry: BulkManageModelEntry,
    field: keyof LlmProviderModelConfig,
    value: string | number | boolean,
  ) => {
    if (entry.source === "discovered" && entry.discovered_model) {
      handleDiscoveredOverride(entry.discovered_model, field, value);
      return;
    }
    if (entry.source === "custom" && entry.custom_index !== undefined) {
      onUpdateModel(entry.custom_index, field, value);
    }
  };

  return (
    <>
      <div className="space-y-2">
      {/* Header */}
      <div className="flex items-center justify-between gap-2">
        <div className="space-y-0.5">
          <span className="text-sm font-medium text-foreground">模型管理</span>
          <p className="text-xs text-muted-foreground">
            {hasAny
              ? `共 ${totalCount} 个模型（${enabledCount} 个启用），拖拽批量切换 · 点击标签编辑属性`
              : "暂无模型，点击「探测」发现可用模型"}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {hasBulkModels && (
            <button
              type="button"
              onClick={() => setBulkPanelOpen(true)}
              className="inline-flex items-center gap-1.5 rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            >
              批量管理
            </button>
          )}
          <button
            type="button"
            onClick={onRefreshModels}
            disabled={isLoadingModels}
            className="inline-flex items-center gap-1.5 rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
            title="用当前表单配置实时探测可用模型（无需先保存）"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={isLoadingModels ? "animate-spin" : ""}>
              <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
              <path d="M21 3v5h-5" />
            </svg>
            {isLoadingModels ? "探测中…" : "探测"}
          </button>
        </div>
      </div>

      {probeError && (
        <p className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          探测失败: {probeError}
        </p>
      )}

      {/* Model chips */}
      <div
        className="flex flex-wrap gap-1.5 select-none"
        onPointerUp={handleDragEnd}
        onPointerLeave={handleDragEnd}
      >
        {displayEntries.map((entry) => {
          if (entry.source === "discovered" && entry.discovered_model) {
            const model = entry.discovered_model;
            const isEditing = editingDiscoveredId === model.id;
            const effectiveTooltip = buildModelTooltip({
              ...model,
              reasoning: entry.reasoning,
              supports_image: entry.supports_image,
              context_window: entry.context_window,
            });
            return (
              <span
                key={entry.key}
                className={`group relative inline-flex touch-none items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-xs transition-all ${
                  isEditing
                    ? "border-primary/40 bg-primary/8 text-primary ring-1 ring-primary/20"
                    : entry.enabled
                      ? "border-success/30 bg-success/10 text-success hover:bg-success/15"
                      : "border-border bg-muted/40 text-muted-foreground hover:bg-muted/60"
                }`}
                title={effectiveTooltip}
                onPointerDown={(e) => { e.preventDefault(); handleDragStart(model.id); }}
                onPointerEnter={() => handleDragEnter(model.id)}
              >
                <span className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full transition-colors ${
                  isEditing ? "bg-primary" : entry.enabled ? "bg-success" : "bg-muted-foreground/30"
                }`} />
                <span className={entry.enabled ? "" : "line-through opacity-60"}>
                  {entry.name}
                </span>
                {entry.has_override && (
                  // eslint-disable-next-line no-restricted-syntax -- 1px 圆点指示
                  <span className="inline-block h-1 w-1 rounded-full bg-warning" title="已自定义属性" />
                )}
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    setEditingDiscoveredId(isEditing ? null : model.id);
                  }}
                  className="ml-0.5 hidden rounded p-0.5 transition-colors group-hover:inline-flex hover:bg-black/10 dark:hover:bg-white/10"
                  onPointerDown={(e) => e.stopPropagation()}
                  title="编辑属性"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="m12 20 9-11-4-4-9 11 1 5 3-1Z" />
                  </svg>
                </button>
              </span>
            );
          }

          const originalIndex = entry.custom_index ?? -1;
          const customModel = originalIndex >= 0 ? customModels[originalIndex] : null;
          if (!customModel) return null;
          const isEditing = editingIndex === originalIndex;
          return (
            <span
              key={entry.key}
              className={`group relative inline-flex touch-none items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-xs transition-all ${
                isEditing
                  ? "border-primary/40 bg-primary/8 text-primary ring-1 ring-primary/20"
                  : entry.enabled
                    ? "border-blue-500/30 bg-blue-500/8 text-blue-700 hover:bg-blue-500/15 dark:text-blue-300"
                    : "border-border bg-muted/40 text-muted-foreground hover:bg-muted/60"
              }`}
              title={buildCustomModelTooltip(customModel)}
              onPointerDown={(e) => { if (entry.id.trim()) { e.preventDefault(); handleDragStart(entry.id); } }}
              onPointerEnter={() => { if (entry.id.trim()) handleDragEnter(entry.id); }}
            >
              <span className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full transition-colors ${
                isEditing ? "bg-primary" : entry.enabled ? "bg-blue-500" : "bg-muted-foreground/30"
              }`} />
              <span className={entry.enabled ? "" : "line-through opacity-60"}>
                {entry.name}
              </span>
              <button
                type="button"
                onClick={(e) => { e.stopPropagation(); setEditingIndex(isEditing ? null : originalIndex); }}
                onPointerDown={(e) => e.stopPropagation()}
                className="ml-0.5 hidden rounded p-0.5 transition-colors group-hover:inline-flex hover:bg-black/10 dark:hover:bg-white/10"
                title="编辑"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="m12 20 9-11-4-4-9 11 1 5 3-1Z" />
                </svg>
              </button>
              <button
                type="button"
                onClick={(e) => { e.stopPropagation(); onRemoveModel(originalIndex); }}
                onPointerDown={(e) => e.stopPropagation()}
                className="hidden rounded p-0.5 transition-colors group-hover:inline-flex hover:bg-destructive/15 hover:text-destructive"
                title="删除"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M18 6 6 18" /><path d="m6 6 12 12" />
                </svg>
              </button>
            </span>
          );
        })}

        {/* Add custom */}
        {!showAddForm && (
          <button
            type="button"
            onClick={() => setShowAddForm(true)}
            className="inline-flex items-center gap-1 rounded-[8px] border border-dashed border-border px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:border-foreground/20 hover:bg-secondary/50 hover:text-foreground"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 5v14" /><path d="M5 12h14" /></svg>
            自定义
          </button>
        )}
      </div>

      {/* Inline editor — discovered model override */}
      {editingDiscoveredId && (() => {
        const model = trueDiscoveredModels.find((m) => m.id === editingDiscoveredId);
        if (!model) return null;
        const overrideIdx = findOverrideIndex(model.id);
        const overrideConfig = overrideIdx >= 0 ? customModels[overrideIdx] : null;
        return (
          <DiscoveredModelEditRow
            model={model}
            override={overrideConfig}
            onOverride={(field, value) => handleDiscoveredOverride(model, field, value)}
            onResetOverride={() => handleRemoveDiscoveredOverride(model.id)}
            onDone={() => setEditingDiscoveredId(null)}
          />
        );
      })()}

      {/* Inline editor — custom model */}
      {editingIndex !== null && (() => {
        const entry = pureCustomEntries.find((e) => e.originalIndex === editingIndex);
        if (!entry) return null;
        return (
          <CustomModelEditRow
            model={entry.model}
            isDiscovered={false}
            onUpdate={(field, value) => onUpdateModel(entry.originalIndex, field, value)}
            onDone={() => setEditingIndex(null)}
            onRemove={() => { onRemoveModel(entry.originalIndex); setEditingIndex(null); }}
          />
        );
      })()}

      {/* New custom model form */}
      {showAddForm && (
        <NewCustomModelForm
          onAdd={(newModel) => {
            onAddModel(newModel);
            setShowAddForm(false);
          }}
          onCancel={() => setShowAddForm(false)}
        />
      )}
      </div>
      <BulkModelManagementPanel
        open={bulkPanelOpen}
        entries={bulkEntries}
        customModels={customModels}
        trueDiscoveredIds={trueDiscoveredIds}
        blockedModels={blockedModels}
        saving={saving}
        onClose={() => setBulkPanelOpen(false)}
        onSetBlockedModels={onSetBlockedModels}
        onUpdateEntry={updateBulkEntry}
        onReplaceModels={onReplaceModels}
        onSave={onSave}
      />
    </>
  );
}

/** 内联编辑某个自定义模型的行 */
function CustomModelEditRow({
  model,
  isDiscovered,
  onUpdate,
  onDone,
  onRemove,
}: {
  model: LlmProviderModelConfig;
  isDiscovered: boolean;
  onUpdate: (field: keyof LlmProviderModelConfig, value: string | number | boolean) => void;
  onDone: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="rounded-[8px] border border-border bg-background/80 p-3">
      {/* ID + Name 行 */}
      <div className="flex flex-wrap items-center gap-2 mb-3">
        {/* eslint-disable-next-line no-restricted-syntax -- 状态指示圆点 */}
        <span className="inline-block h-2 w-2 rounded-full bg-info" />
        <input
          type="text"
          className={`${inputCls} !w-40 !py-1 !px-2 ${isDiscovered ? "opacity-50 cursor-not-allowed" : ""}`}
          value={model.id}
          placeholder="模型 ID"
          onChange={(e) => onUpdate("id", e.target.value)}
          disabled={isDiscovered}
          autoFocus={!isDiscovered}
        />
        <input
          type="text"
          className={`${inputCls} !w-32 !py-1 !px-2 ${isDiscovered ? "opacity-50 cursor-not-allowed" : ""}`}
          value={model.name}
          placeholder="显示名称"
          onChange={(e) => onUpdate("name", e.target.value)}
          disabled={isDiscovered}
        />
      </div>
      {/* 属性行 */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          上下文
          <input
            type="number"
            className={`${inputCls} !w-[72px] !py-1 !px-2 text-center`}
            value={Math.round(model.context_window / 1000)}
            placeholder="200"
            onChange={(e) => onUpdate("context_window", (parseInt(e.target.value) || 0) * 1000)}
          />
          <span>k</span>
        </label>

        <TogglePill label="推理" checked={model.reasoning} onChange={(v) => onUpdate("reasoning", v)} />
        <TogglePill label="图像" checked={model.supports_image} onChange={(v) => onUpdate("supports_image", v)} />

        <div className="flex items-center gap-1.5 ml-auto">
          <button
            type="button"
            onClick={onRemove}
            className="rounded-[6px] border border-destructive/30 px-2 py-1 text-[11px] text-destructive transition-colors hover:bg-destructive/10"
          >
            删除
          </button>
          <button
            type="button"
            onClick={onDone}
            className="rounded-[6px] bg-primary px-3 py-1 text-[11px] text-primary-foreground transition-colors hover:opacity-90"
          >
            完成
          </button>
        </div>
      </div>
    </div>
  );
}

/** Discovered 模型属性编辑行 — 模型 ID 和显示名称锁定 */
function DiscoveredModelEditRow({
  model,
  override,
  onOverride,
  onResetOverride,
  onDone,
}: {
  model: ModelInfo;
  override: LlmProviderModelConfig | null;
  onOverride: (field: keyof LlmProviderModelConfig, value: string | number | boolean) => void;
  onResetOverride: () => void;
  onDone: () => void;
}) {
  const effectiveContextK = Math.round((override?.context_window ?? model.context_window) / 1000);
  const effectiveReasoning = override?.reasoning ?? model.reasoning;
  const effectiveImage = override?.supports_image ?? model.supports_image;

  return (
    <div className="rounded-[8px] border border-border bg-background/80 p-3">
      {/* 标题行 */}
      <div className="flex items-center gap-2 mb-3">
        {/* eslint-disable-next-line no-restricted-syntax -- 状态指示圆点 */}
        <span className="inline-block h-2 w-2 rounded-full bg-success" />
        <span className="text-xs font-medium text-foreground truncate">{model.name || model.id}</span>
        <code className="ml-auto text-[10px] text-muted-foreground/60 font-mono truncate max-w-[180px]">{model.id}</code>
      </div>
      {/* 属性行 */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          上下文
          <input
            type="number"
            className={`${inputCls} !w-[72px] !py-1 !px-2 text-center`}
            value={effectiveContextK}
            onChange={(e) => onOverride("context_window", (parseInt(e.target.value) || 0) * 1000)}
          />
          <span>k</span>
        </label>

        <TogglePill label="推理" checked={effectiveReasoning} onChange={(v) => onOverride("reasoning", v)} />
        <TogglePill label="图像" checked={effectiveImage} onChange={(v) => onOverride("supports_image", v)} />

        <div className="flex items-center gap-1.5 ml-auto">
          {override && (
            <button
              type="button"
              onClick={onResetOverride}
              className="rounded-[6px] border border-border px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            >
              重置
            </button>
          )}
          <button
            type="button"
            onClick={onDone}
            className="rounded-[6px] bg-primary px-3 py-1 text-[11px] text-primary-foreground transition-colors hover:opacity-90"
          >
            完成
          </button>
        </div>
      </div>
    </div>
  );
}

/** 小型开关药丸 — 替代裸 checkbox，视觉更统一 */
function TogglePill({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      onClick={() => onChange(!checked)}
      className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] font-medium transition-all ${
        checked
          ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
          : "border-border bg-muted/40 text-muted-foreground"
      }`}
    >
      <span className={`inline-block h-1.5 w-1.5 rounded-full transition-colors ${
        checked ? "bg-emerald-500" : "bg-muted-foreground/30"
      }`} />
      {label}
    </button>
  );
}

/** 新建自定义模型的内联表单 */
function NewCustomModelForm({
  onAdd,
  onCancel,
}: {
  onAdd: (model: LlmProviderModelConfig) => void;
  onCancel: () => void;
}) {
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [contextWindowK, setContextWindowK] = useState(200);
  const [reasoning, setReasoning] = useState(true);
  const [supportsImage, setSupportsImage] = useState(true);

  const handleSubmit = () => {
    const trimmedId = id.trim();
    if (!trimmedId) return;
    onAdd({
      id: trimmedId,
      name: name.trim(),
      context_window: contextWindowK * 1000,
      reasoning,
      supports_image: supportsImage,
    });
  };

  return (
    <div className="rounded-[8px] border border-border bg-background/80 p-3">
      <p className="text-xs font-medium text-foreground mb-3">添加自定义模型</p>
      {/* ID + Name */}
      <div className="flex flex-wrap items-center gap-2 mb-3">
        {/* eslint-disable-next-line no-restricted-syntax -- 状态指示圆点 */}
        <span className="inline-block h-2 w-2 rounded-full bg-info/50" />
        <input type="text" className={`${inputCls} !w-40 !py-1 !px-2`} value={id} placeholder="模型 ID（必填）" onChange={(e) => setId(e.target.value)} autoFocus />
        <input type="text" className={`${inputCls} !w-32 !py-1 !px-2`} value={name} placeholder="显示名称" onChange={(e) => setName(e.target.value)} />
      </div>
      {/* 属性行 */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          上下文
          <input type="number" className={`${inputCls} !w-[72px] !py-1 !px-2 text-center`} value={contextWindowK} placeholder="200" onChange={(e) => setContextWindowK(parseInt(e.target.value) || 200)} />
          <span>k</span>
        </label>

        <TogglePill label="推理" checked={reasoning} onChange={setReasoning} />
        <TogglePill label="图像" checked={supportsImage} onChange={setSupportsImage} />

        <div className="flex items-center gap-1.5 ml-auto">
          <button type="button" onClick={onCancel} className="rounded-[6px] border border-border px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary">取消</button>
          <button type="button" onClick={handleSubmit} disabled={!id.trim()} className="rounded-[6px] bg-primary px-3 py-1 text-[11px] text-primary-foreground transition-colors hover:opacity-90 disabled:opacity-50">添加</button>
        </div>
      </div>
    </div>
  );
}
