/**
 * React Query hooks for Topics
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient, API_ENDPOINTS } from '../api-client';
import type { Topic, TopicDetail, TopicCreateRequest, Message } from '../types';

// Transform API response to frontend format (snake_case to camelCase)
function transformTopic(apiTopic: any): Topic {
  return {
    name: apiTopic.name,
    partitionCount: apiTopic.partitions || apiTopic.partition_count || apiTopic.partitionCount || 0,
    replication_factor: apiTopic.replication_factor || 1,
    retentionMs: apiTopic.retention_ms || apiTopic.retentionMs,
    messageCount: apiTopic.message_count || apiTopic.messageCount || 0,
    sizeBytes: apiTopic.size_bytes || apiTopic.sizeBytes || 0,
    messagesPerSecond: apiTopic.messages_per_second || apiTopic.messagesPerSecond || 0,
    createdAt: apiTopic.created_at || apiTopic.createdAt,
    config: apiTopic.config,
  };
}

// Fetch all topics
export function useTopics() {
  return useQuery({
    queryKey: ['topics'],
    queryFn: async () => {
      const topics = await apiClient.get<any[]>(API_ENDPOINTS.topics);
      return topics.map(transformTopic);
    },
  });
}

// Fetch single topic details
export function useTopic(name: string) {
  return useQuery({
    queryKey: ['topic', name],
    queryFn: async () => {
      const topic = await apiClient.get<any>(API_ENDPOINTS.topic(name));
      return transformTopic(topic);
    },
    enabled: !!name,
  });
}

// Transform API partition to frontend format
function transformPartition(apiPartition: any): {
  id: number;
  topic: string;
  leader: string | null;
  highWatermark: number;
  lowWatermark: number;
  messageCount: number;
  sizeBytes: number;
} {
  return {
    id: apiPartition.partition_id ?? apiPartition.id ?? 0,
    topic: apiPartition.topic || '',
    leader: apiPartition.leader_agent_id || apiPartition.leader || null,
    highWatermark: apiPartition.high_watermark ?? apiPartition.highWatermark ?? 0,
    lowWatermark: apiPartition.low_watermark ?? apiPartition.lowWatermark ?? 0,
    messageCount: apiPartition.high_watermark ?? apiPartition.messageCount ?? 0,
    sizeBytes: apiPartition.size_bytes ?? apiPartition.sizeBytes ?? 0,
  };
}

// Fetch topic partitions
export function useTopicPartitions(name: string) {
  return useQuery({
    queryKey: ['topic-partitions', name],
    queryFn: async () => {
      const partitions = await apiClient.get<any[]>(API_ENDPOINTS.topicPartitions(name));
      return partitions.map(transformPartition);
    },
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

// Fetch all partitions across all topics
export function useAllPartitions() {
  return useQuery({
    queryKey: ['all-partitions'],
    queryFn: async () => {
      // First get all topics
      const topics = await apiClient.get<any[]>(API_ENDPOINTS.topics);

      // Then fetch partitions for each topic
      const allPartitions = await Promise.all(
        topics.map(async (topic) => {
          try {
            const partitions = await apiClient.get<any[]>(
              API_ENDPOINTS.topicPartitions(topic.name)
            );
            return partitions.map(transformPartition);
          } catch {
            return [];
          }
        })
      );

      // Flatten and return
      return allPartitions.flat();
    },
  });
}
