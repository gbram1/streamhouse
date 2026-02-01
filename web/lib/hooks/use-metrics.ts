/**
 * React Query hooks for Metrics
 */

import { useQuery } from '@tanstack/react-query';
import { apiClient, API_ENDPOINTS } from '../api-client';
import type {
  OverviewMetrics,
  ThroughputMetrics,
  LatencyMetrics,
  ErrorMetrics,
  StorageMetrics,
  TimeRange,
} from '../types';

// Fetch overview metrics
export function useOverviewMetrics() {
  return useQuery({
    queryKey: ['metrics', 'overview'],
    queryFn: () => apiClient.get<OverviewMetrics>(API_ENDPOINTS.overview),
    refetchInterval: 5000, // Refresh every 5 seconds
  });
}

// Fetch throughput metrics
export function useThroughputMetrics(timeRange: TimeRange = '1h') {
  return useQuery({
    queryKey: ['metrics', 'throughput', timeRange],
    queryFn: () =>
      apiClient.get<ThroughputMetrics[]>(API_ENDPOINTS.metricsThroughput, {
        range: timeRange,
      }),
    refetchInterval: 10000,
  });
}

// Fetch latency metrics
export function useLatencyMetrics(timeRange: TimeRange = '1h') {
  return useQuery({
    queryKey: ['metrics', 'latency', timeRange],
    queryFn: () =>
      apiClient.get<LatencyMetrics[]>(API_ENDPOINTS.metricsLatency, {
        range: timeRange,
      }),
    refetchInterval: 10000,
  });
}

// Fetch error metrics
export function useErrorMetrics(timeRange: TimeRange = '1h') {
  return useQuery({
    queryKey: ['metrics', 'errors', timeRange],
    queryFn: () =>
      apiClient.get<ErrorMetrics[]>(API_ENDPOINTS.metricsErrors, {
        range: timeRange,
      }),
    refetchInterval: 10000,
  });
}

// Fetch storage metrics
export function useStorageMetrics() {
  return useQuery({
    queryKey: ['metrics', 'storage'],
    queryFn: () => apiClient.get<StorageMetrics>('/api/v1/metrics/storage'),
    refetchInterval: 30000, // Refresh every 30 seconds
  });
}
