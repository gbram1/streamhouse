'use client';

import { useState, useCallback } from 'react';
import { useWebSocket } from './use-websocket';
import { useAppStore } from '../store';

export interface RealtimeMetrics {
  timestamp: number;
  throughput: {
    messagesPerSecond: number;
    bytesPerSecond: number;
  };
  latency: {
    p50: number;
    p95: number;
    p99: number;
    avg: number;
  };
  errors: {
    errorRate: number;
    errorCount: number;
  };
  storage: {
    totalSizeBytes: number;
    cacheHitRate: number;
  };
  consumerLag: {
    totalLag: number;
    groupsWithLag: number;
  };
}

const WS_BASE_URL = process.env.NEXT_PUBLIC_WS_URL || 'ws://localhost:8080';

export function useRealtimeMetrics() {
  const autoRefresh = useAppStore((state) => state.autoRefresh);
  const [metrics, setMetrics] = useState<RealtimeMetrics[]>([]);

  const handleMessage = useCallback((data: RealtimeMetrics) => {
    setMetrics((prev) => {
      const updated = [...prev, data];
      // Keep last 100 data points
      return updated.slice(-100);
    });
  }, []);

  const { isConnected, reconnectAttempts } = useWebSocket<RealtimeMetrics>({
    url: `${WS_BASE_URL}/ws/metrics`,
    onMessage: handleMessage,
    onError: (error) => {
      console.error('WebSocket error:', error);
    },
  });

  // Only connect if auto-refresh is enabled
  const shouldConnect = autoRefresh;

  return {
    metrics,
    isConnected: shouldConnect ? isConnected : false,
    reconnectAttempts,
    latestMetrics: metrics[metrics.length - 1] || null,
  };
}

export function useRealtimeTopicMetrics(topicName: string) {
  const autoRefresh = useAppStore((state) => state.autoRefresh);
  const [throughput, setThroughput] = useState<number[]>([]);
  const [lag, setLag] = useState<number[]>([]);

  const handleMessage = useCallback((data: any) => {
    if (data.topic === topicName) {
      setThroughput((prev) => [...prev.slice(-50), data.throughput]);
      setLag((prev) => [...prev.slice(-50), data.lag]);
    }
  }, [topicName]);

  const { isConnected } = useWebSocket({
    url: `${WS_BASE_URL}/ws/topics/${topicName}`,
    onMessage: handleMessage,
  });

  return {
    throughput,
    lag,
    isConnected: autoRefresh ? isConnected : false,
  };
}

export function useRealtimeConsumerMetrics(groupId: string) {
  const autoRefresh = useAppStore((state) => state.autoRefresh);
  const [lag, setLag] = useState<number[]>([]);
  const [members, setMembers] = useState<number>(0);

  const handleMessage = useCallback((data: any) => {
    if (data.groupId === groupId) {
      setLag((prev) => [...prev.slice(-50), data.totalLag]);
      setMembers(data.memberCount);
    }
  }, [groupId]);

  const { isConnected } = useWebSocket({
    url: `${WS_BASE_URL}/ws/consumers/${groupId}`,
    onMessage: handleMessage,
  });

  return {
    lag,
    members,
    isConnected: autoRefresh ? isConnected : false,
  };
}
