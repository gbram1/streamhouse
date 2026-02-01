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
  partitionCount: number;
  retentionMs?: number;
  messageCount: number;
  sizeBytes: number;
  messagesPerSecond: number;
  config: Record<string, string>;
}

export interface TopicDetail extends Topic {
  partitions: Partition[];
}

export interface Partition {
  id: number;
  topic: string;
  leader: string;
  replicas: string[];
  lowWatermark: number;
  highWatermark: number;
  sizeBytes: number;
}

export interface Message {
  offset: number;
  partition: number;
  timestamp: number;
  key?: string;
  value: string;
  headers: Record<string, string>;
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
  partitionCount: number;
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

// Time range for charts
export type TimeRange = '5m' | '15m' | '1h' | '6h' | '24h' | '7d' | '30d' | 'custom';

// Dashboard preferences
export interface DashboardPreferences {
  theme: 'light' | 'dark' | 'system';
  autoRefresh: boolean;
  refreshInterval: number;
  defaultTimeRange: TimeRange;
}
