"use client";

import { useState, useEffect } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Activity, Database, Zap, AlertCircle, CheckCircle, TrendingUp, Server, Loader2 } from "lucide-react";
import Link from "next/link";
import { apiClient } from "@/lib/api/client";
import type { Topic, Agent, MetricsSnapshot } from "@/lib/api/client";

export default function Dashboard() {
  const [metrics, setMetrics] = useState<MetricsSnapshot | null>(null);
  const [topics, setTopics] = useState<Topic[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchData = async () => {
      try {
        setLoading(true);
        const [metricsData, topicsData, agentsData] = await Promise.all([
          apiClient.getMetrics(),
          apiClient.listTopics(),
          apiClient.listAgents(),
        ]);
        setMetrics(metricsData);
        setTopics(topicsData);
        setAgents(agentsData);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to fetch data');
        console.error('Error fetching dashboard data:', err);
      } finally {
        setLoading(false);
      }
    };

    fetchData();
    // Refresh every 5 seconds
    const interval = setInterval(fetchData, 5000);
    return () => clearInterval(interval);
  }, []);

  if (loading && !metrics) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-gray-950 flex items-center justify-center">
        <div className="flex items-center space-x-2">
          <Loader2 className="h-6 w-6 animate-spin" />
          <span>Loading dashboard...</span>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-gray-950 flex items-center justify-center">
        <Card className="w-96">
          <CardHeader>
            <CardTitle className="flex items-center space-x-2 text-red-600">
              <AlertCircle className="h-5 w-5" />
              <span>Error</span>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p>{error}</p>
            <Button onClick={() => window.location.reload()} className="mt-4">
              Retry
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }
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
                <Button variant="default">Dashboard</Button>
              </Link>
              <Link href="/topics">
                <Button variant="ghost">Topics</Button>
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
        {/* Stats Overview */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Active Topics</CardTitle>
              <Database className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{metrics?.topics_count ?? 0}</div>
              <p className="text-xs text-muted-foreground">
                {topics.length} topics configured
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Active Agents</CardTitle>
              <Server className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{metrics?.agents_count ?? 0}</div>
              <p className="text-xs text-muted-foreground">
                {agents.length > 0 ? 'All healthy' : 'No agents running'}
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Total Messages</CardTitle>
              <TrendingUp className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{metrics?.total_messages.toLocaleString() ?? 0}</div>
              <p className="text-xs text-muted-foreground">
                Across all partitions
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Partitions</CardTitle>
              <Activity className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{metrics?.partitions_count ?? 0}</div>
              <p className="text-xs text-muted-foreground">
                Total partition count
              </p>
            </CardContent>
          </Card>
        </div>

        {/* Tabs */}
        <Tabs defaultValue="topics" className="space-y-4">
          <TabsList>
            <TabsTrigger value="topics">Topics</TabsTrigger>
            <TabsTrigger value="agents">Agents</TabsTrigger>
            <TabsTrigger value="consumers">Consumer Groups</TabsTrigger>
          </TabsList>

          {/* Topics Tab */}
          <TabsContent value="topics" className="space-y-4">
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <div>
                    <CardTitle>Topics</CardTitle>
                    <CardDescription>
                      Manage your event streams
                    </CardDescription>
                  </div>
                  <Link href="/topics/create">
                    <Button>Create Topic</Button>
                  </Link>
                </div>
              </CardHeader>
              <CardContent>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Name</TableHead>
                      <TableHead>Partitions</TableHead>
                      <TableHead>Messages/sec</TableHead>
                      <TableHead>Lag</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {topics.length === 0 ? (
                      <TableRow>
                        <TableCell colSpan={6} className="text-center text-muted-foreground">
                          No topics found. Create your first topic to get started.
                        </TableCell>
                      </TableRow>
                    ) : (
                      topics.map((topic) => (
                        <TableRow key={topic.name}>
                          <TableCell className="font-medium">{topic.name}</TableCell>
                          <TableCell>{topic.partitions}</TableCell>
                          <TableCell>-</TableCell>
                          <TableCell>-</TableCell>
                          <TableCell>
                            <Badge variant="default" className="bg-green-500">
                              <CheckCircle className="mr-1 h-3 w-3" />
                              Active
                            </Badge>
                          </TableCell>
                          <TableCell>
                            <Link href={`/topics/${topic.name}`}>
                              <Button variant="ghost" size="sm">View</Button>
                            </Link>
                          </TableCell>
                        </TableRow>
                      ))
                    )}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Agents Tab */}
          <TabsContent value="agents" className="space-y-4">
            <Card>
              <CardHeader>
                <CardTitle>Agents</CardTitle>
                <CardDescription>
                  Stateless agents handling partitions
                </CardDescription>
              </CardHeader>
              <CardContent>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Agent ID</TableHead>
                      <TableHead>Address</TableHead>
                      <TableHead>Zone</TableHead>
                      <TableHead>Active Leases</TableHead>
                      <TableHead>Last Heartbeat</TableHead>
                      <TableHead>Status</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {agents.length === 0 ? (
                      <TableRow>
                        <TableCell colSpan={6} className="text-center text-muted-foreground">
                          No agents found. Start an agent to begin processing partitions.
                        </TableCell>
                      </TableRow>
                    ) : (
                      agents.map((agent) => {
                        const lastHeartbeat = new Date(agent.last_heartbeat);
                        const now = new Date();
                        const secondsAgo = Math.floor((now.getTime() - lastHeartbeat.getTime()) / 1000);
                        const timeAgo = secondsAgo < 60 ? `${secondsAgo}s ago` : `${Math.floor(secondsAgo / 60)}m ago`;
                        const isHealthy = secondsAgo < 30;

                        return (
                          <TableRow key={agent.agent_id}>
                            <TableCell className="font-medium">{agent.agent_id}</TableCell>
                            <TableCell>{agent.address}</TableCell>
                            <TableCell>{agent.availability_zone || '-'}</TableCell>
                            <TableCell>{agent.active_leases}</TableCell>
                            <TableCell>{timeAgo}</TableCell>
                            <TableCell>
                              <Badge variant={isHealthy ? "default" : "secondary"} className={isHealthy ? "bg-green-500" : ""}>
                                {isHealthy ? <CheckCircle className="mr-1 h-3 w-3" /> : <AlertCircle className="mr-1 h-3 w-3" />}
                                {isHealthy ? 'Healthy' : 'Stale'}
                              </Badge>
                            </TableCell>
                          </TableRow>
                        );
                      })
                    )}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Consumer Groups Tab */}
          <TabsContent value="consumers" className="space-y-4">
            <Card>
              <CardHeader>
                <CardTitle>Consumer Groups</CardTitle>
                <CardDescription>
                  Monitor consumer lag and offsets
                </CardDescription>
              </CardHeader>
              <CardContent>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Group ID</TableHead>
                      <TableHead>Topic</TableHead>
                      <TableHead>Members</TableHead>
                      <TableHead>Total Lag</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    <TableRow>
                      <TableCell colSpan={6} className="text-center text-muted-foreground">
                        Consumer groups API endpoint not yet implemented.
                      </TableCell>
                    </TableRow>
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      </main>
    </div>
  );
}
