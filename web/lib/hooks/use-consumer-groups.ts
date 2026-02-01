/**
 * React Query hooks for Consumer Groups
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient, API_ENDPOINTS } from '../api-client';
import type { ConsumerGroup, ConsumerGroupDetail, ResetOffsetsRequest } from '../types';

// Fetch all consumer groups
export function useConsumerGroups() {
  return useQuery({
    queryKey: ['consumer-groups'],
    queryFn: () => apiClient.get<ConsumerGroup[]>(API_ENDPOINTS.consumerGroups),
  });
}

// Fetch single consumer group details
export function useConsumerGroup(id: string) {
  return useQuery({
    queryKey: ['consumer-group', id],
    queryFn: () => apiClient.get<ConsumerGroupDetail>(API_ENDPOINTS.consumerGroup(id)),
    enabled: !!id,
  });
}

// Fetch consumer group lag
export function useConsumerGroupLag(id: string) {
  return useQuery({
    queryKey: ['consumer-group-lag', id],
    queryFn: () => apiClient.get(API_ENDPOINTS.consumerGroupLag(id)),
    enabled: !!id,
    refetchInterval: 5000, // Refresh lag every 5 seconds
  });
}

// Reset offsets mutation
export function useResetOffsets() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ groupId, data }: { groupId: string; data: ResetOffsetsRequest }) =>
      apiClient.post(API_ENDPOINTS.consumerGroupResetOffsets(groupId), data),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['consumer-group', variables.groupId] });
      queryClient.invalidateQueries({ queryKey: ['consumer-group-lag', variables.groupId] });
    },
  });
}

// Delete consumer group mutation
export function useDeleteConsumerGroup() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) =>
      apiClient.delete(API_ENDPOINTS.consumerGroup(id)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['consumer-groups'] });
    },
  });
}
