import { api } from './client';

// ─── Types ───

export interface LlmProvider {
  id: string;
  name: string;
  slug: string;
  protocol: 'anthropic' | 'gemini' | 'openai_compatible';
  api_key: string;
  api_key_configured: boolean;
  base_url: string;
  wire_api: string;
  default_model: string;
  models: unknown;
  blocked_models: unknown;
  env_api_key: string;
  discovery_url: string;
  sort_order: number;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateLlmProviderRequest {
  name: string;
  slug: string;
  protocol: string;
  api_key?: string;
  base_url?: string;
  wire_api?: string;
  default_model?: string;
  models?: unknown;
  blocked_models?: unknown;
  env_api_key?: string;
  discovery_url?: string;
  enabled?: boolean;
}

export interface UpdateLlmProviderRequest {
  name?: string;
  protocol?: string;
  api_key?: string;
  base_url?: string;
  wire_api?: string;
  default_model?: string;
  models?: unknown;
  blocked_models?: unknown;
  env_api_key?: string;
  discovery_url?: string;
  sort_order?: number;
  enabled?: boolean;
}

// ─── API ───

export const llmProvidersApi = {
  list: () => api.get<LlmProvider[]>('/llm-providers'),

  get: (id: string) => api.get<LlmProvider>(`/llm-providers/${id}`),

  create: (req: CreateLlmProviderRequest) =>
    api.post<LlmProvider>('/llm-providers', req),

  update: (id: string, req: UpdateLlmProviderRequest) =>
    api.put<LlmProvider>(`/llm-providers/${id}`, req),

  delete: (id: string) => api.delete<{ deleted: boolean }>(`/llm-providers/${id}`),

  reorder: (ids: string[]) =>
    api.post<{ reordered: boolean }>('/llm-providers/reorder', { ids }),
};
