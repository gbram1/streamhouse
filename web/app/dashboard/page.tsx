"use client";

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Activity, Database, Zap, AlertCircle, CheckCircle, TrendingUp, Server } from "lucide-react";
import Link from "next/link";

export default function Dashboard() {
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
              <div className="text-2xl font-bold">12</div>
              <p className="text-xs text-muted-foreground">
                +2 from last week
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Active Agents</CardTitle>
              <Server className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">3</div>
              <p className="text-xs text-muted-foreground">
                All healthy
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Throughput</CardTitle>
              <TrendingUp className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">45.2K/s</div>
              <p className="text-xs text-muted-foreground">
                Messages per second
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Storage</CardTitle>
              <Activity className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">128 GB</div>
              <p className="text-xs text-muted-foreground">
                Used in S3
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
                    <TableRow>
                      <TableCell className="font-medium">orders</TableCell>
                      <TableCell>6</TableCell>
                      <TableCell>12.5K</TableCell>
                      <TableCell>0</TableCell>
                      <TableCell>
                        <Badge variant="default" className="bg-green-500">
                          <CheckCircle className="mr-1 h-3 w-3" />
                          Healthy
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Link href="/topics/orders">
                          <Button variant="ghost" size="sm">View</Button>
                        </Link>
                      </TableCell>
                    </TableRow>
                    <TableRow>
                      <TableCell className="font-medium">users</TableCell>
                      <TableCell>3</TableCell>
                      <TableCell>8.2K</TableCell>
                      <TableCell>145</TableCell>
                      <TableCell>
                        <Badge variant="secondary">
                          <AlertCircle className="mr-1 h-3 w-3" />
                          Warning
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Link href="/topics/users">
                          <Button variant="ghost" size="sm">View</Button>
                        </Link>
                      </TableCell>
                    </TableRow>
                    <TableRow>
                      <TableCell className="font-medium">events</TableCell>
                      <TableCell>9</TableCell>
                      <TableCell>24.5K</TableCell>
                      <TableCell>0</TableCell>
                      <TableCell>
                        <Badge variant="default" className="bg-green-500">
                          <CheckCircle className="mr-1 h-3 w-3" />
                          Healthy
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Link href="/topics/events">
                          <Button variant="ghost" size="sm">View</Button>
                        </Link>
                      </TableCell>
                    </TableRow>
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
                    <TableRow>
                      <TableCell className="font-medium">agent-001</TableCell>
                      <TableCell>10.0.1.15:9090</TableCell>
                      <TableCell>us-east-1a</TableCell>
                      <TableCell>6</TableCell>
                      <TableCell>2s ago</TableCell>
                      <TableCell>
                        <Badge variant="default" className="bg-green-500">
                          <CheckCircle className="mr-1 h-3 w-3" />
                          Healthy
                        </Badge>
                      </TableCell>
                    </TableRow>
                    <TableRow>
                      <TableCell className="font-medium">agent-002</TableCell>
                      <TableCell>10.0.1.16:9090</TableCell>
                      <TableCell>us-east-1b</TableCell>
                      <TableCell>6</TableCell>
                      <TableCell>3s ago</TableCell>
                      <TableCell>
                        <Badge variant="default" className="bg-green-500">
                          <CheckCircle className="mr-1 h-3 w-3" />
                          Healthy
                        </Badge>
                      </TableCell>
                    </TableRow>
                    <TableRow>
                      <TableCell className="font-medium">agent-003</TableCell>
                      <TableCell>10.0.1.17:9090</TableCell>
                      <TableCell>us-east-1c</TableCell>
                      <TableCell>6</TableCell>
                      <TableCell>1s ago</TableCell>
                      <TableCell>
                        <Badge variant="default" className="bg-green-500">
                          <CheckCircle className="mr-1 h-3 w-3" />
                          Healthy
                        </Badge>
                      </TableCell>
                    </TableRow>
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
                      <TableCell className="font-medium">order-processor</TableCell>
                      <TableCell>orders</TableCell>
                      <TableCell>3</TableCell>
                      <TableCell>0</TableCell>
                      <TableCell>
                        <Badge variant="default" className="bg-green-500">
                          <CheckCircle className="mr-1 h-3 w-3" />
                          Current
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Button variant="ghost" size="sm">View</Button>
                      </TableCell>
                    </TableRow>
                    <TableRow>
                      <TableCell className="font-medium">analytics</TableCell>
                      <TableCell>events</TableCell>
                      <TableCell>5</TableCell>
                      <TableCell>1,245</TableCell>
                      <TableCell>
                        <Badge variant="secondary">
                          <AlertCircle className="mr-1 h-3 w-3" />
                          Lagging
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Button variant="ghost" size="sm">View</Button>
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
