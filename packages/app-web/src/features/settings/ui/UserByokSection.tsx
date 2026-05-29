import { useState } from "react";
import { llmProvidersApi, type EffectiveLlmProvider } from "../../../api/llmProviders";
import {
  useDeleteUserCredentialMutation,
  useEffectiveLlmProvidersQuery,
  useSaveUserCredentialMutation,
  useVerifyUserCredentialMutation,
} from "../model/llmProviderQueries";
import { OAuthLoginWizard } from "./OAuthLoginWizard";
import { btnPrimaryCls, inputCls, SectionCard } from "./primitives";

export function UserByokSection({ onRefreshModels }: { onRefreshModels: () => void }) {
  const providersQuery = useEffectiveLlmProvidersQuery();
  const saveCredential = useSaveUserCredentialMutation();
  const verifyCredential = useVerifyUserCredentialMutation();
  const deleteCredential = useDeleteUserCredentialMutation();
  const [drafts, setDrafts] = useState<Record<string, string>>({});

  const providers = providersQuery.data ?? [];
  const error =
    providersQuery.error ??
    saveCredential.error ??
    verifyCredential.error ??
    deleteCredential.error;
  const saving =
    saveCredential.isPending ||
    verifyCredential.isPending ||
    deleteCredential.isPending;

  const updateDraft = (providerId: string, value: string) => {
    setDrafts((current) => ({ ...current, [providerId]: value }));
  };

  const handleSaveCredential = async (providerId: string) => {
    const apiKey = drafts[providerId]?.trim() ?? "";
    if (!apiKey) return;
    const saved = await saveCredential.mutateAsync({ providerId, apiKey });
    if (saved) {
      setDrafts((current) => ({ ...current, [providerId]: "" }));
      onRefreshModels();
    }
  };

  const handleDeleteCredential = async (providerId: string) => {
    await deleteCredential.mutateAsync(providerId);
    onRefreshModels();
  };

  const handleVerifyCredential = async (providerId: string) => {
    const verified = await verifyCredential.mutateAsync(providerId);
    if (verified) {
      onRefreshModels();
    }
  };

  const handleOAuthCredentialChanged = async () => {
    await providersQuery.refetch();
    onRefreshModels();
  };

  return (
    <SectionCard title="个人 BYOK">
      <p className="text-xs text-muted-foreground -mt-2">
        个人凭据只对当前账号生效，会优先用于允许 BYOK 的 Provider。
      </p>
      {error && (
        <p className="rounded-[8px] border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {error instanceof Error ? error.message : String(error)}
        </p>
      )}
      {providersQuery.isPending ? (
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
                      {provider.slug} · {providerStatusLabel(provider)}
                    </p>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    <CredentialVerificationBadge provider={provider} />
                    <span className="rounded-[6px] border border-border bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                      {credentialModeLabel(provider.credential_mode)}
                    </span>
                  </div>
                </div>

                {canByok && provider.protocol === "openai_codex" ? (
                  <CodexCredentialRow
                    provider={provider}
                    saving={saving}
                    onCompleted={handleOAuthCredentialChanged}
                    onDelete={() => void handleDeleteCredential(provider.id)}
                  />
                ) : canByok ? (
                  <div className="mt-3 space-y-2">
                    <div className="grid gap-2 md:grid-cols-[1fr_auto_auto_auto]">
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
                        保存并验证
                      </button>
                      <button
                        type="button"
                        className="rounded-[8px] border border-border px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
                        disabled={saving || !provider.user_api_key_configured}
                        onClick={() => void handleVerifyCredential(provider.id)}
                      >
                        验证
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
                    {provider.user_api_key_configured && (
                      <CredentialVerificationText provider={provider} />
                    )}
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

function CredentialVerificationBadge({ provider }: { provider: EffectiveLlmProvider }) {
  if (!provider.user_api_key_configured) return null;

  switch (provider.user_credential_verification_status) {
    case "verified":
      return (
        <span className="rounded-[6px] border border-success/40 bg-success/10 px-2 py-0.5 text-[11px] font-medium text-success">
          已验证
        </span>
      );
    case "failed":
      return (
        <span className="rounded-[6px] border border-destructive/40 bg-destructive/10 px-2 py-0.5 text-[11px] font-medium text-destructive">
          验证失败
        </span>
      );
    case "unverified":
    default:
      return (
        <span className="rounded-[6px] border border-border bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          未验证
        </span>
      );
  }
}

function CredentialVerificationText({ provider }: { provider: EffectiveLlmProvider }) {
  const message = provider.user_credential_verification_message ?? credentialVerificationLabel(provider);
  const toneClass = provider.user_credential_verification_status === "failed"
    ? "text-destructive"
    : "text-muted-foreground";

  return (
    <p className={`text-xs ${toneClass}`}>
      {message}
    </p>
  );
}

function credentialVerificationLabel(provider: EffectiveLlmProvider) {
  switch (provider.user_credential_verification_status) {
    case "verified":
      return provider.user_credential_verified_at
        ? `验证通过 · ${formatVerificationTime(provider.user_credential_verified_at)}`
        : "验证通过";
    case "failed":
      return "验证失败";
    case "unverified":
    default:
      return "尚未验证";
  }
}

function formatVerificationTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function CodexCredentialRow({
  provider,
  saving,
  onCompleted,
  onDelete,
}: {
  provider: EffectiveLlmProvider;
  saving: boolean;
  onCompleted: () => Promise<void>;
  onDelete: () => void;
}) {
  const statusText = provider.user_api_key_configured
    ? `${provider.user_api_key_preview ?? "ChatGPT OAuth"} · ${credentialVerificationLabel(provider)}`
    : "未验证";

  return (
    <div className="mt-3 rounded-[8px] border border-border bg-muted/20 px-3 py-3">
      <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <div className="min-w-0">
          <p className="text-sm font-medium text-foreground">ChatGPT OAuth</p>
          <p className="mt-1 text-xs text-muted-foreground">{statusText}</p>
        </div>
        <div className="flex shrink-0 flex-wrap items-start gap-2">
          <OAuthLoginWizard
            start={() => llmProvidersApi.startUserCodexOAuth(provider.id)}
            getStatus={llmProvidersApi.getCodexOAuthStatus}
            cancel={llmProvidersApi.cancelCodexOAuth}
            onCompleted={onCompleted}
            idleLabel={provider.user_api_key_configured ? "重新验证 ChatGPT" : "通过 ChatGPT 登录"}
            authLinkLabel="打开 ChatGPT 授权页"
            openedMessage="已在外部浏览器打开 ChatGPT 授权页，等待授权完成…"
            manualMessage="请打开 ChatGPT 授权页并完成登录，完成后这里会自动更新状态。"
            completedMessage="个人 Codex 登录已完成"
            failedMessage="个人 Codex 登录失败"
            disabled={saving}
            surface="inline"
          />
          <button
            type="button"
            className="rounded-[8px] border border-border px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
            disabled={saving || !provider.user_api_key_configured}
            onClick={onDelete}
          >
            移除验证
          </button>
        </div>
      </div>
    </div>
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

function providerStatusLabel(provider: EffectiveLlmProvider) {
  if (provider.protocol === "openai_codex") {
    if (!provider.enabled) return "已禁用";
    if (provider.user_api_key_configured) return "个人凭据生效";
    if (provider.status === "platform_provided") return "平台 ChatGPT 凭据生效";
    if (provider.credential_mode !== "global_only") return "需要 ChatGPT 登录";
  }

  switch (provider.status) {
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
      return provider.status;
  }
}
