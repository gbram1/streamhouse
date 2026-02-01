'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { MetricCard } from '@/components/ui/metric-card';
import { Card } from '@/components/ui/card';
import { Database, Users, Activity, HardDrive, Heart, Server, FileCode2 } from 'lucide-react';

export default function Dashboard() {
  return (
    <DashboardLayout
      title="Overview"
      description="System health and performance at a glance"
    >
      {/* Metrics Grid */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-4">
        <MetricCard
          title="Messages/sec"
          value="0"
          description="Real-time throughput"
          icon={Activity}
        />

        <MetricCard
          title="Active Topics"
          value="0"
          description="Event streams"
          icon={Database}
        />

        <MetricCard
          title="Active Consumers"
          value="0"
          description="Consumer groups"
          icon={Users}
        />

        <MetricCard
          title="Total Storage"
          value="0 MB"
          description="Across all topics"
          icon={HardDrive}
        />

        <MetricCard
          title="System Health"
          value="Healthy"
          description="Overall status"
          icon={Heart}
          className="border-green-500/50"
        />

        <MetricCard
          title="Active Agents"
          value="0"
          description="Stateless brokers"
          icon={Server}
        />

        <MetricCard
          title="Schemas"
          value="0"
          description="Registered schemas"
          icon={FileCode2}
        />

        <MetricCard
          title="Partitions"
          value="0"
          description="Total partitions"
          icon={Database}
        />
      </div>

      {/* Charts Section */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* Message Throughput Chart */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold">Message Throughput (24h)</h3>
          <div className="mt-4 flex h-64 items-center justify-center text-muted-foreground">
            <p>Chart will be rendered here with Recharts</p>
          </div>
        </Card>

        {/* Consumer Lag Summary */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold">Consumer Lag Summary</h3>
          <div className="mt-4 flex h-64 items-center justify-center text-muted-foreground">
            <p>Lag chart will be rendered here</p>
          </div>
        </Card>
      </div>

      {/* Recent Activity */}
      <div className="mt-6">
        <Card className="p-6">
          <h3 className="text-lg font-semibold">Recent Activity</h3>
          <div className="mt-4 space-y-4">
            <div className="text-sm text-muted-foreground">
              <p>• No recent activity</p>
            </div>
          </div>
        </Card>
      </div>

      {/* Quick Actions */}
      <div className="mt-6 grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
        <Card className="p-6 hover:bg-accent cursor-pointer transition-colors">
          <h4 className="font-medium">Create Topic</h4>
          <p className="mt-2 text-sm text-muted-foreground">
            Add a new event stream
          </p>
        </Card>
        <Card className="p-6 hover:bg-accent cursor-pointer transition-colors">
          <h4 className="font-medium">View All Topics</h4>
          <p className="mt-2 text-sm text-muted-foreground">
            Browse and manage topics
          </p>
        </Card>
        <Card className="p-6 hover:bg-accent cursor-pointer transition-colors">
          <h4 className="font-medium">Monitor Consumers</h4>
          <p className="mt-2 text-sm text-muted-foreground">
            Check consumer lag
          </p>
        </Card>
        <Card className="p-6 hover:bg-accent cursor-pointer transition-colors">
          <h4 className="font-medium">Browse Schemas</h4>
          <p className="mt-2 text-sm text-muted-foreground">
            View schema registry
          </p>
        </Card>
      </div>
    </DashboardLayout>
  );
}
