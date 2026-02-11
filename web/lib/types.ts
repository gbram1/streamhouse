/**
 * Type Definitions for StreamHouse API
 */

// Overview Metrics
export interface OverviewMetrics {
  messagesPerSecond: number;
  activeTopics: number;
  activeConsumers: number;
  totalStorage: number;
  systemHealth: 'healthy' | 'degraded' | 'unhealthy';
  activeAgents: number;
  schemaCount: number;
}

// Topics
export interface Topic {
  name: string;
  partitionCount?: number;  // May come as partition_count from API
  partition_count?: number;  // API uses snake_case
  partitions?: number;  // From create request
  retentionMs?: number;
  replication_factor?: number;
  messageCount?: number;
  sizeBytes?: number;
  messagesPerSecond?: number;
  config?: Record<string, string>;
  createdAt?: number | string;  // May be timestamp or ISO string
  created_at?: string;  // API uses snake_case and ISO format
}

export interface TopicDetail extends Omit<Topic, 'partitions'> {
  partitions: Partition[];
}

export interface Partition {
  id: number;
  topic: string;
  leader: string;
  replicas: string[];
  lowWatermark: number;
  highWatermark: number;
  messageCount: number;
  sizeBytes: number;
}

export interface Message {
  offset: number;
  partition: number;
  timestamp: number;
  key?: string | number[] | null;
  value: string | number[] | null;
  headers?: Record<string, string>;
  schemaId?: number;
}

// Consumer Groups
export interface ConsumerGroup {
  id: string;
  state: 'stable' | 'rebalancing' | 'dead';
  memberCount: number;
  totalLag: number;
  lagTrend: 'increasing' | 'decreasing' | 'stable';
}

export interface ConsumerGroupDetail extends ConsumerGroup {
  members: ConsumerMember[];
  partitionLags: PartitionLag[];
}

export interface ConsumerMember {
  memberId: string;
  clientId: string;
  host: string;
  assignedPartitions: number[];
}

export interface PartitionLag {
  partition: number;
  currentOffset: number;
  logEndOffset: number;
  lag: number;
}

// Agents
export interface Agent {
  id: string;
  address: string;
  availabilityZone?: string;
  health: 'healthy' | 'degraded' | 'unhealthy';
  lastHeartbeat: number;
  cpuUsage: number;
  memoryUsage: number;
  diskUsage: number;
  partitionCount: number;
}

export interface AgentDetail extends Agent {
  partitions: Partition[];
  networkThroughput: {
    bytesIn: number;
    bytesOut: number;
  };
  diskIO: {
    readBytes: number;
    writeBytes: number;
  };
  cacheStats: {
    hitRate: number;
    size: number;
    evictionRate: number;
  };
}

// Schema Registry
export interface Schema {
  id: number;
  subject: string;
  version: number;
  schema: string;
  schemaType: 'AVRO' | 'PROTOBUF' | 'JSON';
  compatibilityMode: string;
}

export interface SchemaSubject {
  subject: string;
  latestVersion: number;
  latestSchemaId: number;
  schemaType: 'AVRO' | 'PROTOBUF' | 'JSON';
  compatibilityMode: string;
  versionCount: number;
}

// Metrics
export interface ThroughputMetrics {
  timestamp: number;
  messagesPerSecond: number;
  bytesPerSecond: number;
  requestsPerSecond: number;
}

export interface LatencyMetrics {
  timestamp: number;
  p50: number;
  p95: number;
  p99: number;
  producerLatency: number;
  consumerLatency: number;
}

export interface ErrorMetrics {
  timestamp: number;
  errorRate: number;
  errorTypes: Record<string, number>;
  failedMessages: number;
}

// Storage
export interface StorageMetrics {
  totalSizeBytes: number;
  storageByTopic: Record<string, number>;
  segmentCount: number;
  retentionCleanupCount: number;
  cacheHitRate: number;
  cacheSize: number;
  walSize: number;
  walUncommittedEntries: number;
  walSyncLagMs: number;
  walEnabled: boolean;
  s3RequestCount: number;
  s3ThrottleRate: number;
  s3AvgLatencyMs: number;
  cacheEvictions: number;
}

// WebSocket Messages
export interface WSMetricsUpdate {
  type: 'metrics';
  data: OverviewMetrics;
}

export interface WSLogMessage {
  type: 'log';
  level: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  timestamp: number;
  component: string;
}

export interface WSAlertMessage {
  type: 'alert';
  severity: 'info' | 'warning' | 'error' | 'critical';
  message: string;
  timestamp: number;
}

// API Response Types
export interface TopicCreateRequest {
  name: string;
  partitions: number;  // Backend expects 'partitions' not 'partitionCount'
  replication_factor?: number;
  retentionMs?: number;
  config?: Record<string, string>;
}

export interface SchemaRegistrationRequest {
  schema: string;
  schemaType: 'AVRO' | 'PROTOBUF' | 'JSON';
}

export interface ResetOffsetsRequest {
  topic: string;
  partition?: number;
  offset: number;
}

// Multi-Tenancy Types

// Organizations
export interface Organization {
  id: string;
  name: string;
  slug: string;
  plan: 'free' | 'pro' | 'enterprise';
  status: 'active' | 'suspended' | 'deleted';
  created_at: number;
}

export interface CreateOrganizationRequest {
  name: string;
  slug: string;
  plan?: 'free' | 'pro' | 'enterprise';
}

export interface UpdateOrganizationRequest {
  name?: string;
  plan?: 'free' | 'pro' | 'enterprise';
  status?: 'active' | 'suspended' | 'deleted';
}

// Organization Quotas
export interface OrganizationQuota {
  organization_id: string;
  max_topics: number;
  max_partitions_per_topic: number;
  max_total_partitions: number;
  max_storage_bytes: number;
  max_retention_days: number;
  max_produce_bytes_per_sec: number;
  max_consume_bytes_per_sec: number;
  max_requests_per_sec: number;
  max_consumer_groups: number;
  max_schemas: number;
  max_connections: number;
}

// Organization Usage
export interface OrganizationUsage {
  organization_id: string;
  topics_count: number;
  partitions_count: number;
  storage_bytes: number;
  produce_bytes_last_hour: number;
  consume_bytes_last_hour: number;
  requests_last_hour: number;
  consumer_groups_count: number;
  schemas_count: number;
}

// API Keys
export interface ApiKey {
  id: string;
  organization_id: string;
  name: string;
  key_prefix: string;
  permissions: string[];
  scopes: string[];
  expires_at: number | null;
  last_used_at: number | null;
  created_at: number;
}

export interface ApiKeyCreated extends ApiKey {
  key: string; // The full key, only returned at creation time
}

export interface CreateApiKeyRequest {
  name: string;
  permissions?: string[];
  scopes?: string[];
  expires_in_ms?: number;
}

// Time range for charts
export type TimeRange = '5m' | '15m' | '1h' | '6h' | '24h' | '7d' | '30d' | 'custom';

// Dashboard preferences
export interface DashboardPreferences {
  theme: 'light' | 'dark' | 'system';
  autoRefresh: boolean;
  refreshInterval: number;
  defaultTimeRange: TimeRange;
}
