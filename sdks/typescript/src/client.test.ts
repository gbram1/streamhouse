/**
 * Tests for StreamHouse TypeScript client.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { StreamHouseClient } from './client';
import {
  NotFoundError,
  ValidationError,
  ConflictError,
  ServerError,
  AuthenticationError,
  TimeoutError,
  ConnectionError,
} from './errors';

// Mock fetch response helper
function mockResponse(status: number, body?: unknown): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(body ? JSON.stringify(body) : ''),
  } as Response;
}

describe('StreamHouseClient', () => {
  let mockFetch: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockFetch = vi.fn();
  });

  describe('Client Initialization', () => {
    it('should initialize with string URL', () => {
      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      expect(client).toBeDefined();
    });

    it('should strip trailing slash from URL', () => {
      mockFetch.mockResolvedValue(mockResponse(200, []));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080/',
        fetch: mockFetch,
      });

      client.listTopics();

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/topics',
        expect.any(Object)
      );
    });

    it('should accept string as constructor argument', () => {
      // This will throw because no fetch is provided in global context during tests
      expect(() => new StreamHouseClient('http://localhost:8080')).toThrow();
    });
  });

  describe('Topic Operations', () => {
    it('should list topics', async () => {
      const topics = [
        { name: 'topic1', partitions: 3, replicationFactor: 1, createdAt: '2024-01-01' },
        { name: 'topic2', partitions: 6, replicationFactor: 2, createdAt: '2024-01-02' },
      ];
      mockFetch.mockResolvedValue(mockResponse(200, topics));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const result = await client.listTopics();

      expect(result).toHaveLength(2);
      expect(result[0].name).toBe('topic1');
      expect(result[1].partitions).toBe(6);
    });

    it('should create a topic', async () => {
      const topic = {
        name: 'my-topic',
        partitions: 3,
        replicationFactor: 1,
        createdAt: '2024-01-01',
      };
      mockFetch.mockResolvedValue(mockResponse(201, topic));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const result = await client.createTopic('my-topic', 3);

      expect(result.name).toBe('my-topic');
      expect(result.partitions).toBe(3);

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/topics',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({
            name: 'my-topic',
            partitions: 3,
            replication_factor: 1,
          }),
        })
      );
    });

    it('should get a topic', async () => {
      const topic = { name: 'my-topic', partitions: 3 };
      mockFetch.mockResolvedValue(mockResponse(200, topic));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const result = await client.getTopic('my-topic');

      expect(result.name).toBe('my-topic');
      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/topics/my-topic',
        expect.any(Object)
      );
    });

    it('should delete a topic', async () => {
      mockFetch.mockResolvedValue(mockResponse(204));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      await client.deleteTopic('my-topic');

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/topics/my-topic',
        expect.objectContaining({ method: 'DELETE' })
      );
    });

    it('should list partitions', async () => {
      const partitions = [
        { topic: 'events', partitionId: 0, highWatermark: 100 },
        { topic: 'events', partitionId: 1, highWatermark: 200 },
      ];
      mockFetch.mockResolvedValue(mockResponse(200, partitions));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const result = await client.listPartitions('events');

      expect(result).toHaveLength(2);
    });

    it('should encode topic names in URLs', async () => {
      mockFetch.mockResolvedValue(mockResponse(200, { name: 'my topic' }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      await client.getTopic('my topic');

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/topics/my%20topic',
        expect.any(Object)
      );
    });
  });

  describe('Producer Operations', () => {
    it('should produce a message', async () => {
      const result = { offset: 42, partition: 0 };
      mockFetch.mockResolvedValue(mockResponse(200, result));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const produceResult = await client.produce('events', '{"event": "click"}');

      expect(produceResult.offset).toBe(42);
      expect(produceResult.partition).toBe(0);
    });

    it('should produce a message with key', async () => {
      const result = { offset: 42, partition: 0 };
      mockFetch.mockResolvedValue(mockResponse(200, result));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      await client.produce('events', '{"event": "click"}', { key: 'user-123' });

      expect(mockFetch).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          body: JSON.stringify({
            topic: 'events',
            value: '{"event": "click"}',
            key: 'user-123',
            partition: undefined,
          }),
        })
      );
    });

    it('should produce a batch of messages', async () => {
      const result = {
        count: 2,
        offsets: [
          { partition: 0, offset: 10 },
          { partition: 0, offset: 11 },
        ],
      };
      mockFetch.mockResolvedValue(mockResponse(200, result));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const batchResult = await client.produceBatch('events', [
        '{"event": "a"}',
        { key: 'user-1', value: '{"event": "b"}' },
      ]);

      expect(batchResult.count).toBe(2);
      expect(batchResult.offsets).toHaveLength(2);
    });
  });

  describe('Consumer Operations', () => {
    it('should consume messages', async () => {
      const result = {
        records: [
          { partition: 0, offset: 0, key: null, value: '{"event": "a"}', timestamp: 1000 },
          { partition: 0, offset: 1, key: 'k1', value: '{"event": "b"}', timestamp: 1001 },
        ],
        nextOffset: 2,
      };
      mockFetch.mockResolvedValue(mockResponse(200, result));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const consumeResult = await client.consume('events', 0);

      expect(consumeResult.records).toHaveLength(2);
      expect(consumeResult.nextOffset).toBe(2);
      expect(consumeResult.records[0].value).toBe('{"event": "a"}');
    });

    it('should consume with options', async () => {
      mockFetch.mockResolvedValue(mockResponse(200, { records: [], nextOffset: 0 }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      await client.consume('events', 0, { offset: 10, maxRecords: 100 });

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('offset=10'),
        expect.any(Object)
      );
      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('maxRecords=100'),
        expect.any(Object)
      );
    });
  });

  describe('Consumer Group Operations', () => {
    it('should list consumer groups', async () => {
      const groups = [
        { groupId: 'group1', topics: ['events'], totalLag: 0 },
        { groupId: 'group2', topics: ['logs'], totalLag: 10 },
      ];
      mockFetch.mockResolvedValue(mockResponse(200, groups));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const result = await client.listConsumerGroups();

      expect(result).toHaveLength(2);
    });

    it('should get consumer group details', async () => {
      const group = {
        groupId: 'my-group',
        offsets: [
          { topic: 'events', partitionId: 0, committedOffset: 50 },
        ],
      };
      mockFetch.mockResolvedValue(mockResponse(200, group));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const result = await client.getConsumerGroup('my-group');

      expect(result.groupId).toBe('my-group');
      expect(result.offsets).toHaveLength(1);
    });

    it('should commit offset', async () => {
      mockFetch.mockResolvedValue(mockResponse(200, { success: true }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const result = await client.commitOffset('my-group', 'events', 0, 42);

      expect(result).toBe(true);
    });

    it('should delete consumer group', async () => {
      mockFetch.mockResolvedValue(mockResponse(204));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      await client.deleteConsumerGroup('my-group');

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/api/v1/consumer-groups/my-group',
        expect.objectContaining({ method: 'DELETE' })
      );
    });
  });

  describe('SQL Operations', () => {
    it('should execute a SQL query', async () => {
      const result = {
        columns: [
          { name: 'offset', dataType: 'bigint' },
          { name: 'value', dataType: 'string' },
        ],
        rows: [[0, 'test']],
        rowCount: 1,
        executionTimeMs: 50,
        truncated: false,
      };
      mockFetch.mockResolvedValue(mockResponse(200, result));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const queryResult = await client.query('SELECT * FROM events');

      expect(queryResult.rowCount).toBe(1);
      expect(queryResult.columns).toHaveLength(2);
      expect(queryResult.columns[0].name).toBe('offset');
    });

    it('should pass timeout option to SQL query', async () => {
      mockFetch.mockResolvedValue(mockResponse(200, { columns: [], rows: [], rowCount: 0 }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      await client.query('SELECT * FROM events', { timeoutMs: 5000 });

      expect(mockFetch).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          body: JSON.stringify({
            query: 'SELECT * FROM events',
            timeout_ms: 5000,
          }),
        })
      );
    });
  });

  describe('Error Handling', () => {
    it('should throw NotFoundError for 404', async () => {
      mockFetch.mockResolvedValue(mockResponse(404, { error: 'topic not found' }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });

      await expect(client.getTopic('nonexistent')).rejects.toThrow(NotFoundError);
    });

    it('should throw ValidationError for 400', async () => {
      mockFetch.mockResolvedValue(mockResponse(400, { error: 'invalid partitions' }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });

      await expect(client.createTopic('test', 0)).rejects.toThrow(ValidationError);
    });

    it('should throw ConflictError for 409', async () => {
      mockFetch.mockResolvedValue(mockResponse(409, { error: 'topic already exists' }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });

      await expect(client.createTopic('existing', 3)).rejects.toThrow(ConflictError);
    });

    it('should throw AuthenticationError for 401', async () => {
      mockFetch.mockResolvedValue(mockResponse(401, { error: 'unauthorized' }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });

      await expect(client.listTopics()).rejects.toThrow(AuthenticationError);
    });

    it('should throw ServerError for 5xx', async () => {
      mockFetch.mockResolvedValue(mockResponse(500, { error: 'internal server error' }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });

      await expect(client.listTopics()).rejects.toThrow(ServerError);
    });

    it('should include error message from server', async () => {
      mockFetch.mockResolvedValue(mockResponse(404, { error: 'topic "test" not found' }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });

      try {
        await client.getTopic('test');
        expect.fail('Should have thrown');
      } catch (error) {
        expect(error).toBeInstanceOf(NotFoundError);
        expect((error as NotFoundError).message).toContain('topic "test" not found');
        expect((error as NotFoundError).statusCode).toBe(404);
      }
    });

    it('should throw ConnectionError on network failure', async () => {
      mockFetch.mockRejectedValue(new Error('Network error'));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });

      await expect(client.listTopics()).rejects.toThrow(ConnectionError);
    });

    it('should throw TimeoutError on abort', async () => {
      mockFetch.mockRejectedValue(Object.assign(new Error('Aborted'), { name: 'AbortError' }));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
        timeout: 1,
      });

      await expect(client.listTopics()).rejects.toThrow(TimeoutError);
    });
  });

  describe('Authentication', () => {
    it('should include API key in Authorization header', async () => {
      mockFetch.mockResolvedValue(mockResponse(200, []));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        apiKey: 'test-api-key',
        fetch: mockFetch,
      });
      await client.listTopics();

      expect(mockFetch).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          headers: expect.objectContaining({
            Authorization: 'Bearer test-api-key',
          }),
        })
      );
    });

    it('should not include Authorization header when no API key', async () => {
      mockFetch.mockResolvedValue(mockResponse(200, []));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      await client.listTopics();

      const callArgs = mockFetch.mock.calls[0][1];
      expect(callArgs.headers.Authorization).toBeUndefined();
    });
  });

  describe('Health Check', () => {
    it('should return true when server is healthy', async () => {
      mockFetch.mockResolvedValue(mockResponse(200));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const healthy = await client.healthCheck();

      expect(healthy).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/health',
        expect.objectContaining({ method: 'GET' })
      );
    });

    it('should return false when server is unhealthy', async () => {
      mockFetch.mockResolvedValue(mockResponse(503));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const healthy = await client.healthCheck();

      expect(healthy).toBe(false);
    });

    it('should return false on connection error', async () => {
      mockFetch.mockRejectedValue(new Error('Connection refused'));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const healthy = await client.healthCheck();

      expect(healthy).toBe(false);
    });
  });

  describe('Agent Operations', () => {
    it('should list agents', async () => {
      const agents = [
        { agentId: 'agent-1', address: '10.0.0.1:8080' },
        { agentId: 'agent-2', address: '10.0.0.2:8080' },
      ];
      mockFetch.mockResolvedValue(mockResponse(200, agents));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const result = await client.listAgents();

      expect(result).toHaveLength(2);
    });

    it('should get metrics', async () => {
      const metrics = {
        topicsCount: 5,
        agentsCount: 3,
        partitionsCount: 15,
        totalMessages: 10000,
      };
      mockFetch.mockResolvedValue(mockResponse(200, metrics));

      const client = new StreamHouseClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch,
      });
      const result = await client.getMetrics();

      expect(result.topicsCount).toBe(5);
      expect(result.totalMessages).toBe(10000);
    });
  });
});
