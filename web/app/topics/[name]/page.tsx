"use client";

import { useState, useEffect } from "react";
import { use } from "react";
import Link from "next/link";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Separator } from "@/components/ui/separator";
import { Zap, ArrowLeft, Loader2, AlertCircle, CheckCircle, Database, Server, Eye } from "lucide-react";
import { apiClient } from "@/lib/api/client";
import type { Topic, Partition } from "@/lib/api/client";

export default function TopicDetailPage({ params }: { params: Promise<{ name: string }> }) {
  const resolvedParams = use(params);
  const topicName = decodeURIComponent(resolvedParams.name);

  const [topic, setTopic] = useState<Topic | null>(null);
  const [partitions, setPartitions] = useState<Partition[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchTopicDetails = async () => {
      try {
        setLoading(true);
        const [topicData, partitionsData] = await Promise.all([
          apiClient.getTopic(topicName),
          apiClient.listPartitions(topicName),
        ]);
        setTopic(topicData);
        setPartitions(partitionsData);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to fetch topic details');
        console.error('Error fetching topic details:', err);
      } finally {
        setLoading(false);
      }
    };

    fetchTopicDetails();
    // Refresh every 10 seconds
    const interval = setInterval(fetchTopicDetails, 10000);
    return () => clearInterval(interval);
  }, [topicName]);

  if (loading && !topic) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-gray-950">
        <header className="border-b bg-white dark:bg-gray-900">
          <div className="container mx-auto px-4 py-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center space-x-2">
                <Zap className="h-8 w-8 text-blue-600" />
                <h1 className="text-2xl font-bold">StreamHouse</h1>
              </div>
              <nav className="flex items-center space-x-4">
                <Link href="/dashboard">
                  <Button variant="ghost">Dashboard</Button>
                </Link>
                <Link href="/topics">
                  <Button variant="default">Topics</Button>
                </Link>
                <Link href="/schemas">
                  <Button variant="ghost">Schemas</Button>
                </Link>
                <Link href="/agents">
                  <Button variant="ghost">Agents</Button>
                </Link>
                <Link href="/console">
                  <Button variant="ghost">Console</Button>
                </Link>
                <Button variant="outline">Sign Out</Button>
              </nav>
            </div>
          </div>
        </header>
        <main className="container mx-auto px-4 py-8 flex items-center justify-center">
          <div className="flex items-center space-x-2">
            <Loader2 className="h-6 w-6 animate-spin" />
            <span>Loading topic details...</span>
          </div>
        </main>
      </div>
    );
  }

  if (error || !topic) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-gray-950">
        <header className="border-b bg-white dark:bg-gray-900">
          <div className="container mx-auto px-4 py-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center space-x-2">
                <Zap className="h-8 w-8 text-blue-600" />
                <h1 className="text-2xl font-bold">StreamHouse</h1>
              </div>
              <nav className="flex items-center space-x-4">
                <Link href="/dashboard">
                  <Button variant="ghost">Dashboard</Button>
                </Link>
                <Link href="/topics">
                  <Button variant="default">Topics</Button>
                </Link>
                <Link href="/schemas">
                  <Button variant="ghost">Schemas</Button>
                </Link>
                <Link href="/agents">
                  <Button variant="ghost">Agents</Button>
                </Link>
                <Link href="/console">
                  <Button variant="ghost">Console</Button>
                </Link>
                <Button variant="outline">Sign Out</Button>
              </nav>
            </div>
          </div>
        </header>
        <main className="container mx-auto px-4 py-8">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center space-x-2 text-red-600">
                <AlertCircle className="h-5 w-5" />
                <span>Error</span>
              </CardTitle>
            </CardHeader>
            <CardContent>
              <p>{error || 'Topic not found'}</p>
              <Link href="/topics">
                <Button className="mt-4">
                  <ArrowLeft className="mr-2 h-4 w-4" />
                  Back to Topics
                </Button>
              </Link>
            </CardContent>
          </Card>
        </main>
      </div>
    );
  }

  const totalMessages = partitions.reduce((sum, p) => sum + p.high_watermark, 0);

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-gray-950">
      {/* Header */}
      <header className="border-b bg-white dark:bg-gray-900">
        <div className="container mx-auto px-4 py-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <Zap className="h-8 w-8 text-blue-600" />
              <h1 className="text-2xl font-bold">StreamHouse</h1>
            </div>
            <nav className="flex items-center space-x-4">
              <Link href="/dashboard">
                <Button variant="ghost">Dashboard</Button>
              </Link>
              <Link href="/topics">
                <Button variant="default">Topics</Button>
              </Link>
              <Link href="/agents">
                <Button variant="ghost">Agents</Button>
              </Link>
              <Link href="/console">
                <Button variant="ghost">Console</Button>
              </Link>
              <Button variant="outline">Sign Out</Button>
            </nav>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="container mx-auto px-4 py-8">
        {/* Back Button */}
        <div className="mb-6">
          <Link href="/topics">
            <Button variant="ghost" size="sm">
              <ArrowLeft className="mr-2 h-4 w-4" />
              Back to Topics
            </Button>
          </Link>
        </div>

        {/* Topic Header */}
        <div className="mb-6">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-3xl font-bold">{topic.name}</h2>
              <p className="text-muted-foreground mt-1">
                Created {new Date(topic.created_at).toLocaleDateString()}
              </p>
            </div>
            <Badge variant="default" className="bg-green-500">
              <CheckCircle className="mr-1 h-3 w-3" />
              Active
            </Badge>
          </div>
        </div>

        {/* Stats Cards */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Partitions</CardTitle>
              <Database className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{topic.partitions}</div>
              <p className="text-xs text-muted-foreground">
                Total partition count
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Total Messages</CardTitle>
              <Database className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{totalMessages.toLocaleString()}</div>
              <p className="text-xs text-muted-foreground">
                Across all partitions
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Replication Factor</CardTitle>
              <Server className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{topic.replication_factor}</div>
              <p className="text-xs text-muted-foreground">
                Copies per partition
              </p>
            </CardContent>
          </Card>
        </div>

        <Separator className="my-8" />

        {/* Partitions Table */}
        <Card>
          <CardHeader>
            <CardTitle>Partitions</CardTitle>
            <CardDescription>
              Detailed view of all partitions for this topic
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Partition ID</TableHead>
                  <TableHead>Leader Agent</TableHead>
                  <TableHead>High Watermark</TableHead>
                  <TableHead>Low Watermark</TableHead>
                  <TableHead>Messages</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {partitions.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={7} className="text-center text-muted-foreground">
                      No partitions found
                    </TableCell>
                  </TableRow>
                ) : (
                  partitions.map((partition) => (
                    <TableRow key={partition.partition_id}>
                      <TableCell className="font-medium">
                        {partition.partition_id}
                      </TableCell>
                      <TableCell>
                        {partition.leader_agent_id ? (
                          <code className="text-xs bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded">
                            {partition.leader_agent_id}
                          </code>
                        ) : (
                          <span className="text-muted-foreground">-</span>
                        )}
                      </TableCell>
                      <TableCell>{partition.high_watermark.toLocaleString()}</TableCell>
                      <TableCell>{partition.low_watermark.toLocaleString()}</TableCell>
                      <TableCell>
                        {(partition.high_watermark - partition.low_watermark).toLocaleString()}
                      </TableCell>
                      <TableCell>
                        <Badge variant="default" className="bg-green-500">
                          <CheckCircle className="mr-1 h-3 w-3" />
                          Active
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Link href={`/topics/${topicName}/partitions/${partition.partition_id}/messages`}>
                          <Button variant="outline" size="sm">
                            <Eye className="mr-2 h-4 w-4" />
                            View Messages
                          </Button>
                        </Link>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </main>
    </div>
  );
}
