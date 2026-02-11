'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { HardDrive, Database, Zap, Activity, CheckCircle, XCircle } from 'lucide-react';
import { useStorageMetrics } from '@/lib/hooks/use-metrics';
import { formatBytes, formatCompactNumber, formatPercent } from '@/lib/utils';

export default function StoragePage() {
  const { data: storage, isLoading } = useStorageMetrics();

  return (
    <DashboardLayout
      title="Storage & Caching"
      description="S3 storage, WAL, and cache insights"
    >
      {/* Storage Overview */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Storage</h3>
            <HardDrive className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : formatBytes(storage?.totalSizeBytes || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">S3/MinIO storage</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Segment Count</h3>
            <Database className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : formatCompactNumber(storage?.segmentCount || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Total segments</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Cache Hit Rate</h3>
            <Zap className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold text-green-600">
            {isLoading ? '...' : formatPercent(storage?.cacheHitRate || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">LRU cache performance</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Cache Size</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : formatBytes(storage?.cacheSize || 0)}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">In-memory cache</p>
        </Card>
      </div>

      {/* Storage by Topic */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Storage by Topic</h3>
        {isLoading ? (
          <div className="flex h-64 items-center justify-center text-muted-foreground">
            <p>Loading storage data...</p>
          </div>
        ) : Object.keys(storage?.storageByTopic || {}).length === 0 ? (
          <div className="flex h-64 items-center justify-center text-muted-foreground">
            <p>No storage data available</p>
          </div>
        ) : (
          <div className="space-y-4">
            {Object.entries(storage?.storageByTopic || {})
              .sort(([, a], [, b]) => b - a)
              .slice(0, 10)
              .map(([topic, bytes]) => (
                <div key={topic}>
                  <div className="flex items-center justify-between mb-2">
                    <span className="font-medium">{topic}</span>
                    <span className="text-sm text-muted-foreground">{formatBytes(bytes)}</span>
                  </div>
                  <div className="h-2 bg-secondary rounded-full overflow-hidden">
                    <div
                      className="h-full bg-primary"
                      style={{
                        width: `${((bytes / (storage?.totalSizeBytes || 1)) * 100).toFixed(1)}%`,
                      }}
                    />
                  </div>
                </div>
              ))}
          </div>
        )}
      </Card>

      {/* WAL Status */}
      <Card className="mt-6 p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold">Write-Ahead Log (WAL)</h3>
          {!isLoading && (
            <Badge
              variant={storage?.walEnabled ? 'default' : 'destructive'}
              className={storage?.walEnabled ? 'bg-green-600' : ''}
            >
              {storage?.walEnabled ? (
                <><CheckCircle className="mr-1 h-3 w-3" /> Enabled</>
              ) : (
                <><XCircle className="mr-1 h-3 w-3" /> Disabled</>
              )}
            </Badge>
          )}
        </div>
        <div className="grid grid-cols-1 gap-6 md:grid-cols-3">
          <div>
            <p className="text-sm text-muted-foreground">WAL Size</p>
            <p className="mt-2 text-2xl font-bold">
              {isLoading ? '...' : formatBytes(storage?.walSize || 0)}
            </p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Uncommitted Entries</p>
            <p className="mt-2 text-2xl font-bold">
              {isLoading ? '...' : formatCompactNumber(storage?.walUncommittedEntries || 0)}
            </p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Sync Lag</p>
            <p className="mt-2 text-2xl font-bold">
              {isLoading ? '...' : `${storage?.walSyncLagMs ?? 0}ms`}
            </p>
          </div>
        </div>
      </Card>

      {/* Cache Statistics */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Cache Performance</h3>
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          {/* Hit Rate */}
          <div className="p-4 rounded-lg border bg-card">
            <h4 className="text-sm font-medium text-muted-foreground mb-3">Cache Hit Rate</h4>
            <div className="text-4xl font-bold text-green-600 mb-3">
              {isLoading ? '...' : formatPercent(storage?.cacheHitRate || 0)}
            </div>
            <div className="h-3 bg-secondary rounded-full overflow-hidden">
              <div
                className="h-full bg-green-500 rounded-full transition-all duration-500"
                style={{ width: `${(storage?.cacheHitRate || 0) * 100}%` }}
              />
            </div>
            <p className="text-xs text-muted-foreground mt-2">
              Higher hit rate means fewer S3 requests
            </p>
          </div>

          {/* Evictions */}
          <div className="p-4 rounded-lg border bg-card">
            <h4 className="text-sm font-medium text-muted-foreground mb-3">Cache Evictions</h4>
            <div className="text-4xl font-bold mb-3">
              {isLoading ? '...' : formatCompactNumber(storage?.cacheEvictions || 0)}
            </div>
            <div className="grid grid-cols-2 gap-4 mt-4">
              <div>
                <p className="text-xs text-muted-foreground">Cache Size</p>
                <p className="text-sm font-medium mt-1">
                  {isLoading ? '...' : formatBytes(storage?.cacheSize || 0)}
                </p>
              </div>
              <div>
                <p className="text-xs text-muted-foreground">Eviction Rate</p>
                <p className="text-sm font-medium mt-1">
                  {isLoading ? '...' : `${storage?.cacheEvictions || 0} entries`}
                </p>
              </div>
            </div>
          </div>
        </div>
      </Card>

      {/* Retention Cleanup */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Retention & Cleanup</h3>
        <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
          <div>
            <p className="text-sm text-muted-foreground">Segments Cleaned Up (24h)</p>
            <p className="mt-2 text-3xl font-bold">
              {isLoading ? '...' : formatCompactNumber(storage?.retentionCleanupCount || 0)}
            </p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Avg Segment Size</p>
            <p className="mt-2 text-3xl font-bold">
              {isLoading ? '...' : (storage?.segmentCount || 0) > 0
                ? formatBytes(Math.round((storage?.totalSizeBytes || 0) / (storage?.segmentCount || 1)))
                : '--'}
            </p>
          </div>
        </div>
      </Card>

      {/* S3 Metrics */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">S3/MinIO Metrics</h3>
        <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
          <div>
            <p className="text-sm text-muted-foreground">Request Count (24h)</p>
            <p className="mt-2 text-2xl font-bold">
              {isLoading ? '...' : formatCompactNumber(storage?.s3RequestCount || 0)}
            </p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Throttle Rate</p>
            <p className="mt-2 text-2xl font-bold">
              {isLoading ? '...' : formatPercent(storage?.s3ThrottleRate || 0)}
            </p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Est. Monthly Cost</p>
            <p className="mt-2 text-2xl font-bold">
              {isLoading ? '...' : `$${((storage?.s3RequestCount || 0) * 0.000005).toFixed(2)}`}
            </p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Avg Request Latency</p>
            <p className="mt-2 text-2xl font-bold">
              {isLoading ? '...' : `${storage?.s3AvgLatencyMs ?? '-'}ms`}
            </p>
          </div>
        </div>
      </Card>
    </DashboardLayout>
  );
}
