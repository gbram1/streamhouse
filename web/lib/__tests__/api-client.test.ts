import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { apiClient, API_ENDPOINTS } from '../api-client';

describe('ApiClient', () => {
  const mockFetch = vi.fn();

  beforeEach(() => {
    vi.stubGlobal('fetch', mockFetch);
    apiClient.setOrganizationId(null);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('GET', () => {
    it('sends GET request with correct URL', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ topics: [] }),
      });

      await apiClient.get('/api/v1/topics');
      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/api/v1/topics'),
        expect.objectContaining({ method: 'GET' })
      );
    });

    it('appends query params', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve([]),
      });

      await apiClient.get('/api/v1/topics', { limit: '10' });
      const calledUrl = mockFetch.mock.calls[0][0];
      expect(calledUrl).toContain('limit=10');
    });

    it('includes Content-Type header', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({}),
      });

      await apiClient.get('/api/v1/topics');
      const headers = mockFetch.mock.calls[0][1].headers;
      expect(headers['Content-Type']).toBe('application/json');
    });
  });

  describe('POST', () => {
    it('sends POST with JSON body', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ name: 'test' }),
      });

      await apiClient.post('/api/v1/topics', { name: 'test', partitions: 3 });
      const [, opts] = mockFetch.mock.calls[0];
      expect(opts.method).toBe('POST');
      expect(JSON.parse(opts.body)).toEqual({ name: 'test', partitions: 3 });
    });

    it('sends POST without body', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({}),
      });

      await apiClient.post('/api/v1/topics');
      const [, opts] = mockFetch.mock.calls[0];
      expect(opts.body).toBeUndefined();
    });
  });

  describe('PUT', () => {
    it('sends PUT request', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({}),
      });

      await apiClient.put('/api/v1/topics/test', { partitions: 5 });
      const [, opts] = mockFetch.mock.calls[0];
      expect(opts.method).toBe('PUT');
    });
  });

  describe('DELETE', () => {
    it('sends DELETE request', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({}),
      });

      await apiClient.delete('/api/v1/topics/test');
      const [, opts] = mockFetch.mock.calls[0];
      expect(opts.method).toBe('DELETE');
    });
  });

  describe('PATCH', () => {
    it('sends PATCH request with body', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({}),
      });

      await apiClient.patch('/api/v1/topics/test', { retention: 3600 });
      const [, opts] = mockFetch.mock.calls[0];
      expect(opts.method).toBe('PATCH');
      expect(JSON.parse(opts.body)).toEqual({ retention: 3600 });
    });
  });

  describe('organization context', () => {
    it('does not include org header by default', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({}),
      });

      await apiClient.get('/api/v1/topics');
      const headers = mockFetch.mock.calls[0][1].headers;
      expect(headers['X-Organization-Id']).toBeUndefined();
    });

    it('includes org header when set', async () => {
      apiClient.setOrganizationId('org_123');
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({}),
      });

      await apiClient.get('/api/v1/topics');
      const headers = mockFetch.mock.calls[0][1].headers;
      expect(headers['X-Organization-Id']).toBe('org_123');
    });

    it('removes org header when set to null', async () => {
      apiClient.setOrganizationId('org_123');
      apiClient.setOrganizationId(null);
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({}),
      });

      await apiClient.get('/api/v1/topics');
      const headers = mockFetch.mock.calls[0][1].headers;
      expect(headers['X-Organization-Id']).toBeUndefined();
    });
  });

  describe('error handling', () => {
    it('throws on non-ok response', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 404,
        statusText: 'Not Found',
        text: () => Promise.resolve('Topic not found'),
      });

      await expect(apiClient.get('/api/v1/topics/missing')).rejects.toThrow('Topic not found');
    });

    it('throws with status text when no body', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        text: () => Promise.resolve(''),
      });

      await expect(apiClient.get('/api/v1/topics')).rejects.toThrow('HTTP 500: Internal Server Error');
    });

    it('throws on network error', async () => {
      mockFetch.mockRejectedValueOnce(new Error('Network error'));

      await expect(apiClient.get('/api/v1/topics')).rejects.toThrow('Network error');
    });
  });
});

describe('API_ENDPOINTS', () => {
  it('has static endpoints', () => {
    expect(API_ENDPOINTS.topics).toBe('/api/v1/topics');
    expect(API_ENDPOINTS.overview).toBe('/api/v1/metrics/overview');
    expect(API_ENDPOINTS.agents).toBe('/api/v1/agents');
  });

  it('generates dynamic topic endpoints', () => {
    expect(API_ENDPOINTS.topic('my-topic')).toBe('/api/v1/topics/my-topic');
    expect(API_ENDPOINTS.topicPartitions('my-topic')).toBe('/api/v1/topics/my-topic/partitions');
  });

  it('encodes special characters in URLs', () => {
    expect(API_ENDPOINTS.topic('topic/with/slashes')).toBe(
      '/api/v1/topics/topic%2Fwith%2Fslashes'
    );
  });

  it('generates consumer group endpoints', () => {
    expect(API_ENDPOINTS.consumerGroup('group-1')).toBe('/api/v1/consumer-groups/group-1');
    expect(API_ENDPOINTS.consumerGroupLag('group-1')).toBe('/api/v1/consumer-groups/group-1/lag');
  });

  it('generates schema endpoints', () => {
    expect(API_ENDPOINTS.schemaSubjectVersion('user-events', 3)).toBe(
      '/schemas/subjects/user-events/versions/3'
    );
  });

  it('generates organization endpoints', () => {
    expect(API_ENDPOINTS.organization('org_123')).toBe('/api/v1/organizations/org_123');
    expect(API_ENDPOINTS.organizationApiKeys('org_123')).toBe(
      '/api/v1/organizations/org_123/api-keys'
    );
  });

  it('generates connector endpoints', () => {
    expect(API_ENDPOINTS.connector('my-sink')).toBe('/api/v1/connectors/my-sink');
    expect(API_ENDPOINTS.connectorPause('my-sink')).toBe('/api/v1/connectors/my-sink/pause');
  });
});
