'use client';

import { useState, useRef, useCallback } from 'react';
import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
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
  Play,
  Download,
  Clock,
  Database,
  Copy,
  CheckCircle,
  AlertTriangle,
  Loader2,
} from 'lucide-react';

// API types
interface ColumnInfo {
  name: string;
  dataType: string;
}

interface SqlQueryResponse {
  columns: ColumnInfo[];
  rows: (string | number | null)[][];
  rowCount: number;
  executionTimeMs: number;
  truncated: boolean;
}

interface SqlErrorResponse {
  error: string;
  message: string;
}

// Example queries
const EXAMPLE_QUERIES = [
  {
    name: 'List Topics',
    query: 'SHOW TOPICS;',
  },
  {
    name: 'Select All Messages',
    query: 'SELECT * FROM orders LIMIT 100;',
  },
  {
    name: 'Filter by Key',
    query: "SELECT * FROM orders WHERE key = 'customer-123' LIMIT 50;",
  },
  {
    name: 'Filter by Partition & Offset',
    query: 'SELECT * FROM orders WHERE partition = 0 AND offset >= 0 AND offset < 100;',
  },
  {
    name: 'Count Messages',
    query: 'SELECT COUNT(*) FROM orders WHERE partition = 0;',
  },
  {
    name: 'Describe Topic',
    query: 'DESCRIBE orders;',
  },
  {
    name: 'JSON Extract',
    query: `SELECT
  key,
  offset,
  json_extract(value, '$.customer_id') as customer_id,
  json_extract(value, '$.amount') as amount
FROM orders
LIMIT 50;`,
  },
];

export default function SqlWorkbenchPage() {
  const [query, setQuery] = useState('SELECT * FROM orders LIMIT 100;');
  const [result, setResult] = useState<SqlQueryResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [copied, setCopied] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const executeQuery = useCallback(async () => {
    if (!query.trim()) return;

    setIsLoading(true);
    setError(null);
    setResult(null);

    try {
      const response = await fetch('/api/v1/sql', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query, timeoutMs: 30000 }),
      });

      if (!response.ok) {
        const errorData: SqlErrorResponse = await response.json();
        throw new Error(errorData.message || 'Query failed');
      }

      const data: SqlQueryResponse = await response.json();
      setResult(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to execute query');
    } finally {
      setIsLoading(false);
    }
  }, [query]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        executeQuery();
      }
    },
    [executeQuery]
  );

  const copyResults = useCallback(() => {
    if (!result) return;

    const header = result.columns.map((c) => c.name).join('\t');
    const rows = result.rows.map((row) => row.map((v) => String(v ?? '')).join('\t')).join('\n');
    const text = `${header}\n${rows}`;

    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [result]);

  const exportCsv = useCallback(() => {
    if (!result) return;

    const header = result.columns.map((c) => `"${c.name}"`).join(',');
    const rows = result.rows
      .map((row) =>
        row.map((v) => (typeof v === 'string' ? `"${v.replace(/"/g, '""')}"` : String(v ?? ''))).join(',')
      )
      .join('\n');
    const csv = `${header}\n${rows}`;

    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'query-results.csv';
    a.click();
    URL.revokeObjectURL(url);
  }, [result]);

  const loadExample = useCallback((exampleQuery: string) => {
    setQuery(exampleQuery);
    setError(null);
    setResult(null);
  }, []);

  return (
    <DashboardLayout
      title="SQL Workbench"
      description="Query messages with SQL"
    >
      <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
        {/* Query Editor */}
        <div className="lg:col-span-3 space-y-4">
          <Card className="p-4">
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <h3 className="text-lg font-semibold">Query Editor</h3>
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <span>Ctrl+Enter to execute</span>
                </div>
              </div>
              <Textarea
                ref={textareaRef}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder="Enter SQL query..."
                className="font-mono text-sm min-h-[200px] resize-y"
              />
              <div className="flex items-center gap-2">
                <Button onClick={executeQuery} disabled={isLoading || !query.trim()}>
                  {isLoading ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      Executing...
                    </>
                  ) : (
                    <>
                      <Play className="h-4 w-4 mr-2" />
                      Execute
                    </>
                  )}
                </Button>
                {result && (
                  <>
                    <Button variant="outline" onClick={copyResults}>
                      {copied ? (
                        <>
                          <CheckCircle className="h-4 w-4 mr-2" />
                          Copied
                        </>
                      ) : (
                        <>
                          <Copy className="h-4 w-4 mr-2" />
                          Copy
                        </>
                      )}
                    </Button>
                    <Button variant="outline" onClick={exportCsv}>
                      <Download className="h-4 w-4 mr-2" />
                      Export CSV
                    </Button>
                  </>
                )}
              </div>
            </div>
          </Card>

          {/* Error Display */}
          {error && (
            <Card className="p-4 border-red-500/50 bg-red-500/10">
              <div className="flex items-start gap-2 text-red-500">
                <AlertTriangle className="h-5 w-5 mt-0.5" />
                <div>
                  <h4 className="font-semibold">Query Error</h4>
                  <p className="text-sm">{error}</p>
                </div>
              </div>
            </Card>
          )}

          {/* Results */}
          {result && (
            <Card className="p-4">
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-4">
                    <h3 className="text-lg font-semibold">Results</h3>
                    <div className="flex items-center gap-4 text-sm text-muted-foreground">
                      <span className="flex items-center gap-1">
                        <Database className="h-4 w-4" />
                        {result.rowCount} rows
                        {result.truncated && ' (truncated)'}
                      </span>
                      <span className="flex items-center gap-1">
                        <Clock className="h-4 w-4" />
                        {result.executionTimeMs}ms
                      </span>
                    </div>
                  </div>
                </div>

                <div className="border rounded-lg overflow-hidden">
                  <div className="max-h-[500px] overflow-auto">
                    <Table>
                      <TableHeader className="sticky top-0 bg-background z-10">
                        <TableRow>
                          {result.columns.map((col, i) => (
                            <TableHead key={i} className="font-mono text-xs">
                              {col.name}
                              <span className="text-muted-foreground ml-1">({col.dataType})</span>
                            </TableHead>
                          ))}
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {result.rows.length === 0 ? (
                          <TableRow>
                            <TableCell
                              colSpan={result.columns.length}
                              className="text-center text-muted-foreground py-8"
                            >
                              No results
                            </TableCell>
                          </TableRow>
                        ) : (
                          result.rows.map((row, rowIdx) => (
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

        {/* Examples Sidebar */}
        <div className="space-y-4">
          <Card className="p-4">
            <h3 className="text-lg font-semibold mb-4">Example Queries</h3>
            <div className="space-y-2">
              {EXAMPLE_QUERIES.map((example, i) => (
                <Button
                  key={i}
                  variant="ghost"
                  className="w-full justify-start text-left h-auto py-2"
                  onClick={() => loadExample(example.query)}
                >
                  <span className="truncate">{example.name}</span>
                </Button>
              ))}
            </div>
          </Card>

          <Card className="p-4">
            <h3 className="text-lg font-semibold mb-4">Quick Reference</h3>
            <div className="space-y-3 text-sm">
              <div>
                <h4 className="font-medium">Supported Commands</h4>
                <ul className="text-muted-foreground mt-1 space-y-1">
                  <li>SHOW TOPICS</li>
                  <li>DESCRIBE topic_name</li>
                  <li>SELECT ... FROM topic</li>
                  <li>SELECT COUNT(*) FROM topic</li>
                </ul>
              </div>
              <div>
                <h4 className="font-medium">Built-in Columns</h4>
                <ul className="text-muted-foreground mt-1 space-y-1">
                  <li>topic, partition, offset</li>
                  <li>key, value, timestamp</li>
                </ul>
              </div>
              <div>
                <h4 className="font-medium">Filters</h4>
                <ul className="text-muted-foreground mt-1 space-y-1">
                  <li>WHERE key = &apos;value&apos;</li>
                  <li>WHERE partition = 0</li>
                  <li>WHERE offset &gt;= 100</li>
                  <li>WHERE timestamp &gt;= &apos;...&apos;</li>
                </ul>
              </div>
              <div>
                <h4 className="font-medium">JSON Functions</h4>
                <ul className="text-muted-foreground mt-1 space-y-1">
                  <li>json_extract(value, &apos;$.field&apos;)</li>
                </ul>
              </div>
            </div>
          </Card>

          <Card className="p-4">
            <h3 className="text-lg font-semibold mb-2">Limitations</h3>
            <ul className="text-sm text-muted-foreground space-y-1">
              <li>Max 10,000 rows per query</li>
              <li>No GROUP BY / aggregations</li>
              <li>No JOINs</li>
              <li>Read-only queries</li>
              <li>30 second timeout</li>
            </ul>
          </Card>
        </div>
      </div>
    </DashboardLayout>
  );
}
