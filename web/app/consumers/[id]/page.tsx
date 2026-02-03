'use client';

import { useState } from 'react';
import { useParams, useRouter } from 'next/navigation';
import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useConsumerGroup, useConsumerGroupLag } from '@/lib/hooks/use-consumer-groups';
import { formatCompactNumber, getLagColor } from '@/lib/utils';
import {
  ArrowLeft,
  AlertTriangle,
  Activity,
  Database,
  TrendingUp,
  RefreshCw,
  Clock,
  MoreVertical,
  RotateCcw,
  FastForward,
  Trash2,
} from 'lucide-react';
import Link from 'next/link';

// API response types (matching what backend returns)
interface ConsumerOffsetInfo {
  topic: string;
  partitionId: number;
  committedOffset: number;
  highWatermark: number;
  lag: number;
}

interface ConsumerGroupDetailResponse {
  groupId: string;
  offsets: ConsumerOffsetInfo[];
}

interface ConsumerGroupLagResponse {
  groupId: string;
  totalLag: number;
  partitionCount: number;
  topics: string[];
}

export default function ConsumerGroupDetailPage() {
  const params = useParams();
  const router = useRouter();
  const groupId = params.id as string;

  const { data: groupDetail, isLoading: detailLoading, error: detailError, refetch: refetchDetail } = useConsumerGroup(groupId);
  const { data: lagData, isLoading: lagLoading, refetch: refetchLag } = useConsumerGroupLag(groupId);

  // Dialog states
  const [resetDialogOpen, setResetDialogOpen] = useState(false);
  const [seekDialogOpen, setSeekDialogOpen] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);

  // Reset form state
  const [resetStrategy, setResetStrategy] = useState<'earliest' | 'latest' | 'specific'>('earliest');
  const [resetTopic, setResetTopic] = useState<string>('');
  const [specificOffset, setSpecificOffset] = useState<string>('0');

  // Seek form state
  const [seekTopic, setSeekTopic] = useState<string>('');
  const [seekTimestamp, setSeekTimestamp] = useState<string>('');

  // Cast to proper types
  const detail = groupDetail as ConsumerGroupDetailResponse | undefined;
  const lag = lagData as ConsumerGroupLagResponse | undefined;

  const handleRefresh = () => {
    refetchDetail();
    refetchLag();
  };

  // Reset offsets action
  const handleResetOffsets = async () => {
    setActionLoading(true);
    try {
      const body: Record<string, unknown> = {
        strategy: resetStrategy,
      };
      if (resetTopic) body.topic = resetTopic;
      if (resetStrategy === 'specific') body.offset = parseInt(specificOffset, 10);

      const response = await fetch(`/api/v1/consumer-groups/${encodeURIComponent(groupId)}/reset`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });

      if (!response.ok) {
        throw new Error('Failed to reset offsets');
      }

      setResetDialogOpen(false);
      handleRefresh();
    } catch (error) {
      console.error('Reset failed:', error);
      alert('Failed to reset offsets. Please try again.');
    } finally {
      setActionLoading(false);
    }
  };

  // Seek to timestamp action
  const handleSeekToTimestamp = async () => {
    if (!seekTopic || !seekTimestamp) {
      alert('Please select a topic and enter a timestamp');
      return;
    }

    setActionLoading(true);
    try {
      const timestamp = new Date(seekTimestamp).getTime();
      const response = await fetch(`/api/v1/consumer-groups/${encodeURIComponent(groupId)}/seek`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          topic: seekTopic,
          timestamp,
        }),
      });

      if (!response.ok) {
        throw new Error('Failed to seek to timestamp');
      }

      setSeekDialogOpen(false);
      handleRefresh();
    } catch (error) {
      console.error('Seek failed:', error);
      alert('Failed to seek to timestamp. Please try again.');
    } finally {
      setActionLoading(false);
    }
  };

  // Delete consumer group action
  const handleDeleteConsumerGroup = async () => {
    setActionLoading(true);
    try {
      const response = await fetch(`/api/v1/consumer-groups/${encodeURIComponent(groupId)}`, {
        method: 'DELETE',
      });

      if (!response.ok) {
        throw new Error('Failed to delete consumer group');
      }

      router.push('/consumers');
    } catch (error) {
      console.error('Delete failed:', error);
      alert('Failed to delete consumer group. Please try again.');
    } finally {
      setActionLoading(false);
    }
  };

  // Calculate stats from offsets
  const totalLag = detail?.offsets?.reduce((sum, o) => sum + Math.max(0, o.lag), 0) || lag?.totalLag || 0;
  const partitionCount = detail?.offsets?.length || lag?.partitionCount || 0;
  const topics = [...new Set(detail?.offsets?.map(o => o.topic) || lag?.topics || [])];
  const totalCommitted = detail?.offsets?.reduce((sum, o) => sum + o.committedOffset, 0) || 0;

  // Group offsets by topic
  const offsetsByTopic = detail?.offsets?.reduce((acc, offset) => {
    if (!acc[offset.topic]) {
      acc[offset.topic] = [];
    }
    acc[offset.topic].push(offset);
    return acc;
  }, {} as Record<string, ConsumerOffsetInfo[]>) || {};

  return (
    <DashboardLayout
      title={
        <div className="flex items-center gap-3">
          <Link href="/consumers" className="hover:opacity-70">
            <ArrowLeft className="h-5 w-5" />
          </Link>
          <span>Consumer Group: {groupId}</span>
        </div>
      }
      description="Monitor consumer lag and partition assignments"
    >
      {/* Action Bar */}
      <div className="flex items-center justify-between mb-6">
        <Badge variant="default" className="text-sm">
          {partitionCount} Partitions
        </Badge>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={handleRefresh}>
            <RefreshCw className="h-4 w-4 mr-2" />
            Refresh
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="sm">
                <MoreVertical className="h-4 w-4 mr-2" />
                Actions
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={() => setResetDialogOpen(true)}>
                <RotateCcw className="h-4 w-4 mr-2" />
                Reset Offsets
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setSeekDialogOpen(true)}>
                <FastForward className="h-4 w-4 mr-2" />
                Seek to Timestamp
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={() => setDeleteDialogOpen(true)}
                className="text-red-600 focus:text-red-600"
              >
                <Trash2 className="h-4 w-4 mr-2" />
                Delete Consumer Group
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      {/* Error State */}
      {detailError && (
        <Card className="p-6 border-red-500/50 bg-red-500/10 mb-6">
          <div className="flex items-center gap-2 text-red-500">
            <AlertTriangle className="h-5 w-5" />
            <span>Failed to load consumer group: {(detailError as Error).message}</span>
          </div>
        </Card>
      )}

      {/* Stats Cards */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4 mb-6">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Lag</h3>
            <AlertTriangle className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className={`mt-2 text-3xl font-bold ${getLagColor(totalLag)}`}>
            {detailLoading ? '...' : formatCompactNumber(totalLag)}
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            messages behind
          </p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Partitions</h3>
            <Database className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {detailLoading ? '...' : partitionCount}
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            assigned partitions
          </p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Topics</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {detailLoading ? '...' : topics.length}
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            subscribed topics
          </p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Committed</h3>
            <TrendingUp className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {detailLoading ? '...' : formatCompactNumber(totalCommitted)}
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            total offset
          </p>
        </Card>
      </div>

      {/* Subscribed Topics */}
      <Card className="p-6 mb-6">
        <h3 className="text-lg font-semibold mb-4">Subscribed Topics</h3>
        <div className="flex flex-wrap gap-2">
          {detailLoading ? (
            <span className="text-muted-foreground">Loading...</span>
          ) : topics.length === 0 ? (
            <span className="text-muted-foreground">No topics subscribed</span>
          ) : (
            topics.map((topic) => (
              <Link key={topic} href={`/topics/${topic}`}>
                <Badge variant="secondary" className="hover:bg-secondary/80 cursor-pointer">
                  {topic}
                </Badge>
              </Link>
            ))
          )}
        </div>
      </Card>

      {/* Partition Details by Topic */}
      {Object.entries(offsetsByTopic).map(([topic, offsets]) => (
        <Card key={topic} className="mb-6">
          <div className="p-6">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold">
                <Link href={`/topics/${topic}`} className="hover:text-primary">
                  {topic}
                </Link>
              </h3>
              <Badge variant="outline">
                {offsets.length} partitions
              </Badge>
            </div>

            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Partition</TableHead>
                  <TableHead className="text-right">Committed Offset</TableHead>
                  <TableHead className="text-right">High Watermark</TableHead>
                  <TableHead className="text-right">Lag</TableHead>
                  <TableHead>Lag Bar</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {offsets
                  .sort((a, b) => a.partitionId - b.partitionId)
                  .map((offset) => {
                    const lagPercent = offset.highWatermark > 0
                      ? Math.min(100, (offset.lag / offset.highWatermark) * 100)
                      : 0;

                    return (
                      <TableRow key={`${offset.topic}-${offset.partitionId}`}>
                        <TableCell className="font-medium">
                          Partition {offset.partitionId}
                        </TableCell>
                        <TableCell className="text-right font-mono">
                          {formatCompactNumber(offset.committedOffset)}
                        </TableCell>
                        <TableCell className="text-right font-mono">
                          {formatCompactNumber(offset.highWatermark)}
                        </TableCell>
                        <TableCell className={`text-right font-mono font-bold ${getLagColor(offset.lag)}`}>
                          {formatCompactNumber(Math.max(0, offset.lag))}
                        </TableCell>
                        <TableCell className="w-48">
                          <div className="flex items-center gap-2">
                            <div className="flex-1 h-2 bg-muted rounded-full overflow-hidden">
                              <div
                                className={`h-full transition-all ${
                                  offset.lag === 0
                                    ? 'bg-green-500'
                                    : offset.lag < 100
                                    ? 'bg-yellow-500'
                                    : 'bg-red-500'
                                }`}
                                style={{ width: `${Math.max(lagPercent, offset.lag > 0 ? 5 : 0)}%` }}
                              />
                            </div>
                            <span className="text-xs text-muted-foreground w-12 text-right">
                              {lagPercent.toFixed(0)}%
                            </span>
                          </div>
                        </TableCell>
                      </TableRow>
                    );
                  })}
              </TableBody>
            </Table>

            {/* Topic Summary */}
            <div className="mt-4 pt-4 border-t flex items-center justify-between text-sm">
              <span className="text-muted-foreground">
                Total lag for {topic}:
              </span>
              <span className={`font-bold ${getLagColor(offsets.reduce((sum, o) => sum + Math.max(0, o.lag), 0))}`}>
                {formatCompactNumber(offsets.reduce((sum, o) => sum + Math.max(0, o.lag), 0))} messages
              </span>
            </div>
          </div>
        </Card>
      ))}

      {/* Empty State */}
      {!detailLoading && Object.keys(offsetsByTopic).length === 0 && !detailError && (
        <Card className="p-12">
          <div className="text-center">
            <Clock className="h-12 w-12 mx-auto text-muted-foreground mb-4" />
            <h3 className="text-lg font-semibold mb-2">No Offset Data</h3>
            <p className="text-muted-foreground mb-4">
              This consumer group has not committed any offsets yet.
            </p>
            <p className="text-sm text-muted-foreground">
              Start consuming from a topic and commit offsets to see data here.
            </p>
          </div>
        </Card>
      )}

      {/* Loading State */}
      {detailLoading && (
        <Card className="p-12">
          <div className="text-center">
            <RefreshCw className="h-12 w-12 mx-auto text-muted-foreground mb-4 animate-spin" />
            <h3 className="text-lg font-semibold">Loading consumer group data...</h3>
          </div>
        </Card>
      )}

      {/* Reset Offsets Dialog */}
      <Dialog open={resetDialogOpen} onOpenChange={setResetDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Reset Offsets</DialogTitle>
            <DialogDescription>
              Reset consumer group offsets for {groupId}. This will affect where the consumer resumes reading.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="reset-strategy">Reset Strategy</Label>
              <Select
                value={resetStrategy}
                onValueChange={(v) => setResetStrategy(v as 'earliest' | 'latest' | 'specific')}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select strategy" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="earliest">Earliest (Start from beginning)</SelectItem>
                  <SelectItem value="latest">Latest (Skip to end)</SelectItem>
                  <SelectItem value="specific">Specific Offset</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {resetStrategy === 'specific' && (
              <div className="space-y-2">
                <Label htmlFor="specific-offset">Offset</Label>
                <Input
                  id="specific-offset"
                  type="number"
                  value={specificOffset}
                  onChange={(e) => setSpecificOffset(e.target.value)}
                  placeholder="Enter offset number"
                />
              </div>
            )}
            <div className="space-y-2">
              <Label htmlFor="reset-topic">Topic (Optional)</Label>
              <Select value={resetTopic} onValueChange={setResetTopic}>
                <SelectTrigger>
                  <SelectValue placeholder="All topics" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="">All topics</SelectItem>
                  {topics.map((topic) => (
                    <SelectItem key={topic} value={topic}>
                      {topic}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setResetDialogOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleResetOffsets} disabled={actionLoading}>
              {actionLoading ? 'Resetting...' : 'Reset Offsets'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Seek to Timestamp Dialog */}
      <Dialog open={seekDialogOpen} onOpenChange={setSeekDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Seek to Timestamp</DialogTitle>
            <DialogDescription>
              Move the consumer to a specific point in time. Messages after this timestamp will be consumed.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="seek-topic">Topic</Label>
              <Select value={seekTopic} onValueChange={setSeekTopic}>
                <SelectTrigger>
                  <SelectValue placeholder="Select topic" />
                </SelectTrigger>
                <SelectContent>
                  {topics.map((topic) => (
                    <SelectItem key={topic} value={topic}>
                      {topic}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="seek-timestamp">Timestamp</Label>
              <Input
                id="seek-timestamp"
                type="datetime-local"
                value={seekTimestamp}
                onChange={(e) => setSeekTimestamp(e.target.value)}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setSeekDialogOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleSeekToTimestamp} disabled={actionLoading || !seekTopic || !seekTimestamp}>
              {actionLoading ? 'Seeking...' : 'Seek'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete Consumer Group Dialog */}
      <Dialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Consumer Group</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete the consumer group <strong>{groupId}</strong>?
              This will remove all committed offsets and cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteDialogOpen(false)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleDeleteConsumerGroup} disabled={actionLoading}>
              {actionLoading ? 'Deleting...' : 'Delete Consumer Group'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </DashboardLayout>
  );
}
