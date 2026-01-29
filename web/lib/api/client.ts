/**
 * StreamHouse API Client
 *
 * This client will communicate with the REST API backend
 */

// Use relative URLs in browser (Next.js will proxy via rewrites)
// Use localhost:8080 in Node.js context (SSR)
const API_BASE_URL = typeof window !== 'undefined'
  ? '' // Browser: use relative URLs (proxied by Next.js)
  : process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080'; // Server: direct connection

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
  partitions_count: number;
  total_messages: number;
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

export interface ConsumerGroupInfo {
  groupId: string;
  topics: string[];
  totalLag: number;
  partitionCount: number;
}

export interface ConsumerGroupDetail {
  groupId: string;
  offsets: ConsumerOffsetInfo[];
}

export interface ConsumerOffsetInfo {
  topic: string;
  partitionId: number;
  committedOffset: number;
  highWatermark: number;
  lag: number;
}

export interface ConsumerGroupLag {
  groupId: string;
  totalLag: number;
  partitionCount: number;
  topics: string[];
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

  // Consumer Groups
  async listConsumerGroups(): Promise<ConsumerGroupInfo[]> {
    return this.request<ConsumerGroupInfo[]>('/api/v1/consumer-groups');
  }

  async getConsumerGroup(groupId: string): Promise<ConsumerGroupDetail> {
    return this.request<ConsumerGroupDetail>(`/api/v1/consumer-groups/${groupId}`);
  }

  async getConsumerGroupLag(groupId: string): Promise<ConsumerGroupLag> {
    return this.request<ConsumerGroupLag>(`/api/v1/consumer-groups/${groupId}/lag`);
  }

  // Health
  async health(): Promise<{ status: string }> {
    return this.request<{ status: string }>('/health');
  }
}

export const apiClient = new ApiClient();
