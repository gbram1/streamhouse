'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Activity, Clock, AlertCircle, TrendingUp } from 'lucide-react';
import { useThroughputMetrics, useLatencyMetrics, useErrorMetrics } from '@/lib/hooks/use-metrics';
import { useAppStore } from '@/lib/store';
import { formatCompactNumber } from '@/lib/utils';

export default function PerformancePage() {
  const timeRange = useAppStore((state) => state.timeRange);
  const { data: throughput, isLoading: loadingThroughput } = useThroughputMetrics(timeRange);
  const { data: latency, isLoading: loadingLatency } = useLatencyMetrics(timeRange);
  const { data: errors, isLoading: loadingErrors } = useErrorMetrics(timeRange);

  // Get latest values
  const latest = throughput?.[throughput.length - 1];
  const latestLatency = latency?.[latency.length - 1];
  const latestErrors = errors?.[errors.length - 1];

  return (
    <DashboardLayout
      title="Performance Metrics"
      description="Real-time performance monitoring and analysis"
    >
      {/* Throughput Metrics */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-3">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Messages/sec</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {loadingThroughput ? '...' : formatCompactNumber(latest?.messagesPerSecond || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Current throughput</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Bytes/sec</h3>
            <TrendingUp className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {loadingThroughput ? '...' : formatCompactNumber(latest?.bytesPerSecond || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Network throughput</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Requests/sec</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {loadingThroughput ? '...' : formatCompactNumber(latest?.requestsPerSecond || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">API requests</p>
        </Card>
      </div>

      {/* Latency Metrics */}
      <div className="mt-6 grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">p50 Latency</h3>
            <Clock className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-2xl font-bold">
            {loadingLatency ? '...' : `${latestLatency?.p50 || 0}ms`}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">p95 Latency</h3>
            <Clock className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-2xl font-bold">
            {loadingLatency ? '...' : `${latestLatency?.p95 || 0}ms`}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">p99 Latency</h3>
            <Clock className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-2xl font-bold">
            {loadingLatency ? '...' : `${latestLatency?.p99 || 0}ms`}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Error Rate</h3>
            <AlertCircle className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-2xl font-bold text-red-600">
            {loadingErrors ? '...' : `${((latestErrors?.errorRate || 0) * 100).toFixed(2)}%`}
          </div>
        </Card>
      </div>

      {/* Charts */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* Throughput Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Message Throughput</h3>
          <div className="flex h-64 items-center justify-center text-muted-foreground">
            <p>Recharts line chart will be rendered here</p>
          </div>
        </Card>

        {/* Latency Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Latency Percentiles</h3>
          <div className="flex h-64 items-center justify-center text-muted-foreground">
            <p>Recharts area chart will be rendered here</p>
          </div>
        </Card>

        {/* Error Rate Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Error Rate</h3>
          <div className="flex h-64 items-center justify-center text-muted-foreground">
            <p>Recharts bar chart will be rendered here</p>
          </div>
        </Card>

        {/* Error Types Breakdown */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Error Types</h3>
          <div className="flex h-64 items-center justify-center text-muted-foreground">
            <p>Recharts pie chart will be rendered here</p>
          </div>
        </Card>
      </div>

      {/* Producer/Consumer Latency */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">End-to-End Latency</h3>
        <div className="grid grid-cols-1 gap-6 md:grid-cols-3">
          <div>
            <p className="text-sm text-muted-foreground">Producer Latency</p>
            <p className="mt-2 text-2xl font-bold">
              {loadingLatency ? '...' : `${latestLatency?.producerLatency || 0}ms`}
            </p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Consumer Latency</p>
            <p className="mt-2 text-2xl font-bold">
              {loadingLatency ? '...' : `${latestLatency?.consumerLatency || 0}ms`}
            </p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Total (p50)</p>
            <p className="mt-2 text-2xl font-bold">
              {loadingLatency
                ? '...'
                : `${(latestLatency?.producerLatency || 0) + (latestLatency?.consumerLatency || 0)}ms`}
            </p>
          </div>
        </div>
      </Card>
    </DashboardLayout>
  );
}
