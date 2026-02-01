'use client';

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
import { Server, Cpu, HardDrive, Network, Clock, AlertCircle } from 'lucide-react';
import { formatRelativeTime, getHealthColor, formatPercent } from '@/lib/utils';
import Link from 'next/link';

// Mock data - replace with actual API call
const mockAgents = [
  {
    id: 'agent-1',
    address: 'localhost:9090',
    availabilityZone: 'us-east-1a',
    health: 'healthy' as const,
    lastHeartbeat: Date.now() - 5000,
    cpuUsage: 0.25,
    memoryUsage: 0.42,
    diskUsage: 0.18,
    partitionCount: 8,
  },
  {
    id: 'agent-2',
    address: 'localhost:9091',
    availabilityZone: 'us-east-1b',
    health: 'healthy' as const,
    lastHeartbeat: Date.now() - 3000,
    cpuUsage: 0.18,
    memoryUsage: 0.38,
    diskUsage: 0.22,
    partitionCount: 6,
  },
];

export default function AgentsPage() {
  const agents = mockAgents;
  const healthyAgents = agents.filter((a) => a.health === 'healthy').length;
  const totalPartitions = agents.reduce((acc, a) => acc + a.partitionCount, 0);
  const avgCpu = agents.reduce((acc, a) => acc + a.cpuUsage, 0) / agents.length;

  return (
    <DashboardLayout
      title="Agents"
      description="Monitor stateless agent health and resource usage"
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Active Agents</h3>
            <Server className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{agents.length}</div>
          <p className="mt-1 text-xs text-muted-foreground">
            {healthyAgents}/{agents.length} healthy
          </p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Partitions</h3>
            <Server className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{totalPartitions}</div>
          <p className="mt-1 text-xs text-muted-foreground">
            Avg {(totalPartitions / agents.length).toFixed(1)} per agent
          </p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Avg CPU Usage</h3>
            <Cpu className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{formatPercent(avgCpu)}</div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Availability Zones</h3>
            <Network className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {new Set(agents.map((a) => a.availabilityZone)).size}
          </div>
        </Card>
      </div>

      {/* Agents Table */}
      <Card className="mt-6">
        <div className="p-6">
          <h3 className="text-lg font-semibold mb-6">Agent Status</h3>

          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Agent ID</TableHead>
                <TableHead>Address</TableHead>
                <TableHead>Zone</TableHead>
                <TableHead>Health</TableHead>
                <TableHead>Last Heartbeat</TableHead>
                <TableHead>CPU</TableHead>
                <TableHead>Memory</TableHead>
                <TableHead>Disk</TableHead>
                <TableHead>Partitions</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {agents.map((agent) => (
                <TableRow key={agent.id}>
                  <TableCell className="font-medium">
                    <Link href={`/agents/${agent.id}`} className="hover:text-primary">
                      {agent.id}
                    </Link>
                  </TableCell>
                  <TableCell className="font-mono text-sm">{agent.address}</TableCell>
                  <TableCell>
                    <Badge variant="outline">{agent.availabilityZone}</Badge>
                  </TableCell>
                  <TableCell>
                    <Badge
                      variant={agent.health === 'healthy' ? 'default' : 'destructive'}
                      className={
                        agent.health === 'healthy' ? 'bg-green-500' : ''
                      }
                    >
                      {agent.health}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <Clock className="h-4 w-4 text-muted-foreground" />
                      <span className="text-sm">
                        {formatRelativeTime(agent.lastHeartbeat)}
                      </span>
                    </div>
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-16 bg-secondary rounded-full overflow-hidden">
                        <div
                          className="h-full bg-primary"
                          style={{ width: `${agent.cpuUsage * 100}%` }}
                        />
                      </div>
                      <span className="text-sm">{formatPercent(agent.cpuUsage)}</span>
                    </div>
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-16 bg-secondary rounded-full overflow-hidden">
                        <div
                          className="h-full bg-primary"
                          style={{ width: `${agent.memoryUsage * 100}%` }}
                        />
                      </div>
                      <span className="text-sm">{formatPercent(agent.memoryUsage)}</span>
                    </div>
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-16 bg-secondary rounded-full overflow-hidden">
                        <div
                          className="h-full bg-primary"
                          style={{ width: `${agent.diskUsage * 100}%` }}
                        />
                      </div>
                      <span className="text-sm">{formatPercent(agent.diskUsage)}</span>
                    </div>
                  </TableCell>
                  <TableCell>{agent.partitionCount}</TableCell>
                  <TableCell>
                    <Link href={`/agents/${agent.id}`}>
                      <Button variant="ghost" size="sm">
                        Details
                      </Button>
                    </Link>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </Card>

      {/* Agent Topology Map */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Agent Topology</h3>
        <div className="flex h-64 items-center justify-center text-muted-foreground">
          <p>Visual topology map will be rendered here</p>
        </div>
      </Card>
    </DashboardLayout>
  );
}
