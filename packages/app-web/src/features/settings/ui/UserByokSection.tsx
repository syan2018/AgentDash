import { useEffect, useState } from "react";
import { llmProvidersApi } from "../../../api/llmProviders";
import { useLlmByokStore } from "../../../stores/llmProviderStore";
import { OAuthLoginWizard } from "./OAuthLoginWizard";
import { btnPrimaryCls, inputCls, SectionCard } from "./primitives";

export function UserByokSection({ onRefreshModels }: { onRefreshModels: () => void }) {
  const {
    providers,
    loading,
    error,
    saving,
    fetchProviders,
    saveCredential,
    deleteCredential,
  } = useLlmByokStore();
  const [drafts, setDrafts] = useState<Record<string, string>>({});

  useEffect(() => {
    void fetchProviders();
  }, [fetchProviders]);

  const updateDraft = (providerId: string, value: string) => {
    setDrafts((current) => ({ ...current, [providerId]: value }));
  };

  const handleSaveCredential = async (providerId: string) => {
    const apiKey = drafts[providerId]?.trim() ?? "";
    if (!apiKey) return;
    const saved = await saveCredential(providerId, apiKey);
    if (saved) {
      setDrafts((current) => ({ ...current, [providerId]: "" }));
      onRefreshModels();
    }
  };

  const handleDeleteCredential = async (providerId: string) => {
    await deleteCredential(providerId);
    onRefreshModels();
  };

  const handleOAuthCredentialChanged = async () => {
    await fetchProviders();
    onRefreshModels();
  };

  return (
    <SectionCard title="个人 BYOK">
      <p className="text-xs text-muted-foreground -mt-2">
        个人凭据只对当前账号生效，会优先用于允许 BYOK 的 Provider。
      </p>
      {error && (
        <p className="rounded-[8px] border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {error}
        </p>
      )}
      {loading ? (
        <p className="text-xs text-muted-foreground py-2">加载中…</p>
      ) : (
        <div className="space-y-2">
          {providers.map((provider) => {
            const canByok = provider.credential_mode === "global_or_user" || provider.credential_mode === "user_required";
            const draft = drafts[provider.id] ?? "";
            return (
              <div key={provider.id} className="rounded-[8px] border border-border bg-background/80 px-4 py-3">
                <div className="flex flex-wrap items-start gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-medium text-foreground">{provider.name}</p>
                    <p className="text-xs text-muted-foreground">
                      {provider.slug} · {providerStatusLabel(provider.status)}
                    </p>
                  </div>
                  <span className="rounded-[6px] border border-border bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                    {credentialModeLabel(provider.credential_mode)}
                  </span>
                </div>

                {canByok && provider.protocol === "openai_codex" ? (
                  <div className="mt-3 grid gap-2 md:grid-cols-[1fr_auto]">
                    <OAuthLoginWizard
                      start={() => llmProvidersApi.startUserCodexOAuth(provider.id)}
                      getStatus={llmProvidersApi.getCodexOAuthStatus}
                      cancel={llmProvidersApi.cancelCodexOAuth}
                      onCompleted={handleOAuthCredentialChanged}
                      idleLabel={provider.user_api_key_configured ? "重新登录 ChatGPT" : "通过 ChatGPT 登录"}
                      authLinkLabel="打开 ChatGPT 授权页"
                      openedMessage="已在外部浏览器打开 ChatGPT 授权页，等待授权完成…"
                      manualMessage="请打开 ChatGPT 授权页并完成登录，完成后这里会自动更新状态。"
                      completedMessage="个人 Codex 登录已完成"
                      failedMessage="个人 Codex 登录失败"
                      disabled={saving}
                      className="bg-background/80"
                    />
                    <button
                      type="button"
                      className="h-fit rounded-[8px] border border-border px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
                      disabled={saving || !provider.user_api_key_configured}
                      onClick={() => void handleDeleteCredential(provider.id)}
                    >
                      删除
                    </button>
                  </div>
                ) : canByok ? (
                  <div className="mt-3 grid gap-2 md:grid-cols-[1fr_auto_auto]">
                    <input
                      type="password"
                      className={inputCls}
                      value={draft}
                      placeholder={provider.user_api_key_configured ? provider.user_api_key_preview ?? "已配置个人 Key" : "输入个人 API Key"}
                      onChange={(e) => updateDraft(provider.id, e.target.value)}
                    />
                    <button
                      type="button"
                      className={btnPrimaryCls}
                      disabled={saving || draft.trim().length === 0}
                      onClick={() => void handleSaveCredential(provider.id)}
                    >
                      保存
                    </button>
                    <button
                      type="button"
                      className="rounded-[8px] border border-border px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
                      disabled={saving || !provider.user_api_key_configured}
                      onClick={() => void handleDeleteCredential(provider.id)}
                    >
                      删除
                    </button>
                  </div>
                ) : (
                  <p className="mt-3 text-xs text-muted-foreground">该 Provider 由平台统一提供，不需要配置个人 Key。</p>
                )}
              </div>
            );
          })}
        </div>
      )}
    </SectionCard>
  );
}

function credentialModeLabel(mode: string) {
  switch (mode) {
    case "global_or_user":
      return "允许 BYOK";
    case "user_required":
      return "需要 BYOK";
    case "global_only":
      return "平台托管";
    default:
      return mode;
  }
}

function providerStatusLabel(status: string) {
  switch (status) {
    case "platform_provided":
      return "平台 Key 生效";
    case "user_byok_active":
      return "个人 Key 生效";
    case "needs_user_key":
      return "需要个人 Key";
    case "disabled":
      return "已禁用";
    case "no_key_endpoint":
      return "无需 Key 的端点";
    case "unavailable":
      return "暂不可用";
    default:
      return status;
  }
}
