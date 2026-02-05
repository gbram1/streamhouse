"use client";

import { useState, useEffect } from "react";
import { DashboardLayout } from "@/components/layout/dashboard-layout";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Send, Loader2, AlertCircle, CheckCircle, Clock, Trash2, Copy, Database } from "lucide-react";
import { apiClient } from "@/lib/api/client";
import { cn } from "@/lib/utils";
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
        partition: partitionNum ?? -1,
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

  // Clear message log
  const clearMessages = () => setSentMessages([]);

  // Copy value to clipboard
  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  return (
    <DashboardLayout
      title="Producer Console"
      description="Send test messages to your topics"
    >
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Producer Form */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Send className="h-5 w-5" />
              Produce Message
            </CardTitle>
            <CardDescription>
              Send messages to any topic through the web console
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-5">
            {/* Topic Selector */}
            <div className="space-y-2">
              <Label htmlFor="topic">Topic</Label>
              {loadingTopics ? (
                <Skeleton className="h-10 w-full" />
              ) : topicsError ? (
                <div className="flex items-center space-x-2 text-red-600 text-sm p-3 bg-red-50 dark:bg-red-950/50 rounded-md border border-red-200 dark:border-red-800">
                  <AlertCircle className="h-4 w-4 shrink-0" />
                  <span>{topicsError}</span>
                </div>
              ) : topics.length === 0 ? (
                <div className="flex items-center space-x-2 text-muted-foreground text-sm p-3 bg-muted/50 rounded-md border">
                  <Database className="h-4 w-4 shrink-0" />
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
                        <div className="flex items-center gap-2">
                          <span>{topic.name}</span>
                          <Badge variant="secondary" className="text-xs">
                            {topic.partitions} partitions
                          </Badge>
                        </div>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
            </div>

            {/* Partition Selector */}
            <div className="space-y-2">
              <Label htmlFor="partition">Partition</Label>
              <Select value={partition} onValueChange={setPartition} disabled={!selectedTopic}>
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
                placeholder="user-123"
                value={messageKey}
                onChange={(e) => setMessageKey(e.target.value)}
                disabled={sending || !selectedTopic}
                className="font-mono text-sm"
              />
              <p className="text-xs text-muted-foreground">
                Messages with the same key are routed to the same partition
              </p>
            </div>

            {/* Message Value */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label htmlFor="value">Value</Label>
                <span className="text-xs text-muted-foreground">
                  {messageValue.length} characters
                </span>
              </div>
              <Textarea
                id="value"
                placeholder='{"order_id": 42, "amount": 99.99, "user": "alice"}'
                value={messageValue}
                onChange={(e) => setMessageValue(e.target.value)}
                onKeyDown={handleKeyPress}
                disabled={sending || !selectedTopic}
                className="min-h-[150px] font-mono text-sm resize-y"
              />
              <p className="text-xs text-muted-foreground">
                Press <kbd className="px-1.5 py-0.5 bg-muted rounded text-xs">⌘</kbd> + <kbd className="px-1.5 py-0.5 bg-muted rounded text-xs">Enter</kbd> to send
              </p>
            </div>

            {/* Send Button */}
            <Button
              onClick={handleSendMessage}
              disabled={sending || !selectedTopic || !messageValue}
              className="w-full"
              size="lg"
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
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
            <div>
              <CardTitle>Recent Messages</CardTitle>
              <CardDescription>
                Last {sentMessages.length} of 50 messages
              </CardDescription>
            </div>
            {sentMessages.length > 0 && (
              <Button variant="ghost" size="sm" onClick={clearMessages}>
                <Trash2 className="mr-2 h-4 w-4" />
                Clear
              </Button>
            )}
          </CardHeader>
          <CardContent>
            {sentMessages.length === 0 ? (
              <div className="text-center text-muted-foreground py-12">
                <div className="mx-auto w-12 h-12 rounded-full bg-muted/50 flex items-center justify-center mb-4">
                  <Send className="h-6 w-6 opacity-50" />
                </div>
                <p className="font-medium">No messages sent yet</p>
                <p className="text-sm mt-1">Send a message to see it here</p>
              </div>
            ) : (
              <div className="space-y-3 max-h-[550px] overflow-y-auto pr-2">
                {sentMessages.map((msg) => (
                  <div
                    key={msg.id}
                    className={cn(
                      "p-4 rounded-lg border transition-colors",
                      msg.status === 'success'
                        ? 'bg-green-500/5 border-green-500/20 hover:bg-green-500/10'
                        : 'bg-red-500/5 border-red-500/20 hover:bg-red-500/10'
                    )}
                  >
                    <div className="flex items-start justify-between mb-3">
                      <div className="flex items-center gap-2">
                        {msg.status === 'success' ? (
                          <div className="h-6 w-6 rounded-full bg-green-500/20 flex items-center justify-center">
                            <CheckCircle className="h-4 w-4 text-green-600" />
                          </div>
                        ) : (
                          <div className="h-6 w-6 rounded-full bg-red-500/20 flex items-center justify-center">
                            <AlertCircle className="h-4 w-4 text-red-600" />
                          </div>
                        )}
                        <div>
                          <span className="font-medium text-sm">
                            {msg.status === 'success' ? 'Sent successfully' : 'Failed to send'}
                          </span>
                          <div className="flex items-center gap-1 text-xs text-muted-foreground">
                            <Clock className="h-3 w-3" />
                            {msg.timestamp.toLocaleTimeString()}
                          </div>
                        </div>
                      </div>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        onClick={() => copyToClipboard(msg.value)}
                        title="Copy value"
                      >
                        <Copy className="h-3.5 w-3.5" />
                      </Button>
                    </div>

                    <div className="space-y-2 text-sm">
                      <div className="flex items-center gap-2">
                        <span className="text-muted-foreground w-16">Topic</span>
                        <Badge variant="outline" className="font-mono text-xs">{msg.topic}</Badge>
                      </div>
                      {msg.status === 'success' && (
                        <div className="flex items-center gap-4">
                          <div className="flex items-center gap-2">
                            <span className="text-muted-foreground w-16">Partition</span>
                            <code className="text-xs bg-muted px-1.5 py-0.5 rounded">{msg.partition}</code>
                          </div>
                          <div className="flex items-center gap-2">
                            <span className="text-muted-foreground">Offset</span>
                            <code className="text-xs bg-muted px-1.5 py-0.5 rounded">{msg.offset}</code>
                          </div>
                        </div>
                      )}
                      <div className="flex items-start gap-2">
                        <span className="text-muted-foreground w-16 shrink-0">Key</span>
                        <code className="text-xs font-mono break-all">{msg.key}</code>
                      </div>
                      <div className="flex items-start gap-2">
                        <span className="text-muted-foreground w-16 shrink-0">Value</span>
                        <code className="text-xs font-mono break-all bg-muted/50 px-2 py-1 rounded max-h-20 overflow-auto block w-full">
                          {msg.value}
                        </code>
                      </div>
                      {msg.error && (
                        <div className="mt-2 pt-2 border-t border-red-200 dark:border-red-800">
                          <span className="text-red-600 font-medium text-xs">Error: </span>
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
    </DashboardLayout>
  );
}
