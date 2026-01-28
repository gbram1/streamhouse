"use client";

import { useState, useEffect } from "react";
import { use } from "react";
import Link from "next/link";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Input } from "@/components/ui/input";
import { Zap, ArrowLeft, Loader2, AlertCircle, ChevronLeft, ChevronRight, RefreshCw } from "lucide-react";
import { apiClient } from "@/lib/api/client";
import type { ConsumedRecord } from "@/lib/api/client";

export default function MessagesPage({
  params
}: {
  params: Promise<{ name: string; partition: string }>
}) {
  const resolvedParams = use(params);
  const topicName = decodeURIComponent(resolvedParams.name);
  const partitionId = parseInt(resolvedParams.partition);

  const [messages, setMessages] = useState<ConsumedRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [currentOffset, setCurrentOffset] = useState(0);
  const [nextOffset, setNextOffset] = useState(0);
  const [pageSize, setPageSize] = useState(20);
  const [refreshing, setRefreshing] = useState(false);

  const fetchMessages = async (offset: number = currentOffset) => {
    try {
      setLoading(true);
      setError(null);
      const response = await apiClient.consume(topicName, partitionId, offset, pageSize);
      setMessages(response.records);
      setNextOffset(response.nextOffset);
      setCurrentOffset(offset);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to fetch messages');
      console.error('Error fetching messages:', err);
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  useEffect(() => {
    fetchMessages(0);
  }, [topicName, partitionId, pageSize]);

  const handleRefresh = () => {
    setRefreshing(true);
    fetchMessages(currentOffset);
  };

  const handlePrevious = () => {
    const newOffset = Math.max(0, currentOffset - pageSize);
    fetchMessages(newOffset);
  };

  const handleNext = () => {
    if (messages.length === pageSize) {
      fetchMessages(nextOffset);
    }
  };

  const formatTimestamp = (timestamp: number) => {
    return new Date(timestamp).toLocaleString();
  };

  const truncateValue = (value: string, maxLength: number = 100) => {
    if (value.length <= maxLength) return value;
    return value.substring(0, maxLength) + '...';
  };

  if (loading && messages.length === 0) {
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
            <span>Loading messages...</span>
          </div>
        </main>
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
        {/* Breadcrumb */}
        <div className="mb-6">
          <Link href={`/topics/${topicName}`}>
            <Button variant="ghost" size="sm">
              <ArrowLeft className="mr-2 h-4 w-4" />
              Back to {topicName}
            </Button>
          </Link>
        </div>

        {/* Page Header */}
        <div className="mb-6">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-3xl font-bold">Messages</h2>
              <p className="text-muted-foreground mt-1">
                Topic: <span className="font-medium">{topicName}</span> | Partition:{" "}
                <span className="font-medium">{partitionId}</span>
              </p>
            </div>
            <div className="flex items-center space-x-2">
              <Button
                variant="outline"
                size="sm"
                onClick={handleRefresh}
                disabled={refreshing}
              >
                <RefreshCw className={`mr-2 h-4 w-4 ${refreshing ? 'animate-spin' : ''}`} />
                Refresh
              </Button>
            </div>
          </div>
        </div>

        {/* Error State */}
        {error && (
          <Card className="mb-6">
            <CardHeader>
              <CardTitle className="flex items-center space-x-2 text-red-600">
                <AlertCircle className="h-5 w-5" />
                <span>Error</span>
              </CardTitle>
            </CardHeader>
            <CardContent>
              <p>{error}</p>
            </CardContent>
          </Card>
        )}

        {/* Messages Table */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle>Messages</CardTitle>
                <CardDescription>
                  Showing {messages.length} message{messages.length !== 1 ? 's' : ''} starting from offset {currentOffset}
                </CardDescription>
              </div>
              <div className="flex items-center space-x-2">
                <label htmlFor="pageSize" className="text-sm text-muted-foreground">
                  Per page:
                </label>
                <Input
                  id="pageSize"
                  type="number"
                  min="1"
                  max="1000"
                  value={pageSize}
                  onChange={(e) => setPageSize(parseInt(e.target.value) || 20)}
                  className="w-20"
                />
              </div>
            </div>
          </CardHeader>
          <CardContent>
            {messages.length === 0 ? (
              <div className="text-center py-12 text-muted-foreground">
                <AlertCircle className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <p className="text-lg font-medium mb-2">No messages found</p>
                <p className="text-sm">
                  This partition has no messages at offset {currentOffset}
                </p>
              </div>
            ) : (
              <>
                <div className="overflow-x-auto">
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead className="w-[100px]">Offset</TableHead>
                        <TableHead className="w-[150px]">Key</TableHead>
                        <TableHead>Value</TableHead>
                        <TableHead className="w-[200px]">Timestamp</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {messages.map((message) => (
                        <TableRow key={message.offset}>
                          <TableCell className="font-mono text-sm">
                            <Badge variant="outline">{message.offset}</Badge>
                          </TableCell>
                          <TableCell className="font-mono text-xs">
                            {message.key ? (
                              <code className="bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded">
                                {truncateValue(message.key, 30)}
                              </code>
                            ) : (
                              <span className="text-muted-foreground italic">null</span>
                            )}
                          </TableCell>
                          <TableCell className="font-mono text-xs max-w-md">
                            <code className="bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded block overflow-x-auto whitespace-pre-wrap break-all">
                              {truncateValue(message.value, 200)}
                            </code>
                          </TableCell>
                          <TableCell className="text-sm text-muted-foreground">
                            {formatTimestamp(message.timestamp)}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </div>

                {/* Pagination Controls */}
                <div className="flex items-center justify-between mt-4 pt-4 border-t">
                  <div className="text-sm text-muted-foreground">
                    Offset: {currentOffset} - {nextOffset - 1}
                  </div>
                  <div className="flex items-center space-x-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handlePrevious}
                      disabled={currentOffset === 0 || loading}
                    >
                      <ChevronLeft className="h-4 w-4 mr-1" />
                      Previous
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handleNext}
                      disabled={messages.length < pageSize || loading}
                    >
                      Next
                      <ChevronRight className="h-4 w-4 ml-1" />
                    </Button>
                  </div>
                </div>
              </>
            )}
          </CardContent>
        </Card>
      </main>
    </div>
  );
}
