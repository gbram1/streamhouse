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
import { useTopics } from '@/lib/hooks/use-topics';
import { Activity, Send, AlertCircle, Info } from 'lucide-react';
import Link from 'next/link';

export default function ProducersPage() {
  const { data: topics, isLoading } = useTopics();

  // Calculate aggregate stats from topics
  const totalMessages = topics?.reduce((acc, t) => acc + (t.messageCount || 0), 0) || 0;
  const topicsWithMessages = topics?.filter((t) => (t.messageCount || 0) > 0).length || 0;

  return (
    <DashboardLayout
      title="Producers"
      description="Monitor producer activity and performance"
    >
      {/* Info Banner */}
      <Card className="p-4 mb-6 bg-muted/50">
        <div className="flex items-start gap-3">
          <Info className="h-5 w-5 text-muted-foreground mt-0.5" />
          <div>
            <p className="text-sm">
              <strong>Note:</strong> StreamHouse uses a stateless REST API for message production.
              Producers connect, send messages, and disconnect. There are no persistent producer connections to track.
            </p>
            <p className="text-sm text-muted-foreground mt-1">
              Use the <Link href="/topics" className="text-primary hover:underline">Topics</Link> page to see message counts per topic,
              or the <Link href="/monitoring" className="text-primary hover:underline">Monitoring</Link> page for throughput metrics.
            </p>
          </div>
        </div>
      </Card>

      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Messages</h3>
            <Send className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : totalMessages.toLocaleString()}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Written to all topics</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Active Topics</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : topicsWithMessages}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Topics with messages</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Topics</h3>
            <Send className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : topics?.length || 0}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Available for writing</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Error Rate</h3>
            <AlertCircle className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold text-green-600">0%</div>
          <p className="mt-1 text-xs text-muted-foreground">No errors recorded</p>
        </Card>
      </div>

      {/* Message Distribution by Topic */}
      <Card className="mt-6">
        <div className="p-6">
          <h3 className="text-lg font-semibold mb-4">Message Distribution by Topic</h3>

          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Topic</TableHead>
                <TableHead>Partitions</TableHead>
                <TableHead>Messages</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableRow>
                  <TableCell colSpan={4} className="text-center">
                    Loading topics...
                  </TableCell>
                </TableRow>
              ) : topics?.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={4} className="text-center text-muted-foreground">
                    No topics found. Create a topic to start producing messages.
                  </TableCell>
                </TableRow>
              ) : (
                topics?.map((topic) => (
                  <TableRow key={topic.name}>
                    <TableCell className="font-medium">
                      <Link href={`/topics/${topic.name}`} className="hover:text-primary">
                        {topic.name}
                      </Link>
                    </TableCell>
                    <TableCell>{topic.partitionCount}</TableCell>
                    <TableCell>{(topic.messageCount || 0).toLocaleString()}</TableCell>
                    <TableCell>
                      <Badge variant={(topic.messageCount || 0) > 0 ? 'default' : 'secondary'}>
                        {(topic.messageCount || 0) > 0 ? 'Active' : 'Empty'}
                      </Badge>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </div>
      </Card>

      {/* How to Produce Messages */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">How to Produce Messages</h3>
        <div className="space-y-4">
          <div>
            <h4 className="font-medium mb-2">REST API</h4>
            <pre className="bg-muted p-3 rounded-md text-sm overflow-x-auto">
{`curl -X POST http://localhost:8080/api/v1/produce \\
  -H "Content-Type: application/json" \\
  -d '{
    "topic": "my-topic",
    "partition": 0,
    "key": "my-key",
    "value": "{\\"message\\": \\"Hello World\\"}"
  }'`}
            </pre>
          </div>
          <div>
            <h4 className="font-medium mb-2">gRPC Client</h4>
            <p className="text-sm text-muted-foreground">
              Connect to <code className="bg-muted px-1 rounded">localhost:50051</code> using
              the StreamHouse gRPC protocol for high-throughput production.
            </p>
          </div>
        </div>
      </Card>
    </DashboardLayout>
  );
}
