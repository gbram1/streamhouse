'use client';

import { useState, useMemo } from 'react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
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
import { Search, Download, ChevronLeft, ChevronRight, Eye, Copy, Check } from 'lucide-react';
import { formatBytes, formatDate, downloadJSON } from '@/lib/utils';
import { Message } from '@/lib/types';

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
  messages: Message[];
  isLoading?: boolean;
}

export function MessageBrowser({ topicName, messages, isLoading }: MessageBrowserProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [currentPage, setCurrentPage] = useState(1);
  const [selectedMessage, setSelectedMessage] = useState<Message | null>(null);
  const [copied, setCopied] = useState(false);
  const pageSize = 50;

  // Filter messages by search query
  const filteredMessages = useMemo(() => {
    if (!searchQuery) return messages;

    const query = searchQuery.toLowerCase();
    return messages.filter((msg) => {
      const keyStr = decodeValue(msg.key);
      const valueStr = decodeValue(msg.value);

      return (
        keyStr.toLowerCase().includes(query) ||
        valueStr.toLowerCase().includes(query) ||
        msg.partition.toString().includes(query) ||
        msg.offset.toString().includes(query)
      );
    });
  }, [messages, searchQuery]);

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

  return (
    <div className="space-y-4">
      {/* Search and Actions */}
      <div className="flex items-center gap-4">
        <div className="relative flex-1">
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
        <Button variant="outline" onClick={handleDownloadMessages} disabled={filteredMessages.length === 0}>
          <Download className="mr-2 h-4 w-4" />
          Export ({filteredMessages.length})
        </Button>
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
                <TableCell colSpan={7} className="text-center">
                  Loading messages...
                </TableCell>
              </TableRow>
            ) : paginatedMessages.length === 0 ? (
              <TableRow>
                <TableCell colSpan={7} className="text-center text-muted-foreground">
                  {searchQuery ? 'No messages match your search' : 'No messages found'}
                </TableCell>
              </TableRow>
            ) : (
              paginatedMessages.map((message, idx) => (
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
