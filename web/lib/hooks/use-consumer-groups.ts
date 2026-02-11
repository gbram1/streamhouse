/**
 * React Query hooks for Consumer Groups
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient, API_ENDPOINTS } from '../api-client';
import type { ConsumerGroup, ConsumerGroupDetail, ResetOffsetsRequest } from '../types';

// API response shape (what backend returns)
interface ConsumerGroupApiResponse {
  groupId: string;
  topics: string[];
  totalLag: number;
  partitionCount: number;
}

// Fetch all consumer groups
export function useConsumerGroups() {
  return useQuery({
    queryKey: ['consumer-groups'],
    queryFn: async () => {
      const data = await apiClient.get<ConsumerGroupApiResponse[]>(API_ENDPOINTS.consumerGroups);
      // Transform API response to match UI expected format
      return data.map((group): ConsumerGroup => ({
        id: group.groupId,
        state: (group as any).state || (group.partitionCount > 0 ? 'stable' : 'empty'),
        memberCount: group.partitionCount,
        totalLag: Math.max(0, group.totalLag),
        lagTrend: (group as any).lagTrend || (group.totalLag > 1000 ? 'increasing' : 'stable'),
      }));
    },
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
