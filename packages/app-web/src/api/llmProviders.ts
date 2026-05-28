import { api } from './client';
import type {
  CreateLlmProviderRequest,
  DeleteLlmProviderUserCredentialResponse,
  EffectiveLlmProviderDto,
  JsonValue,
  LlmProviderAdminDto,
  ProbeLlmProviderModelDto,
  ProbeLlmProviderModelsRequest,
  UpdateLlmProviderRequest,
  UpsertLlmProviderUserCredentialRequest,
} from '../generated/llm-provider-contracts';

export type LlmProvider = LlmProviderAdminDto;
export type EffectiveLlmProvider = EffectiveLlmProviderDto;
export type ProbeModelEntry = ProbeLlmProviderModelDto;
export type {
  CreateLlmProviderRequest,
  JsonValue,
  UpdateLlmProviderRequest,
  ProbeLlmProviderModelsRequest,
};

export interface StartCodexOAuthResponse {
  flow_id: string;
  auth_url: string;
  expires_at: string;
}

export interface CodexOAuthStatusResponse {
  flow_id: string;
  status: 'pending' | 'completed' | 'failed';
  message?: string;
}

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

  getCodexOAuthStatus: (flowId: string) =>
    api.get<CodexOAuthStatusResponse>(`/llm-providers/codex-oauth/${flowId}`),

  cancelCodexOAuth: (flowId: string) =>
    api.post<CodexOAuthStatusResponse>(`/llm-providers/codex-oauth/${flowId}/cancel`, {}),
};
