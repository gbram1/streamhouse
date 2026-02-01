'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Database, Server, HardDrive, Activity } from 'lucide-react';

export default function PartitionsPage() {
  return (
    <DashboardLayout
      title="Partitions"
      description="Partition distribution and health monitoring"
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Partitions</h3>
            <Database className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0</div>
          <p className="mt-1 text-xs text-muted-foreground">Across all topics</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Leaders</h3>
            <Server className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0</div>
          <p className="mt-1 text-xs text-muted-foreground">Partition leaders</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Avg Size</h3>
            <HardDrive className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0 MB</div>
          <p className="mt-1 text-xs text-muted-foreground">Per partition</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Rebalancing</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0</div>
          <p className="mt-1 text-xs text-muted-foreground">Active operations</p>
        </Card>
      </div>

      {/* Partition Map */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Partition Distribution Map</h3>
        <div className="flex h-96 items-center justify-center text-muted-foreground">
          <p>Visual partition topology will be rendered here</p>
        </div>
      </Card>

      {/* Partition Size Heatmap */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Partition Size Heatmap</h3>
        <div className="flex h-64 items-center justify-center text-muted-foreground">
          <p>Size heatmap will be rendered here</p>
        </div>
      </Card>

      {/* Rebalancing History */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Rebalancing History</h3>
        <div className="flex h-48 items-center justify-center text-muted-foreground">
          <p>No recent rebalancing operations</p>
        </div>
      </Card>
    </DashboardLayout>
  );
}
