/**
 * React Query hooks for Connectors
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient, API_ENDPOINTS } from '../api-client';

export interface Connector {
  name: string;
  connectorType: string;
  connectorClass: string;
  topics: string[];
  config: Record<string, string>;
  state: string;
  errorMessage: string | null;
  recordsProcessed: number;
  createdAt: string;
  updatedAt: string;
}

export interface CreateConnectorRequest {
  name: string;
  connectorType: string;
  connectorClass: string;
  topics: string[];
  config?: Record<string, string>;
}

// Fetch all connectors
export function useConnectors() {
  return useQuery({
    queryKey: ['connectors'],
    queryFn: async () => {
      const connectors = await apiClient.get<Connector[]>(API_ENDPOINTS.connectors);
      return connectors;
    },
    refetchInterval: 5000,
  });
}

// Create connector mutation
export function useCreateConnector() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: CreateConnectorRequest) =>
      apiClient.post<Connector>(API_ENDPOINTS.connectors, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['connectors'] });
    },
  });
}

// Delete connector mutation
export function useDeleteConnector() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (name: string) =>
      apiClient.delete(API_ENDPOINTS.connector(name)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['connectors'] });
    },
  });
}

// Pause connector mutation
export function usePauseConnector() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (name: string) =>
      apiClient.post<Connector>(API_ENDPOINTS.connectorPause(name)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['connectors'] });
    },
  });
}

// Resume connector mutation
export function useResumeConnector() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (name: string) =>
      apiClient.post<Connector>(API_ENDPOINTS.connectorResume(name)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['connectors'] });
    },
  });
}
