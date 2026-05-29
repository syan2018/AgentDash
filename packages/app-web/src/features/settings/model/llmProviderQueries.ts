import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  llmProvidersApi,
  type CreateLlmProviderRequest,
  type UpdateLlmProviderRequest,
} from "../../../api/llmProviders";

export const llmProvidersKey = ["llm-providers"] as const;
export const effectiveLlmProvidersKey = ["llm-providers", "effective"] as const;

export function useLlmProvidersQuery() {
  return useQuery({
    queryKey: llmProvidersKey,
    queryFn: llmProvidersApi.list,
  });
}

export function useEffectiveLlmProvidersQuery() {
  return useQuery({
    queryKey: effectiveLlmProvidersKey,
    queryFn: llmProvidersApi.listEffective,
  });
}

export function useCreateLlmProviderMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: CreateLlmProviderRequest) => llmProvidersApi.create(request),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: llmProvidersKey });
    },
  });
}

export function useUpdateLlmProviderMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: UpdateLlmProviderRequest }) =>
      llmProvidersApi.update(id, request),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: llmProvidersKey });
      await queryClient.invalidateQueries({ queryKey: effectiveLlmProvidersKey });
    },
  });
}

export function useDeleteLlmProviderMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => llmProvidersApi.delete(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: llmProvidersKey });
      await queryClient.invalidateQueries({ queryKey: effectiveLlmProvidersKey });
    },
  });
}

export function useSaveUserCredentialMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ providerId, apiKey }: { providerId: string; apiKey: string }) =>
      llmProvidersApi.saveUserCredential(providerId, { api_key: apiKey }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: effectiveLlmProvidersKey });
    },
  });
}

export function useVerifyUserCredentialMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (providerId: string) => llmProvidersApi.verifyUserCredential(providerId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: effectiveLlmProvidersKey });
    },
  });
}

export function useDeleteUserCredentialMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (providerId: string) => llmProvidersApi.deleteUserCredential(providerId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: effectiveLlmProvidersKey });
    },
  });
}
