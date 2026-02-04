/**
 * React Query hooks for API Keys
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient, API_ENDPOINTS } from '../api-client';
import type { ApiKey, ApiKeyCreated, CreateApiKeyRequest } from '../types';

// Fetch API keys for an organization
export function useApiKeys(organizationId: string) {
  return useQuery({
    queryKey: ['api-keys', organizationId],
    queryFn: () => apiClient.get<ApiKey[]>(API_ENDPOINTS.organizationApiKeys(organizationId)),
    enabled: !!organizationId,
  });
}

// Fetch single API key
export function useApiKey(id: string) {
  return useQuery({
    queryKey: ['api-key', id],
    queryFn: () => apiClient.get<ApiKey>(API_ENDPOINTS.apiKey(id)),
    enabled: !!id,
  });
}

// Create API key mutation
export function useCreateApiKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ organizationId, data }: { organizationId: string; data: CreateApiKeyRequest }) =>
      apiClient.post<ApiKeyCreated>(API_ENDPOINTS.organizationApiKeys(organizationId), data),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['api-keys', variables.organizationId] });
    },
  });
}

// Revoke API key mutation
export function useRevokeApiKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, organizationId }: { id: string; organizationId: string }) =>
      apiClient.delete(API_ENDPOINTS.apiKey(id)),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['api-keys', variables.organizationId] });
    },
  });
}
