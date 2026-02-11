'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { MetricCard } from '@/components/ui/metric-card';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { AreaChart } from '@/components/charts/area-chart';
import { BarChart } from '@/components/charts/bar-chart';
import { Skeleton, SkeletonChart } from '@/components/ui/skeleton';
import { NoDataAvailable } from '@/components/ui/empty-state';
import {
  Database,
  Users,
  Activity,
  HardDrive,
  Heart,
  Server,
  FileCode2,
  Plus,
  Search,
  Terminal,
  FileSearch,
  ArrowRight,
  TrendingUp,
  TrendingDown,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { formatBytes } from '@/lib/utils';
import Link from 'next/link';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

interface ThroughputPoint {
  timestamp: number;
  messagesPerSecond: number;
  bytesPerSecond: number;
  time?: string;
}

interface ConsumerLagPoint {
  topic: string;
  lag: number;
}

interface DashboardMetrics {
  topicsCount: number;
  agentsCount: number;
  consumerGroupsCount: number;
  totalStorageBytes: number;
  partitionsCount: number;
  schemasCount: number;
  messagesPerSecond: number;
  messagesPerSecondTrend?: number;
}

export default function Dashboard() {
  const [metrics, setMetrics] = useState<DashboardMetrics | null>(null);
  const [throughputData, setThroughputData] = useState<ThroughputPoint[]>([]);
  const [consumerLagData, setConsumerLagData] = useState<ConsumerLagPoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [apiConnected, setApiConnected] = useState(false);

  useEffect(() => {
    const fetchMetrics = async () => {
      try {
        // Fetch all metrics in parallel
        const [topics, agents, consumerGroups, storage, schemas, throughput] = await Promise.all([
          fetch(`${API_URL}/api/v1/topics`).then(r => r.ok ? r.json() : null).catch(() => null),
          fetch(`${API_URL}/api/v1/agents`).then(r => r.ok ? r.json() : null).catch(() => null),
          fetch(`${API_URL}/api/v1/consumer-groups`).then(r => r.ok ? r.json() : null).catch(() => null),
          fetch(`${API_URL}/api/v1/metrics/storage`).then(r => r.ok ? r.json() : null).catch(() => null),
          fetch(`${API_URL}/schemas/subjects`).then(r => r.ok ? r.json() : null).catch(() => null),
          fetch(`${API_URL}/api/v1/metrics/throughput?time_range=5m`).then(r => r.ok ? r.json() : null).catch(() => null),
        ]);

        // Check if at least one endpoint responded
        const connected = topics !== null || agents !== null || storage !== null;
        setApiConnected(connected);

        if (!connected) {
          setMetrics(null);
          setThroughputData([]);
          setConsumerLagData([]);
          return;
        }

        // Calculate partitions count from topics
        const partitionsCount = (topics || []).reduce((acc: number, topic: any) =>
          acc + (topic.partition_count || topic.partitions || 0), 0);

        // Get latest and previous throughput for trend calculation
        const throughputArr = throughput || [];
        const latestThroughput = throughputArr.length > 0 ? throughputArr[throughputArr.length - 1] : null;
        const previousThroughput = throughputArr.length > 1 ? throughputArr[throughputArr.length - 2] : null;

        // Calculate trend percentage
        let trend: number | undefined;
        if (latestThroughput && previousThroughput && previousThroughput.messagesPerSecond > 0) {
          trend = Math.round(((latestThroughput.messagesPerSecond - previousThroughput.messagesPerSecond) / previousThroughput.messagesPerSecond) * 100);
        }

        // Format throughput data for chart
        const formattedThroughput = throughputArr.map((point: any) => ({
          ...point,
          time: new Date(point.timestamp * 1000).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' }),
        }));
        setThroughputData(formattedThroughput);

        // Prepare consumer lag data from consumer groups
        const lagData: ConsumerLagPoint[] = (consumerGroups || [])
          .filter((g: any) => g.totalLag > 0)
          .slice(0, 5)
          .map((g: any) => ({
            topic: (g.groupId || g.id || 'unknown').length > 15
              ? (g.groupId || g.id || 'unknown').slice(0, 15) + '...'
              : (g.groupId || g.id || 'unknown'),
            lag: g.totalLag,
          }));
        setConsumerLagData(lagData);

        setMetrics({
          topicsCount: (topics || []).length,
          agentsCount: (agents || []).length,
          consumerGroupsCount: (consumerGroups || []).length,
          totalStorageBytes: storage?.totalSizeBytes || 0,
          partitionsCount,
          schemasCount: (schemas || []).length || 0,
          messagesPerSecond: Math.round(latestThroughput?.messagesPerSecond || 0),
          messagesPerSecondTrend: trend,
        });
      } catch (error) {
        console.error('Failed to fetch dashboard metrics:', error);
        setApiConnected(false);
        setMetrics(null);
      } finally {
        setLoading(false);
      }
    };

    fetchMetrics();
    const interval = setInterval(fetchMetrics, 5000);
    return () => clearInterval(interval);
  }, []);

  // Helper: show "--" when not connected, actual value when connected
  const displayValue = (value: number | undefined, formatter?: (v: number) => string): string => {
    if (!apiConnected || metrics === null) return '--';
    const v = value ?? 0;
    return formatter ? formatter(v) : v.toString();
  };

  // Skeleton loading for metric cards
  const MetricSkeleton = () => (
    <Card className="p-6">
      <div className="flex items-center justify-between">
        <Skeleton className="h-4 w-24" />
        <Skeleton className="h-5 w-5 rounded" />
      </div>
      <div className="mt-3">
        <Skeleton className="h-8 w-16" />
        <Skeleton className="mt-2 h-3 w-20" />
      </div>
    </Card>
  );

  return (
    <DashboardLayout
      title="Overview"
      description="System health and performance at a glance"
    >
      {/* Metrics Grid */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {loading ? (
          <>
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
          </>
        ) : (
          <>
            <MetricCard
              title="Messages/sec"
              value={displayValue(metrics?.messagesPerSecond)}
              description="Real-time throughput"
              icon={Activity}
              trend={apiConnected && metrics?.messagesPerSecondTrend !== undefined ? {
                value: metrics.messagesPerSecondTrend,
                label: "vs previous"
              } : undefined}
            />

            <MetricCard
              title="Active Topics"
              value={displayValue(metrics?.topicsCount)}
              description="Event streams"
              icon={Database}
            />

            <MetricCard
              title="Consumer Groups"
              value={displayValue(metrics?.consumerGroupsCount)}
              description="Active consumers"
              icon={Users}
            />

            <MetricCard
              title="Total Storage"
              value={apiConnected && metrics ? formatBytes(metrics.totalStorageBytes) : '--'}
              description="Across all topics"
              icon={HardDrive}
            />

            <MetricCard
              title="System Health"
              value={apiConnected ? 'Healthy' : 'Unavailable'}
              description={apiConnected ? 'All systems operational' : 'Server not connected'}
              icon={Heart}
              className={apiConnected
                ? 'border-green-500/30 bg-green-500/5'
                : 'border-yellow-500/30 bg-yellow-500/5'
              }
            />

            <MetricCard
              title="Active Agents"
              value={displayValue(metrics?.agentsCount)}
              description="Stateless brokers"
              icon={Server}
            />

            <MetricCard
              title="Schemas"
              value={displayValue(metrics?.schemasCount)}
              description="Registered schemas"
              icon={FileCode2}
            />

            <MetricCard
              title="Partitions"
              value={displayValue(metrics?.partitionsCount)}
              description="Total partitions"
              icon={Database}
            />
          </>
        )}
      </div>

      {/* Charts Section */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* Message Throughput Chart */}
        <Card className="p-6">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h3 className="text-lg font-semibold">Message Throughput</h3>
              <p className="text-sm text-muted-foreground">Messages per second over time</p>
            </div>
            {!loading && throughputData.length > 0 && (
              <div className="flex items-center gap-1 text-sm">
                {metrics?.messagesPerSecondTrend !== undefined && metrics.messagesPerSecondTrend > 0 ? (
                  <TrendingUp className="h-4 w-4 text-green-500" />
                ) : metrics?.messagesPerSecondTrend !== undefined && metrics.messagesPerSecondTrend < 0 ? (
                  <TrendingDown className="h-4 w-4 text-red-500" />
                ) : null}
                <span className="font-medium">{metrics?.messagesPerSecond?.toLocaleString() ?? 0}/s</span>
              </div>
            )}
          </div>
          {loading ? (
            <SkeletonChart height={250} />
          ) : throughputData.length > 0 ? (
            <AreaChart
              data={throughputData}
              xKey="time"
              areas={[
                { key: 'messagesPerSecond', color: '#3b82f6', name: 'Messages/sec' },
              ]}
              height={250}
            />
          ) : (
            <div className="h-64">
              <NoDataAvailable
                message={apiConnected ? "No throughput data yet" : "Server not connected"}
                description={apiConnected
                  ? "Start producing messages to see throughput metrics"
                  : "Connect to a running StreamHouse server to see metrics"
                }
              />
            </div>
          )}
        </Card>

        {/* Consumer Lag Summary */}
        <Card className="p-6">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h3 className="text-lg font-semibold">Consumer Lag</h3>
              <p className="text-sm text-muted-foreground">Top 5 consumer groups by lag</p>
            </div>
            {!loading && (
              <Link href="/consumer-groups">
                <Button variant="ghost" size="sm">
                  View All
                  <ArrowRight className="ml-1 h-4 w-4" />
                </Button>
              </Link>
            )}
          </div>
          {loading ? (
            <SkeletonChart height={250} />
          ) : consumerLagData.length > 0 ? (
            <BarChart
              data={consumerLagData}
              xKey="topic"
              bars={[
                { key: 'lag', color: '#f59e0b', name: 'Messages Behind' },
              ]}
              height={250}
            />
          ) : (
            <div className="h-64">
              <NoDataAvailable
                message={apiConnected ? "No consumer lag detected" : "Server not connected"}
                description={apiConnected
                  ? "All consumer groups are caught up"
                  : "Connect to a running StreamHouse server to see consumer lag"
                }
              />
            </div>
          )}
        </Card>
      </div>

      {/* Quick Actions */}
      <div className="mt-6">
        <h3 className="text-lg font-semibold mb-4">Quick Actions</h3>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <Link href="/topics/new" className="block">
            <Card className="p-5 hover:bg-accent cursor-pointer transition-all hover:shadow-md group">
              <div className="flex items-start gap-4">
                <div className="rounded-lg bg-blue-500/10 p-2.5 text-blue-500 group-hover:bg-blue-500/20">
                  <Plus className="h-5 w-5" />
                </div>
                <div className="flex-1">
                  <h4 className="font-medium">Create Topic</h4>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Add a new event stream
                  </p>
                </div>
                <ArrowRight className="h-4 w-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
              </div>
            </Card>
          </Link>

          <Link href="/topics" className="block">
            <Card className="p-5 hover:bg-accent cursor-pointer transition-all hover:shadow-md group">
              <div className="flex items-start gap-4">
                <div className="rounded-lg bg-purple-500/10 p-2.5 text-purple-500 group-hover:bg-purple-500/20">
                  <Search className="h-5 w-5" />
                </div>
                <div className="flex-1">
                  <h4 className="font-medium">Browse Topics</h4>
                  <p className="mt-1 text-sm text-muted-foreground">
                    View and manage topics
                  </p>
                </div>
                <ArrowRight className="h-4 w-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
              </div>
            </Card>
          </Link>

          <Link href="/console" className="block">
            <Card className="p-5 hover:bg-accent cursor-pointer transition-all hover:shadow-md group">
              <div className="flex items-start gap-4">
                <div className="rounded-lg bg-green-500/10 p-2.5 text-green-500 group-hover:bg-green-500/20">
                  <Terminal className="h-5 w-5" />
                </div>
                <div className="flex-1">
                  <h4 className="font-medium">Producer Console</h4>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Send test messages
                  </p>
                </div>
                <ArrowRight className="h-4 w-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
              </div>
            </Card>
          </Link>

          <Link href="/sql" className="block">
            <Card className="p-5 hover:bg-accent cursor-pointer transition-all hover:shadow-md group">
              <div className="flex items-start gap-4">
                <div className="rounded-lg bg-orange-500/10 p-2.5 text-orange-500 group-hover:bg-orange-500/20">
                  <FileSearch className="h-5 w-5" />
                </div>
                <div className="flex-1">
                  <h4 className="font-medium">SQL Workbench</h4>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Query your streams
                  </p>
                </div>
                <ArrowRight className="h-4 w-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
              </div>
            </Card>
          </Link>
        </div>
      </div>
    </DashboardLayout>
  );
}
