'use client';

import { useState } from 'react';
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
import { Building2, Plus, Trash2, Edit, Calendar, Users } from 'lucide-react';
import { formatRelativeTime } from '@/lib/utils';
import Link from 'next/link';
import {
  useOrganizations,
  useCreateOrganization,
  useUpdateOrganization,
  useDeleteOrganization,
} from '@/lib/hooks/use-organizations';
import type { Organization } from '@/lib/types';

function getPlanBadgeVariant(plan: string): 'default' | 'secondary' | 'outline' {
  switch (plan) {
    case 'enterprise':
      return 'default';
    case 'pro':
      return 'secondary';
    default:
      return 'outline';
  }
}

function getStatusBadgeColor(status: string): string {
  switch (status) {
    case 'active':
      return 'bg-green-500';
    case 'suspended':
      return 'bg-yellow-500';
    case 'deleted':
      return 'bg-red-500';
    default:
      return 'bg-gray-500';
  }
}

export default function OrganizationsPage() {
  const { data: organizations, isLoading } = useOrganizations();
  const createMutation = useCreateOrganization();
  const updateMutation = useUpdateOrganization();
  const deleteMutation = useDeleteOrganization();

  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [editOrg, setEditOrg] = useState<Organization | null>(null);

  // Form state for creating organizations
  const [newName, setNewName] = useState('');
  const [newSlug, setNewSlug] = useState('');
  const [newPlan, setNewPlan] = useState<'free' | 'pro' | 'enterprise'>('free');

  // Form state for editing organizations
  const [editPlan, setEditPlan] = useState<'free' | 'pro' | 'enterprise'>('free');
  const [editStatus, setEditStatus] = useState<'active' | 'suspended' | 'deleted'>('active');

  const handleCreate = async () => {
    try {
      await createMutation.mutateAsync({
        name: newName,
        slug: newSlug,
        plan: newPlan,
      });
      setCreateDialogOpen(false);
      setNewName('');
      setNewSlug('');
      setNewPlan('free');
    } catch (error) {
      console.error('Failed to create organization:', error);
    }
  };

  const handleUpdate = async () => {
    if (!editOrg) return;
    try {
      await updateMutation.mutateAsync({
        id: editOrg.id,
        data: {
          plan: editPlan,
          status: editStatus,
        },
      });
      setEditOrg(null);
    } catch (error) {
      console.error('Failed to update organization:', error);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteMutation.mutateAsync(id);
    } catch (error) {
      console.error('Failed to delete organization:', error);
    }
  };

  const openEditDialog = (org: Organization) => {
    setEditOrg(org);
    setEditPlan(org.plan);
    setEditStatus(org.status);
  };

  // Auto-generate slug from name
  const handleNameChange = (name: string) => {
    setNewName(name);
    setNewSlug(name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, ''));
  };

  const orgCount = organizations?.length || 0;
  const activeCount = organizations?.filter(o => o.status === 'active').length || 0;
  const planCounts = {
    free: organizations?.filter(o => o.plan === 'free').length || 0,
    pro: organizations?.filter(o => o.plan === 'pro').length || 0,
    enterprise: organizations?.filter(o => o.plan === 'enterprise').length || 0,
  };

  return (
    <DashboardLayout
      title="Organizations"
      description="Manage multi-tenant organizations and their settings"
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Organizations</h3>
            <Building2 className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{orgCount}</div>
          <p className="mt-1 text-xs text-muted-foreground">
            {activeCount} active
          </p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Enterprise</h3>
            <Users className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{planCounts.enterprise}</div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Pro</h3>
            <Users className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{planCounts.pro}</div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Free</h3>
            <Users className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{planCounts.free}</div>
        </Card>
      </div>

      {/* Organizations Table */}
      <Card className="mt-6">
        <div className="p-6">
          <div className="flex items-center justify-between mb-6">
            <h3 className="text-lg font-semibold">Organizations</h3>
            <Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
              <DialogTrigger asChild>
                <Button>
                  <Plus className="h-4 w-4 mr-2" />
                  New Organization
                </Button>
              </DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Create Organization</DialogTitle>
                  <DialogDescription>
                    Create a new organization for multi-tenant isolation.
                  </DialogDescription>
                </DialogHeader>
                <div className="grid gap-4 py-4">
                  <div className="grid gap-2">
                    <Label htmlFor="name">Name</Label>
                    <Input
                      id="name"
                      value={newName}
                      onChange={(e) => handleNameChange(e.target.value)}
                      placeholder="Acme Corporation"
                    />
                  </div>
                  <div className="grid gap-2">
                    <Label htmlFor="slug">Slug</Label>
                    <Input
                      id="slug"
                      value={newSlug}
                      onChange={(e) => setNewSlug(e.target.value)}
                      placeholder="acme-corp"
                    />
                    <p className="text-xs text-muted-foreground">
                      URL-friendly identifier (lowercase, hyphens only)
                    </p>
                  </div>
                  <div className="grid gap-2">
                    <Label htmlFor="plan">Plan</Label>
                    <Select value={newPlan} onValueChange={(v: 'free' | 'pro' | 'enterprise') => setNewPlan(v)}>
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="free">Free</SelectItem>
                        <SelectItem value="pro">Pro</SelectItem>
                        <SelectItem value="enterprise">Enterprise</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>
                <DialogFooter>
                  <Button variant="outline" onClick={() => setCreateDialogOpen(false)}>
                    Cancel
                  </Button>
                  <Button onClick={handleCreate} disabled={!newName || !newSlug || createMutation.isPending}>
                    {createMutation.isPending ? 'Creating...' : 'Create'}
                  </Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          </div>

          {isLoading && (
            <div className="flex h-32 items-center justify-center">
              <p className="text-muted-foreground">Loading organizations...</p>
            </div>
          )}

          {!isLoading && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Slug</TableHead>
                  <TableHead>Plan</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Created</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {organizations?.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={6} className="text-center text-muted-foreground">
                      No organizations found
                    </TableCell>
                  </TableRow>
                ) : (
                  organizations?.map((org) => (
                    <TableRow key={org.id}>
                      <TableCell className="font-medium">
                        <Link
                          href={`/settings/organizations/${org.id}`}
                          className="hover:text-primary"
                        >
                          {org.name}
                        </Link>
                      </TableCell>
                      <TableCell className="font-mono text-sm">{org.slug}</TableCell>
                      <TableCell>
                        <Badge variant={getPlanBadgeVariant(org.plan)}>
                          {org.plan}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Badge className={getStatusBadgeColor(org.status)}>
                          {org.status}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          <Calendar className="h-4 w-4 text-muted-foreground" />
                          <span className="text-sm">
                            {formatRelativeTime(org.created_at)}
                          </span>
                        </div>
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => openEditDialog(org)}
                          >
                            <Edit className="h-4 w-4" />
                          </Button>
                          <AlertDialog>
                            <AlertDialogTrigger asChild>
                              <Button variant="ghost" size="sm">
                                <Trash2 className="h-4 w-4 text-destructive" />
                              </Button>
                            </AlertDialogTrigger>
                            <AlertDialogContent>
                              <AlertDialogHeader>
                                <AlertDialogTitle>Delete Organization</AlertDialogTitle>
                                <AlertDialogDescription>
                                  Are you sure you want to delete &quot;{org.name}&quot;? This action
                                  cannot be undone and will delete all associated data.
                                </AlertDialogDescription>
                              </AlertDialogHeader>
                              <AlertDialogFooter>
                                <AlertDialogCancel>Cancel</AlertDialogCancel>
                                <AlertDialogAction
                                  onClick={() => handleDelete(org.id)}
                                  className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                                >
                                  Delete
                                </AlertDialogAction>
                              </AlertDialogFooter>
                            </AlertDialogContent>
                          </AlertDialog>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          )}
        </div>
      </Card>

      {/* Edit Dialog */}
      <Dialog open={!!editOrg} onOpenChange={(open) => !open && setEditOrg(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Edit Organization</DialogTitle>
            <DialogDescription>
              Update organization settings for {editOrg?.name}.
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="edit-plan">Plan</Label>
              <Select value={editPlan} onValueChange={(v: 'free' | 'pro' | 'enterprise') => setEditPlan(v)}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="free">Free</SelectItem>
                  <SelectItem value="pro">Pro</SelectItem>
                  <SelectItem value="enterprise">Enterprise</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-2">
              <Label htmlFor="edit-status">Status</Label>
              <Select value={editStatus} onValueChange={(v: 'active' | 'suspended' | 'deleted') => setEditStatus(v)}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="active">Active</SelectItem>
                  <SelectItem value="suspended">Suspended</SelectItem>
                  <SelectItem value="deleted">Deleted</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditOrg(null)}>
              Cancel
            </Button>
            <Button onClick={handleUpdate} disabled={updateMutation.isPending}>
              {updateMutation.isPending ? 'Saving...' : 'Save Changes'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </DashboardLayout>
  );
}
