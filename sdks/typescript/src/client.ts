/**
 * StreamHouse TypeScript Client
 *
 * Provides a type-safe API for interacting with StreamHouse.
 */

import {
  Topic,
  Partition,
  ProduceResult,
  BatchRecord,
  BatchProduceResult,
  ConsumeResult,
  ConsumerGroup,
  ConsumerGroupDetail,
  Agent,
  SqlResult,
  MetricsSnapshot,
  StreamHouseClientOptions,
} from './types';

import {
  StreamHouseError,
  ConnectionError,
  NotFoundError,
  ValidationError,
  TimeoutError,
  ConflictError,
  AuthenticationError,
  ServerError,
} from './errors';

/**
 * Client for interacting with StreamHouse streaming data platform.
 *
 * @example
 * ```typescript
 * const client = new StreamHouseClient({ baseUrl: 'http://localhost:8080' });
 *
 * // Create a topic
 * const topic = await client.createTopic('events', 3);
 *
 * // Produce a message
 * const result = await client.produce('events', '{"event": "click"}');
 *
 * // Consume messages
 * const messages = await client.consume('events', 0);
 * ```
 */
export class StreamHouseClient {
  private readonly baseUrl: string;
  private readonly apiUrl: string;
  private readonly apiKey?: string;
  private readonly timeout: number;
  private readonly fetchFn: typeof fetch;

  /**
   * Create a new StreamHouse client.
   *
   * @param options - Client configuration options
   */
  constructor(options: StreamHouseClientOptions | string) {
    if (typeof options === 'string') {
      options = { baseUrl: options };
    }

    this.baseUrl = options.baseUrl.replace(/\/$/, '');
    this.apiUrl = `${this.baseUrl}/api/v1`;
    this.apiKey = options.apiKey;
    this.timeout = options.timeout ?? 30000;
    this.fetchFn = options.fetch ?? globalThis.fetch;

    if (!this.fetchFn) {
      throw new Error(
        'No fetch implementation found. Please provide one via options.fetch or use a Node.js version with native fetch support.'
      );
    }
  }

  /**
   * Get headers for requests.
   */
  private getHeaders(): Record<string, string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };
    if (this.apiKey) {
      headers['Authorization'] = `Bearer ${this.apiKey}`;
    }
    return headers;
  }

  /**
   * Handle HTTP error responses.
   */
  private async handleResponseError(response: Response): Promise<never> {
    let message: string;
    try {
      const data = await response.json();
      message = data.error ?? JSON.stringify(data);
    } catch {
      message = await response.text();
    }

    switch (response.status) {
      case 400:
        throw new ValidationError(message);
      case 401:
        throw new AuthenticationError(message);
      case 404:
        throw new NotFoundError(message);
      case 408:
        throw new TimeoutError(message);
      case 409:
        throw new ConflictError(message);
      default:
        if (response.status >= 500) {
          throw new ServerError(message, response.status);
        }
        throw new StreamHouseError(message, response.status);
    }
  }

  /**
   * Make an HTTP request.
   */
  private async request<T>(
    method: string,
    path: string,
    options?: {
      body?: unknown;
      params?: Record<string, string | number | undefined>;
    }
  ): Promise<T> {
    let url = `${this.apiUrl}${path}`;

    // Add query parameters
    if (options?.params) {
      const searchParams = new URLSearchParams();
      for (const [key, value] of Object.entries(options.params)) {
        if (value !== undefined) {
          searchParams.set(key, String(value));
        }
      }
      const queryString = searchParams.toString();
      if (queryString) {
        url += `?${queryString}`;
      }
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await this.fetchFn(url, {
        method,
        headers: this.getHeaders(),
        body: options?.body ? JSON.stringify(options.body) : undefined,
        signal: controller.signal,
      });

      if (!response.ok) {
        await this.handleResponseError(response);
      }

      if (response.status === 204) {
        return undefined as T;
      }

      const text = await response.text();
      return text ? JSON.parse(text) : undefined;
    } catch (error) {
      if (error instanceof StreamHouseError) {
        throw error;
      }
      if (error instanceof Error && error.name === 'AbortError') {
        throw new TimeoutError('Request timed out');
      }
      throw new ConnectionError(`Connection failed: ${error}`);
    } finally {
      clearTimeout(timeoutId);
    }
  }

  // ===========================================================================
  // Topic Operations
  // ===========================================================================

  /**
   * List all topics.
   */
  async listTopics(): Promise<Topic[]> {
    return this.request<Topic[]>('GET', '/topics');
  }

  /**
   * Create a new topic.
   */
  async createTopic(
    name: string,
    partitions: number,
    replicationFactor: number = 1
  ): Promise<Topic> {
    return this.request<Topic>('POST', '/topics', {
      body: { name, partitions, replication_factor: replicationFactor },
    });
  }

  /**
   * Get topic details.
   */
  async getTopic(name: string): Promise<Topic> {
    return this.request<Topic>('GET', `/topics/${encodeURIComponent(name)}`);
  }

  /**
   * Delete a topic.
   */
  async deleteTopic(name: string): Promise<void> {
    await this.request<void>('DELETE', `/topics/${encodeURIComponent(name)}`);
  }

  /**
   * List partitions for a topic.
   */
  async listPartitions(topic: string): Promise<Partition[]> {
    return this.request<Partition[]>(
      'GET',
      `/topics/${encodeURIComponent(topic)}/partitions`
    );
  }

  // ===========================================================================
  // Producer Operations
  // ===========================================================================

  /**
   * Produce a single message.
   */
  async produce(
    topic: string,
    value: string,
    options?: { key?: string; partition?: number }
  ): Promise<ProduceResult> {
    return this.request<ProduceResult>('POST', '/produce', {
      body: {
        topic,
        value,
        key: options?.key,
        partition: options?.partition,
      },
    });
  }

  /**
   * Produce a batch of messages.
   */
  async produceBatch(
    topic: string,
    records: (string | BatchRecord)[]
  ): Promise<BatchProduceResult> {
    const formattedRecords = records.map((r) =>
      typeof r === 'string' ? { value: r } : r
    );

    return this.request<BatchProduceResult>('POST', '/produce/batch', {
      body: { topic, records: formattedRecords },
    });
  }

  // ===========================================================================
  // Consumer Operations
  // ===========================================================================

  /**
   * Consume messages from a partition.
   */
  async consume(
    topic: string,
    partition: number,
    options?: { offset?: number; maxRecords?: number }
  ): Promise<ConsumeResult> {
    return this.request<ConsumeResult>('GET', '/consume', {
      params: {
        topic,
        partition,
        offset: options?.offset,
        maxRecords: options?.maxRecords,
      },
    });
  }

  // ===========================================================================
  // Consumer Group Operations
  // ===========================================================================

  /**
   * List all consumer groups.
   */
  async listConsumerGroups(): Promise<ConsumerGroup[]> {
    return this.request<ConsumerGroup[]>('GET', '/consumer-groups');
  }

  /**
   * Get consumer group details.
   */
  async getConsumerGroup(groupId: string): Promise<ConsumerGroupDetail> {
    return this.request<ConsumerGroupDetail>(
      'GET',
      `/consumer-groups/${encodeURIComponent(groupId)}`
    );
  }

  /**
   * Commit a consumer offset.
   */
  async commitOffset(
    groupId: string,
    topic: string,
    partition: number,
    offset: number
  ): Promise<boolean> {
    const result = await this.request<{ success: boolean }>(
      'POST',
      '/consumer-groups/commit',
      {
        body: {
          group_id: groupId,
          topic,
          partition,
          offset,
        },
      }
    );
    return result.success;
  }

  /**
   * Delete a consumer group.
   */
  async deleteConsumerGroup(groupId: string): Promise<void> {
    await this.request<void>(
      'DELETE',
      `/consumer-groups/${encodeURIComponent(groupId)}`
    );
  }

  // ===========================================================================
  // SQL Operations
  // ===========================================================================

  /**
   * Execute a SQL query.
   */
  async query(sql: string, options?: { timeoutMs?: number }): Promise<SqlResult> {
    return this.request<SqlResult>('POST', '/sql', {
      body: {
        query: sql,
        timeout_ms: options?.timeoutMs,
      },
    });
  }

  // ===========================================================================
  // Agent/Cluster Operations
  // ===========================================================================

  /**
   * List all agents.
   */
  async listAgents(): Promise<Agent[]> {
    return this.request<Agent[]>('GET', '/agents');
  }

  /**
   * Get cluster metrics.
   */
  async getMetrics(): Promise<MetricsSnapshot> {
    return this.request<MetricsSnapshot>('GET', '/metrics');
  }

  // ===========================================================================
  // Health Check
  // ===========================================================================

  /**
   * Check if the server is healthy.
   */
  async healthCheck(): Promise<boolean> {
    try {
      const response = await this.fetchFn(`${this.baseUrl}/health`, {
        method: 'GET',
      });
      return response.ok;
    } catch {
      return false;
    }
  }
}
