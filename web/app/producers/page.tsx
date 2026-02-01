'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Activity, Send, AlertCircle, TrendingUp } from 'lucide-react';

export default function ProducersPage() {
  return (
    <DashboardLayout
      title="Producers"
      description="Monitor producer activity and performance"
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Active Producers</h3>
            <Send className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0</div>
          <p className="mt-1 text-xs text-muted-foreground">Currently active</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Message Rate</h3>
            <Activity className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0/s</div>
          <p className="mt-1 text-xs text-muted-foreground">Messages per second</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Byte Rate</h3>
            <TrendingUp className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0 MB/s</div>
          <p className="mt-1 text-xs text-muted-foreground">Network throughput</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Error Rate</h3>
            <AlertCircle className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0%</div>
          <p className="mt-1 text-xs text-muted-foreground">Failed messages</p>
        </Card>
      </div>

      {/* Producer Metrics Chart */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Producer Throughput</h3>
        <div className="flex h-64 items-center justify-center text-muted-foreground">
          <p>Producer metrics will be displayed here when available</p>
        </div>
      </Card>

      {/* Schema Usage */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Top Schemas by Message Count</h3>
        <div className="flex h-64 items-center justify-center text-muted-foreground">
          <p>Schema usage chart will be rendered here</p>
        </div>
      </Card>
    </DashboardLayout>
  );
}
