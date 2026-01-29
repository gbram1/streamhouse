"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Zap, Loader2, AlertCircle, Users, TrendingUp, Database } from "lucide-react";
import { apiClient } from "@/lib/api/client";
import type { ConsumerGroupInfo } from "@/lib/api/client";

export default function ConsumerGroupsPage() {
  const [groups, setGroups] = useState<ConsumerGroupInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchGroups = async () => {
      try {
        setLoading(true);
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
    // Refresh every 10 seconds
    const interval = setInterval(fetchGroups, 10000);
    return () => clearInterval(interval);
  }, []);

  if (loading && groups.length === 0) {
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
                <Link href="/agents">
                  <Button variant="ghost">Agents</Button>
                </Link>
                <Link href="/consumer-groups">
                  <Button variant="default">Consumer Groups</Button>
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
            <span>Loading consumer groups...</span>
          </div>
        </main>
      </div>
    );
  }

  if (error) {
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
                <Link href="/agents">
                  <Button variant="ghost">Agents</Button>
                </Link>
                <Link href="/consumer-groups">
                  <Button variant="default">Consumer Groups</Button>
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
              <p>{error}</p>
              <Button onClick={() => window.location.reload()} className="mt-4">
                Retry
              </Button>
            </CardContent>
          </Card>
        </main>
      </div>
    );
  }

  const totalLag = groups.reduce((sum, g) => sum + g.totalLag, 0);
  const totalPartitions = groups.reduce((sum, g) => sum + g.partitionCount, 0);

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
                <Button variant="ghost">Agents</Button>
              </Link>
              <Link href="/consumer-groups">
                <Button variant="default">Consumer Groups</Button>
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
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Consumer Groups</CardTitle>
              <Users className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{groups.length}</div>
              <p className="text-xs text-muted-foreground">
                Active consumer groups
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Total Lag</CardTitle>
              <TrendingUp className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{totalLag.toLocaleString()}</div>
              <p className="text-xs text-muted-foreground">
                Messages behind
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Total Partitions</CardTitle>
              <Database className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{totalPartitions}</div>
              <p className="text-xs text-muted-foreground">
                Being consumed
              </p>
            </CardContent>
          </Card>
        </div>

        {/* Consumer Groups Table */}
        <Card>
          <CardHeader>
            <CardTitle>Consumer Groups</CardTitle>
            <CardDescription>
              Monitor consumer group lag and consumption progress
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Group ID</TableHead>
                  <TableHead>Topics</TableHead>
                  <TableHead>Partitions</TableHead>
                  <TableHead>Total Lag</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {groups.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={5} className="text-center text-muted-foreground">
                      <div className="py-8">
                        <Users className="h-12 w-12 mx-auto mb-4 opacity-50" />
                        <p>No consumer groups found</p>
                        <p className="text-xs mt-2">Consumer groups will appear here when they start consuming</p>
                      </div>
                    </TableCell>
                  </TableRow>
                ) : (
                  groups.map((group) => {
                    const lagStatus = group.totalLag === 0 ? 'caught-up' : group.totalLag > 1000 ? 'lagging' : 'normal';

                    return (
                      <TableRow key={group.groupId}>
                        <TableCell className="font-medium">
                          <code className="text-xs bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded">
                            {group.groupId}
                          </code>
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-wrap gap-1">
                            {group.topics.map((topic) => (
                              <Badge key={topic} variant="outline" className="text-xs">
                                {topic}
                              </Badge>
                            ))}
                          </div>
                        </TableCell>
                        <TableCell>
                          <Badge variant="outline">{group.partitionCount}</Badge>
                        </TableCell>
                        <TableCell>
                          <span className={`font-mono text-sm ${
                            group.totalLag > 1000 ? 'text-red-600 font-bold' : ''
                          }`}>
                            {group.totalLag.toLocaleString()}
                          </span>
                        </TableCell>
                        <TableCell>
                          <Badge
                            variant={lagStatus === 'caught-up' ? 'default' : 'secondary'}
                            className={
                              lagStatus === 'caught-up' ? 'bg-green-500' :
                              lagStatus === 'lagging' ? 'bg-red-500' : 'bg-yellow-500'
                            }
                          >
                            {lagStatus === 'caught-up' ? 'Caught Up' :
                             lagStatus === 'lagging' ? 'Lagging' : 'Normal'}
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
      </main>
    </div>
  );
}
