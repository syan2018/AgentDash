import {
  llmProvidersApi,
  type CodexOAuthStatusResponse,
  type ProbeLlmProviderModelsRequest,
  type ProbeModelEntry,
  type StartCodexOAuthResponse,
} from "../../../api/llmProviders";
import { getStoredToken } from "../../../api/client";
import { API_ORIGIN } from "../../../api/origin";
import { getDesktopAppBridge } from "../../../desktop/localRuntimeBridge";

export interface CodexOAuthActions {
  start: () => Promise<StartCodexOAuthResponse>;
  getStatus: (flowId: string) => Promise<CodexOAuthStatusResponse>;
  cancel: (flowId: string) => Promise<CodexOAuthStatusResponse>;
}

export function probeLlmProviderModels(
  request: ProbeLlmProviderModelsRequest,
): Promise<ProbeModelEntry[]> {
  return llmProvidersApi.probeModels(request);
}

export function createAdminCodexOAuthActions(providerId: string): CodexOAuthActions {
  return {
    start: () => startDesktopCodexOAuth(providerId, "global_provider"),
    getStatus: llmProvidersApi.getCodexOAuthStatus,
    cancel: cancelDesktopCodexOAuth,
  };
}

export function createUserCodexOAuthActions(providerId: string): CodexOAuthActions {
  return {
    start: () => startDesktopCodexOAuth(providerId, "user_byok"),
    getStatus: llmProvidersApi.getCodexOAuthStatus,
    cancel: cancelDesktopCodexOAuth,
  };
}

export function hasDesktopCodexOAuthBridge(): boolean {
  return !!getDesktopAppBridge()?.startCodexOAuth;
}

async function startDesktopCodexOAuth(
  providerId: string,
  target: "global_provider" | "user_byok",
): Promise<StartCodexOAuthResponse> {
  const desktopApp = getDesktopAppBridge();
  if (!desktopApp?.startCodexOAuth) {
    throw new Error("ChatGPT OAuth 需要在 AgentDash 桌面端完成");
  }
  const accessToken = getStoredToken();
  if (!accessToken) {
    throw new Error("ChatGPT OAuth 需要当前登录会话");
  }
  const apiOrigin = await resolveDesktopApiOrigin(desktopApp);
  return desktopApp.startCodexOAuth({
    api_origin: apiOrigin,
    access_token: accessToken,
    provider_id: providerId,
    target,
  });
}

async function cancelDesktopCodexOAuth(flowId: string): Promise<CodexOAuthStatusResponse> {
  const desktopApp = getDesktopAppBridge();
  if (desktopApp?.cancelCodexOAuth) {
    return desktopApp.cancelCodexOAuth(flowId);
  }
  return llmProvidersApi.cancelCodexOAuth(flowId);
}

async function resolveDesktopApiOrigin(
  desktopApp: NonNullable<ReturnType<typeof getDesktopAppBridge>>,
): Promise<string> {
  if (API_ORIGIN) return API_ORIGIN;
  const snapshot = await desktopApp.getDesktopApiSnapshot().catch(() => null);
  const origin = snapshot?.origin?.trim().replace(/\/+$/, "");
  if (origin) return origin;
  throw new Error("无法确定 Dashboard API 地址，无法启动 ChatGPT OAuth");
}
