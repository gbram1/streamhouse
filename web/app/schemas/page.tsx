'use client';

import { useState, useEffect } from 'react';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from '@/components/ui/table';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Plus, Eye, Trash2, AlertCircle } from 'lucide-react';
import { Alert, AlertDescription } from '@/components/ui/alert';

interface Schema {
  id: number;
  subject: string;
  version: number;
  schema: string;
  schemaType: 'AVRO' | 'PROTOBUF' | 'JSON';
}

interface Subject {
  name: string;
  versions: number[];
  latestVersion?: Schema;
}

export default function SchemasPage() {
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [selectedSubject, setSelectedSubject] = useState<string | null>(null);
  const [selectedSchema, setSelectedSchema] = useState<Schema | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Registration form state
  const [newSubject, setNewSubject] = useState('');
  const [newSchema, setNewSchema] = useState('');
  const [newSchemaType, setNewSchemaType] = useState<'AVRO' | 'PROTOBUF' | 'JSON'>('AVRO');
  const [registerError, setRegisterError] = useState<string | null>(null);
  const [registerSuccess, setRegisterSuccess] = useState(false);

  // Schema Registry URL (can be configured via env var)
  const SCHEMA_REGISTRY_URL = process.env.NEXT_PUBLIC_SCHEMA_REGISTRY_URL || 'http://localhost:8081';

  useEffect(() => {
    fetchSubjects();
  }, []);

  const fetchSubjects = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const response = await fetch(`${SCHEMA_REGISTRY_URL}/subjects`);

      if (!response.ok) {
        throw new Error('Failed to fetch subjects');
      }

      const subjectNames: string[] = await response.json();

      // Fetch latest version for each subject
      const subjectsWithVersions = await Promise.all(
        subjectNames.map(async (name) => {
          try {
            const versionsRes = await fetch(`${SCHEMA_REGISTRY_URL}/subjects/${name}/versions`);
            const versions: number[] = await versionsRes.json();

            const latestRes = await fetch(`${SCHEMA_REGISTRY_URL}/subjects/${name}/versions/latest`);
            const latestVersion: Schema = await latestRes.json();

            return { name, versions, latestVersion };
          } catch (e) {
            return { name, versions: [], latestVersion: undefined };
          }
        })
      );

      setSubjects(subjectsWithVersions);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to connect to Schema Registry');
    } finally {
      setIsLoading(false);
    }
  };

  const viewSchema = async (subject: string, version: number) => {
    try {
      const response = await fetch(`${SCHEMA_REGISTRY_URL}/subjects/${subject}/versions/${version}`);
      const schema: Schema = await response.json();
      setSelectedSchema(schema);
    } catch (e) {
      setError('Failed to fetch schema');
    }
  };

  const registerSchema = async () => {
    setRegisterError(null);
    setRegisterSuccess(false);

    try {
      // Validate JSON if Avro or JSON schema
      if (newSchemaType === 'AVRO' || newSchemaType === 'JSON') {
        JSON.parse(newSchema);
      }

      const response = await fetch(`${SCHEMA_REGISTRY_URL}/subjects/${newSubject}/versions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          schema: newSchema,
          schemaType: newSchemaType,
          references: [],
          metadata: null,
        }),
      });

      if (!response.ok) {
        const errorData = await response.json();
        throw new Error(errorData.message || 'Failed to register schema');
      }

      const result = await response.json();
      setRegisterSuccess(true);
      setNewSubject('');
      setNewSchema('');

      // Refresh subjects list
      setTimeout(() => {
        fetchSubjects();
        setRegisterSuccess(false);
      }, 1500);

    } catch (e) {
      setRegisterError(e instanceof Error ? e.message : 'Invalid schema');
    }
  };

  const deleteSubject = async (subject: string) => {
    if (!confirm(`Are you sure you want to delete subject "${subject}"? This will delete all versions.`)) {
      return;
    }

    try {
      await fetch(`${SCHEMA_REGISTRY_URL}/subjects/${subject}`, { method: 'DELETE' });
      fetchSubjects();
    } catch (e) {
      setError('Failed to delete subject');
    }
  };

  const formatSchema = (schemaStr: string, schemaType: string) => {
    try {
      if (schemaType === 'AVRO' || schemaType === 'JSON') {
        return JSON.stringify(JSON.parse(schemaStr), null, 2);
      }
      return schemaStr;
    } catch {
      return schemaStr;
    }
  };

  const getSchemaTypeColor = (type: string) => {
    switch (type) {
      case 'AVRO': return 'bg-blue-500';
      case 'PROTOBUF': return 'bg-purple-500';
      case 'JSON': return 'bg-green-500';
      default: return 'bg-gray-500';
    }
  };

  if (isLoading) {
    return (
      <div className="container mx-auto p-6">
        <div className="text-center py-12">Loading schemas...</div>
      </div>
    );
  }

  return (
    <div className="container mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold">Schema Registry</h1>
          <p className="text-gray-600 mt-1">
            Manage schemas for data validation and evolution
          </p>
        </div>

        <Dialog>
          <DialogTrigger asChild>
            <Button>
              <Plus className="h-4 w-4 mr-2" />
              Register Schema
            </Button>
          </DialogTrigger>
          <DialogContent className="max-w-3xl">
            <DialogHeader>
              <DialogTitle>Register New Schema</DialogTitle>
              <DialogDescription>
                Add a new schema version to a subject
              </DialogDescription>
            </DialogHeader>

            <div className="space-y-4">
              <div>
                <Label htmlFor="subject">Subject Name</Label>
                <Input
                  id="subject"
                  placeholder="e.g., users-value, orders-key"
                  value={newSubject}
                  onChange={(e) => setNewSubject(e.target.value)}
                />
                <p className="text-sm text-gray-500 mt-1">
                  Convention: &lt;topic&gt;-&lt;key|value&gt;
                </p>
              </div>

              <div>
                <Label htmlFor="schemaType">Schema Format</Label>
                <Select value={newSchemaType} onValueChange={(v: any) => setNewSchemaType(v)}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="AVRO">Avro (Recommended)</SelectItem>
                    <SelectItem value="JSON">JSON Schema</SelectItem>
                    <SelectItem value="PROTOBUF">Protobuf</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div>
                <Label htmlFor="schema">Schema Definition</Label>
                <Textarea
                  id="schema"
                  placeholder={
                    newSchemaType === 'AVRO'
                      ? '{"type": "record", "name": "User", "fields": [{"name": "id", "type": "long"}]}'
                      : newSchemaType === 'JSON'
                      ? '{"type": "object", "properties": {"id": {"type": "number"}}}'
                      : 'message User { int64 id = 1; }'
                  }
                  value={newSchema}
                  onChange={(e) => setNewSchema(e.target.value)}
                  rows={10}
                  className="font-mono text-sm"
                />
              </div>

              {registerError && (
                <Alert variant="destructive">
                  <AlertCircle className="h-4 w-4" />
                  <AlertDescription>{registerError}</AlertDescription>
                </Alert>
              )}

              {registerSuccess && (
                <Alert className="bg-green-50 border-green-200">
                  <AlertDescription className="text-green-800">
                    Schema registered successfully!
                  </AlertDescription>
                </Alert>
              )}
            </div>

            <DialogFooter>
              <Button onClick={registerSchema} disabled={!newSubject || !newSchema}>
                Register Schema
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>

      {/* Connection Error */}
      {error && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            {error}
            <br />
            <span className="text-sm">
              Make sure the Schema Registry server is running on {SCHEMA_REGISTRY_URL}
            </span>
          </AlertDescription>
        </Alert>
      )}

      {/* Subjects Table */}
      <Card>
        <CardHeader>
          <CardTitle>Registered Subjects</CardTitle>
          <CardDescription>
            {subjects.length} subject{subjects.length !== 1 ? 's' : ''} registered
          </CardDescription>
        </CardHeader>
        <CardContent>
          {subjects.length === 0 ? (
            <div className="text-center py-12 text-gray-500">
              <p className="mb-4">No schemas registered yet</p>
              <p className="text-sm">Click "Register Schema" to add your first schema</p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Subject</TableHead>
                  <TableHead>Format</TableHead>
                  <TableHead>Versions</TableHead>
                  <TableHead>Latest Version</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {subjects.map((subject) => (
                  <TableRow key={subject.name}>
                    <TableCell className="font-mono text-sm">
                      {subject.name}
                    </TableCell>
                    <TableCell>
                      {subject.latestVersion && (
                        <Badge className={getSchemaTypeColor(subject.latestVersion.schemaType)}>
                          {subject.latestVersion.schemaType}
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline">
                        {subject.versions.length} version{subject.versions.length !== 1 ? 's' : ''}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      {subject.latestVersion && (
                        <span className="text-sm text-gray-600">
                          v{subject.latestVersion.version} (ID: {subject.latestVersion.id})
                        </span>
                      )}
                    </TableCell>
                    <TableCell className="text-right space-x-2">
                      {subject.latestVersion && (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => viewSchema(subject.name, subject.latestVersion!.version)}
                        >
                          <Eye className="h-4 w-4 mr-1" />
                          View
                        </Button>
                      )}
                      <Button
                        variant="destructive"
                        size="sm"
                        onClick={() => deleteSubject(subject.name)}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Schema Viewer Dialog */}
      {selectedSchema && (
        <Dialog open={!!selectedSchema} onOpenChange={() => setSelectedSchema(null)}>
          <DialogContent className="max-w-4xl max-h-[80vh] overflow-y-auto">
            <DialogHeader>
              <DialogTitle>
                {selectedSchema.subject} - Version {selectedSchema.version}
              </DialogTitle>
              <DialogDescription>
                Schema ID: {selectedSchema.id} | Format: {selectedSchema.schemaType}
              </DialogDescription>
            </DialogHeader>

            <div className="bg-gray-900 text-gray-100 p-4 rounded-lg overflow-x-auto">
              <pre className="text-sm">
                {formatSchema(selectedSchema.schema, selectedSchema.schemaType)}
              </pre>
            </div>
          </DialogContent>
        </Dialog>
      )}
    </div>
  );
}
