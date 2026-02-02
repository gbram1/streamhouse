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
import { useEffect, useState } from 'react';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

interface Agent {
  agent_id: string;
  address: string;
  availability_zone: string;
  agent_group: string;
  last_heartbeat: number;
  started_at: number;
  active_leases: number;
}

export default function AgentsPage() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchAgents = async () => {
      try {
        const response = await fetch(`${API_URL}/api/v1/agents`);
        const data = await response.json();
        setAgents(data);
      } catch (error) {
        console.error('Failed to fetch agents:', error);
      } finally {
        setLoading(false);
      }
    };

    fetchAgents();
    const interval = setInterval(fetchAgents, 5000); // Refresh every 5 seconds
    return () => clearInterval(interval);
  }, []);
  const healthyAgents = agents.length; // All agents considered healthy if they're in the list
  const totalPartitions = agents.reduce((acc, a) => acc + a.active_leases, 0);
  const avgCpu = 0; // CPU metrics not yet implemented

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
            {new Set(agents.map((a) => a.availability_zone)).size}
          </div>
        </Card>
      </div>

      {loading && (
        <div className="mt-6 flex h-32 items-center justify-center">
          <p className="text-muted-foreground">Loading agents...</p>
        </div>
      )}

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
              {agents.length === 0 && !loading ? (
                <TableRow>
                  <TableCell colSpan={10} className="text-center text-muted-foreground">
                    No agents found
                  </TableCell>
                </TableRow>
              ) : (
                agents.map((agent) => (
                  <TableRow key={agent.agent_id}>
                    <TableCell className="font-medium">
                      <Link href={`/agents/${agent.agent_id}`} className="hover:text-primary">
                        {agent.agent_id}
                      </Link>
                    </TableCell>
                    <TableCell className="font-mono text-sm">{agent.address}</TableCell>
                    <TableCell>
                      <Badge variant="outline">{agent.availability_zone}</Badge>
                    </TableCell>
                    <TableCell>
                      <Badge variant="default" className="bg-green-500">
                        healthy
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <Clock className="h-4 w-4 text-muted-foreground" />
                        <span className="text-sm">
                          {formatRelativeTime(agent.last_heartbeat)}
                        </span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <div className="h-2 w-16 bg-secondary rounded-full overflow-hidden">
                          <div className="h-full bg-primary" style={{ width: '0%' }} />
                        </div>
                        <span className="text-sm">N/A</span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <div className="h-2 w-16 bg-secondary rounded-full overflow-hidden">
                          <div className="h-full bg-primary" style={{ width: '0%' }} />
                        </div>
                        <span className="text-sm">N/A</span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <div className="h-2 w-16 bg-secondary rounded-full overflow-hidden">
                          <div className="h-full bg-primary" style={{ width: '0%' }} />
                        </div>
                        <span className="text-sm">N/A</span>
                      </div>
                    </TableCell>
                    <TableCell>{agent.active_leases}</TableCell>
                    <TableCell>
                      <Link href={`/agents/${agent.agent_id}`}>
                        <Button variant="ghost" size="sm">
                          Details
                        </Button>
                      </Link>
                    </TableCell>
                  </TableRow>
                ))
              )}
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
