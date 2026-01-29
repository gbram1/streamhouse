"use client";

import { useState, useEffect } from "react";
import { use } from "react";
import Link from "next/link";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Separator } from "@/components/ui/separator";
import { Zap, ArrowLeft, Loader2, AlertCircle, CheckCircle, Server, Activity, Clock, Database, Eye } from "lucide-react";
import { apiClient } from "@/lib/api/client";
import type { Agent, Partition } from "@/lib/api/client";

export default function AgentDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const resolvedParams = use(params);
  const agentId = decodeURIComponent(resolvedParams.id);

  const [agent, setAgent] = useState<Agent | null>(null);
  const [partitions, setPartitions] = useState<Partition[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchAgentDetails = async () => {
      try {
        setLoading(true);

        // Fetch agent details
        const agentData = await apiClient.getAgent(agentId);
        setAgent(agentData);

        // Fetch all topics to get partitions managed by this agent
        const topics = await apiClient.listTopics();
        const allPartitions: Partition[] = [];

        for (const topic of topics) {
          const topicPartitions = await apiClient.listPartitions(topic.name);
          const agentPartitions = topicPartitions.filter(
            p => p.leader_agent_id === agentId
          );
          allPartitions.push(...agentPartitions);
        }

        setPartitions(allPartitions);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to fetch agent details');
        console.error('Error fetching agent details:', err);
      } finally {
        setLoading(false);
      }
    };

    fetchAgentDetails();
    // Refresh every 10 seconds
    const interval = setInterval(fetchAgentDetails, 10000);
    return () => clearInterval(interval);
  }, [agentId]);

  const formatTimeAgo = (timestamp: number): string => {
    const now = Date.now();
    const secondsAgo = Math.floor((now - timestamp) / 1000);

    if (secondsAgo < 60) return `${secondsAgo}s ago`;
    if (secondsAgo < 3600) return `${Math.floor(secondsAgo / 60)}m ago`;
    if (secondsAgo < 86400) return `${Math.floor(secondsAgo / 3600)}h ago`;
    return `${Math.floor(secondsAgo / 86400)}d ago`;
  };

  const formatUptime = (timestamp: number): string => {
    const now = Date.now();
    const seconds = Math.floor((now - timestamp) / 1000);

    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);

    if (days > 0) return `${days}d ${hours}h ${minutes}m`;
    if (hours > 0) return `${hours}h ${minutes}m`;
    return `${minutes}m`;
  };

  const isAgentHealthy = (lastHeartbeat: number): boolean => {
    const now = Date.now();
    const secondsAgo = Math.floor((now - lastHeartbeat) / 1000);
    return secondsAgo < 30;
  };

  if (loading && !agent) {
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
                  <Button variant="ghost">Topics</Button>
                </Link>
                <Link href="/schemas">
                  <Button variant="ghost">Schemas</Button>
                </Link>
                <Link href="/agents">
                  <Button variant="default">Agents</Button>
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
            <span>Loading agent details...</span>
          </div>
        </main>
      </div>
    );
  }

  if (error || !agent) {
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
                  <Button variant="ghost">Topics</Button>
                </Link>
                <Link href="/schemas">
                  <Button variant="ghost">Schemas</Button>
                </Link>
                <Link href="/agents">
                  <Button variant="default">Agents</Button>
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
              <p>{error || 'Agent not found'}</p>
              <Link href="/agents">
                <Button className="mt-4">
                  <ArrowLeft className="mr-2 h-4 w-4" />
                  Back to Agents
                </Button>
              </Link>
            </CardContent>
          </Card>
        </main>
      </div>
    );
  }

  const healthy = isAgentHealthy(agent.last_heartbeat);
  const totalMessages = partitions.reduce((sum, p) => sum + p.high_watermark, 0);

  // Group partitions by topic
  const partitionsByTopic = partitions.reduce((acc, partition) => {
    if (!acc[partition.topic]) {
      acc[partition.topic] = [];
    }
    acc[partition.topic].push(partition);
    return acc;
  }, {} as Record<string, Partition[]>);

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
                <Button variant="ghost">Topics</Button>
              </Link>
              <Link href="/agents">
                <Button variant="default">Agents</Button>
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
          <Link href="/agents">
            <Button variant="ghost" size="sm">
              <ArrowLeft className="mr-2 h-4 w-4" />
              Back to Agents
            </Button>
          </Link>
        </div>

        {/* Agent Header */}
        <div className="mb-6">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-3xl font-bold">{agent.agent_id}</h2>
              <p className="text-muted-foreground mt-1">
                Agent Details and Partition Assignments
              </p>
            </div>
            <Badge
              variant={healthy ? "default" : "secondary"}
              className={healthy ? "bg-green-500" : "bg-yellow-500"}
            >
              {healthy ? (
                <CheckCircle className="mr-1 h-3 w-3" />
              ) : (
                <AlertCircle className="mr-1 h-3 w-3" />
              )}
              {healthy ? 'Healthy' : 'Stale'}
            </Badge>
          </div>
        </div>

        {/* Stats Cards */}
        <div className="grid grid-cols-1 md:grid-cols-4 gap-6 mb-8">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Active Leases</CardTitle>
              <Activity className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{agent.active_leases}</div>
              <p className="text-xs text-muted-foreground">
                Partitions managed
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
              <CardTitle className="text-sm font-medium">Uptime</CardTitle>
              <Clock className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{formatUptime(agent.started_at)}</div>
              <p className="text-xs text-muted-foreground">
                Since {new Date(agent.started_at).toLocaleDateString()}
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Last Heartbeat</CardTitle>
              <Clock className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{formatTimeAgo(agent.last_heartbeat)}</div>
              <p className="text-xs text-muted-foreground">
                {new Date(agent.last_heartbeat).toLocaleTimeString()}
              </p>
            </CardContent>
          </Card>
        </div>

        {/* Agent Details Card */}
        <Card className="mb-8">
          <CardHeader>
            <CardTitle>Agent Information</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <p className="text-sm text-muted-foreground">Address</p>
                <p className="font-mono text-sm">{agent.address}</p>
              </div>
              <div>
                <p className="text-sm text-muted-foreground">Availability Zone</p>
                <p className="text-sm">{agent.availability_zone || '-'}</p>
              </div>
              <div>
                <p className="text-sm text-muted-foreground">Agent Group</p>
                <p className="text-sm">{agent.agent_group || '-'}</p>
              </div>
              <div>
                <p className="text-sm text-muted-foreground">Topics Managed</p>
                <p className="text-sm">{Object.keys(partitionsByTopic).length}</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Separator className="my-8" />

        {/* Partitions by Topic */}
        <div className="space-y-6">
          <h3 className="text-2xl font-bold">Managed Partitions</h3>

          {Object.keys(partitionsByTopic).length === 0 ? (
            <Card>
              <CardContent className="py-12">
                <div className="text-center text-muted-foreground">
                  <Server className="h-12 w-12 mx-auto mb-4 opacity-50" />
                  <p className="text-lg font-medium mb-2">No Partitions Assigned</p>
                  <p className="text-sm">
                    This agent is not currently managing any partitions
                  </p>
                </div>
              </CardContent>
            </Card>
          ) : (
            Object.entries(partitionsByTopic).map(([topic, topicPartitions]) => (
              <Card key={topic}>
                <CardHeader>
                  <CardTitle className="flex items-center justify-between">
                    <span>{topic}</span>
                    <Badge variant="outline">{topicPartitions.length} partitions</Badge>
                  </CardTitle>
                  <CardDescription>
                    Partitions from this topic managed by {agent.agent_id}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>Partition ID</TableHead>
                        <TableHead>High Watermark</TableHead>
                        <TableHead>Low Watermark</TableHead>
                        <TableHead>Messages</TableHead>
                        <TableHead>Actions</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {topicPartitions.map((partition) => (
                        <TableRow key={partition.partition_id}>
                          <TableCell className="font-medium">
                            <Badge variant="outline">{partition.partition_id}</Badge>
                          </TableCell>
                          <TableCell>{partition.high_watermark.toLocaleString()}</TableCell>
                          <TableCell>{partition.low_watermark.toLocaleString()}</TableCell>
                          <TableCell>
                            {(partition.high_watermark - partition.low_watermark).toLocaleString()}
                          </TableCell>
                          <TableCell>
                            <Link href={`/topics/${topic}/partitions/${partition.partition_id}/messages`}>
                              <Button variant="outline" size="sm">
                                <Eye className="mr-2 h-4 w-4" />
                                View Messages
                              </Button>
                            </Link>
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </CardContent>
              </Card>
            ))
          )}
        </div>
      </main>
    </div>
  );
}
