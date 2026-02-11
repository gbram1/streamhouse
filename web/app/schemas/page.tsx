'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
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
import { useSchemaSubjects } from '@/lib/hooks/use-schemas';
import { FileCode2, Search, Plus, Filter } from 'lucide-react';
import Link from 'next/link';
import { useState } from 'react';

export default function SchemasPage() {
  const { data: subjects, isLoading } = useSchemaSubjects();
  const [searchQuery, setSearchQuery] = useState('');
  const [formatFilter, setFormatFilter] = useState<string>('all');

  const filteredSubjects = subjects?.filter((subject) => {
    const matchesSearch = subject.subject
      .toLowerCase()
      .includes(searchQuery.toLowerCase());
    const matchesFormat =
      formatFilter === 'all' || subject.schemaType === formatFilter;
    return matchesSearch && matchesFormat;
  });

  const avroCount = subjects?.filter((s) => s.schemaType === 'AVRO').length || 0;
  const protobufCount = subjects?.filter((s) => s.schemaType === 'PROTOBUF').length || 0;
  const jsonCount = subjects?.filter((s) => s.schemaType === 'JSON').length || 0;

  return (
    <DashboardLayout
      title="Schema Registry"
      description="Manage schema evolution and compatibility"
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Schemas</h3>
            <FileCode2 className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : subjects?.length || 0}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Avro Schemas</h3>
            <FileCode2 className="h-5 w-5 text-blue-500" />
          </div>
          <div className="mt-2 text-3xl font-bold">{isLoading ? '...' : avroCount}</div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Protobuf Schemas</h3>
            <FileCode2 className="h-5 w-5 text-purple-500" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : protobufCount}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">JSON Schemas</h3>
            <FileCode2 className="h-5 w-5 text-green-500" />
          </div>
          <div className="mt-2 text-3xl font-bold">{isLoading ? '...' : jsonCount}</div>
        </Card>
      </div>

      {/* Schemas Table */}
      <Card className="mt-6">
        <div className="p-6">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <div className="relative">
                <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  placeholder="Search schemas..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-10 w-80"
                />
              </div>
              <Select value={formatFilter} onValueChange={setFormatFilter}>
                <SelectTrigger className="w-40">
                  <Filter className="mr-2 h-4 w-4" />
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All Formats</SelectItem>
                  <SelectItem value="AVRO">Avro</SelectItem>
                  <SelectItem value="PROTOBUF">Protobuf</SelectItem>
                  <SelectItem value="JSON">JSON</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <Link href="/schemas/new">
              <Button>
                <Plus className="mr-2 h-4 w-4" />
                Register Schema
              </Button>
            </Link>
          </div>

          <div className="mt-6">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Subject</TableHead>
                  <TableHead>Format</TableHead>
                  <TableHead>Latest Version</TableHead>
                  <TableHead>Version Count</TableHead>
                  <TableHead>Compatibility</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {isLoading ? (
                  <TableRow>
                    <TableCell colSpan={6} className="text-center">
                      Loading schemas...
                    </TableCell>
                  </TableRow>
                ) : filteredSubjects?.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={6} className="text-center text-muted-foreground">
                      {searchQuery || formatFilter !== 'all'
                        ? 'No schemas match your filters'
                        : 'No schemas registered yet. Register your first schema to get started.'}
                    </TableCell>
                  </TableRow>
                ) : (
                  filteredSubjects?.map((subject) => (
                    <TableRow key={subject.subject}>
                      <TableCell className="font-medium">
                        <Link
                          href={`/schemas/${subject.latestSchemaId}`}
                          className="hover:text-primary"
                        >
                          {subject.subject}
                        </Link>
                      </TableCell>
                      <TableCell>
                        <Badge
                          variant="outline"
                          className={
                            subject.schemaType === 'AVRO'
                              ? 'border-blue-500 text-blue-500'
                              : subject.schemaType === 'PROTOBUF'
                              ? 'border-purple-500 text-purple-500'
                              : 'border-green-500 text-green-500'
                          }
                        >
                          {subject.schemaType}
                        </Badge>
                      </TableCell>
                      <TableCell>v{subject.latestVersion}</TableCell>
                      <TableCell>{subject.versionCount}</TableCell>
                      <TableCell>
                        <Badge variant="secondary">{subject.compatibilityMode}</Badge>
                      </TableCell>
                      <TableCell>
                        <Link href={`/schemas/${subject.latestSchemaId}`}>
                          <Button variant="ghost" size="sm">
                            View
                          </Button>
                        </Link>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>
        </div>
      </Card>

      {/* Schema Format Distribution */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Schema Format Distribution</h3>
        {isLoading ? (
          <div className="flex h-32 items-center justify-center text-muted-foreground">
            <p>Loading...</p>
          </div>
        ) : !subjects || subjects.length === 0 ? (
          <div className="flex h-32 items-center justify-center text-muted-foreground">
            <p>Register schemas to see format distribution</p>
          </div>
        ) : (
          <div className="space-y-4">
            {[
              { label: 'Avro', count: avroCount, color: 'bg-blue-500' },
              { label: 'Protobuf', count: protobufCount, color: 'bg-purple-500' },
              { label: 'JSON', count: jsonCount, color: 'bg-green-500' },
            ]
              .filter((f) => f.count > 0)
              .map((format) => (
                <div key={format.label}>
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-sm font-medium">{format.label}</span>
                    <span className="text-sm text-muted-foreground">
                      {format.count} schema{format.count !== 1 ? 's' : ''} ({Math.round((format.count / subjects.length) * 100)}%)
                    </span>
                  </div>
                  <div className="h-2.5 bg-secondary rounded-full overflow-hidden">
                    <div
                      className={`h-full ${format.color} rounded-full transition-all duration-300`}
                      style={{ width: `${(format.count / subjects.length) * 100}%` }}
                    />
                  </div>
                </div>
              ))}
          </div>
        )}
      </Card>
    </DashboardLayout>
  );
}
