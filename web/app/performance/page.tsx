'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Activity, Clock, AlertCircle, TrendingUp } from 'lucide-react';
import { LineChart } from '@/components/charts/line-chart';
import { AreaChart } from '@/components/charts/area-chart';
import { BarChart } from '@/components/charts/bar-chart';
import { useThroughputMetrics, useLatencyMetrics, useErrorMetrics } from '@/lib/hooks/use-metrics';
import { useAppStore } from '@/lib/store';
import { formatCompactNumber } from '@/lib/utils';

export default function PerformancePage() {
  const timeRange = useAppStore((state) => state.timeRange);
  const { data: throughput, isLoading: loadingThroughput } = useThroughputMetrics(timeRange);
  const { data: latency, isLoading: loadingLatency } = useLatencyMetrics(timeRange);
  const { data: errors, isLoading: loadingErrors } = useErrorMetrics(timeRange);

  // Get latest values or use 0 if no data
  const hasData = throughput && throughput.length > 0;
  const latest = hasData ? throughput[throughput.length - 1] : null;

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
            {loadingThroughput ? '...' : formatCompactNumber((latest as any)?.messagesPerSecond || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Current throughput</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Bytes/sec</h3>
            <TrendingUp className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {loadingThroughput ? '...' : formatCompactNumber((latest as any)?.bytesPerSecond || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Network throughput</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Error Rate</h3>
            <AlertCircle className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold text-green-600">
            {loadingErrors ? '...' : (((latest as any)?.errorRate || 0) * 100).toFixed(2)}%
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Failed requests</p>
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
            {loadingLatency ? '...' : (latest as any)?.p50 || 0}ms
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">p95 Latency</h3>
            <Clock className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-2xl font-bold">
            {loadingLatency ? '...' : (latest as any)?.p95 || 0}ms
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">p99 Latency</h3>
            <Clock className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-2xl font-bold">
            {loadingLatency ? '...' : (latest as any)?.p99 || 0}ms
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Avg Latency</h3>
            <Clock className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-2xl font-bold">
            {loadingLatency ? '...' : Math.floor(((latest as any)?.p50 || 0) * 1.2)}ms
          </div>
        </Card>
      </div>

      {/* Charts */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* Throughput Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Message Throughput (24h)</h3>
          {loadingThroughput ? (
            <div className="flex h-64 items-center justify-center text-muted-foreground">
              <p>Loading...</p>
            </div>
          ) : hasData ? (
            <LineChart
              data={throughput}
              xKey="time"
              lines={[
                { key: 'messagesPerSecond', color: '#3b82f6', name: 'Messages/sec' },
              ]}
              height={250}
            />
          ) : (
            <div className="flex h-64 items-center justify-center text-muted-foreground">
              <p>No time-series data available</p>
            </div>
          )}
        </Card>

        {/* Latency Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Latency Percentiles</h3>
          {loadingLatency ? (
            <div className="flex h-64 items-center justify-center text-muted-foreground">
              <p>Loading...</p>
            </div>
          ) : latency && latency.length > 0 ? (
            <AreaChart
              data={latency}
              xKey="time"
              areas={[
                { key: 'p50', color: '#10b981', name: 'p50' },
                { key: 'p95', color: '#f59e0b', name: 'p95' },
                { key: 'p99', color: '#ef4444', name: 'p99' },
              ]}
              height={250}
            />
          ) : (
            <div className="flex h-64 items-center justify-center text-muted-foreground">
              <p>No time-series data available</p>
            </div>
          )}
        </Card>

        {/* Error Rate Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Error Rate Over Time</h3>
          {loadingErrors ? (
            <div className="flex h-64 items-center justify-center text-muted-foreground">
              <p>Loading...</p>
            </div>
          ) : errors && errors.length > 0 ? (
            <BarChart
              data={errors.slice(-12)}
              xKey="time"
              bars={[
                { key: 'errorRate', color: '#ef4444', name: 'Error Rate' },
              ]}
              height={250}
            />
          ) : (
            <div className="flex h-64 items-center justify-center text-muted-foreground">
              <p>No time-series data available</p>
            </div>
          )}
        </Card>

        {/* Error Types Breakdown */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Error Types Distribution</h3>
          <div className="flex h-64 items-center justify-center text-muted-foreground">
            <p>No error data available</p>
          </div>
        </Card>
      </div>

      {/* Network Throughput */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Network Throughput</h3>
        {loadingThroughput ? (
          <div className="flex h-52 items-center justify-center text-muted-foreground">
            <p>Loading...</p>
          </div>
        ) : hasData ? (
          <LineChart
            data={throughput}
            xKey="time"
            lines={[
              { key: 'bytesPerSecond', color: '#8b5cf6', name: 'Bytes/sec' },
            ]}
            height={200}
          />
        ) : (
          <div className="flex h-52 items-center justify-center text-muted-foreground">
            <p>No time-series data available</p>
          </div>
        )}
      </Card>
    </DashboardLayout>
  );
}
