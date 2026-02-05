/**
 * Type definitions for StreamHouse client.
 */

/** Topic information */
export interface Topic {
  name: string;
  partitions: number;
  replicationFactor: number;
  createdAt: string;
  messageCount: number;
  sizeBytes: number;
}

/** Partition information */
export interface Partition {
  topic: string;
  partitionId: number;
  leaderAgentId: string | null;
  highWatermark: number;
  lowWatermark: number;
}

/** Result of producing a message */
export interface ProduceResult {
  offset: number;
  partition: number;
}

/** A record in a batch produce request */
export interface BatchRecord {
  value: string;
  key?: string;
  partition?: number;
}

/** Result of a single record in batch produce */
export interface BatchRecordResult {
  partition: number;
  offset: number;
}

/** Result of batch produce */
export interface BatchProduceResult {
  count: number;
  offsets: BatchRecordResult[];
}

/** A consumed message record */
export interface ConsumedRecord {
  partition: number;
  offset: number;
  key: string | null;
  value: string;
  timestamp: number;
}

/** Result of consuming messages */
export interface ConsumeResult {
  records: ConsumedRecord[];
  nextOffset: number;
}

/** Consumer group offset information */
export interface ConsumerOffset {
  topic: string;
  partitionId: number;
  committedOffset: number;
  highWatermark: number;
  lag: number;
}

/** Consumer group summary */
export interface ConsumerGroup {
  groupId: string;
  topics: string[];
  totalLag: number;
  partitionCount: number;
}

/** Detailed consumer group information */
export interface ConsumerGroupDetail {
  groupId: string;
  offsets: ConsumerOffset[];
}

/** Agent/server information */
export interface Agent {
  agentId: string;
  address: string;
  availabilityZone: string;
  agentGroup: string;
  lastHeartbeat: number;
  startedAt: number;
  activeLeases: number;
}

/** Column metadata for SQL results */
export interface ColumnInfo {
  name: string;
  dataType: string;
}

/** SQL query result */
export interface SqlResult {
  columns: ColumnInfo[];
  rows: unknown[][];
  rowCount: number;
  executionTimeMs: number;
  truncated: boolean;
}

/** Cluster metrics snapshot */
export interface MetricsSnapshot {
  topicsCount: number;
  agentsCount: number;
  partitionsCount: number;
  totalMessages: number;
}

/** Request to create a topic */
export interface CreateTopicRequest {
  name: string;
  partitions: number;
  replicationFactor?: number;
}

/** Request to produce a message */
export interface ProduceRequest {
  topic: string;
  value: string;
  key?: string;
  partition?: number;
}

/** Request to produce a batch of messages */
export interface BatchProduceRequest {
  topic: string;
  records: BatchRecord[];
}

/** Request to consume messages */
export interface ConsumeRequest {
  topic: string;
  partition: number;
  offset?: number;
  maxRecords?: number;
}

/** Request to commit an offset */
export interface CommitOffsetRequest {
  groupId: string;
  topic: string;
  partition: number;
  offset: number;
}

/** Request to execute SQL */
export interface SqlQueryRequest {
  query: string;
  timeoutMs?: number;
}

/** Client configuration options */
export interface StreamHouseClientOptions {
  /** Base URL of the StreamHouse server */
  baseUrl: string;
  /** API key for authentication */
  apiKey?: string;
  /** Request timeout in milliseconds (default: 30000) */
  timeout?: number;
  /** Custom fetch implementation (for Node.js compatibility) */
  fetch?: typeof fetch;
}
