'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { MetricCard } from '@/components/ui/metric-card';
import { Card } from '@/components/ui/card';
import { LineChart } from '@/components/charts/line-chart';
import { Database, Users, Activity, HardDrive, Heart, Server, FileCode2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { formatBytes } from '@/lib/utils';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

interface ThroughputPoint {
  timestamp: number;
  messagesPerSecond: number;
  bytesPerSecond: number;
}

interface DashboardMetrics {
  topicsCount: number;
  agentsCount: number;
  consumerGroupsCount: number;
  totalStorageBytes: number;
  partitionsCount: number;
  schemasCount: number;
  messagesPerSecond: number;
}

export default function Dashboard() {
  const [metrics, setMetrics] = useState<DashboardMetrics>({
    topicsCount: 0,
    agentsCount: 0,
    consumerGroupsCount: 0,
    totalStorageBytes: 0,
    partitionsCount: 0,
    schemasCount: 0,
    messagesPerSecond: 0,
  });
  const [throughputData, setThroughputData] = useState<ThroughputPoint[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchMetrics = async () => {
      try {
        // Fetch all metrics in parallel
        const [topics, agents, consumerGroups, storage, schemas, throughput] = await Promise.all([
          fetch(`${API_URL}/api/v1/topics`).then(r => r.json()),
          fetch(`${API_URL}/api/v1/agents`).then(r => r.json()),
          fetch(`${API_URL}/api/v1/consumer-groups`).then(r => r.json()),
          fetch(`${API_URL}/api/v1/metrics/storage`).then(r => r.json()),
          fetch(`${API_URL}/schemas/subjects`).then(r => r.json()).catch(() => []),
          fetch(`${API_URL}/api/v1/metrics/throughput?time_range=5m`).then(r => r.json()).catch(() => []),
        ]);

        // Calculate partitions count from topics
        const partitionsCount = topics.reduce((acc: number, topic: any) =>
          acc + (topic.partition_count || topic.partitions || 0), 0);

        // Get latest throughput
        const latestThroughput = throughput.length > 0 ? throughput[throughput.length - 1] : null;

        // Format throughput data for chart
        const formattedThroughput = throughput.map((point: any) => ({
          ...point,
          time: new Date(point.timestamp * 1000).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' }),
        }));
        setThroughputData(formattedThroughput);

        setMetrics({
          topicsCount: topics.length,
          agentsCount: agents.length,
          consumerGroupsCount: consumerGroups.length,
          totalStorageBytes: storage.totalSizeBytes || 0,
          partitionsCount,
          schemasCount: schemas.length || 0,
          messagesPerSecond: Math.round(latestThroughput?.messagesPerSecond || 0),
        });
      } catch (error) {
        console.error('Failed to fetch dashboard metrics:', error);
      } finally {
        setLoading(false);
      }
    };

    fetchMetrics();
    const interval = setInterval(fetchMetrics, 5000); // Refresh every 5 seconds
    return () => clearInterval(interval);
  }, []);

  return (
    <DashboardLayout
      title="Overview"
      description="System health and performance at a glance"
    >
      {/* Metrics Grid */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-4">
        <MetricCard
          title="Messages/sec"
          value={loading ? '...' : metrics.messagesPerSecond.toString()}
          description="Real-time throughput"
          icon={Activity}
        />

        <MetricCard
          title="Active Topics"
          value={loading ? '...' : metrics.topicsCount.toString()}
          description="Event streams"
          icon={Database}
        />

        <MetricCard
          title="Active Consumers"
          value={loading ? '...' : metrics.consumerGroupsCount.toString()}
          description="Consumer groups"
          icon={Users}
        />

        <MetricCard
          title="Total Storage"
          value={loading ? '...' : formatBytes(metrics.totalStorageBytes)}
          description="Across all topics"
          icon={HardDrive}
        />

        <MetricCard
          title="System Health"
          value="Healthy"
          description="Overall status"
          icon={Heart}
          className="border-green-500/50"
        />

        <MetricCard
          title="Active Agents"
          value={loading ? '...' : metrics.agentsCount.toString()}
          description="Stateless brokers"
          icon={Server}
        />

        <MetricCard
          title="Schemas"
          value={loading ? '...' : metrics.schemasCount.toString()}
          description="Registered schemas"
          icon={FileCode2}
        />

        <MetricCard
          title="Partitions"
          value={loading ? '...' : metrics.partitionsCount.toString()}
          description="Total partitions"
          icon={Database}
        />
      </div>

      {/* Charts Section */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* Message Throughput Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Message Throughput</h3>
          {loading ? (
            <div className="flex h-64 items-center justify-center text-muted-foreground">
              <p>Loading...</p>
            </div>
          ) : throughputData.length > 0 ? (
            <LineChart
              data={throughputData}
              xKey="time"
              lines={[
                { key: 'messagesPerSecond', color: '#3b82f6', name: 'Messages/sec' },
              ]}
              height={250}
            />
          ) : (
            <div className="flex h-64 items-center justify-center text-muted-foreground">
              <p>No throughput data available</p>
            </div>
          )}
        </Card>

        {/* Consumer Lag Summary */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Consumer Lag Summary</h3>
          <div className="flex h-64 items-center justify-center text-muted-foreground">
            <p>Consumer lag chart coming soon</p>
          </div>
        </Card>
      </div>

      {/* Recent Activity */}
      <div className="mt-6">
        <Card className="p-6">
          <h3 className="text-lg font-semibold">Recent Activity</h3>
          <div className="mt-4 space-y-4">
            <div className="text-sm text-muted-foreground">
              <p>• No recent activity</p>
            </div>
          </div>
        </Card>
      </div>

      {/* Quick Actions */}
      <div className="mt-6 grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
        <Card className="p-6 hover:bg-accent cursor-pointer transition-colors">
          <h4 className="font-medium">Create Topic</h4>
          <p className="mt-2 text-sm text-muted-foreground">
            Add a new event stream
          </p>
        </Card>
        <Card className="p-6 hover:bg-accent cursor-pointer transition-colors">
          <h4 className="font-medium">View All Topics</h4>
          <p className="mt-2 text-sm text-muted-foreground">
            Browse and manage topics
          </p>
        </Card>
        <Card className="p-6 hover:bg-accent cursor-pointer transition-colors">
          <h4 className="font-medium">Monitor Consumers</h4>
          <p className="mt-2 text-sm text-muted-foreground">
            Check consumer lag
          </p>
        </Card>
        <Card className="p-6 hover:bg-accent cursor-pointer transition-colors">
          <h4 className="font-medium">Browse Schemas</h4>
          <p className="mt-2 text-sm text-muted-foreground">
            View schema registry
          </p>
        </Card>
      </div>
    </DashboardLayout>
  );
}
