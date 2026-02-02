'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { useTopics } from '@/lib/hooks/use-topics';
import { formatBytes, formatCompactNumber } from '@/lib/utils';
import { Search, Plus, Star, Database, Activity, HardDrive } from 'lucide-react';
import Link from 'next/link';
import { useState } from 'react';
import { useAppStore } from '@/lib/store';

export default function TopicsPage() {
  const { data: topics, isLoading } = useTopics();
  const [searchQuery, setSearchQuery] = useState('');
  const favoriteTopics = useAppStore((state) => state.favoriteTopics);
  const toggleFavoriteTopic = useAppStore((state) => state.toggleFavoriteTopic);

  const filteredTopics = topics?.filter((topic) =>
    topic.name.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <DashboardLayout
      title="Topics"
      description="Manage your event streams and partitions"
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Topics</h3>
            <Database className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : topics?.length || 0}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Partitions</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : topics?.reduce((acc, t) => acc + (t.partitionCount || 0), 0) || 0}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Messages</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading
              ? '...'
              : formatCompactNumber(topics?.reduce((acc, t) => acc + (t.messageCount || 0), 0) || 0)}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Storage</h3>
            <HardDrive className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading
              ? '...'
              : formatBytes(topics?.reduce((acc, t) => acc + (t.sizeBytes || 0), 0) || 0)}
          </div>
        </Card>
      </div>

      {/* Topics Table */}
      <Card className="mt-6">
        <div className="p-6">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <div className="relative flex-1">
                <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  placeholder="Search topics..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-10 w-80"
                />
              </div>
            </div>
            <Link href="/topics/new">
              <Button>
                <Plus className="mr-2 h-4 w-4" />
                Create Topic
              </Button>
            </Link>
          </div>

          <div className="mt-6">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-12"></TableHead>
                  <TableHead>Name</TableHead>
                  <TableHead>Partitions</TableHead>
                  <TableHead>Messages</TableHead>
                  <TableHead>Size</TableHead>
                  <TableHead>Msg/sec</TableHead>
                  <TableHead>Retention</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {isLoading ? (
                  <TableRow>
                    <TableCell colSpan={8} className="text-center">
                      Loading topics...
                    </TableCell>
                  </TableRow>
                ) : filteredTopics?.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={8} className="text-center text-muted-foreground">
                      {searchQuery ? 'No topics match your search' : 'No topics yet. Create your first topic to get started.'}
                    </TableCell>
                  </TableRow>
                ) : (
                  filteredTopics?.map((topic) => (
                    <TableRow key={topic.name}>
                      <TableCell>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-8 w-8"
                          onClick={() => toggleFavoriteTopic(topic.name)}
                        >
                          <Star
                            className={`h-4 w-4 ${
                              favoriteTopics.includes(topic.name)
                                ? 'fill-yellow-400 text-yellow-400'
                                : 'text-muted-foreground'
                            }`}
                          />
                        </Button>
                      </TableCell>
                      <TableCell className="font-medium">
                        <Link
                          href={`/topics/${topic.name}`}
                          className="hover:text-primary"
                        >
                          {topic.name}
                        </Link>
                      </TableCell>
                      <TableCell>{topic.partitionCount || 0}</TableCell>
                      <TableCell>{formatCompactNumber(topic.messageCount || 0)}</TableCell>
                      <TableCell>{formatBytes(topic.sizeBytes || 0)}</TableCell>
                      <TableCell>{formatCompactNumber(topic.messagesPerSecond || 0)}</TableCell>
                      <TableCell>
                        {topic.retentionMs
                          ? `${Math.floor(topic.retentionMs / 86400000)}d`
                          : 'Forever'}
                      </TableCell>
                      <TableCell>
                        <Link href={`/topics/${topic.name}`}>
                          <Button variant="ghost" size="sm">
                            View
                          </Button>
                        </Link>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>
        </div>
      </Card>
    </DashboardLayout>
  );
}
