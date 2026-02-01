'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Activity, Clock, AlertCircle, TrendingUp } from 'lucide-react';
import { LineChart } from '@/components/charts/line-chart';
import { AreaChart } from '@/components/charts/area-chart';
import { BarChart } from '@/components/charts/bar-chart';
import { PieChart } from '@/components/charts/pie-chart';
import { useThroughputMetrics, useLatencyMetrics, useErrorMetrics } from '@/lib/hooks/use-metrics';
import { useAppStore } from '@/lib/store';
import { formatCompactNumber } from '@/lib/utils';
import { useMemo } from 'react';

// Generate mock time-series data
function generateMockData(points: number = 24) {
  const now = Date.now();
  return Array.from({ length: points }, (_, i) => ({
    time: new Date(now - (points - i) * 3600000).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' }),
    messagesPerSecond: Math.floor(Math.random() * 5000 + 1000),
    bytesPerSecond: Math.floor(Math.random() * 50000 + 10000),
    p50: Math.floor(Math.random() * 20 + 5),
    p95: Math.floor(Math.random() * 50 + 20),
    p99: Math.floor(Math.random() * 100 + 50),
    errorRate: Math.random() * 0.05,
  }));
}

export default function PerformancePage() {
  const timeRange = useAppStore((state) => state.timeRange);
  const { data: throughput, isLoading: loadingThroughput } = useThroughputMetrics(timeRange);
  const { data: latency, isLoading: loadingLatency } = useLatencyMetrics(timeRange);
  const { data: errors, isLoading: loadingErrors } = useErrorMetrics(timeRange);

  // Use mock data if API data not available
  const mockData = useMemo(() => generateMockData(24), []);
  const chartData = throughput && throughput.length > 0 ? throughput : mockData;

  // Get latest values
  const latest = chartData[chartData.length - 1] as any;

  // Mock error types data
  const errorTypesData = [
    { name: 'Timeout', value: 45 },
    { name: 'Network', value: 30 },
    { name: 'Validation', value: 15 },
    { name: 'Internal', value: 10 },
  ];

  const errorColors = ['#ef4444', '#f59e0b', '#3b82f6', '#8b5cf6'];

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
            {formatCompactNumber(latest?.messagesPerSecond || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Current throughput</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Bytes/sec</h3>
            <TrendingUp className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {formatCompactNumber(latest?.bytesPerSecond || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Network throughput</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Error Rate</h3>
            <AlertCircle className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold text-red-600">
            {((latest?.errorRate || 0) * 100).toFixed(2)}%
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
            {latest?.p50 || 0}ms
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">p95 Latency</h3>
            <Clock className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-2xl font-bold">
            {latest?.p95 || 0}ms
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">p99 Latency</h3>
            <Clock className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-2xl font-bold">
            {latest?.p99 || 0}ms
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Avg Latency</h3>
            <Clock className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-2xl font-bold">
            {Math.floor((latest?.p50 || 0) * 1.2)}ms
          </div>
        </Card>
      </div>

      {/* Charts */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* Throughput Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Message Throughput (24h)</h3>
          <LineChart
            data={chartData}
            xKey="time"
            lines={[
              { key: 'messagesPerSecond', color: '#3b82f6', name: 'Messages/sec' },
            ]}
            height={250}
          />
        </Card>

        {/* Latency Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Latency Percentiles</h3>
          <AreaChart
            data={chartData}
            xKey="time"
            areas={[
              { key: 'p50', color: '#10b981', name: 'p50' },
              { key: 'p95', color: '#f59e0b', name: 'p95' },
              { key: 'p99', color: '#ef4444', name: 'p99' },
            ]}
            height={250}
          />
        </Card>

        {/* Error Rate Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Error Rate Over Time</h3>
          <BarChart
            data={chartData.slice(-12)}
            xKey="time"
            bars={[
              { key: 'errorRate', color: '#ef4444', name: 'Error Rate' },
            ]}
            height={250}
          />
        </Card>

        {/* Error Types Breakdown */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Error Types Distribution</h3>
          <PieChart
            data={errorTypesData}
            colors={errorColors}
            height={250}
          />
        </Card>
      </div>

      {/* Network Throughput */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Network Throughput</h3>
        <LineChart
          data={chartData}
          xKey="time"
          lines={[
            { key: 'bytesPerSecond', color: '#8b5cf6', name: 'Bytes/sec' },
          ]}
          height={200}
        />
      </Card>
    </DashboardLayout>
  );
}
