'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { MetricCard } from '@/components/ui/metric-card';
import { DataTable, Column } from '@/components/ui/data-table';
import { Skeleton, SkeletonCard } from '@/components/ui/skeleton';
import { EmptyTopics } from '@/components/ui/empty-state';
import { useTopics } from '@/lib/hooks/use-topics';
import { formatBytes, formatCompactNumber } from '@/lib/utils';
import { Plus, Star, Database, Activity, HardDrive, Trash2, Settings, Eye, MoreHorizontal } from 'lucide-react';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useAppStore } from '@/lib/store';

interface Topic {
  name: string;
  partitionCount?: number;
  messageCount?: number;
  sizeBytes?: number;
  messagesPerSecond?: number;
  retentionMs?: number;
}

export default function TopicsPage() {
  const { data: topics, isLoading } = useTopics();
  const router = useRouter();
  const favoriteTopics = useAppStore((state) => state.favoriteTopics);
  const toggleFavoriteTopic = useAppStore((state) => state.toggleFavoriteTopic);

  const columns: Column<Topic>[] = [
    {
      id: 'favorite',
      header: '',
      className: 'w-12',
      cell: (row) => (
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          onClick={(e) => {
            e.stopPropagation();
            toggleFavoriteTopic(row.name);
          }}
        >
          <Star
            className={`h-4 w-4 ${
              favoriteTopics.includes(row.name)
                ? 'fill-yellow-400 text-yellow-400'
                : 'text-muted-foreground hover:text-yellow-400'
            }`}
          />
        </Button>
      ),
    },
    {
      id: 'name',
      header: 'Name',
      accessorKey: 'name',
      sortable: true,
      cell: (row) => (
        <div className="flex items-center gap-2">
          <span className="font-medium">{row.name}</span>
          {favoriteTopics.includes(row.name) && (
            <Badge variant="secondary" className="text-xs">Favorite</Badge>
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
        <Badge variant="outline">{row.partitionCount || 0}</Badge>
      ),
    },
    {
      id: 'messages',
      header: 'Messages',
      accessorKey: 'messageCount',
      sortable: true,
      cell: (row) => formatCompactNumber(row.messageCount || 0),
    },
    {
      id: 'size',
      header: 'Size',
      accessorKey: 'sizeBytes',
      sortable: true,
      cell: (row) => formatBytes(row.sizeBytes || 0),
    },
    {
      id: 'throughput',
      header: 'Throughput',
      accessorKey: 'messagesPerSecond',
      sortable: true,
      cell: (row) => (
        <span className="text-muted-foreground">
          {formatCompactNumber(row.messagesPerSecond || 0)}/s
        </span>
      ),
    },
    {
      id: 'retention',
      header: 'Retention',
      accessorKey: 'retentionMs',
      sortable: true,
      cell: (row) => (
        <Badge variant="secondary">
          {row.retentionMs
            ? `${Math.floor(row.retentionMs / 86400000)}d`
            : 'Forever'}
        </Badge>
      ),
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
      </div>
    </Card>
  );

  return (
    <DashboardLayout
      title="Topics"
      description="Manage your event streams and partitions"
      actions={
        <Link href="/topics/new">
          <Button>
            <Plus className="mr-2 h-4 w-4" />
            Create Topic
          </Button>
        </Link>
      }
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {isLoading ? (
          <>
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
          </>
        ) : (
          <>
            <MetricCard
              title="Total Topics"
              value={(topics?.length || 0).toString()}
              icon={Database}
            />
            <MetricCard
              title="Total Partitions"
              value={(topics?.reduce((acc, t) => acc + (t.partitionCount || 0), 0) || 0).toString()}
              icon={Activity}
            />
            <MetricCard
              title="Total Messages"
              value={formatCompactNumber(topics?.reduce((acc, t) => acc + (t.messageCount || 0), 0) || 0)}
              icon={Activity}
            />
            <MetricCard
              title="Total Storage"
              value={formatBytes(topics?.reduce((acc, t) => acc + (t.sizeBytes || 0), 0) || 0)}
              icon={HardDrive}
            />
          </>
        )}
      </div>

      {/* Topics Table */}
      <Card className="mt-6">
        <div className="p-6">
          <DataTable
            data={topics || []}
            columns={columns}
            isLoading={isLoading}
            loadingRows={5}
            searchKey="name"
            searchPlaceholder="Search topics..."
            pageSize={10}
            pageSizeOptions={[10, 25, 50, 100]}
            selectable
            getRowId={(row) => row.name}
            onRowClick={(row) => router.push(`/topics/${row.name}`)}
            emptyState={<EmptyTopics />}
            bulkActions={[
              {
                label: 'Delete',
                icon: <Trash2 className="mr-2 h-4 w-4" />,
                variant: 'destructive',
                onClick: (selected) => {
                  console.log('Delete topics:', selected.map(t => t.name));
                },
              },
            ]}
            rowActions={(row) => [
              {
                label: 'View Details',
                icon: <Eye className="mr-2 h-4 w-4" />,
                onClick: () => router.push(`/topics/${row.name}`),
              },
              {
                label: 'Settings',
                icon: <Settings className="mr-2 h-4 w-4" />,
                onClick: () => router.push(`/topics/${row.name}/settings`),
              },
              {
                label: 'Delete',
                icon: <Trash2 className="mr-2 h-4 w-4" />,
                variant: 'destructive',
                onClick: () => {
                  console.log('Delete topic:', row.name);
                },
              },
            ]}
          />
        </div>
      </Card>
    </DashboardLayout>
  );
}
