'use client';

import { useState, useCallback, useEffect } from 'react';
import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Sparkles,
  Play,
  Clock,
  Database,
  Copy,
  CheckCircle,
  AlertTriangle,
  Loader2,
  History,
  RefreshCw,
  Trash2,
  FileSearch,
  Lightbulb,
  Code,
  ChevronRight,
  Zap,
  Search,
} from 'lucide-react';

// API types
interface QueryResults {
  columns: string[];
  rows: (string | number | null | object)[][];
  rowCount: number;
  executionTimeMs: number;
  truncated: boolean;
}

interface CostEstimate {
  sql?: string;
  estimatedRows: number;
  estimatedBytes: number;
  topics: string[];
  estimatedTimeMs: number;
  costTier: string;
  warnings: string[];
  suggestions: string[];
}

interface AskQueryResponse {
  queryId: string;
  question: string;
  sql: string;
  explanation: string;
  results?: QueryResults;
  topicsUsed: string[];
  confidence: number;
  suggestions: string[];
  costEstimate?: CostEstimate;
}

interface QueryHistoryEntry {
  id: string;
  question: string;
  sql: string;
  explanation: string;
  topicsUsed: string[];
  confidence: number;
  createdAt: number;
  parentId?: string;
  refinementCount: number;
}

interface InferredField {
  path: string;
  jsonType: string;
  nullable: boolean;
  occurrenceRate: number;
  uniqueValues?: number;
  sampleValues: unknown[];
  description?: string;
  suggestedSqlType: string;
}

interface IndexRecommendation {
  field: string;
  reason: string;
  priority: string;
  exampleQuery: string;
}

interface InferredSchema {
  topic: string;
  sampleCount: number;
  fields: InferredField[];
  indexRecommendations: IndexRecommendation[];
  confidence: number;
  summary?: string;
}

interface Topic {
  name: string;
  partitionCount: number;
}

// Example prompts
const EXAMPLE_PROMPTS = [
  "How many messages are in the orders topic?",
  "Show me the 10 most recent orders",
  "What are the unique customer IDs in orders?",
  "Show orders from partition 0",
  "Extract the amount field from the last 20 orders",
];

export default function AIQueryPage() {
  const [activeTab, setActiveTab] = useState('query');

  // Query state
  const [question, setQuestion] = useState('');
  const [queryResult, setQueryResult] = useState<AskQueryResponse | null>(null);
  const [queryError, setQueryError] = useState<string | null>(null);
  const [isQuerying, setIsQuerying] = useState(false);
  const [selectedTopics, setSelectedTopics] = useState<string[]>([]);
  const [topics, setTopics] = useState<Topic[]>([]);
  const [executeQuery, setExecuteQuery] = useState(true);

  // History state
  const [history, setHistory] = useState<QueryHistoryEntry[]>([]);
  const [isLoadingHistory, setIsLoadingHistory] = useState(false);

  // Schema inference state
  const [schemaTopic, setSchemaTopic] = useState('');
  const [schemaResult, setSchemaResult] = useState<InferredSchema | null>(null);
  const [schemaError, setSchemaError] = useState<string | null>(null);
  const [isInferring, setIsInferring] = useState(false);

  // AI health status
  const [aiStatus, setAiStatus] = useState<'loading' | 'configured' | 'not_configured'>('loading');

  // Copied state
  const [copied, setCopied] = useState(false);

  // Check AI health on mount
  useEffect(() => {
    const checkHealth = async () => {
      try {
        const res = await fetch('/api/v1/ai/health');
        if (res.ok) {
          setAiStatus('configured');
        } else {
          setAiStatus('not_configured');
        }
      } catch {
        setAiStatus('not_configured');
      }
    };
    checkHealth();
  }, []);

  // Load topics
  useEffect(() => {
    const loadTopics = async () => {
      try {
        const res = await fetch('/api/v1/topics');
        if (res.ok) {
          const data = await res.json();
          setTopics(data);
        }
      } catch (err) {
        console.error('Failed to load topics:', err);
      }
    };
    loadTopics();
  }, []);

  // Load history
  const loadHistory = useCallback(async () => {
    setIsLoadingHistory(true);
    try {
      const res = await fetch('/api/v1/query/history?limit=50');
      if (res.ok) {
        const data = await res.json();
        setHistory(data.queries || []);
      }
    } catch (err) {
      console.error('Failed to load history:', err);
    } finally {
      setIsLoadingHistory(false);
    }
  }, []);

  useEffect(() => {
    if (activeTab === 'history') {
      loadHistory();
    }
  }, [activeTab, loadHistory]);

  // Ask query
  const handleAskQuery = useCallback(async () => {
    if (!question.trim()) return;

    setIsQuerying(true);
    setQueryError(null);
    setQueryResult(null);

    try {
      const res = await fetch('/api/v1/query/ask', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          question,
          topics: selectedTopics.length > 0 ? selectedTopics : undefined,
          execute: executeQuery,
          timeoutMs: 30000,
        }),
      });

      if (!res.ok) {
        const error = await res.json();
        throw new Error(error.message || 'Query failed');
      }

      const data: AskQueryResponse = await res.json();
      setQueryResult(data);
    } catch (err) {
      setQueryError(err instanceof Error ? err.message : 'Failed to process query');
    } finally {
      setIsQuerying(false);
    }
  }, [question, selectedTopics, executeQuery]);

  // Refine query (TODO: add UI for refinement)
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const handleRefineQuery = useCallback(async (queryId: string, refinement: string) => {
    setIsQuerying(true);
    setQueryError(null);

    try {
      const res = await fetch(`/api/v1/query/history/${queryId}/refine`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          refinement,
          execute: executeQuery,
        }),
      });

      if (!res.ok) {
        const error = await res.json();
        throw new Error(error.message || 'Refinement failed');
      }

      const data: AskQueryResponse = await res.json();
      setQueryResult(data);
    } catch (err) {
      setQueryError(err instanceof Error ? err.message : 'Failed to refine query');
    } finally {
      setIsQuerying(false);
    }
  }, [executeQuery]);

  // Infer schema
  const handleInferSchema = useCallback(async () => {
    if (!schemaTopic) return;

    setIsInferring(true);
    setSchemaError(null);
    setSchemaResult(null);

    try {
      const res = await fetch('/api/v1/schema/infer', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          topic: schemaTopic,
          sampleSize: 100,
          generateDescriptions: true,
        }),
      });

      if (!res.ok) {
        const error = await res.json();
        throw new Error(error.message || 'Schema inference failed');
      }

      const data: InferredSchema = await res.json();
      setSchemaResult(data);
    } catch (err) {
      setSchemaError(err instanceof Error ? err.message : 'Failed to infer schema');
    } finally {
      setIsInferring(false);
    }
  }, [schemaTopic]);

  // Delete query from history
  const handleDeleteQuery = useCallback(async (queryId: string) => {
    try {
      await fetch(`/api/v1/query/history/${queryId}`, { method: 'DELETE' });
      setHistory(prev => prev.filter(q => q.id !== queryId));
    } catch (err) {
      console.error('Failed to delete query:', err);
    }
  }, []);

  // Copy SQL
  const copySql = useCallback((sql: string) => {
    navigator.clipboard.writeText(sql);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, []);

  // Load example
  const loadExample = useCallback((prompt: string) => {
    setQuestion(prompt);
    setQueryError(null);
    setQueryResult(null);
  }, []);

  // Use history entry
  const handleUseHistoryEntry = useCallback((entry: QueryHistoryEntry) => {
    setQuestion(entry.question);
    setActiveTab('query');
  }, []);

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const formatBytes = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleString();
  };

  const getCostTierColor = (tier: string) => {
    switch (tier) {
      case 'low': return 'bg-green-500/20 text-green-500';
      case 'medium': return 'bg-yellow-500/20 text-yellow-500';
      case 'high': return 'bg-red-500/20 text-red-500';
      default: return 'bg-gray-500/20 text-gray-500';
    }
  };

  const getPriorityColor = (priority: string) => {
    switch (priority) {
      case 'high': return 'bg-red-500/20 text-red-500';
      case 'medium': return 'bg-yellow-500/20 text-yellow-500';
      case 'low': return 'bg-blue-500/20 text-blue-500';
      default: return 'bg-gray-500/20 text-gray-500';
    }
  };

  if (aiStatus === 'loading') {
    return (
      <DashboardLayout title="AI Query" description="Loading...">
        <div className="flex items-center justify-center h-64">
          <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
        </div>
      </DashboardLayout>
    );
  }

  if (aiStatus === 'not_configured') {
    return (
      <DashboardLayout
        title={
          <div className="flex items-center gap-2">
            <Sparkles className="h-6 w-6 text-purple-500" />
            AI Query
          </div>
        }
        description="Query your data with natural language"
      >
        <Card className="p-8 text-center">
          <AlertTriangle className="h-12 w-12 mx-auto text-yellow-500 mb-4" />
          <h2 className="text-xl font-semibold mb-2">AI Not Configured</h2>
          <p className="text-muted-foreground mb-4">
            The ANTHROPIC_API_KEY environment variable is not set.
          </p>
          <div className="text-left max-w-md mx-auto bg-muted p-4 rounded-lg">
            <p className="text-sm font-medium mb-2">To enable AI queries:</p>
            <ol className="text-sm text-muted-foreground space-y-2 list-decimal list-inside">
              <li>Get an API key from <a href="https://console.anthropic.com" target="_blank" rel="noopener noreferrer" className="text-blue-500 hover:underline">console.anthropic.com</a></li>
              <li>Set the ANTHROPIC_API_KEY environment variable</li>
              <li>Restart the StreamHouse server</li>
            </ol>
          </div>
        </Card>
      </DashboardLayout>
    );
  }

  return (
    <DashboardLayout
      title={
        <div className="flex items-center gap-2">
          <Sparkles className="h-6 w-6 text-purple-500" />
          AI Query
        </div>
      }
      description="Query your data with natural language"
    >
      <Tabs value={activeTab} onValueChange={setActiveTab} className="space-y-6">
        <TabsList>
          <TabsTrigger value="query" className="flex items-center gap-2">
            <Sparkles className="h-4 w-4" />
            Ask Question
          </TabsTrigger>
          <TabsTrigger value="schema" className="flex items-center gap-2">
            <FileSearch className="h-4 w-4" />
            Schema Inference
          </TabsTrigger>
          <TabsTrigger value="history" className="flex items-center gap-2">
            <History className="h-4 w-4" />
            History
          </TabsTrigger>
        </TabsList>

        {/* Ask Question Tab */}
        <TabsContent value="query" className="space-y-6">
          <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
            {/* Main Query Area */}
            <div className="lg:col-span-3 space-y-4">
              <Card className="p-4">
                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <h3 className="text-lg font-semibold flex items-center gap-2">
                      <Search className="h-5 w-5" />
                      Ask a Question
                    </h3>
                    <Badge variant="outline" className="bg-purple-500/10 text-purple-500">
                      Powered by Claude
                    </Badge>
                  </div>

                  <Textarea
                    value={question}
                    onChange={(e) => setQuestion(e.target.value)}
                    placeholder="Ask a question about your data in plain English..."
                    className="min-h-[100px] text-base"
                    onKeyDown={(e) => {
                      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
                        e.preventDefault();
                        handleAskQuery();
                      }
                    }}
                  />

                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-4">
                      <Select
                        value={selectedTopics.length === 0 ? 'all' : selectedTopics[0]}
                        onValueChange={(v) => setSelectedTopics(v === 'all' ? [] : [v])}
                      >
                        <SelectTrigger className="w-[200px]">
                          <SelectValue placeholder="Select topic" />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="all">All Topics</SelectItem>
                          {topics.map((topic) => (
                            <SelectItem key={topic.name} value={topic.name}>
                              {topic.name}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>

                      <label className="flex items-center gap-2 text-sm">
                        <input
                          type="checkbox"
                          checked={executeQuery}
                          onChange={(e) => setExecuteQuery(e.target.checked)}
                          className="rounded"
                        />
                        Execute query
                      </label>
                    </div>

                    <div className="flex items-center gap-2">
                      <span className="text-sm text-muted-foreground">Ctrl+Enter to run</span>
                      <Button onClick={handleAskQuery} disabled={isQuerying || !question.trim()}>
                        {isQuerying ? (
                          <>
                            <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                            Processing...
                          </>
                        ) : (
                          <>
                            <Sparkles className="h-4 w-4 mr-2" />
                            Ask
                          </>
                        )}
                      </Button>
                    </div>
                  </div>
                </div>
              </Card>

              {/* Error Display */}
              {queryError && (
                <Card className="p-4 border-red-500/50 bg-red-500/10">
                  <div className="flex items-start gap-2 text-red-500">
                    <AlertTriangle className="h-5 w-5 mt-0.5" />
                    <div>
                      <h4 className="font-semibold">Error</h4>
                      <p className="text-sm">{queryError}</p>
                    </div>
                  </div>
                </Card>
              )}

              {/* Results */}
              {queryResult && (
                <div className="space-y-4">
                  {/* Explanation Card */}
                  <Card className="p-4">
                    <div className="space-y-4">
                      <div className="flex items-start justify-between">
                        <div className="flex-1">
                          <h4 className="font-semibold flex items-center gap-2">
                            <Lightbulb className="h-4 w-4 text-yellow-500" />
                            Explanation
                          </h4>
                          <p className="text-muted-foreground mt-1">{queryResult.explanation}</p>
                        </div>
                        <Badge
                          variant="outline"
                          className={queryResult.confidence >= 0.8 ? 'bg-green-500/10 text-green-500' : 'bg-yellow-500/10 text-yellow-500'}
                        >
                          {Math.round(queryResult.confidence * 100)}% confidence
                        </Badge>
                      </div>

                      {/* Generated SQL */}
                      <div>
                        <div className="flex items-center justify-between mb-2">
                          <h4 className="font-semibold flex items-center gap-2">
                            <Code className="h-4 w-4" />
                            Generated SQL
                          </h4>
                          <Button variant="ghost" size="sm" onClick={() => copySql(queryResult.sql)}>
                            {copied ? (
                              <CheckCircle className="h-4 w-4 text-green-500" />
                            ) : (
                              <Copy className="h-4 w-4" />
                            )}
                          </Button>
                        </div>
                        <pre className="bg-muted p-3 rounded-lg text-sm font-mono overflow-x-auto">
                          {queryResult.sql}
                        </pre>
                      </div>

                      {/* Topics & Cost */}
                      <div className="flex items-center gap-4 text-sm">
                        <span className="flex items-center gap-1">
                          <Database className="h-4 w-4" />
                          Topics: {queryResult.topicsUsed.join(', ')}
                        </span>
                        {queryResult.costEstimate && (
                          <>
                            <Badge variant="outline" className={getCostTierColor(queryResult.costEstimate.costTier)}>
                              {queryResult.costEstimate.costTier} cost
                            </Badge>
                            <span className="text-muted-foreground">
                              ~{queryResult.costEstimate.estimatedRows.toLocaleString()} rows
                            </span>
                          </>
                        )}
                      </div>

                      {/* Suggestions */}
                      {queryResult.suggestions.length > 0 && (
                        <div className="border-t pt-3">
                          <h5 className="text-sm font-medium mb-2">Suggestions</h5>
                          <ul className="text-sm text-muted-foreground space-y-1">
                            {queryResult.suggestions.map((s, i) => (
                              <li key={i} className="flex items-start gap-2">
                                <ChevronRight className="h-4 w-4 mt-0.5" />
                                {s}
                              </li>
                            ))}
                          </ul>
                        </div>
                      )}
                    </div>
                  </Card>

                  {/* Query Results */}
                  {queryResult.results && (
                    <Card className="p-4">
                      <div className="space-y-4">
                        <div className="flex items-center justify-between">
                          <h3 className="text-lg font-semibold">Results</h3>
                          <div className="flex items-center gap-4 text-sm text-muted-foreground">
                            <span className="flex items-center gap-1">
                              <Database className="h-4 w-4" />
                              {queryResult.results.rowCount} rows
                              {queryResult.results.truncated && ' (truncated)'}
                            </span>
                            <span className="flex items-center gap-1">
                              <Clock className="h-4 w-4" />
                              {queryResult.results.executionTimeMs}ms
                            </span>
                          </div>
                        </div>

                        <div className="border rounded-lg overflow-hidden">
                          <div className="max-h-[400px] overflow-auto">
                            <Table>
                              <TableHeader className="sticky top-0 bg-background z-10">
                                <TableRow>
                                  {queryResult.results.columns.map((col, i) => (
                                    <TableHead key={i} className="font-mono text-xs">
                                      {col}
                                    </TableHead>
                                  ))}
                                </TableRow>
                              </TableHeader>
                              <TableBody>
                                {queryResult.results.rows.length === 0 ? (
                                  <TableRow>
                                    <TableCell
                                      colSpan={queryResult.results.columns.length}
                                      className="text-center text-muted-foreground py-8"
                                    >
                                      No results
                                    </TableCell>
                                  </TableRow>
                                ) : (
                                  queryResult.results.rows.map((row, rowIdx) => (
                                    <TableRow key={rowIdx}>
                                      {row.map((cell, cellIdx) => (
                                        <TableCell key={cellIdx} className="font-mono text-xs">
                                          {cell === null ? (
                                            <span className="text-muted-foreground">NULL</span>
                                          ) : typeof cell === 'object' ? (
                                            <span className="text-xs">{JSON.stringify(cell)}</span>
                                          ) : (
                                            String(cell)
                                          )}
                                        </TableCell>
                                      ))}
                                    </TableRow>
                                  ))
                                )}
                              </TableBody>
                            </Table>
                          </div>
                        </div>
                      </div>
                    </Card>
                  )}
                </div>
              )}
            </div>

            {/* Sidebar */}
            <div className="space-y-4">
              <Card className="p-4">
                <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                  <Zap className="h-4 w-4" />
                  Quick Start
                </h3>
                <div className="space-y-2">
                  {EXAMPLE_PROMPTS.map((prompt, i) => (
                    <Button
                      key={i}
                      variant="ghost"
                      className="w-full justify-start text-left h-auto py-2 text-sm"
                      onClick={() => loadExample(prompt)}
                    >
                      <span className="truncate">{prompt}</span>
                    </Button>
                  ))}
                </div>
              </Card>

              <Card className="p-4">
                <h3 className="text-lg font-semibold mb-4">Tips</h3>
                <ul className="text-sm text-muted-foreground space-y-2">
                  <li className="flex items-start gap-2">
                    <ChevronRight className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    Be specific about what you want
                  </li>
                  <li className="flex items-start gap-2">
                    <ChevronRight className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    Mention topic names for accuracy
                  </li>
                  <li className="flex items-start gap-2">
                    <ChevronRight className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    Specify field names if you know them
                  </li>
                  <li className="flex items-start gap-2">
                    <ChevronRight className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    Use history to refine queries
                  </li>
                </ul>
              </Card>
            </div>
          </div>
        </TabsContent>

        {/* Schema Inference Tab */}
        <TabsContent value="schema" className="space-y-6">
          <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
            <div className="lg:col-span-3 space-y-4">
              <Card className="p-4">
                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <h3 className="text-lg font-semibold flex items-center gap-2">
                      <FileSearch className="h-5 w-5" />
                      Schema Inference
                    </h3>
                    <Badge variant="outline" className="bg-purple-500/10 text-purple-500">
                      AI-Powered Analysis
                    </Badge>
                  </div>

                  <p className="text-muted-foreground">
                    Automatically analyze your topic&apos;s JSON structure and get AI-generated field descriptions and index recommendations.
                  </p>

                  <div className="flex items-center gap-4">
                    <Select value={schemaTopic} onValueChange={setSchemaTopic}>
                      <SelectTrigger className="w-[300px]">
                        <SelectValue placeholder="Select a topic to analyze" />
                      </SelectTrigger>
                      <SelectContent>
                        {topics.map((topic) => (
                          <SelectItem key={topic.name} value={topic.name}>
                            {topic.name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>

                    <Button onClick={handleInferSchema} disabled={isInferring || !schemaTopic}>
                      {isInferring ? (
                        <>
                          <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                          Analyzing...
                        </>
                      ) : (
                        <>
                          <Sparkles className="h-4 w-4 mr-2" />
                          Analyze Schema
                        </>
                      )}
                    </Button>
                  </div>
                </div>
              </Card>

              {/* Schema Error */}
              {schemaError && (
                <Card className="p-4 border-red-500/50 bg-red-500/10">
                  <div className="flex items-start gap-2 text-red-500">
                    <AlertTriangle className="h-5 w-5 mt-0.5" />
                    <div>
                      <h4 className="font-semibold">Error</h4>
                      <p className="text-sm">{schemaError}</p>
                    </div>
                  </div>
                </Card>
              )}

              {/* Schema Results */}
              {schemaResult && (
                <div className="space-y-4">
                  {/* Summary */}
                  <Card className="p-4">
                    <div className="flex items-start justify-between">
                      <div>
                        <h3 className="text-lg font-semibold">{schemaResult.topic}</h3>
                        {schemaResult.summary && (
                          <p className="text-muted-foreground mt-1">{schemaResult.summary}</p>
                        )}
                        <p className="text-sm text-muted-foreground mt-2">
                          Analyzed {schemaResult.sampleCount} messages, found {schemaResult.fields.length} fields
                        </p>
                      </div>
                      <Badge
                        variant="outline"
                        className={schemaResult.confidence >= 0.8 ? 'bg-green-500/10 text-green-500' : 'bg-yellow-500/10 text-yellow-500'}
                      >
                        {Math.round(schemaResult.confidence * 100)}% confidence
                      </Badge>
                    </div>
                  </Card>

                  {/* Fields */}
                  <Card className="p-4">
                    <h3 className="text-lg font-semibold mb-4">Fields</h3>
                    <div className="border rounded-lg overflow-hidden">
                      <Table>
                        <TableHeader>
                          <TableRow>
                            <TableHead>Field Path</TableHead>
                            <TableHead>Type</TableHead>
                            <TableHead>SQL Type</TableHead>
                            <TableHead>Nullable</TableHead>
                            <TableHead>Occurrence</TableHead>
                            <TableHead>Description</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {schemaResult.fields.map((field, i) => (
                            <TableRow key={i}>
                              <TableCell className="font-mono text-sm">{field.path}</TableCell>
                              <TableCell>
                                <Badge variant="outline">{field.jsonType}</Badge>
                              </TableCell>
                              <TableCell className="text-muted-foreground">{field.suggestedSqlType}</TableCell>
                              <TableCell>
                                {field.nullable ? (
                                  <Badge variant="outline" className="bg-yellow-500/10 text-yellow-500">nullable</Badge>
                                ) : (
                                  <Badge variant="outline" className="bg-green-500/10 text-green-500">required</Badge>
                                )}
                              </TableCell>
                              <TableCell>{Math.round(field.occurrenceRate * 100)}%</TableCell>
                              <TableCell className="text-sm text-muted-foreground max-w-[300px] truncate">
                                {field.description || '-'}
                              </TableCell>
                            </TableRow>
                          ))}
                        </TableBody>
                      </Table>
                    </div>
                  </Card>

                  {/* Index Recommendations */}
                  {schemaResult.indexRecommendations.length > 0 && (
                    <Card className="p-4">
                      <h3 className="text-lg font-semibold mb-4">Index Recommendations</h3>
                      <div className="space-y-3">
                        {schemaResult.indexRecommendations.map((rec, i) => (
                          <div key={i} className="border rounded-lg p-3">
                            <div className="flex items-center justify-between mb-2">
                              <span className="font-mono font-medium">{rec.field}</span>
                              <Badge variant="outline" className={getPriorityColor(rec.priority)}>
                                {rec.priority} priority
                              </Badge>
                            </div>
                            <p className="text-sm text-muted-foreground mb-2">{rec.reason}</p>
                            <pre className="bg-muted p-2 rounded text-xs font-mono overflow-x-auto">
                              {rec.exampleQuery}
                            </pre>
                          </div>
                        ))}
                      </div>
                    </Card>
                  )}
                </div>
              )}
            </div>

            {/* Sidebar */}
            <div className="space-y-4">
              <Card className="p-4">
                <h3 className="text-lg font-semibold mb-4">What This Does</h3>
                <ul className="text-sm text-muted-foreground space-y-2">
                  <li className="flex items-start gap-2">
                    <ChevronRight className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    Samples messages from your topic
                  </li>
                  <li className="flex items-start gap-2">
                    <ChevronRight className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    Detects JSON field types
                  </li>
                  <li className="flex items-start gap-2">
                    <ChevronRight className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    Identifies nullable fields
                  </li>
                  <li className="flex items-start gap-2">
                    <ChevronRight className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    Generates AI descriptions
                  </li>
                  <li className="flex items-start gap-2">
                    <ChevronRight className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    Recommends indexes
                  </li>
                </ul>
              </Card>
            </div>
          </div>
        </TabsContent>

        {/* History Tab */}
        <TabsContent value="history" className="space-y-6">
          <Card className="p-4">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold flex items-center gap-2">
                <History className="h-5 w-5" />
                Query History
              </h3>
              <Button variant="outline" size="sm" onClick={loadHistory} disabled={isLoadingHistory}>
                {isLoadingHistory ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <RefreshCw className="h-4 w-4" />
                )}
              </Button>
            </div>

            {history.length === 0 ? (
              <div className="text-center py-12 text-muted-foreground">
                <History className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <p>No queries yet</p>
                <p className="text-sm">Ask a question to get started</p>
              </div>
            ) : (
              <div className="space-y-3">
                {history.map((entry) => (
                  <div key={entry.id} className="border rounded-lg p-4">
                    <div className="flex items-start justify-between">
                      <div className="flex-1">
                        <p className="font-medium">{entry.question}</p>
                        <p className="text-sm text-muted-foreground mt-1">{entry.explanation}</p>
                        <div className="flex items-center gap-3 mt-2 text-xs text-muted-foreground">
                          <span>{formatDate(entry.createdAt)}</span>
                          <span>Topics: {entry.topicsUsed.join(', ')}</span>
                          {entry.refinementCount > 0 && (
                            <Badge variant="outline" className="text-xs">
                              Refined {entry.refinementCount}x
                            </Badge>
                          )}
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleUseHistoryEntry(entry)}
                        >
                          <Play className="h-4 w-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDeleteQuery(entry.id)}
                        >
                          <Trash2 className="h-4 w-4 text-red-500" />
                        </Button>
                      </div>
                    </div>
                    <pre className="bg-muted p-2 rounded text-xs font-mono mt-3 overflow-x-auto">
                      {entry.sql}
                    </pre>
                  </div>
                ))}
              </div>
            )}
          </Card>
        </TabsContent>
      </Tabs>
    </DashboardLayout>
  );
}
