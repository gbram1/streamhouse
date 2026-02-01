/**
 * React Query hooks for Topics
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient, API_ENDPOINTS } from '../api-client';
import type { Topic, TopicDetail, TopicCreateRequest, Message } from '../types';

// Fetch all topics
export function useTopics() {
  return useQuery({
    queryKey: ['topics'],
    queryFn: () => apiClient.get<Topic[]>(API_ENDPOINTS.topics),
  });
}

// Fetch single topic details
export function useTopic(name: string) {
  return useQuery({
    queryKey: ['topic', name],
    queryFn: () => apiClient.get<TopicDetail>(API_ENDPOINTS.topic(name)),
    enabled: !!name,
  });
}

// Fetch topic messages
export function useTopicMessages(
  name: string,
  options: { offset?: number; limit?: number; partition?: number } = {}
) {
  return useQuery({
    queryKey: ['topic-messages', name, options],
    queryFn: () =>
      apiClient.get<Message[]>(
        API_ENDPOINTS.topicMessages(name),
        {
          offset: String(options.offset || 0),
          limit: String(options.limit || 100),
          ...(options.partition !== undefined && { partition: String(options.partition) }),
        }
      ),
    enabled: !!name,
  });
}

// Create topic mutation
export function useCreateTopic() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: TopicCreateRequest) =>
      apiClient.post<Topic>(API_ENDPOINTS.topics, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['topics'] });
    },
  });
}

// Delete topic mutation
export function useDeleteTopic() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (name: string) =>
      apiClient.delete(API_ENDPOINTS.topic(name)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['topics'] });
    },
  });
}

// Update topic config mutation
export function useUpdateTopicConfig() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ name, config }: { name: string; config: Record<string, string> }) =>
      apiClient.put(API_ENDPOINTS.topicConfig(name), config),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['topic', variables.name] });
    },
  });
}
