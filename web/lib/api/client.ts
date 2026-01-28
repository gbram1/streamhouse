/**
 * StreamHouse API Client
 *
 * This client will communicate with the REST API backend (to be implemented in Phase 10)
 */

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:3001';

export interface Topic {
  name: string;
  partitions: number;
  replication_factor: number;
  created_at: string;
  config: Record<string, string>;
}

export interface Agent {
  agent_id: string;
  address: string;
  availability_zone: string;
  agent_group: string;
  last_heartbeat: number;
  started_at: number;
  active_leases: number;
}

export interface Partition {
  topic: string;
  partition_id: number;
  leader_agent_id: string | null;
  high_watermark: number;
  low_watermark: number;
}

export interface ConsumerGroup {
  group_id: string;
  topic: string;
  members: number;
  state: string;
  total_lag: number;
}

export interface ProduceRequest {
  topic: string;
  key?: string;
  value: string;
  partition?: number;
}

export interface MetricsSnapshot {
  topics_count: number;
  agents_count: number;
  throughput_per_sec: number;
  total_storage_bytes: number;
}

export interface ConsumedRecord {
  partition: number;
  offset: number;
  key: string | null;
  value: string;
  timestamp: number;
}

export interface ConsumeResponse {
  records: ConsumedRecord[];
  nextOffset: number;
}

class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string = API_BASE_URL) {
    this.baseUrl = baseUrl;
  }

  private async request<T>(
    path: string,
    options?: RequestInit
  ): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`API Error: ${response.status} - ${error}`);
    }

    return response.json();
  }

  // Topics
  async listTopics(): Promise<Topic[]> {
    return this.request<Topic[]>('/api/v1/topics');
  }

  async getTopic(name: string): Promise<Topic> {
    return this.request<Topic>(`/api/v1/topics/${name}`);
  }

  async createTopic(
    name: string,
    partitions: number,
    replicationFactor: number = 1
  ): Promise<Topic> {
    return this.request<Topic>('/api/v1/topics', {
      method: 'POST',
      body: JSON.stringify({
        name,
        partitions,
        replication_factor: replicationFactor,
      }),
    });
  }

  async deleteTopic(name: string): Promise<void> {
    await this.request(`/api/v1/topics/${name}`, {
      method: 'DELETE',
    });
  }

  // Partitions
  async listPartitions(topic: string): Promise<Partition[]> {
    return this.request<Partition[]>(`/api/v1/topics/${topic}/partitions`);
  }

  async getPartition(topic: string, partitionId: number): Promise<Partition> {
    return this.request<Partition>(
      `/api/v1/topics/${topic}/partitions/${partitionId}`
    );
  }

  // Agents
  async listAgents(): Promise<Agent[]> {
    return this.request<Agent[]>('/api/v1/agents');
  }

  async getAgent(agentId: string): Promise<Agent> {
    return this.request<Agent>(`/api/v1/agents/${agentId}`);
  }

  // Consumer Groups
  async listConsumerGroups(): Promise<ConsumerGroup[]> {
    return this.request<ConsumerGroup[]>('/api/v1/consumer-groups');
  }

  async getConsumerGroup(groupId: string): Promise<ConsumerGroup> {
    return this.request<ConsumerGroup>(`/api/v1/consumer-groups/${groupId}`);
  }

  // Produce
  async produce(request: ProduceRequest): Promise<{ offset: number }> {
    return this.request<{ offset: number }>('/api/v1/produce', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  // Consume
  async consume(
    topic: string,
    partition: number,
    offset: number = 0,
    maxRecords: number = 100
  ): Promise<ConsumeResponse> {
    const params = new URLSearchParams({
      topic,
      partition: partition.toString(),
      offset: offset.toString(),
      maxRecords: maxRecords.toString(),
    });
    return this.request<ConsumeResponse>(`/api/v1/consume?${params}`);
  }

  // Metrics
  async getMetrics(): Promise<MetricsSnapshot> {
    return this.request<MetricsSnapshot>('/api/v1/metrics');
  }

  // Health
  async health(): Promise<{ status: string }> {
    return this.request<{ status: string }>('/health');
  }
}

export const apiClient = new ApiClient();
