import { api } from './client';
import type {
  CodexOAuthStatusResponse,
  CreateLlmProviderRequest,
  DeleteLlmProviderUserCredentialResponse,
  EffectiveLlmProviderDto,
  JsonValue,
  LlmProviderAdminDto,
  ProbeLlmProviderModelDto,
  ProbeLlmProviderModelsRequest,
  StartCodexOAuthResponse,
  UpdateLlmProviderRequest,
  UpsertLlmProviderUserCredentialRequest,
} from '../generated/llm-provider-contracts';

export type LlmProvider = LlmProviderAdminDto;
export type EffectiveLlmProvider = EffectiveLlmProviderDto;
export type ProbeModelEntry = ProbeLlmProviderModelDto;
export type {
  CreateLlmProviderRequest,
  CodexOAuthStatusResponse,
  JsonValue,
  UpdateLlmProviderRequest,
  ProbeLlmProviderModelsRequest,
  StartCodexOAuthResponse,
};

export const llmProvidersApi = {
  list: () => api.get<LlmProvider[]>('/llm-providers'),

  listEffective: () => api.get<EffectiveLlmProvider[]>('/llm-providers/effective'),

  get: (id: string) => api.get<LlmProvider>(`/llm-providers/${id}`),

  create: (req: CreateLlmProviderRequest) =>
    api.post<LlmProvider>('/llm-providers', req),

  update: (id: string, req: UpdateLlmProviderRequest) =>
    api.put<LlmProvider>(`/llm-providers/${id}`, req),

  delete: (id: string) => api.delete<{ deleted: boolean }>(`/llm-providers/${id}`),

  reorder: (ids: string[]) =>
    api.post<{ reordered: boolean }>('/llm-providers/reorder', { ids }),

  probeModels: (req: ProbeLlmProviderModelsRequest) =>
    api.post<ProbeModelEntry[]>('/llm-providers/probe-models', req),

  probeUserModels: (providerId: string, req: ProbeLlmProviderModelsRequest) =>
    api.post<ProbeModelEntry[]>(`/llm-providers/${providerId}/probe-models`, req),

  saveUserCredential: (providerId: string, req: UpsertLlmProviderUserCredentialRequest) =>
    api.put<EffectiveLlmProvider>(`/llm-providers/${providerId}/user-credential`, req),

  deleteUserCredential: (providerId: string) =>
    api.delete<DeleteLlmProviderUserCredentialResponse>(`/llm-providers/${providerId}/user-credential`),

  startCodexOAuth: (providerId: string) =>
    api.post<StartCodexOAuthResponse>(`/llm-providers/${providerId}/codex-oauth/start`, {}),

  startUserCodexOAuth: (providerId: string) =>
    api.post<StartCodexOAuthResponse>(`/llm-providers/${providerId}/user-credential/codex-oauth/start`, {}),

  getCodexOAuthStatus: (flowId: string) =>
    api.get<CodexOAuthStatusResponse>(`/llm-providers/codex-oauth/${flowId}`),

  cancelCodexOAuth: (flowId: string) =>
    api.post<CodexOAuthStatusResponse>(`/llm-providers/codex-oauth/${flowId}/cancel`, {}),
};
