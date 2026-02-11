'use client';

import { useState, useEffect, useRef, useCallback } from 'react';
import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Play,
  Square,
  Users,
  Clock,
  Activity,
  BarChart3,
  MessageSquare,
  AlertTriangle,
  Loader2,
  Zap,
} from 'lucide-react';
import { apiClient } from '@/lib/api/client';
import type { Topic, Partition } from '@/lib/api/client';
import { cn, formatCompactNumber } from '@/lib/utils';

interface ConsumerInstance {
  id: number;
  assignedPartitions: number[];
  messagesConsumed: number;
  currentOffset: Record<number, number>;
  processing: boolean;
}

interface PartitionProgress {
  partitionId: number;
  currentOffset: number;
  highWatermark: number;
  lag: number;
  assignedConsumer: number | null;
}

interface ConsumedMessage {
  id: string;
  partition: number;
  offset: number;
  key: string | null;
  value: string;
  timestamp: number;
  consumerId: number;
  consumedAt: number;
}

interface SimulationStats {
  totalConsumed: number;
  avgProcessingTime: number;
  totalLag: number;
  startTime: number | null;
  messagesPerSecond: number;
}

export default function ConsumerSimulatorPage() {
  // Topic and group configuration
  const [topics, setTopics] = useState<Topic[]>([]);
  const [loadingTopics, setLoadingTopics] = useState(true);
  const [selectedTopic, setSelectedTopic] = useState<string>('');
  const [groupId, setGroupId] = useState<string>('simulator-group-1');
  const [existingGroups, setExistingGroups] = useState<string[]>([]);
  const [selectedExistingGroup, setSelectedExistingGroup] = useState<string>('');

  // Simulation configuration
  const [consumerCount, setConsumerCount] = useState<number>(3);
  const [processingDelay, setProcessingDelay] = useState<number>(100);
  const [autoCommit, setAutoCommit] = useState<boolean>(true);

  // Simulation state
  const [running, setRunning] = useState(false);
  const [consumers, setConsumers] = useState<ConsumerInstance[]>([]);
  const [partitionProgress, setPartitionProgress] = useState<PartitionProgress[]>([]);
  const [consumedMessages, setConsumedMessages] = useState<ConsumedMessage[]>([]);
  const [stats, setStats] = useState<SimulationStats>({
    totalConsumed: 0,
    avgProcessingTime: 0,
    totalLag: 0,
    startTime: null,
    messagesPerSecond: 0,
  });

  // Refs for simulation loop
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const runningRef = useRef(false);
  const consumersRef = useRef<ConsumerInstance[]>([]);
  const statsRef = useRef<SimulationStats>(stats);
  const messagesRef = useRef<ConsumedMessage[]>([]);

  // Fetch topics
  useEffect(() => {
    const fetchTopics = async () => {
      try {
        setLoadingTopics(true);
        const data = await apiClient.listTopics();
        setTopics(data);
        if (data.length > 0 && !selectedTopic) {
          setSelectedTopic(data[0].name);
        }
      } catch (err) {
        console.error('Error fetching topics:', err);
      } finally {
        setLoadingTopics(false);
      }
    };
    fetchTopics();
  }, []);

  // Fetch existing consumer groups
  useEffect(() => {
    const fetchGroups = async () => {
      try {
        const groups = await apiClient.listConsumerGroups();
        setExistingGroups(groups.map((g) => g.groupId));
      } catch (err) {
        console.error('Error fetching consumer groups:', err);
      }
    };
    fetchGroups();
  }, []);

  // Get partition info for selected topic
  const fetchPartitions = useCallback(async (topicName: string): Promise<Partition[]> => {
    try {
      return await apiClient.listPartitions(topicName);
    } catch {
      return [];
    }
  }, []);

  // Assign partitions round-robin across consumers
  const assignPartitions = (partitionCount: number, numConsumers: number): ConsumerInstance[] => {
    const instances: ConsumerInstance[] = Array.from({ length: numConsumers }, (_, i) => ({
      id: i,
      assignedPartitions: [],
      messagesConsumed: 0,
      currentOffset: {},
      processing: false,
    }));

    for (let p = 0; p < partitionCount; p++) {
      const consumerIdx = p % numConsumers;
      instances[consumerIdx].assignedPartitions.push(p);
      instances[consumerIdx].currentOffset[p] = 0;
    }

    return instances;
  };

  // Start simulation
  const startSimulation = async () => {
    if (!selectedTopic) return;

    const partitions = await fetchPartitions(selectedTopic);
    if (partitions.length === 0) return;

    const effectiveGroupId = selectedExistingGroup || groupId;
    const numConsumers = Math.min(consumerCount, partitions.length);
    const instances = assignPartitions(partitions.length, numConsumers);

    // Initialize partition progress
    const progress: PartitionProgress[] = partitions.map((p) => {
      const assignedConsumer = instances.find((c) =>
        c.assignedPartitions.includes(p.partition_id)
      );
      return {
        partitionId: p.partition_id,
        currentOffset: 0,
        highWatermark: p.high_watermark,
        lag: p.high_watermark,
        assignedConsumer: assignedConsumer?.id ?? null,
      };
    });

    setConsumers(instances);
    consumersRef.current = instances;
    setPartitionProgress(progress);
    setConsumedMessages([]);
    messagesRef.current = [];
    const newStats = {
      totalConsumed: 0,
      avgProcessingTime: 0,
      totalLag: progress.reduce((acc, p) => acc + p.lag, 0),
      startTime: Date.now(),
      messagesPerSecond: 0,
    };
    setStats(newStats);
    statsRef.current = newStats;
    setRunning(true);
    runningRef.current = true;

    // Start polling loop
    intervalRef.current = setInterval(async () => {
      if (!runningRef.current) return;

      try {
        // Refresh partition high watermarks
        const freshPartitions = await apiClient.listPartitions(selectedTopic);

        const updatedProgress: PartitionProgress[] = [];
        const newMessages: ConsumedMessage[] = [];
        const updatedConsumers = [...consumersRef.current];

        for (const consumer of updatedConsumers) {
          if (!runningRef.current) break;

          for (const partId of consumer.assignedPartitions) {
            if (!runningRef.current) break;

            const freshPart = freshPartitions.find((p) => p.partition_id === partId);
            const hw = freshPart?.high_watermark ?? 0;
            const currentOff = consumer.currentOffset[partId] ?? 0;

            if (currentOff < hw) {
              // Consume a batch of records
              try {
                const response = await apiClient.consume(
                  selectedTopic,
                  partId,
                  currentOff,
                  5 // small batch per tick
                );

                if (response.records && response.records.length > 0) {
                  // Simulate processing delay
                  if (processingDelay > 0) {
                    await new Promise((r) => setTimeout(r, processingDelay / 10)); // scaled down
                  }

                  const nextOffset = response.nextOffset ?? (currentOff + response.records.length);
                  consumer.currentOffset[partId] = nextOffset;
                  consumer.messagesConsumed += response.records.length;

                  for (const record of response.records) {
                    newMessages.push({
                      id: `${partId}-${record.offset}-${Date.now()}`,
                      partition: partId,
                      offset: record.offset,
                      key: record.key,
                      value: typeof record.value === 'string' ? record.value : JSON.stringify(record.value),
                      timestamp: record.timestamp,
                      consumerId: consumer.id,
                      consumedAt: Date.now(),
                    });
                  }

                  updatedProgress.push({
                    partitionId: partId,
                    currentOffset: nextOffset,
                    highWatermark: hw,
                    lag: Math.max(0, hw - nextOffset),
                    assignedConsumer: consumer.id,
                  });
                } else {
                  updatedProgress.push({
                    partitionId: partId,
                    currentOffset: currentOff,
                    highWatermark: hw,
                    lag: Math.max(0, hw - currentOff),
                    assignedConsumer: consumer.id,
                  });
                }
              } catch {
                updatedProgress.push({
                  partitionId: partId,
                  currentOffset: currentOff,
                  highWatermark: hw,
                  lag: Math.max(0, hw - currentOff),
                  assignedConsumer: consumer.id,
                });
              }
            } else {
              updatedProgress.push({
                partitionId: partId,
                currentOffset: currentOff,
                highWatermark: hw,
                lag: 0,
                assignedConsumer: consumer.id,
              });
            }
          }
        }

        consumersRef.current = updatedConsumers;

        // Update state
        setConsumers([...updatedConsumers]);

        if (updatedProgress.length > 0) {
          setPartitionProgress(updatedProgress);
        }

        if (newMessages.length > 0) {
          const allMessages = [...newMessages, ...messagesRef.current].slice(0, 20);
          messagesRef.current = allMessages;
          setConsumedMessages(allMessages);
        }

        // Update stats
        const totalConsumed = updatedConsumers.reduce((acc, c) => acc + c.messagesConsumed, 0);
        const totalLag = updatedProgress.reduce((acc, p) => acc + p.lag, 0);
        const elapsed = (Date.now() - (statsRef.current.startTime || Date.now())) / 1000;
        const mps = elapsed > 0 ? totalConsumed / elapsed : 0;

        const updatedStats = {
          ...statsRef.current,
          totalConsumed,
          totalLag,
          messagesPerSecond: Math.round(mps * 10) / 10,
          avgProcessingTime: processingDelay,
        };
        statsRef.current = updatedStats;
        setStats(updatedStats);
      } catch (err) {
        console.error('Simulation tick error:', err);
      }
    }, 500);
  };

  // Stop simulation
  const stopSimulation = () => {
    runningRef.current = false;
    setRunning(false);
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }
  };

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      runningRef.current = false;
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
    };
  }, []);

  const selectedTopicData = topics.find((t) => t.name === selectedTopic);

  return (
    <DashboardLayout
      title="Consumer Simulator"
      description="Simulate consumer groups consuming from topics in real-time"
    >
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Configuration Panel */}
        <Card className="lg:col-span-1">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Users className="h-5 w-5" />
              Configuration
            </CardTitle>
            <CardDescription>
              Set up your consumer simulation parameters
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-5">
            {/* Consumer Group */}
            <div className="space-y-2">
              <Label>Consumer Group</Label>
              <Input
                placeholder="my-consumer-group"
                value={groupId}
                onChange={(e) => setGroupId(e.target.value)}
                disabled={running}
              />
              {existingGroups.length > 0 && (
                <div className="space-y-1">
                  <span className="text-xs text-muted-foreground">Or select existing:</span>
                  <Select
                    value={selectedExistingGroup}
                    onValueChange={(val) => {
                      setSelectedExistingGroup(val);
                      if (val) setGroupId(val);
                    }}
                    disabled={running}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select existing group" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value=" ">-- New group --</SelectItem>
                      {existingGroups.map((g) => (
                        <SelectItem key={g} value={g}>
                          {g}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
            </div>

            {/* Topic */}
            <div className="space-y-2">
              <Label>Topic</Label>
              {loadingTopics ? (
                <div className="text-sm text-muted-foreground p-3 bg-muted/50 rounded-md border">
                  Loading topics...
                </div>
              ) : topics.length === 0 ? (
                <div className="text-sm text-muted-foreground p-3 bg-muted/50 rounded-md border">
                  No topics found. Create a topic first.
                </div>
              ) : (
                <Select value={selectedTopic} onValueChange={setSelectedTopic} disabled={running}>
                  <SelectTrigger>
                    <SelectValue placeholder="Select a topic" />
                  </SelectTrigger>
                  <SelectContent>
                    {topics.map((topic) => (
                      <SelectItem key={topic.name} value={topic.name}>
                        <div className="flex items-center gap-2">
                          <span>{topic.name}</span>
                          <Badge variant="secondary" className="text-xs">
                            {topic.partitions}p
                          </Badge>
                        </div>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
            </div>

            {/* Number of Consumers */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label>Virtual Consumers</Label>
                <span className="text-sm font-medium text-muted-foreground">{consumerCount}</span>
              </div>
              <input
                type="range"
                min={1}
                max={10}
                value={consumerCount}
                onChange={(e) => setConsumerCount(parseInt(e.target.value))}
                disabled={running}
                className="w-full h-2 bg-secondary rounded-full appearance-none cursor-pointer accent-primary"
              />
              <div className="flex justify-between text-xs text-muted-foreground">
                <span>1</span>
                <span>5</span>
                <span>10</span>
              </div>
            </div>

            {/* Processing Delay */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label>Processing Delay</Label>
                <span className="text-sm font-medium text-muted-foreground">{processingDelay}ms</span>
              </div>
              <input
                type="range"
                min={0}
                max={1000}
                step={50}
                value={processingDelay}
                onChange={(e) => setProcessingDelay(parseInt(e.target.value))}
                disabled={running}
                className="w-full h-2 bg-secondary rounded-full appearance-none cursor-pointer accent-primary"
              />
              <div className="flex justify-between text-xs text-muted-foreground">
                <span>0ms</span>
                <span>500ms</span>
                <span>1000ms</span>
              </div>
            </div>

            {/* Auto Commit */}
            <div className="flex items-center gap-3">
              <Checkbox
                checked={autoCommit}
                onCheckedChange={(checked) => setAutoCommit(checked === true)}
                disabled={running}
              />
              <Label className="cursor-pointer">Auto-commit offsets</Label>
            </div>

            {/* Start/Stop Button */}
            <Button
              onClick={running ? stopSimulation : startSimulation}
              disabled={!selectedTopic || loadingTopics}
              className={cn('w-full', running && 'bg-red-600 hover:bg-red-700')}
              size="lg"
            >
              {running ? (
                <>
                  <Square className="mr-2 h-4 w-4" />
                  Stop Simulation
                </>
              ) : (
                <>
                  <Play className="mr-2 h-4 w-4" />
                  Start Simulation
                </>
              )}
            </Button>

            {running && (
              <div className="flex items-center justify-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="h-3 w-3 animate-spin" />
                <span>Simulation running...</span>
              </div>
            )}
          </CardContent>
        </Card>

        {/* Main Display Area */}
        <div className="lg:col-span-2 space-y-6">
          {/* Stats Panel */}
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <Card className="p-4">
              <div className="flex items-center gap-2 text-sm text-muted-foreground mb-1">
                <MessageSquare className="h-4 w-4" />
                <span>Consumed</span>
              </div>
              <div className="text-2xl font-bold">
                {formatCompactNumber(stats.totalConsumed)}
              </div>
            </Card>
            <Card className="p-4">
              <div className="flex items-center gap-2 text-sm text-muted-foreground mb-1">
                <Zap className="h-4 w-4" />
                <span>Throughput</span>
              </div>
              <div className="text-2xl font-bold">
                {stats.messagesPerSecond}/s
              </div>
            </Card>
            <Card className="p-4">
              <div className="flex items-center gap-2 text-sm text-muted-foreground mb-1">
                <Clock className="h-4 w-4" />
                <span>Avg Processing</span>
              </div>
              <div className="text-2xl font-bold">
                {stats.avgProcessingTime}ms
              </div>
            </Card>
            <Card className="p-4">
              <div className="flex items-center gap-2 text-sm text-muted-foreground mb-1">
                <AlertTriangle className="h-4 w-4" />
                <span>Total Lag</span>
              </div>
              <div className={cn(
                'text-2xl font-bold',
                stats.totalLag > 0 ? 'text-yellow-600' : 'text-green-600'
              )}>
                {formatCompactNumber(stats.totalLag)}
              </div>
            </Card>
          </div>

          {/* Consumer Group State */}
          {consumers.length > 0 && (
            <Card>
              <CardHeader className="pb-3">
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">Consumer Instances</CardTitle>
                  <Badge variant={running ? 'default' : 'secondary'} className={running ? 'bg-green-600' : ''}>
                    {running ? 'stable' : 'stopped'}
                  </Badge>
                </div>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                  {consumers.map((consumer) => (
                    <div
                      key={consumer.id}
                      className="p-3 rounded-lg border bg-card"
                    >
                      <div className="flex items-center justify-between mb-2">
                        <span className="text-sm font-medium">Consumer {consumer.id}</span>
                        <Badge variant="outline" className="text-xs">
                          {consumer.assignedPartitions.length}p
                        </Badge>
                      </div>
                      <div className="text-xs text-muted-foreground">
                        Partitions: [{consumer.assignedPartitions.join(', ')}]
                      </div>
                      <div className="text-xs text-muted-foreground mt-1">
                        Messages: {formatCompactNumber(consumer.messagesConsumed)}
                      </div>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}

          {/* Partition Lag Visualization */}
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-base flex items-center gap-2">
                <BarChart3 className="h-4 w-4" />
                Partition Lag
              </CardTitle>
              <CardDescription>
                Current offset vs high watermark for each partition
              </CardDescription>
            </CardHeader>
            <CardContent>
              {partitionProgress.length === 0 ? (
                <div className="flex h-32 items-center justify-center text-muted-foreground">
                  <div className="text-center">
                    <BarChart3 className="mx-auto h-8 w-8 opacity-50 mb-2" />
                    <p className="text-sm">Start a simulation to see partition lag</p>
                  </div>
                </div>
              ) : (
                <div className="space-y-3">
                  {partitionProgress.map((pp) => {
                    const progress = pp.highWatermark > 0
                      ? (pp.currentOffset / pp.highWatermark) * 100
                      : 100;
                    return (
                      <div key={pp.partitionId}>
                        <div className="flex items-center justify-between mb-1">
                          <div className="flex items-center gap-2">
                            <span className="text-sm font-medium">P{pp.partitionId}</span>
                            {pp.assignedConsumer !== null && (
                              <Badge variant="outline" className="text-xs">
                                C{pp.assignedConsumer}
                              </Badge>
                            )}
                          </div>
                          <div className="flex items-center gap-3 text-xs text-muted-foreground">
                            <span>
                              {formatCompactNumber(pp.currentOffset)} / {formatCompactNumber(pp.highWatermark)}
                            </span>
                            <span className={cn(
                              'font-medium',
                              pp.lag > 0 ? 'text-yellow-600' : 'text-green-600'
                            )}>
                              lag: {formatCompactNumber(pp.lag)}
                            </span>
                          </div>
                        </div>
                        <div className="h-2.5 bg-secondary rounded-full overflow-hidden">
                          <div
                            className={cn(
                              'h-full rounded-full transition-all duration-300',
                              pp.lag === 0 ? 'bg-green-500' : pp.lag < 100 ? 'bg-yellow-500' : 'bg-primary'
                            )}
                            style={{ width: `${Math.min(100, progress)}%` }}
                          />
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </CardContent>
          </Card>

          {/* Real-time Message Display */}
          <Card>
            <CardHeader className="pb-3">
              <div className="flex items-center justify-between">
                <div>
                  <CardTitle className="text-base flex items-center gap-2">
                    <Activity className="h-4 w-4" />
                    Consumed Messages
                  </CardTitle>
                  <CardDescription>Last 20 consumed messages</CardDescription>
                </div>
                {consumedMessages.length > 0 && (
                  <Badge variant="secondary">{consumedMessages.length} messages</Badge>
                )}
              </div>
            </CardHeader>
            <CardContent>
              {consumedMessages.length === 0 ? (
                <div className="flex h-32 items-center justify-center text-muted-foreground">
                  <div className="text-center">
                    <MessageSquare className="mx-auto h-8 w-8 opacity-50 mb-2" />
                    <p className="text-sm">No messages consumed yet</p>
                  </div>
                </div>
              ) : (
                <div className="space-y-2 max-h-100 overflow-y-auto pr-2">
                  {consumedMessages.map((msg) => (
                    <div
                      key={msg.id}
                      className="p-3 rounded-lg border bg-card hover:bg-accent/50 transition-colors"
                    >
                      <div className="flex items-center justify-between mb-1.5">
                        <div className="flex items-center gap-2">
                          <Badge variant="outline" className="text-xs font-mono">
                            P{msg.partition}:O{msg.offset}
                          </Badge>
                          <Badge variant="secondary" className="text-xs">
                            C{msg.consumerId}
                          </Badge>
                        </div>
                        <span className="text-xs text-muted-foreground">
                          {new Date(msg.consumedAt).toLocaleTimeString()}
                        </span>
                      </div>
                      {msg.key && (
                        <div className="text-xs text-muted-foreground mb-1">
                          Key: <code className="bg-muted px-1 py-0.5 rounded">{msg.key}</code>
                        </div>
                      )}
                      <div className="text-xs font-mono bg-muted/50 px-2 py-1.5 rounded max-h-16 overflow-auto break-all">
                        {msg.value}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </DashboardLayout>
  );
}
