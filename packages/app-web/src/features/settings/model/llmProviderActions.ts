import {
  llmProvidersApi,
  type CodexOAuthStatusResponse,
  type ProbeLlmProviderModelsRequest,
  type ProbeModelEntry,
  type StartCodexOAuthResponse,
} from "../../../api/llmProviders";

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
    start: () => llmProvidersApi.startCodexOAuth(providerId),
    getStatus: llmProvidersApi.getCodexOAuthStatus,
    cancel: llmProvidersApi.cancelCodexOAuth,
  };
}
