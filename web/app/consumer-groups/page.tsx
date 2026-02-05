"use client";

import { useState, useEffect } from "react";
import { DashboardLayout } from "@/components/layout/dashboard-layout";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { MetricCard } from "@/components/ui/metric-card";
import { DataTable, Column } from "@/components/ui/data-table";
import { Skeleton, SkeletonChart } from "@/components/ui/skeleton";
import { EmptyConsumerGroups } from "@/components/ui/empty-state";
import { BarChart } from "@/components/charts/bar-chart";
import { Users, TrendingUp, Database, AlertTriangle, CheckCircle2 } from "lucide-react";
import { apiClient } from "@/lib/api/client";
import type { ConsumerGroupInfo } from "@/lib/api/client";

export default function ConsumerGroupsPage() {
  const [groups, setGroups] = useState<ConsumerGroupInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchGroups = async () => {
      try {
        const data = await apiClient.listConsumerGroups();
        setGroups(data);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to fetch consumer groups');
        console.error('Error fetching consumer groups:', err);
      } finally {
        setLoading(false);
      }
    };

    fetchGroups();
    const interval = setInterval(fetchGroups, 10000);
    return () => clearInterval(interval);
  }, []);

  const totalLag = groups.reduce((sum, g) => sum + g.totalLag, 0);
  const totalPartitions = groups.reduce((sum, g) => sum + g.partitionCount, 0);
  const caughtUpCount = groups.filter(g => g.totalLag === 0).length;
  const laggingCount = groups.filter(g => g.totalLag > 1000).length;

  // Prepare lag data for chart (top 10 groups by lag)
  const lagChartData = groups
    .filter(g => g.totalLag > 0)
    .sort((a, b) => b.totalLag - a.totalLag)
    .slice(0, 10)
    .map(g => ({
      group: g.groupId.length > 12 ? g.groupId.slice(0, 12) + '...' : g.groupId,
      lag: g.totalLag,
    }));

  const columns: Column<ConsumerGroupInfo>[] = [
    {
      id: 'groupId',
      header: 'Group ID',
      accessorKey: 'groupId',
      sortable: true,
      cell: (row) => (
        <code className="text-xs bg-muted px-2 py-1 rounded font-mono">
          {row.groupId}
        </code>
      ),
    },
    {
      id: 'topics',
      header: 'Topics',
      cell: (row) => (
        <div className="flex flex-wrap gap-1 max-w-50">
          {row.topics.slice(0, 3).map((topic) => (
            <Badge key={topic} variant="outline" className="text-xs">
              {topic}
            </Badge>
          ))}
          {row.topics.length > 3 && (
            <Badge variant="secondary" className="text-xs">
              +{row.topics.length - 3} more
            </Badge>
          )}
        </div>
      ),
    },
    {
      id: 'partitions',
      header: 'Partitions',
      accessorKey: 'partitionCount',
      sortable: true,
      cell: (row) => (
        <Badge variant="outline">{row.partitionCount}</Badge>
      ),
    },
    {
      id: 'lag',
      header: 'Total Lag',
      accessorKey: 'totalLag',
      sortable: true,
      cell: (row) => (
        <span className={`font-mono text-sm ${row.totalLag > 1000 ? 'text-red-600 font-bold' : ''}`}>
          {row.totalLag.toLocaleString()}
        </span>
      ),
    },
    {
      id: 'status',
      header: 'Status',
      sortable: true,
      accessorFn: (row) => {
        if (row.totalLag === 0) return 'caught-up';
        if (row.totalLag > 1000) return 'lagging';
        return 'normal';
      },
      cell: (row) => {
        const lagStatus = row.totalLag === 0 ? 'caught-up' : row.totalLag > 1000 ? 'lagging' : 'normal';
        return (
          <Badge
            variant="secondary"
            className={
              lagStatus === 'caught-up' ? 'bg-green-500/20 text-green-700 dark:text-green-400 border-green-500/30' :
              lagStatus === 'lagging' ? 'bg-red-500/20 text-red-700 dark:text-red-400 border-red-500/30' :
              'bg-yellow-500/20 text-yellow-700 dark:text-yellow-400 border-yellow-500/30'
            }
          >
            {lagStatus === 'caught-up' && <CheckCircle2 className="mr-1 h-3 w-3" />}
            {lagStatus === 'lagging' && <AlertTriangle className="mr-1 h-3 w-3" />}
            {lagStatus === 'caught-up' ? 'Caught Up' : lagStatus === 'lagging' ? 'Lagging' : 'Normal'}
          </Badge>
        );
      },
    },
  ];

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
      title="Consumer Groups"
      description="Monitor consumer group lag and consumption progress"
    >
      {/* Stats Overview */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {loading ? (
          <>
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
          </>
        ) : (
          <>
            <MetricCard
              title="Consumer Groups"
              value={groups.length.toString()}
              description="Active groups"
              icon={Users}
            />
            <MetricCard
              title="Total Lag"
              value={totalLag.toLocaleString()}
              description="Messages behind"
              icon={TrendingUp}
              className={totalLag > 10000 ? 'border-red-500/30' : ''}
            />
            <MetricCard
              title="Caught Up"
              value={caughtUpCount.toString()}
              description="Groups at head"
              icon={CheckCircle2}
              className="border-green-500/30"
            />
            <MetricCard
              title="Total Partitions"
              value={totalPartitions.toString()}
              description="Being consumed"
              icon={Database}
            />
          </>
        )}
      </div>

      {/* Lag Visualization */}
      <Card className="mt-6 p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">Consumer Lag Distribution</h3>
          <p className="text-sm text-muted-foreground">Top 10 consumer groups by lag</p>
        </div>
        {loading ? (
          <SkeletonChart height={250} />
        ) : lagChartData.length > 0 ? (
          <BarChart
            data={lagChartData}
            xKey="group"
            bars={[{ key: 'lag', color: '#f59e0b', name: 'Messages Behind' }]}
            height={250}
          />
        ) : (
          <div className="flex h-64 items-center justify-center text-muted-foreground">
            <div className="text-center">
              <CheckCircle2 className="mx-auto h-12 w-12 text-green-500 opacity-50 mb-4" />
              <p>All consumer groups are caught up!</p>
              <p className="text-sm mt-1">No lag detected across all groups</p>
            </div>
          </div>
        )}
      </Card>

      {/* Consumer Groups Table */}
      <Card className="mt-6">
        <div className="p-6">
          <DataTable
            data={groups}
            columns={columns}
            isLoading={loading}
            loadingRows={5}
            searchKey="groupId"
            searchPlaceholder="Search consumer groups..."
            pageSize={10}
            pageSizeOptions={[10, 25, 50]}
            getRowId={(row) => row.groupId}
            emptyState={<EmptyConsumerGroups />}
          />
        </div>
      </Card>
    </DashboardLayout>
  );
}
