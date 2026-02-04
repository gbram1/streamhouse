'use client';

import { useState, useMemo } from 'react';
import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
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
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog';
import { Checkbox } from '@/components/ui/checkbox';
import { Key, Plus, Trash2, Copy, Check, Calendar, Clock, AlertCircle } from 'lucide-react';
import { formatRelativeTime } from '@/lib/utils';
import { useOrganizations } from '@/lib/hooks/use-organizations';
import { useApiKeys, useCreateApiKey, useRevokeApiKey } from '@/lib/hooks/use-api-keys';
import type { ApiKeyCreated } from '@/lib/types';

export default function ApiKeysPage() {
  const { data: organizations, isLoading: orgsLoading } = useOrganizations();
  const [selectedOrgId, setSelectedOrgId] = useState<string>('');
  const { data: apiKeys, isLoading: keysLoading } = useApiKeys(selectedOrgId);
  const createMutation = useCreateApiKey();
  const revokeMutation = useRevokeApiKey();

  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createdKey, setCreatedKey] = useState<ApiKeyCreated | null>(null);
  const [copied, setCopied] = useState(false);

  // Form state for creating API keys
  const [keyName, setKeyName] = useState('');
  const [permissions, setPermissions] = useState<string[]>(['read', 'write']);
  const [expiresIn, setExpiresIn] = useState<string>('never');

  const handleCreate = async () => {
    if (!selectedOrgId) return;
    try {
      const expiresInMs = expiresIn === 'never' ? undefined :
        expiresIn === '7d' ? 7 * 24 * 60 * 60 * 1000 :
        expiresIn === '30d' ? 30 * 24 * 60 * 60 * 1000 :
        expiresIn === '90d' ? 90 * 24 * 60 * 60 * 1000 :
        365 * 24 * 60 * 60 * 1000;

      const result = await createMutation.mutateAsync({
        organizationId: selectedOrgId,
        data: {
          name: keyName,
          permissions,
          expires_in_ms: expiresInMs,
        },
      });
      setCreatedKey(result);
      setKeyName('');
      setPermissions(['read', 'write']);
      setExpiresIn('never');
    } catch (error) {
      console.error('Failed to create API key:', error);
    }
  };

  const handleRevoke = async (keyId: string) => {
    try {
      await revokeMutation.mutateAsync({ id: keyId, organizationId: selectedOrgId });
    } catch (error) {
      console.error('Failed to revoke API key:', error);
    }
  };

  const copyToClipboard = async (text: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const togglePermission = (perm: string) => {
    setPermissions(prev =>
      prev.includes(perm)
        ? prev.filter(p => p !== perm)
        : [...prev, perm]
    );
  };

  const selectedOrg = organizations?.find(o => o.id === selectedOrgId);
  const keyCount = apiKeys?.length || 0;
  const now = useMemo(() => Date.now(), [apiKeys]);
  const activeKeyCount = apiKeys?.filter(k => !k.expires_at || k.expires_at > now).length || 0;

  return (
    <DashboardLayout
      title="API Keys"
      description="Manage API keys for authentication"
    >
      {/* Organization Selector */}
      <Card className="p-6">
        <div className="flex items-center gap-4">
          <Label htmlFor="org-select" className="whitespace-nowrap">Select Organization</Label>
          <Select value={selectedOrgId} onValueChange={setSelectedOrgId}>
            <SelectTrigger className="w-[300px]">
              <SelectValue placeholder="Choose an organization..." />
            </SelectTrigger>
            <SelectContent>
              {organizations?.map((org) => (
                <SelectItem key={org.id} value={org.id}>
                  {org.name} ({org.slug})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {orgsLoading && <span className="text-sm text-muted-foreground">Loading...</span>}
        </div>
      </Card>

      {selectedOrgId && (
        <>
          {/* Quick Stats */}
          <div className="grid grid-cols-1 gap-6 md:grid-cols-3 mt-6">
            <Card className="p-6">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-medium text-muted-foreground">Total API Keys</h3>
                <Key className="h-5 w-5 text-muted-foreground" />
              </div>
              <div className="mt-2 text-3xl font-bold">{keyCount}</div>
              <p className="mt-1 text-xs text-muted-foreground">
                {activeKeyCount} active
              </p>
            </Card>

            <Card className="p-6">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-medium text-muted-foreground">Organization</h3>
                <Key className="h-5 w-5 text-muted-foreground" />
              </div>
              <div className="mt-2 text-lg font-bold">{selectedOrg?.name}</div>
              <p className="mt-1 text-xs text-muted-foreground font-mono">
                {selectedOrg?.slug}
              </p>
            </Card>

            <Card className="p-6">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-medium text-muted-foreground">Plan</h3>
                <Key className="h-5 w-5 text-muted-foreground" />
              </div>
              <div className="mt-2">
                <Badge variant={selectedOrg?.plan === 'enterprise' ? 'default' : 'secondary'}>
                  {selectedOrg?.plan}
                </Badge>
              </div>
            </Card>
          </div>

          {/* API Keys Table */}
          <Card className="mt-6">
            <div className="p-6">
              <div className="flex items-center justify-between mb-6">
                <h3 className="text-lg font-semibold">API Keys</h3>
                <Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
                  <DialogTrigger asChild>
                    <Button>
                      <Plus className="h-4 w-4 mr-2" />
                      Create API Key
                    </Button>
                  </DialogTrigger>
                  <DialogContent>
                    <DialogHeader>
                      <DialogTitle>Create API Key</DialogTitle>
                      <DialogDescription>
                        Create a new API key for {selectedOrg?.name}.
                      </DialogDescription>
                    </DialogHeader>
                    <div className="grid gap-4 py-4">
                      <div className="grid gap-2">
                        <Label htmlFor="key-name">Name</Label>
                        <Input
                          id="key-name"
                          value={keyName}
                          onChange={(e) => setKeyName(e.target.value)}
                          placeholder="Production API Key"
                        />
                      </div>
                      <div className="grid gap-2">
                        <Label>Permissions</Label>
                        <div className="flex flex-wrap gap-4">
                          {['read', 'write', 'admin'].map((perm) => (
                            <label key={perm} className="flex items-center gap-2 cursor-pointer">
                              <Checkbox
                                checked={permissions.includes(perm)}
                                onCheckedChange={() => togglePermission(perm)}
                              />
                              <span className="text-sm capitalize">{perm}</span>
                            </label>
                          ))}
                        </div>
                      </div>
                      <div className="grid gap-2">
                        <Label htmlFor="expires">Expiration</Label>
                        <Select value={expiresIn} onValueChange={setExpiresIn}>
                          <SelectTrigger>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="never">Never expires</SelectItem>
                            <SelectItem value="7d">7 days</SelectItem>
                            <SelectItem value="30d">30 days</SelectItem>
                            <SelectItem value="90d">90 days</SelectItem>
                            <SelectItem value="365d">1 year</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>
                    </div>
                    <DialogFooter>
                      <Button variant="outline" onClick={() => setCreateDialogOpen(false)}>
                        Cancel
                      </Button>
                      <Button onClick={handleCreate} disabled={!keyName || permissions.length === 0 || createMutation.isPending}>
                        {createMutation.isPending ? 'Creating...' : 'Create'}
                      </Button>
                    </DialogFooter>
                  </DialogContent>
                </Dialog>
              </div>

              {keysLoading && (
                <div className="flex h-32 items-center justify-center">
                  <p className="text-muted-foreground">Loading API keys...</p>
                </div>
              )}

              {!keysLoading && (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Name</TableHead>
                      <TableHead>Key Prefix</TableHead>
                      <TableHead>Permissions</TableHead>
                      <TableHead>Created</TableHead>
                      <TableHead>Last Used</TableHead>
                      <TableHead>Expires</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {apiKeys?.length === 0 ? (
                      <TableRow>
                        <TableCell colSpan={7} className="text-center text-muted-foreground">
                          No API keys found
                        </TableCell>
                      </TableRow>
                    ) : (
                      apiKeys?.map((key) => {
                        const isExpired = key.expires_at && key.expires_at < now;
                        return (
                          <TableRow key={key.id}>
                            <TableCell className="font-medium">{key.name}</TableCell>
                            <TableCell className="font-mono text-sm">{key.key_prefix}...</TableCell>
                            <TableCell>
                              <div className="flex flex-wrap gap-1">
                                {key.permissions.map((perm) => (
                                  <Badge key={perm} variant="outline" className="text-xs">
                                    {perm}
                                  </Badge>
                                ))}
                              </div>
                            </TableCell>
                            <TableCell>
                              <div className="flex items-center gap-2">
                                <Calendar className="h-4 w-4 text-muted-foreground" />
                                <span className="text-sm">
                                  {formatRelativeTime(key.created_at)}
                                </span>
                              </div>
                            </TableCell>
                            <TableCell>
                              {key.last_used_at ? (
                                <div className="flex items-center gap-2">
                                  <Clock className="h-4 w-4 text-muted-foreground" />
                                  <span className="text-sm">
                                    {formatRelativeTime(key.last_used_at)}
                                  </span>
                                </div>
                              ) : (
                                <span className="text-sm text-muted-foreground">Never</span>
                              )}
                            </TableCell>
                            <TableCell>
                              {key.expires_at ? (
                                <div className="flex items-center gap-2">
                                  {isExpired ? (
                                    <AlertCircle className="h-4 w-4 text-destructive" />
                                  ) : (
                                    <Clock className="h-4 w-4 text-muted-foreground" />
                                  )}
                                  <span className={`text-sm ${isExpired ? 'text-destructive' : ''}`}>
                                    {formatRelativeTime(key.expires_at)}
                                  </span>
                                </div>
                              ) : (
                                <span className="text-sm text-muted-foreground">Never</span>
                              )}
                            </TableCell>
                            <TableCell>
                              <AlertDialog>
                                <AlertDialogTrigger asChild>
                                  <Button variant="ghost" size="sm">
                                    <Trash2 className="h-4 w-4 text-destructive" />
                                  </Button>
                                </AlertDialogTrigger>
                                <AlertDialogContent>
                                  <AlertDialogHeader>
                                    <AlertDialogTitle>Revoke API Key</AlertDialogTitle>
                                    <AlertDialogDescription>
                                      Are you sure you want to revoke &quot;{key.name}&quot;? This action
                                      cannot be undone and will immediately invalidate this key.
                                    </AlertDialogDescription>
                                  </AlertDialogHeader>
                                  <AlertDialogFooter>
                                    <AlertDialogCancel>Cancel</AlertDialogCancel>
                                    <AlertDialogAction
                                      onClick={() => handleRevoke(key.id)}
                                      className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                                    >
                                      Revoke
                                    </AlertDialogAction>
                                  </AlertDialogFooter>
                                </AlertDialogContent>
                              </AlertDialog>
                            </TableCell>
                          </TableRow>
                        );
                      })
                    )}
                  </TableBody>
                </Table>
              )}
            </div>
          </Card>
        </>
      )}

      {!selectedOrgId && !orgsLoading && (
        <Card className="mt-6 p-12">
          <div className="text-center text-muted-foreground">
            <Key className="h-12 w-12 mx-auto mb-4 opacity-50" />
            <h3 className="text-lg font-medium mb-2">Select an Organization</h3>
            <p className="text-sm">Choose an organization above to manage its API keys.</p>
          </div>
        </Card>
      )}

      {/* Created Key Dialog - Shows the key only once */}
      <Dialog open={!!createdKey} onOpenChange={(open) => !open && setCreatedKey(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>API Key Created</DialogTitle>
            <DialogDescription>
              Make sure to copy your API key now. You won&apos;t be able to see it again!
            </DialogDescription>
          </DialogHeader>
          <div className="py-4">
            <Label className="text-sm font-medium">Your API Key</Label>
            <div className="mt-2 flex items-center gap-2">
              <Input
                value={createdKey?.key || ''}
                readOnly
                className="font-mono text-sm"
              />
              <Button
                variant="outline"
                size="icon"
                onClick={() => copyToClipboard(createdKey?.key || '')}
              >
                {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
              </Button>
            </div>
            <div className="mt-4 p-4 bg-yellow-50 dark:bg-yellow-900/20 rounded-lg border border-yellow-200 dark:border-yellow-800">
              <div className="flex items-start gap-3">
                <AlertCircle className="h-5 w-5 text-yellow-600 dark:text-yellow-500 mt-0.5" />
                <div className="text-sm text-yellow-800 dark:text-yellow-200">
                  <p className="font-medium">Important</p>
                  <p className="mt-1">
                    This key will only be displayed once. Store it securely and never share
                    it publicly. If you lose it, you&apos;ll need to create a new key.
                  </p>
                </div>
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button onClick={() => { setCreatedKey(null); setCreateDialogOpen(false); }}>
              Done
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </DashboardLayout>
  );
}
