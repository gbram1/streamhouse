'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { MetricCard } from '@/components/ui/metric-card';
import { Card } from '@/components/ui/card';
import { LineChart } from '@/components/charts/line-chart';
import { AreaChart } from '@/components/charts/area-chart';
import { Database, Users, Activity, HardDrive, Heart, Server, FileCode2 } from 'lucide-react';
import { useMemo } from 'react';

// Generate mock data
function generateMockThroughputData() {
  const now = Date.now();
  return Array.from({ length: 24 }, (_, i) => ({
    time: new Date(now - (24 - i) * 3600000).toLocaleTimeString('en-US', { hour: '2-digit' }),
    messages: Math.floor(Math.random() * 3000 + 500),
  }));
}

function generateMockLagData() {
  const now = Date.now();
  return Array.from({ length: 24 }, (_, i) => ({
    time: new Date(now - (24 - i) * 3600000).toLocaleTimeString('en-US', { hour: '2-digit' }),
    lag: Math.floor(Math.random() * 5000 + 100),
  }));
}

export default function Dashboard() {
  const mockThroughputData = useMemo(() => generateMockThroughputData(), []);
  const mockLagData = useMemo(() => generateMockLagData(), []);
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
          <h3 className="text-lg font-semibold mb-4">Message Throughput (24h)</h3>
          <LineChart
            data={mockThroughputData}
            xKey="time"
            lines={[
              { key: 'messages', color: '#3b82f6', name: 'Messages/sec' },
            ]}
            height={250}
          />
        </Card>

        {/* Consumer Lag Summary */}
        <Card className="p-6">
          <h3 className="text-lg font-semibold mb-4">Consumer Lag Summary</h3>
          <AreaChart
            data={mockLagData}
            xKey="time"
            areas={[
              { key: 'lag', color: '#ef4444', name: 'Total Lag' },
            ]}
            height={250}
          />
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
