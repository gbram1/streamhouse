/**
 * API Client Configuration
 *
 * Centralized HTTP client for all StreamHouse API requests
 */

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

export interface ApiResponse<T> {
  data?: T;
  error?: string;
}

class ApiClient {
  private baseUrl: string;
  private organizationId: string | null = null;

  constructor(baseUrl: string = API_BASE_URL) {
    this.baseUrl = baseUrl;
  }

  setOrganizationId(id: string | null) {
    this.organizationId = id;
  }

  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`;

    const config: RequestInit = {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...(this.organizationId && { 'X-Organization-Id': this.organizationId }),
        ...options.headers,
      },
    };

    try {
      const response = await fetch(url, config);

      if (!response.ok) {
        const error = await response.text();
        throw new Error(error || `HTTP ${response.status}: ${response.statusText}`);
      }

      return await response.json();
    } catch (error) {
      console.error(`API Error [${endpoint}]:`, error);
      throw error;
    }
  }

  // GET request
  async get<T>(endpoint: string, params?: Record<string, string>): Promise<T> {
    const queryString = params
      ? '?' + new URLSearchParams(params).toString()
      : '';

    return this.request<T>(`${endpoint}${queryString}`, {
      method: 'GET',
    });
  }

  // POST request
  async post<T>(endpoint: string, body?: unknown): Promise<T> {
    return this.request<T>(endpoint, {
      method: 'POST',
      body: body ? JSON.stringify(body) : undefined,
    });
  }

  // PUT request
  async put<T>(endpoint: string, body?: unknown): Promise<T> {
    return this.request<T>(endpoint, {
      method: 'PUT',
      body: body ? JSON.stringify(body) : undefined,
    });
  }

  // DELETE request
  async delete<T>(endpoint: string): Promise<T> {
    return this.request<T>(endpoint, {
      method: 'DELETE',
    });
  }

  // PATCH request
  async patch<T>(endpoint: string, body?: unknown): Promise<T> {
    return this.request<T>(endpoint, {
      method: 'PATCH',
      body: body ? JSON.stringify(body) : undefined,
    });
  }
}

export const apiClient = new ApiClient();

// API endpoint definitions
export const API_ENDPOINTS = {
  // Overview
  overview: '/api/v1/metrics/overview',

  // Topics
  topics: '/api/v1/topics',
  topic: (name: string) => `/api/v1/topics/${encodeURIComponent(name)}`,
  topicPartitions: (name: string) => `/api/v1/topics/${encodeURIComponent(name)}/partitions`,
  topicMessages: (name: string) => `/api/v1/topics/${encodeURIComponent(name)}/messages`,
  topicConfig: (name: string) => `/api/v1/topics/${encodeURIComponent(name)}/config`,

  // Consumer Groups
  consumerGroups: '/api/v1/consumer-groups',
  consumerGroup: (id: string) => `/api/v1/consumer-groups/${encodeURIComponent(id)}`,
  consumerGroupLag: (id: string) => `/api/v1/consumer-groups/${encodeURIComponent(id)}/lag`,
  consumerGroupResetOffsets: (id: string) => `/api/v1/consumer-groups/${encodeURIComponent(id)}/reset-offsets`,

  // Agents
  agents: '/api/v1/agents',
  agent: (id: string) => `/api/v1/agents/${encodeURIComponent(id)}`,
  agentMetrics: (id: string) => `/api/v1/agents/${encodeURIComponent(id)}/metrics`,
  agentPartitions: (id: string) => `/api/v1/agents/${encodeURIComponent(id)}/partitions`,

  // Schema Registry
  schemas: '/schemas',
  schemaSubjects: '/schemas/subjects',
  schemaSubject: (subject: string) => `/schemas/subjects/${encodeURIComponent(subject)}`,
  schemaSubjectVersions: (subject: string) => `/schemas/subjects/${encodeURIComponent(subject)}/versions`,
  schemaSubjectVersion: (subject: string, version: number) =>
    `/schemas/subjects/${encodeURIComponent(subject)}/versions/${version}`,
  schemaById: (id: number) => `/schemas/schemas/ids/${id}`,

  // Metrics
  metricsThroughput: '/api/v1/metrics/throughput',
  metricsLatency: '/api/v1/metrics/latency',
  metricsErrors: '/api/v1/metrics/errors',

  // WebSocket
  wsMetrics: '/api/v1/ws/metrics',
  wsLogs: '/api/v1/ws/logs',
  wsAlerts: '/api/v1/ws/alerts',

  // Organizations
  organizations: '/api/v1/organizations',
  organization: (id: string) => `/api/v1/organizations/${encodeURIComponent(id)}`,
  organizationQuota: (id: string) => `/api/v1/organizations/${encodeURIComponent(id)}/quota`,
  organizationUsage: (id: string) => `/api/v1/organizations/${encodeURIComponent(id)}/usage`,
  organizationApiKeys: (orgId: string) => `/api/v1/organizations/${encodeURIComponent(orgId)}/api-keys`,

  // API Keys
  apiKey: (id: string) => `/api/v1/api-keys/${encodeURIComponent(id)}`,

  // Connectors
  connectors: '/api/v1/connectors',
  connector: (name: string) => `/api/v1/connectors/${encodeURIComponent(name)}`,
  connectorPause: (name: string) => `/api/v1/connectors/${encodeURIComponent(name)}/pause`,
  connectorResume: (name: string) => `/api/v1/connectors/${encodeURIComponent(name)}/resume`,
} as const;
