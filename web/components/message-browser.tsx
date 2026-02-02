'use client';

import { useState, useMemo, useCallback } from 'react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Search, Download, ChevronLeft, ChevronRight, Eye, Copy, Check, Loader2, RefreshCw } from 'lucide-react';
import { formatBytes, formatDate, downloadJSON } from '@/lib/utils';
import { Message } from '@/lib/types';
import { useTopicMessagesInfinite, useTopicPartitions } from '@/lib/hooks/use-topics';

// Helper to decode value that could be string or byte array
function decodeValue(value: string | number[] | null | undefined): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'string') return value;
  if (Array.isArray(value)) {
    try {
      return new TextDecoder().decode(new Uint8Array(value));
    } catch {
      return '';
    }
  }
  return String(value);
}

// Helper to get byte length of value
function getValueSize(value: string | number[] | null | undefined): number {
  if (value === null || value === undefined) return 0;
  if (typeof value === 'string') return new TextEncoder().encode(value).length;
  if (Array.isArray(value)) return value.length;
  return 0;
}

interface MessageBrowserProps {
  topicName: string;
  messages?: Message[];  // Optional - for backward compatibility
  isLoading?: boolean;
}

export function MessageBrowser({ topicName, messages: initialMessages, isLoading: initialLoading }: MessageBrowserProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [currentPage, setCurrentPage] = useState(1);
  const [selectedMessage, setSelectedMessage] = useState<Message | null>(null);
  const [copied, setCopied] = useState(false);
  const [selectedPartition, setSelectedPartition] = useState<string>('all');
  const pageSize = 50;

  // Get partitions for filter dropdown
  const { data: partitions } = useTopicPartitions(topicName);

  // Use infinite query for server-side pagination
  const partitionFilter = selectedPartition === 'all' ? undefined : parseInt(selectedPartition);
  const {
    data,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    isLoading: queryLoading,
    refetch,
  } = useTopicMessagesInfinite(topicName, { partition: partitionFilter, pageSize: 200 });

  // Combine all pages of messages
  const allMessages = useMemo(() => {
    if (data?.pages) {
      return data.pages.flatMap(page => page.messages);
    }
    return initialMessages || [];
  }, [data, initialMessages]);

  const isLoading = queryLoading || initialLoading;
  const totalLoaded = allMessages.length;

  // Filter messages by search query
  const filteredMessages = useMemo(() => {
    if (!searchQuery) return allMessages;

    const query = searchQuery.toLowerCase();
    return allMessages.filter((msg) => {
      const keyStr = decodeValue(msg.key);
      const valueStr = decodeValue(msg.value);

      return (
        keyStr.toLowerCase().includes(query) ||
        valueStr.toLowerCase().includes(query) ||
        msg.partition.toString().includes(query) ||
        msg.offset.toString().includes(query)
      );
    });
  }, [allMessages, searchQuery]);

  // Paginate messages
  const paginatedMessages = useMemo(() => {
    const start = (currentPage - 1) * pageSize;
    const end = start + pageSize;
    return filteredMessages.slice(start, end);
  }, [filteredMessages, currentPage]);

  const totalPages = Math.ceil(filteredMessages.length / pageSize);

  const handleCopyMessage = (message: Message) => {
    const valueStr = decodeValue(message.value);
    navigator.clipboard.writeText(valueStr);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleDownloadMessages = () => {
    const exportData = filteredMessages.map((msg) => ({
      partition: msg.partition,
      offset: msg.offset,
      timestamp: new Date(msg.timestamp).toISOString(),
      key: decodeValue(msg.key) || null,
      value: decodeValue(msg.value) || null,
      headers: msg.headers,
    }));

    downloadJSON(exportData, `${topicName}-messages-${Date.now()}.json`);
  };

  const formatMessageValue = (value: string | number[] | null | undefined) => {
    const str = decodeValue(value);
    if (!str) return '-';

    // Try to parse as JSON for pretty display
    try {
      const parsed = JSON.parse(str);
      return JSON.stringify(parsed, null, 2);
    } catch {
      return str;
    }
  };

  const truncateValue = (value: string | number[] | null | undefined, maxLength: number = 100) => {
    const str = decodeValue(value);
    if (!str) return '-';
    return str.length > maxLength ? str.substring(0, maxLength) + '...' : str;
  };

  const handlePartitionChange = (value: string) => {
    setSelectedPartition(value);
    setCurrentPage(1);
  };

  const handleRefresh = useCallback(() => {
    refetch();
  }, [refetch]);

  return (
    <div className="space-y-4">
      {/* Search, Filter, and Actions */}
      <div className="flex items-center gap-4 flex-wrap">
        <div className="relative flex-1 min-w-[200px]">
          <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            placeholder="Search messages by key, value, partition, or offset..."
            value={searchQuery}
            onChange={(e) => {
              setSearchQuery(e.target.value);
              setCurrentPage(1); // Reset to first page on search
            }}
            className="pl-10"
          />
        </div>

        {/* Partition Filter */}
        <Select value={selectedPartition} onValueChange={handlePartitionChange}>
          <SelectTrigger className="w-[180px]">
            <SelectValue placeholder="All Partitions" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All Partitions</SelectItem>
            {partitions?.map((p) => (
              <SelectItem key={p.id} value={p.id.toString()}>
                Partition {p.id}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Button variant="outline" size="icon" onClick={handleRefresh} disabled={isLoading}>
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </Button>

        <Button variant="outline" onClick={handleDownloadMessages} disabled={filteredMessages.length === 0}>
          <Download className="mr-2 h-4 w-4" />
          Export ({filteredMessages.length})
        </Button>
      </div>

      {/* Stats Bar */}
      <div className="flex items-center gap-4 text-sm text-muted-foreground">
        <span>
          <strong>{totalLoaded.toLocaleString()}</strong> messages loaded
        </span>
        {searchQuery && (
          <span>
            <strong>{filteredMessages.length.toLocaleString()}</strong> matching search
          </span>
        )}
        {hasNextPage && (
          <Button
            variant="link"
            size="sm"
            onClick={() => fetchNextPage()}
            disabled={isFetchingNextPage}
            className="text-primary"
          >
            {isFetchingNextPage ? (
              <>
                <Loader2 className="mr-2 h-3 w-3 animate-spin" />
                Loading more...
              </>
            ) : (
              'Load more messages'
            )}
          </Button>
        )}
      </div>

      {/* Messages Table */}
      <Card>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-[80px]">Partition</TableHead>
              <TableHead className="w-[100px]">Offset</TableHead>
              <TableHead className="w-[180px]">Timestamp</TableHead>
              <TableHead className="w-[150px]">Key</TableHead>
              <TableHead>Value</TableHead>
              <TableHead className="w-[80px]">Size</TableHead>
              <TableHead className="w-[100px]">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading ? (
              <TableRow>
                <TableCell colSpan={7} className="text-center py-8">
                  <div className="flex items-center justify-center gap-2">
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Loading messages...
                  </div>
                </TableCell>
              </TableRow>
            ) : paginatedMessages.length === 0 ? (
              <TableRow>
                <TableCell colSpan={7} className="text-center text-muted-foreground py-8">
                  {searchQuery ? 'No messages match your search' : 'No messages found'}
                </TableCell>
              </TableRow>
            ) : (
              paginatedMessages.map((message) => (
                <TableRow key={`${message.partition}-${message.offset}`}>
                  <TableCell>
                    <Badge variant="outline">{message.partition}</Badge>
                  </TableCell>
                  <TableCell className="font-mono text-xs">{message.offset}</TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {formatDate(message.timestamp)}
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {message.key ? truncateValue(message.key, 30) : '-'}
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {truncateValue(message.value, 80)}
                  </TableCell>
                  <TableCell className="text-xs">
                    {formatBytes(getValueSize(message.value))}
                  </TableCell>
                  <TableCell>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setSelectedMessage(message)}
                    >
                      <Eye className="h-4 w-4" />
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </Card>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between">
          <div className="text-sm text-muted-foreground">
            Showing {(currentPage - 1) * pageSize + 1} to{' '}
            {Math.min(currentPage * pageSize, filteredMessages.length)} of {filteredMessages.length} messages
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setCurrentPage((p) => Math.max(1, p - 1))}
              disabled={currentPage === 1}
            >
              <ChevronLeft className="h-4 w-4" />
              Previous
            </Button>
            <div className="text-sm">
              Page {currentPage} of {totalPages}
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setCurrentPage((p) => Math.min(totalPages, p + 1))}
              disabled={currentPage === totalPages}
            >
              Next
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        </div>
      )}

      {/* Load More at End */}
      {hasNextPage && currentPage === totalPages && filteredMessages.length > 0 && (
        <div className="flex justify-center pt-4">
          <Button
            variant="outline"
            onClick={() => fetchNextPage()}
            disabled={isFetchingNextPage}
          >
            {isFetchingNextPage ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Loading more...
              </>
            ) : (
              'Load more messages'
            )}
          </Button>
        </div>
      )}

      {/* Message Detail Dialog */}
      <Dialog open={!!selectedMessage} onOpenChange={() => setSelectedMessage(null)}>
        <DialogContent className="max-w-3xl max-h-[80vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Message Details</DialogTitle>
            <DialogDescription>
              Partition {selectedMessage?.partition} • Offset {selectedMessage?.offset}
            </DialogDescription>
          </DialogHeader>

          {selectedMessage && (
            <div className="space-y-4">
              {/* Metadata */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <p className="text-sm font-medium text-muted-foreground">Timestamp</p>
                  <p className="mt-1 font-mono text-sm">
                    {formatDate(selectedMessage.timestamp)}
                  </p>
                </div>
                <div>
                  <p className="text-sm font-medium text-muted-foreground">Size</p>
                  <p className="mt-1 font-mono text-sm">
                    {formatBytes(getValueSize(selectedMessage.value))}
                  </p>
                </div>
              </div>

              {/* Key */}
              <div>
                <p className="text-sm font-medium text-muted-foreground mb-2">Key</p>
                <Card className="p-4">
                  <pre className="font-mono text-xs overflow-x-auto">
                    {decodeValue(selectedMessage.key) || 'null'}
                  </pre>
                </Card>
              </div>

              {/* Value */}
              <div>
                <div className="flex items-center justify-between mb-2">
                  <p className="text-sm font-medium text-muted-foreground">Value</p>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => handleCopyMessage(selectedMessage)}
                  >
                    {copied ? (
                      <>
                        <Check className="mr-2 h-4 w-4" />
                        Copied
                      </>
                    ) : (
                      <>
                        <Copy className="mr-2 h-4 w-4" />
                        Copy
                      </>
                    )}
                  </Button>
                </div>
                <Card className="p-4">
                  <pre className="font-mono text-xs overflow-x-auto whitespace-pre-wrap">
                    {formatMessageValue(selectedMessage.value)}
                  </pre>
                </Card>
              </div>

              {/* Headers */}
              {selectedMessage.headers && Object.keys(selectedMessage.headers).length > 0 && (
                <div>
                  <p className="text-sm font-medium text-muted-foreground mb-2">Headers</p>
                  <Card className="p-4">
                    <pre className="font-mono text-xs">
                      {JSON.stringify(selectedMessage.headers, null, 2)}
                    </pre>
                  </Card>
                </div>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
