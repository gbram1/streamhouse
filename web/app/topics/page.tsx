"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Zap, Plus, Search, Trash2, Eye, Loader2, AlertCircle, CheckCircle } from "lucide-react";
import { apiClient } from "@/lib/api/client";
import type { Topic } from "@/lib/api/client";

export default function TopicsPage() {
  const [topics, setTopics] = useState<Topic[]>([]);
  const [filteredTopics, setFilteredTopics] = useState<Topic[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  // Create dialog state
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createLoading, setCreateLoading] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [newTopicName, setNewTopicName] = useState("");
  const [newTopicPartitions, setNewTopicPartitions] = useState("3");

  // Delete dialog state
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleteLoading, setDeleteLoading] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [topicToDelete, setTopicToDelete] = useState<Topic | null>(null);

  // Fetch topics
  useEffect(() => {
    const fetchTopics = async () => {
      try {
        setLoading(true);
        const data = await apiClient.listTopics();
        setTopics(data);
        setFilteredTopics(data);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to fetch topics');
        console.error('Error fetching topics:', err);
      } finally {
        setLoading(false);
      }
    };

    fetchTopics();
    // Refresh every 10 seconds
    const interval = setInterval(fetchTopics, 10000);
    return () => clearInterval(interval);
  }, []);

  // Filter topics based on search
  useEffect(() => {
    if (searchQuery.trim() === "") {
      setFilteredTopics(topics);
    } else {
      const filtered = topics.filter((topic) =>
        topic.name.toLowerCase().includes(searchQuery.toLowerCase())
      );
      setFilteredTopics(filtered);
    }
  }, [searchQuery, topics]);

  // Handle create topic
  const handleCreateTopic = async () => {
    if (!newTopicName.trim()) {
      setCreateError("Topic name is required");
      return;
    }

    const partitions = parseInt(newTopicPartitions);
    if (isNaN(partitions) || partitions < 1 || partitions > 100) {
      setCreateError("Partitions must be between 1 and 100");
      return;
    }

    try {
      setCreateLoading(true);
      setCreateError(null);
      await apiClient.createTopic(newTopicName, partitions);

      // Refresh topics list
      const data = await apiClient.listTopics();
      setTopics(data);

      // Close dialog and reset form
      setCreateDialogOpen(false);
      setNewTopicName("");
      setNewTopicPartitions("3");
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : 'Failed to create topic');
    } finally {
      setCreateLoading(false);
    }
  };

  // Handle delete topic
  const handleDeleteTopic = async () => {
    if (!topicToDelete) return;

    try {
      setDeleteLoading(true);
      setDeleteError(null);
      await apiClient.deleteTopic(topicToDelete.name);

      // Refresh topics list
      const data = await apiClient.listTopics();
      setTopics(data);

      // Close dialog
      setDeleteDialogOpen(false);
      setTopicToDelete(null);
    } catch (err) {
      setDeleteError(err instanceof Error ? err.message : 'Failed to delete topic');
    } finally {
      setDeleteLoading(false);
    }
  };

  if (loading && topics.length === 0) {
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
            <span>Loading topics...</span>
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
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle>Topics</CardTitle>
                <CardDescription>
                  Manage your event streams and partitions
                </CardDescription>
              </div>
              <Button onClick={() => setCreateDialogOpen(true)}>
                <Plus className="mr-2 h-4 w-4" />
                Create Topic
              </Button>
            </div>
          </CardHeader>
          <CardContent>
            {/* Search */}
            <div className="mb-6">
              <div className="relative">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-gray-400" />
                <Input
                  placeholder="Search topics..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-10"
                />
              </div>
            </div>

            {/* Topics Table */}
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Partitions</TableHead>
                  <TableHead>Replication Factor</TableHead>
                  <TableHead>Created At</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredTopics.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={6} className="text-center text-muted-foreground">
                      {searchQuery ? "No topics match your search" : "No topics found. Create your first topic to get started."}
                    </TableCell>
                  </TableRow>
                ) : (
                  filteredTopics.map((topic) => (
                    <TableRow key={topic.name}>
                      <TableCell className="font-medium">{topic.name}</TableCell>
                      <TableCell>{topic.partitions}</TableCell>
                      <TableCell>{topic.replication_factor}</TableCell>
                      <TableCell>{new Date(topic.created_at).toLocaleString()}</TableCell>
                      <TableCell>
                        <Badge variant="default" className="bg-green-500">
                          <CheckCircle className="mr-1 h-3 w-3" />
                          Active
                        </Badge>
                      </TableCell>
                      <TableCell className="text-right">
                        <div className="flex justify-end space-x-2">
                          <Link href={`/topics/${topic.name}`}>
                            <Button variant="ghost" size="sm">
                              <Eye className="h-4 w-4" />
                            </Button>
                          </Link>
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => {
                              setTopicToDelete(topic);
                              setDeleteDialogOpen(true);
                            }}
                          >
                            <Trash2 className="h-4 w-4 text-red-500" />
                          </Button>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </main>

      {/* Create Topic Dialog */}
      <Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create Topic</DialogTitle>
            <DialogDescription>
              Create a new topic with the specified number of partitions
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="topic-name">Topic Name</Label>
              <Input
                id="topic-name"
                placeholder="orders, events, logs..."
                value={newTopicName}
                onChange={(e) => setNewTopicName(e.target.value)}
                disabled={createLoading}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="partitions">Number of Partitions</Label>
              <Input
                id="partitions"
                type="number"
                min="1"
                max="100"
                value={newTopicPartitions}
                onChange={(e) => setNewTopicPartitions(e.target.value)}
                disabled={createLoading}
              />
              <p className="text-xs text-muted-foreground">
                More partitions = higher throughput, but more resources
              </p>
            </div>
            {createError && (
              <div className="flex items-center space-x-2 text-red-600 text-sm">
                <AlertCircle className="h-4 w-4" />
                <span>{createError}</span>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setCreateDialogOpen(false);
                setCreateError(null);
                setNewTopicName("");
                setNewTopicPartitions("3");
              }}
              disabled={createLoading}
            >
              Cancel
            </Button>
            <Button onClick={handleCreateTopic} disabled={createLoading}>
              {createLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Create Topic
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete Topic Dialog */}
      <Dialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Topic</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete this topic? This action cannot be undone.
            </DialogDescription>
          </DialogHeader>
          {topicToDelete && (
            <div className="py-4">
              <div className="bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800 rounded-lg p-4">
                <div className="flex items-start space-x-3">
                  <AlertCircle className="h-5 w-5 text-red-600 mt-0.5" />
                  <div className="flex-1">
                    <p className="font-medium text-red-900 dark:text-red-100">
                      Topic: {topicToDelete.name}
                    </p>
                    <p className="text-sm text-red-800 dark:text-red-200 mt-1">
                      This will delete {topicToDelete.partitions} partitions and all associated data.
                    </p>
                  </div>
                </div>
              </div>
              {deleteError && (
                <div className="flex items-center space-x-2 text-red-600 text-sm mt-4">
                  <AlertCircle className="h-4 w-4" />
                  <span>{deleteError}</span>
                </div>
              )}
            </div>
          )}
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setDeleteDialogOpen(false);
                setDeleteError(null);
                setTopicToDelete(null);
              }}
              disabled={deleteLoading}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDeleteTopic}
              disabled={deleteLoading}
            >
              {deleteLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Delete Topic
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
