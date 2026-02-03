'use client';

import { useParams } from 'next/navigation';
import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useSchema, useSubjectVersions, useSchemaVersion } from '@/lib/hooks/use-schemas';
import { FileCode2, ArrowLeft, Copy, Check } from 'lucide-react';
import Link from 'next/link';
import { useState, useEffect } from 'react';

export default function SchemaDetailPage() {
  const params = useParams();
  const schemaId = Number(params.id);

  const { data: schema, isLoading, error } = useSchema(schemaId);
  const [copied, setCopied] = useState(false);
  const [selectedVersion, setSelectedVersion] = useState<number | null>(null);

  // Once we have the schema, fetch all versions for this subject
  const { data: versions } = useSubjectVersions(schema?.subject || '');

  // If user selects a different version, fetch that specific version
  const { data: versionSchema } = useSchemaVersion(
    schema?.subject || '',
    selectedVersion || 0
  );

  // Set the initial selected version once schema loads
  useEffect(() => {
    if (schema && !selectedVersion) {
      setSelectedVersion(schema.version);
    }
  }, [schema, selectedVersion]);

  // Use the version-specific schema if selected, otherwise the original
  const displaySchema = versionSchema || schema;

  const copyToClipboard = () => {
    if (displaySchema?.schema) {
      navigator.clipboard.writeText(displaySchema.schema);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const formatSchema = (schemaStr: string) => {
    try {
      const parsed = JSON.parse(schemaStr);
      return JSON.stringify(parsed, null, 2);
    } catch {
      return schemaStr;
    }
  };

  const getSchemaTypeColor = (type: string) => {
    switch (type) {
      case 'AVRO':
        return 'border-blue-500 text-blue-500';
      case 'PROTOBUF':
        return 'border-purple-500 text-purple-500';
      case 'JSON':
        return 'border-green-500 text-green-500';
      default:
        return '';
    }
  };

  if (isLoading) {
    return (
      <DashboardLayout title="Schema Details" description="Loading...">
        <Card className="p-6">
          <div className="flex items-center justify-center h-64">
            <p className="text-muted-foreground">Loading schema...</p>
          </div>
        </Card>
      </DashboardLayout>
    );
  }

  if (error || !schema) {
    return (
      <DashboardLayout title="Schema Not Found" description="Error loading schema">
        <Card className="p-6">
          <div className="flex flex-col items-center justify-center h-64 gap-4">
            <p className="text-muted-foreground">
              Schema with ID {schemaId} not found
            </p>
            <Link href="/schemas">
              <Button variant="outline">
                <ArrowLeft className="mr-2 h-4 w-4" />
                Back to Schemas
              </Button>
            </Link>
          </div>
        </Card>
      </DashboardLayout>
    );
  }

  return (
    <DashboardLayout
      title={schema.subject}
      description={`Schema ID: ${schemaId}`}
    >
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <Link href="/schemas">
          <Button variant="outline">
            <ArrowLeft className="mr-2 h-4 w-4" />
            Back to Schemas
          </Button>
        </Link>
      </div>

      {/* Schema Info */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4 mb-6">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Schema ID</h3>
            <FileCode2 className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{displaySchema?.id || schemaId}</div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Version</h3>
          </div>
          <div className="mt-2">
            {versions && versions.length > 1 ? (
              <Select
                value={String(selectedVersion || schema.version)}
                onValueChange={(v) => setSelectedVersion(Number(v))}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {versions.map((v) => (
                    <SelectItem key={v} value={String(v)}>
                      Version {v} {v === schema.version ? '(current)' : ''}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <div className="text-3xl font-bold">v{displaySchema?.version || schema.version}</div>
            )}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Format</h3>
          </div>
          <div className="mt-2">
            <Badge
              variant="outline"
              className={`text-lg px-3 py-1 ${getSchemaTypeColor(displaySchema?.schemaType || schema.schemaType)}`}
            >
              {displaySchema?.schemaType || schema.schemaType}
            </Badge>
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Versions</h3>
          </div>
          <div className="mt-2 text-3xl font-bold">{versions?.length || 1}</div>
        </Card>
      </div>

      {/* Schema Content */}
      <Card className="p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold">Schema Definition</h3>
          <Button variant="outline" size="sm" onClick={copyToClipboard}>
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
        <pre className="bg-muted p-4 rounded-lg overflow-x-auto text-sm">
          <code>{formatSchema(displaySchema?.schema || schema.schema)}</code>
        </pre>
      </Card>

      {/* Compatibility Info */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Compatibility</h3>
        <div className="flex items-center gap-4">
          <Badge variant="secondary" className="text-sm">
            {schema.compatibilityMode || 'BACKWARD'}
          </Badge>
          <span className="text-muted-foreground text-sm">
            New schemas must be compatible with previous versions
          </span>
        </div>
      </Card>
    </DashboardLayout>
  );
}
