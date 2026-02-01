'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { MessageBrowser } from '@/components/message-browser';
import { useTopic, useTopicMessages } from '@/lib/hooks/use-topics';
import { useRealtimeTopicMetrics } from '@/lib/hooks/use-realtime-metrics';
import { formatBytes, formatCompactNumber, formatDate } from '@/lib/utils';
import { Activity, HardDrive, Layers, Clock, TrendingUp } from 'lucide-react';
import Link from 'next/link';
import { LineChart } from '@/components/charts/line-chart';
import { useMemo } from 'react';

interface TopicDetailPageProps {
  params: {
    name: string;
  };
}

export default function TopicDetailPage({ params }: TopicDetailPageProps) {
  const topicName = decodeURIComponent(params.name);
  const { data: topic, isLoading } = useTopic(topicName);
  const { data: messages, isLoading: loadingMessages } = useTopicMessages(topicName);
  const { throughput, isConnected } = useRealtimeTopicMetrics(topicName);

  // Generate chart data from realtime throughput
  const throughputData = useMemo(() => {
    if (throughput.length === 0) {
      // Mock data if no real-time data
      const now = Date.now();
      return Array.from({ length: 24 }, (_, i) => ({
        time: new Date(now - (24 - i) * 3600000).toLocaleTimeString('en-US', { hour: '2-digit' }),
        throughput: Math.floor(Math.random() * 1000 + 200),
      }));
    }

    return throughput.map((value, i) => ({
      time: new Date(Date.now() - (throughput.length - i) * 60000).toLocaleTimeString('en-US', {
        hour: '2-digit',
        minute: '2-digit',
      }),
      throughput: value,
    }));
  }, [throughput]);

  if (isLoading) {
    return (
      <DashboardLayout title="Loading..." description="">
        <div className="flex h-64 items-center justify-center">
          <p className="text-muted-foreground">Loading topic details...</p>
        </div>
      </DashboardLayout>
    );
  }

  if (!topic) {
    return (
      <DashboardLayout title="Topic Not Found" description="">
        <Card className="p-6">
          <p className="text-muted-foreground">
            Topic &quot;{topicName}&quot; not found.{' '}
            <Link href="/topics" className="text-primary hover:underline">
              Back to topics
            </Link>
          </p>
        </Card>
      </DashboardLayout>
    );
  }

  return (
    <DashboardLayout
      title={topicName}
      description={`Topic with ${topic.partitionCount} partitions`}
    >
      {/* Real-time Connection Status */}
      {isConnected && (
        <div className="mb-4">
          <Badge variant="outline" className="gap-2">
            <span className="relative flex h-2 w-2">
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-green-400 opacity-75"></span>
              <span className="relative inline-flex h-2 w-2 rounded-full bg-green-500"></span>
            </span>
            Live Updates Active
          </Badge>
        </div>
      )}

      {/* Topic Overview */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Partitions</h3>
            <Layers className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{topic.partitionCount}</div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Messages</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {formatCompactNumber(topic.messageCount)}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Storage Size</h3>
            <HardDrive className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {formatBytes(topic.sizeBytes)}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Created</h3>
            <Clock className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-lg font-bold">
            {formatDate(topic.createdAt)}
          </div>
        </Card>
      </div>

      {/* Throughput Chart */}
      <Card className="mt-6 p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold">Message Throughput</h3>
          <TrendingUp className="h-5 w-5 text-muted-foreground" />
        </div>
        <LineChart
          data={throughputData}
          xKey="time"
          lines={[{ key: 'throughput', color: '#3b82f6', name: 'Messages/sec' }]}
          height={200}
        />
      </Card>

      {/* Tabs for Messages and Configuration */}
      <Tabs defaultValue="messages" className="mt-6">
        <TabsList>
          <TabsTrigger value="messages">Messages</TabsTrigger>
          <TabsTrigger value="partitions">Partitions</TabsTrigger>
          <TabsTrigger value="config">Configuration</TabsTrigger>
        </TabsList>

        <TabsContent value="messages" className="mt-6">
          <MessageBrowser
            topicName={topicName}
            messages={messages || []}
            isLoading={loadingMessages}
          />
        </TabsContent>

        <TabsContent value="partitions" className="mt-6">
          <Card className="p-6">
            <h3 className="text-lg font-semibold mb-4">Partition Details</h3>
            <div className="space-y-4">
              {topic.partitions?.map((partition) => (
                <div
                  key={partition.id}
                  className="flex items-center justify-between border-b pb-4 last:border-b-0"
                >
                  <div>
                    <p className="font-medium">Partition {partition.id}</p>
                    <p className="text-sm text-muted-foreground">
                      Leader: {partition.leader || 'None'}
                    </p>
                  </div>
                  <div className="text-right">
                    <p className="text-sm">
                      {formatCompactNumber(partition.messageCount)} messages
                    </p>
                    <p className="text-xs text-muted-foreground">
                      {formatBytes(partition.sizeBytes)}
                    </p>
                  </div>
                </div>
              ))}
            </div>
          </Card>
        </TabsContent>

        <TabsContent value="config" className="mt-6">
          <Card className="p-6">
            <h3 className="text-lg font-semibold mb-4">Topic Configuration</h3>
            <div className="space-y-3">
              {Object.entries(topic.config || {}).map(([key, value]) => (
                <div key={key} className="flex items-center justify-between border-b pb-2">
                  <span className="font-mono text-sm text-muted-foreground">{key}</span>
                  <span className="font-mono text-sm">{value}</span>
                </div>
              ))}
              {(!topic.config || Object.keys(topic.config).length === 0) && (
                <p className="text-center text-muted-foreground">No configuration overrides</p>
              )}
            </div>
          </Card>
        </TabsContent>
      </Tabs>
    </DashboardLayout>
  );
}
