'use client';

import { use } from 'react';
import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  Building2,
  Key,
  ArrowLeft,
  Database,
  Activity,
  Users,
  HardDrive,
  Clock,
  Layers,
  Network,
} from 'lucide-react';
import { formatRelativeTime, formatBytes } from '@/lib/utils';
import Link from 'next/link';
import {
  useOrganization,
  useOrganizationQuota,
  useOrganizationUsage,
} from '@/lib/hooks/use-organizations';
import { useApiKeys } from '@/lib/hooks/use-api-keys';

function formatBytesPerSec(bytes: number): string {
  return formatBytes(bytes) + '/s';
}

function getUsagePercent(used: number, max: number): number {
  if (max === 0) return 0;
  return Math.min(100, (used / max) * 100);
}

function getUsageColor(percent: number): string {
  if (percent >= 90) return 'bg-red-500';
  if (percent >= 75) return 'bg-yellow-500';
  return 'bg-green-500';
}

interface QuotaItemProps {
  label: string;
  used: number;
  max: number;
  format?: 'number' | 'bytes' | 'bytesPerSec' | 'days';
  icon: React.ReactNode;
}

function QuotaItem({ label, used, max, format = 'number', icon }: QuotaItemProps) {
  const percent = getUsagePercent(used, max);
  const formatValue = (val: number) => {
    switch (format) {
      case 'bytes':
        return formatBytes(val);
      case 'bytesPerSec':
        return formatBytesPerSec(val);
      case 'days':
        return `${val} days`;
      default:
        return val.toLocaleString();
    }
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {icon}
          <span className="text-sm font-medium">{label}</span>
        </div>
        <span className="text-sm text-muted-foreground">
          {formatValue(used)} / {formatValue(max)}
        </span>
      </div>
      <div className="h-2 bg-secondary rounded-full overflow-hidden">
        <div
          className={`h-full transition-all ${getUsageColor(percent)}`}
          style={{ width: `${percent}%` }}
        />
      </div>
      <p className="text-xs text-muted-foreground text-right">
        {percent.toFixed(1)}% used
      </p>
    </div>
  );
}

export default function OrganizationDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);

  const { data: organization, isLoading: orgLoading } = useOrganization(id);
  const { data: quota, isLoading: quotaLoading } = useOrganizationQuota(id);
  const { data: usage, isLoading: usageLoading } = useOrganizationUsage(id);
  const { data: apiKeys, isLoading: keysLoading } = useApiKeys(id);

  const isLoading = orgLoading || quotaLoading || usageLoading;

  if (isLoading) {
    return (
      <DashboardLayout title="Organization" description="Loading...">
        <div className="flex h-64 items-center justify-center">
          <p className="text-muted-foreground">Loading organization details...</p>
        </div>
      </DashboardLayout>
    );
  }

  if (!organization) {
    return (
      <DashboardLayout title="Organization" description="Not found">
        <Card className="p-12 text-center">
          <Building2 className="h-12 w-12 mx-auto mb-4 text-muted-foreground opacity-50" />
          <h3 className="text-lg font-medium mb-2">Organization Not Found</h3>
          <p className="text-sm text-muted-foreground mb-4">
            The organization you&apos;re looking for doesn&apos;t exist.
          </p>
          <Link href="/settings/organizations">
            <Button>
              <ArrowLeft className="h-4 w-4 mr-2" />
              Back to Organizations
            </Button>
          </Link>
        </Card>
      </DashboardLayout>
    );
  }

  return (
    <DashboardLayout
      title={organization.name}
      description={`Organization settings and quota usage for ${organization.slug}`}
    >
      {/* Back Link */}
      <div className="mb-6">
        <Link href="/settings/organizations">
          <Button variant="ghost" size="sm">
            <ArrowLeft className="h-4 w-4 mr-2" />
            Back to Organizations
          </Button>
        </Link>
      </div>

      {/* Organization Info */}
      <Card className="p-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <div className="p-3 rounded-lg bg-primary/10">
              <Building2 className="h-8 w-8 text-primary" />
            </div>
            <div>
              <h2 className="text-2xl font-bold">{organization.name}</h2>
              <p className="text-muted-foreground font-mono">{organization.slug}</p>
            </div>
          </div>
          <div className="flex items-center gap-3">
            <Badge variant={organization.plan === 'enterprise' ? 'default' : 'secondary'}>
              {organization.plan}
            </Badge>
            <Badge className={organization.status === 'active' ? 'bg-green-500' : 'bg-yellow-500'}>
              {organization.status}
            </Badge>
          </div>
        </div>
        <div className="mt-4 pt-4 border-t flex items-center gap-6 text-sm text-muted-foreground">
          <div className="flex items-center gap-2">
            <Clock className="h-4 w-4" />
            <span>Created {formatRelativeTime(organization.created_at)}</span>
          </div>
          <div className="flex items-center gap-2">
            <Key className="h-4 w-4" />
            <span>{apiKeys?.length || 0} API keys</span>
          </div>
        </div>
      </Card>

      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4 mt-6">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Topics</h3>
            <Database className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{usage?.topics_count || 0}</div>
          <p className="mt-1 text-xs text-muted-foreground">
            of {quota?.max_topics || 0} max
          </p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Storage</h3>
            <HardDrive className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{formatBytes(usage?.storage_bytes || 0)}</div>
          <p className="mt-1 text-xs text-muted-foreground">
            of {formatBytes(quota?.max_storage_bytes || 0)} max
          </p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Consumer Groups</h3>
            <Users className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{usage?.consumer_groups_count || 0}</div>
          <p className="mt-1 text-xs text-muted-foreground">
            of {quota?.max_consumer_groups || 0} max
          </p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Requests/Hour</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{(usage?.requests_last_hour || 0).toLocaleString()}</div>
          <p className="mt-1 text-xs text-muted-foreground">
            {((quota?.max_requests_per_sec || 0) * 3600).toLocaleString()} max/hour
          </p>
        </Card>
      </div>

      {/* Quota Usage Dashboard */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-6">Quota Usage</h3>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
          {quota && usage && (
            <>
              <QuotaItem
                label="Topics"
                used={usage.topics_count}
                max={quota.max_topics}
                icon={<Database className="h-4 w-4 text-muted-foreground" />}
              />
              <QuotaItem
                label="Partitions"
                used={usage.partitions_count}
                max={quota.max_total_partitions}
                icon={<Layers className="h-4 w-4 text-muted-foreground" />}
              />
              <QuotaItem
                label="Storage"
                used={usage.storage_bytes}
                max={quota.max_storage_bytes}
                format="bytes"
                icon={<HardDrive className="h-4 w-4 text-muted-foreground" />}
              />
              <QuotaItem
                label="Consumer Groups"
                used={usage.consumer_groups_count}
                max={quota.max_consumer_groups}
                icon={<Users className="h-4 w-4 text-muted-foreground" />}
              />
              <QuotaItem
                label="Produce Throughput"
                used={usage.produce_bytes_last_hour / 3600}
                max={quota.max_produce_bytes_per_sec}
                format="bytesPerSec"
                icon={<Activity className="h-4 w-4 text-muted-foreground" />}
              />
              <QuotaItem
                label="Consume Throughput"
                used={usage.consume_bytes_last_hour / 3600}
                max={quota.max_consume_bytes_per_sec}
                format="bytesPerSec"
                icon={<Activity className="h-4 w-4 text-muted-foreground" />}
              />
              <QuotaItem
                label="Schemas"
                used={usage.schemas_count}
                max={quota.max_schemas}
                icon={<Database className="h-4 w-4 text-muted-foreground" />}
              />
              <QuotaItem
                label="Connections"
                used={0}
                max={quota.max_connections}
                icon={<Network className="h-4 w-4 text-muted-foreground" />}
              />
            </>
          )}
        </div>
      </Card>

      {/* Plan Limits Reference */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Plan Limits Reference</h3>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Resource</TableHead>
              <TableHead className="text-right">Free</TableHead>
              <TableHead className="text-right">Pro</TableHead>
              <TableHead className="text-right">Enterprise</TableHead>
              <TableHead className="text-right">Current</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow>
              <TableCell>Topics</TableCell>
              <TableCell className="text-right">10</TableCell>
              <TableCell className="text-right">100</TableCell>
              <TableCell className="text-right">Unlimited</TableCell>
              <TableCell className="text-right font-medium">{quota?.max_topics}</TableCell>
            </TableRow>
            <TableRow>
              <TableCell>Partitions per Topic</TableCell>
              <TableCell className="text-right">12</TableCell>
              <TableCell className="text-right">48</TableCell>
              <TableCell className="text-right">Unlimited</TableCell>
              <TableCell className="text-right font-medium">{quota?.max_partitions_per_topic}</TableCell>
            </TableRow>
            <TableRow>
              <TableCell>Storage</TableCell>
              <TableCell className="text-right">10 GB</TableCell>
              <TableCell className="text-right">1 TB</TableCell>
              <TableCell className="text-right">Custom</TableCell>
              <TableCell className="text-right font-medium">{formatBytes(quota?.max_storage_bytes || 0)}</TableCell>
            </TableRow>
            <TableRow>
              <TableCell>Produce Throughput</TableCell>
              <TableCell className="text-right">10 MB/s</TableCell>
              <TableCell className="text-right">100 MB/s</TableCell>
              <TableCell className="text-right">Custom</TableCell>
              <TableCell className="text-right font-medium">{formatBytesPerSec(quota?.max_produce_bytes_per_sec || 0)}</TableCell>
            </TableRow>
            <TableRow>
              <TableCell>Consume Throughput</TableCell>
              <TableCell className="text-right">50 MB/s</TableCell>
              <TableCell className="text-right">500 MB/s</TableCell>
              <TableCell className="text-right">Custom</TableCell>
              <TableCell className="text-right font-medium">{formatBytesPerSec(quota?.max_consume_bytes_per_sec || 0)}</TableCell>
            </TableRow>
            <TableRow>
              <TableCell>Consumer Groups</TableCell>
              <TableCell className="text-right">50</TableCell>
              <TableCell className="text-right">500</TableCell>
              <TableCell className="text-right">Unlimited</TableCell>
              <TableCell className="text-right font-medium">{quota?.max_consumer_groups}</TableCell>
            </TableRow>
          </TableBody>
        </Table>
      </Card>

      {/* API Keys Quick View */}
      <Card className="mt-6 p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold">API Keys</h3>
          <Link href="/settings/api-keys">
            <Button variant="outline" size="sm">
              Manage API Keys
            </Button>
          </Link>
        </div>
        {keysLoading ? (
          <p className="text-muted-foreground">Loading API keys...</p>
        ) : apiKeys?.length === 0 ? (
          <p className="text-muted-foreground">No API keys configured.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Key Prefix</TableHead>
                <TableHead>Permissions</TableHead>
                <TableHead>Last Used</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {apiKeys?.slice(0, 5).map((key) => (
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
                  <TableCell className="text-sm text-muted-foreground">
                    {key.last_used_at ? formatRelativeTime(key.last_used_at) : 'Never'}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        {apiKeys && apiKeys.length > 5 && (
          <p className="mt-4 text-sm text-muted-foreground text-center">
            Showing 5 of {apiKeys.length} keys.{' '}
            <Link href="/settings/api-keys" className="text-primary hover:underline">
              View all
            </Link>
          </p>
        )}
      </Card>
    </DashboardLayout>
  );
}
