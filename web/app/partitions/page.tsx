'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { useAllPartitions } from '@/lib/hooks/use-topics';
import { formatBytes } from '@/lib/utils';
import { Database, Server, HardDrive, Activity } from 'lucide-react';
import Link from 'next/link';

export default function PartitionsPage() {
  const { data: partitions, isLoading } = useAllPartitions();

  const totalPartitions = partitions?.length || 0;
  const leadersCount = partitions?.filter((p) => p.leader !== null).length || 0;
  const totalMessages = partitions?.reduce((acc, p) => acc + p.messageCount, 0) || 0;
  const totalSize = partitions?.reduce((acc, p) => acc + p.sizeBytes, 0) || 0;
  const avgSize = totalPartitions > 0 ? totalSize / totalPartitions : 0;

  // Group partitions by topic
  const partitionsByTopic = partitions?.reduce(
    (acc, p) => {
      if (!acc[p.topic]) acc[p.topic] = [];
      acc[p.topic].push(p);
      return acc;
    },
    {} as Record<string, typeof partitions>
  ) || {};

  return (
    <DashboardLayout
      title="Partitions"
      description="Partition distribution and health monitoring"
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Partitions</h3>
            <Database className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : totalPartitions}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Across all topics</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Leaders</h3>
            <Server className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : leadersCount}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Partition leaders</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Messages</h3>
            <HardDrive className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : totalMessages.toLocaleString()}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Across all partitions</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Avg Size</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : formatBytes(avgSize)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Per partition</p>
        </Card>
      </div>

      {/* Partitions by Topic */}
      {Object.entries(partitionsByTopic).map(([topic, topicPartitions]) => (
        <Card key={topic} className="mt-6">
          <div className="p-6">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold">
                <Link href={`/topics/${topic}`} className="hover:text-primary">
                  {topic}
                </Link>
                <Badge variant="secondary" className="ml-2">
                  {topicPartitions?.length || 0} partitions
                </Badge>
              </h3>
            </div>

            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Partition ID</TableHead>
                  <TableHead>Leader</TableHead>
                  <TableHead>High Watermark</TableHead>
                  <TableHead>Low Watermark</TableHead>
                  <TableHead>Messages</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {topicPartitions?.map((partition) => (
                  <TableRow key={`${topic}-${partition.id}`}>
                    <TableCell className="font-medium">{partition.id}</TableCell>
                    <TableCell>
                      {partition.leader ? (
                        <Badge variant="default">{partition.leader}</Badge>
                      ) : (
                        <Badge variant="secondary">No leader</Badge>
                      )}
                    </TableCell>
                    <TableCell>{partition.highWatermark}</TableCell>
                    <TableCell>{partition.lowWatermark}</TableCell>
                    <TableCell>{partition.messageCount}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </Card>
      ))}

      {isLoading && (
        <Card className="mt-6 p-6">
          <div className="flex h-32 items-center justify-center text-muted-foreground">
            <p>Loading partitions...</p>
          </div>
        </Card>
      )}

      {!isLoading && totalPartitions === 0 && (
        <Card className="mt-6 p-6">
          <div className="flex h-32 items-center justify-center text-muted-foreground">
            <p>No partitions found. Create a topic to see partitions here.</p>
          </div>
        </Card>
      )}
    </DashboardLayout>
  );
}
