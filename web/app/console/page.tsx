"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Zap, Send, Loader2, AlertCircle, CheckCircle, Clock } from "lucide-react";
import { apiClient } from "@/lib/api/client";
import type { Topic } from "@/lib/api/client";

interface SentMessage {
  id: string;
  topic: string;
  key: string;
  value: string;
  partition: number;
  offset: number;
  timestamp: Date;
  status: 'success' | 'error';
  error?: string;
}

export default function ConsolePage() {
  const [topics, setTopics] = useState<Topic[]>([]);
  const [loadingTopics, setLoadingTopics] = useState(true);
  const [topicsError, setTopicsError] = useState<string | null>(null);

  // Form state
  const [selectedTopic, setSelectedTopic] = useState<string>("");
  const [messageKey, setMessageKey] = useState("");
  const [messageValue, setMessageValue] = useState("");
  const [partition, setPartition] = useState<string>("auto");
  const [sending, setSending] = useState(false);

  // Sent messages log
  const [sentMessages, setSentMessages] = useState<SentMessage[]>([]);

  // Fetch topics
  useEffect(() => {
    const fetchTopics = async () => {
      try {
        setLoadingTopics(true);
        const data = await apiClient.listTopics();
        setTopics(data);
        setTopicsError(null);
        if (data.length > 0 && !selectedTopic) {
          setSelectedTopic(data[0].name);
        }
      } catch (err) {
        setTopicsError(err instanceof Error ? err.message : 'Failed to fetch topics');
        console.error('Error fetching topics:', err);
      } finally {
        setLoadingTopics(false);
      }
    };

    fetchTopics();
  }, [selectedTopic]);

  // Get partition count for selected topic
  const selectedTopicData = topics.find(t => t.name === selectedTopic);
  const maxPartition = selectedTopicData ? selectedTopicData.partitions - 1 : 0;

  // Handle send message
  const handleSendMessage = async () => {
    if (!selectedTopic || !messageValue) {
      return;
    }

    try {
      setSending(true);

      const partitionNum = partition === "auto" ? undefined : parseInt(partition);

      const response = await apiClient.produce({
        topic: selectedTopic,
        key: messageKey || undefined,
        value: messageValue,
        partition: partitionNum,
      });

      // Add to sent messages log
      const newMessage: SentMessage = {
        id: `${Date.now()}-${Math.random()}`,
        topic: selectedTopic,
        key: messageKey || '(none)',
        value: messageValue,
        partition: response.partition,
        offset: response.offset,
        timestamp: new Date(),
        status: 'success',
      };
      setSentMessages(prev => [newMessage, ...prev].slice(0, 50)); // Keep last 50

      // Clear form
      setMessageValue("");
      setMessageKey("");
    } catch (err) {
      const errorMessage: SentMessage = {
        id: `${Date.now()}-${Math.random()}`,
        topic: selectedTopic,
        key: messageKey || '(none)',
        value: messageValue,
        partition: -1,
        offset: -1,
        timestamp: new Date(),
        status: 'error',
        error: err instanceof Error ? err.message : 'Failed to send message',
      };
      setSentMessages(prev => [errorMessage, ...prev].slice(0, 50));
    } finally {
      setSending(false);
    }
  };

  // Handle key press in value textarea
  const handleKeyPress = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSendMessage();
    }
  };

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
              <Link href="/schemas">
                <Button variant="ghost">Schemas</Button>
              </Link>
              <Link href="/agents">
                <Button variant="ghost">Agents</Button>
              </Link>
              <Link href="/console">
                <Button variant="default">Console</Button>
              </Link>
              <Button variant="outline">Sign Out</Button>
            </nav>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="container mx-auto px-4 py-8">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Producer Form */}
          <Card>
            <CardHeader>
              <CardTitle>Produce Message</CardTitle>
              <CardDescription>
                Send messages to any topic through the web console
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              {/* Topic Selector */}
              <div className="space-y-2">
                <Label htmlFor="topic">Topic</Label>
                {loadingTopics ? (
                  <div className="flex items-center space-x-2 text-muted-foreground">
                    <Loader2 className="h-4 w-4 animate-spin" />
                    <span className="text-sm">Loading topics...</span>
                  </div>
                ) : topicsError ? (
                  <div className="flex items-center space-x-2 text-red-600 text-sm">
                    <AlertCircle className="h-4 w-4" />
                    <span>{topicsError}</span>
                  </div>
                ) : topics.length === 0 ? (
                  <div className="flex items-center space-x-2 text-muted-foreground text-sm">
                    <AlertCircle className="h-4 w-4" />
                    <span>No topics found. Create a topic first.</span>
                  </div>
                ) : (
                  <Select value={selectedTopic} onValueChange={setSelectedTopic}>
                    <SelectTrigger>
                      <SelectValue placeholder="Select a topic" />
                    </SelectTrigger>
                    <SelectContent>
                      {topics.map((topic) => (
                        <SelectItem key={topic.name} value={topic.name}>
                          {topic.name} ({topic.partitions} partitions)
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              </div>

              {/* Partition Selector */}
              <div className="space-y-2">
                <Label htmlFor="partition">Partition</Label>
                <Select value={partition} onValueChange={setPartition}>
                  <SelectTrigger>
                    <SelectValue placeholder="Select partition" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="auto">Auto (round-robin)</SelectItem>
                    {Array.from({ length: maxPartition + 1 }, (_, i) => (
                      <SelectItem key={i} value={i.toString()}>
                        Partition {i}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {/* Message Key */}
              <div className="space-y-2">
                <Label htmlFor="key">Key (optional)</Label>
                <Input
                  id="key"
                  placeholder="user123"
                  value={messageKey}
                  onChange={(e) => setMessageKey(e.target.value)}
                  disabled={sending || !selectedTopic}
                />
                <p className="text-xs text-muted-foreground">
                  Messages with the same key go to the same partition
                </p>
              </div>

              {/* Message Value */}
              <div className="space-y-2">
                <Label htmlFor="value">Value</Label>
                <textarea
                  id="value"
                  placeholder='{"order_id": 42, "amount": 99.99}'
                  value={messageValue}
                  onChange={(e) => setMessageValue(e.target.value)}
                  onKeyDown={handleKeyPress}
                  disabled={sending || !selectedTopic}
                  className="w-full min-h-[120px] px-3 py-2 text-sm border border-input bg-background rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                />
                <p className="text-xs text-muted-foreground">
                  Press Cmd/Ctrl + Enter to send
                </p>
              </div>

              <Separator />

              {/* Send Button */}
              <Button
                onClick={handleSendMessage}
                disabled={sending || !selectedTopic || !messageValue}
                className="w-full"
              >
                {sending ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Sending...
                  </>
                ) : (
                  <>
                    <Send className="mr-2 h-4 w-4" />
                    Send Message
                  </>
                )}
              </Button>
            </CardContent>
          </Card>

          {/* Recent Messages Log */}
          <Card>
            <CardHeader>
              <CardTitle>Recent Messages</CardTitle>
              <CardDescription>
                Last 50 messages sent from this console
              </CardDescription>
            </CardHeader>
            <CardContent>
              {sentMessages.length === 0 ? (
                <div className="text-center text-muted-foreground py-8">
                  <Send className="h-12 w-12 mx-auto mb-4 opacity-50" />
                  <p>No messages sent yet</p>
                  <p className="text-xs mt-2">Send a message to see it here</p>
                </div>
              ) : (
                <div className="space-y-3 max-h-[600px] overflow-y-auto">
                  {sentMessages.map((msg) => (
                    <div
                      key={msg.id}
                      className={`p-4 rounded-lg border ${
                        msg.status === 'success'
                          ? 'bg-green-50 dark:bg-green-950 border-green-200 dark:border-green-800'
                          : 'bg-red-50 dark:bg-red-950 border-red-200 dark:border-red-800'
                      }`}
                    >
                      <div className="flex items-start justify-between mb-2">
                        <div className="flex items-center space-x-2">
                          {msg.status === 'success' ? (
                            <CheckCircle className="h-4 w-4 text-green-600" />
                          ) : (
                            <AlertCircle className="h-4 w-4 text-red-600" />
                          )}
                          <span className="font-medium text-sm">
                            {msg.status === 'success' ? 'Sent' : 'Failed'}
                          </span>
                        </div>
                        <div className="flex items-center space-x-1 text-xs text-muted-foreground">
                          <Clock className="h-3 w-3" />
                          <span>{msg.timestamp.toLocaleTimeString()}</span>
                        </div>
                      </div>

                      <div className="space-y-1 text-sm">
                        <div className="flex items-center space-x-2">
                          <span className="text-muted-foreground">Topic:</span>
                          <Badge variant="outline">{msg.topic}</Badge>
                        </div>
                        {msg.status === 'success' && (
                          <>
                            <div className="flex items-center space-x-2">
                              <span className="text-muted-foreground">Partition:</span>
                              <code className="text-xs">{msg.partition}</code>
                            </div>
                            <div className="flex items-center space-x-2">
                              <span className="text-muted-foreground">Offset:</span>
                              <code className="text-xs">{msg.offset}</code>
                            </div>
                          </>
                        )}
                        <div className="flex items-start space-x-2">
                          <span className="text-muted-foreground">Key:</span>
                          <code className="text-xs break-all">{msg.key}</code>
                        </div>
                        <div className="flex items-start space-x-2">
                          <span className="text-muted-foreground">Value:</span>
                          <code className="text-xs break-all bg-white dark:bg-gray-900 px-2 py-1 rounded">
                            {msg.value}
                          </code>
                        </div>
                        {msg.error && (
                          <div className="flex items-start space-x-2 mt-2 pt-2 border-t border-red-200 dark:border-red-800">
                            <span className="text-red-600 font-medium">Error:</span>
                            <span className="text-red-600 text-xs">{msg.error}</span>
                          </div>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </main>
    </div>
  );
}
