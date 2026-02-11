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
import { Server, Cpu, Network, Clock, BarChart3 } from 'lucide-react';
import { formatRelativeTime, formatDuration } from '@/lib/utils';
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
        if (response.ok) {
          const data = await response.json();
          setAgents(Array.isArray(data) ? data : []);
        }
      } catch (error) {
        console.error('Failed to fetch agents:', error);
      } finally {
        setLoading(false);
      }
    };

    fetchAgents();
    const interval = setInterval(fetchAgents, 5000);
    return () => clearInterval(interval);
  }, []);
  const getAgentHealth = (agent: Agent): 'healthy' | 'degraded' | 'unhealthy' => {
    const now = Date.now();
    const heartbeatAge = now - agent.last_heartbeat;
    if (heartbeatAge > 60000) return 'unhealthy';
    if (heartbeatAge > 30000) return 'degraded';
    return 'healthy';
  };

  const getAgentUptime = (agent: Agent): string => {
    const now = Date.now();
    return formatDuration(now - agent.started_at);
  };

  const healthyAgents = agents.filter((a) => getAgentHealth(a) === 'healthy').length;
  const totalPartitions = agents.reduce((acc, a) => acc + a.active_leases, 0);
  const avgPartitions = agents.length > 0 ? totalPartitions / agents.length : 0;

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
            {agents.length > 0 ? `Avg ${(totalPartitions / agents.length).toFixed(1)} per agent` : 'No agents'}
          </p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Avg Partitions</h3>
            <Cpu className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">{avgPartitions.toFixed(1)}</div>
          <p className="mt-1 text-xs text-muted-foreground">Per agent</p>
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
                <TableHead>Uptime</TableHead>
                <TableHead>Partitions</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {agents.length === 0 && !loading ? (
                <TableRow>
                  <TableCell colSpan={8} className="text-center text-muted-foreground">
                    No agents found
                  </TableCell>
                </TableRow>
              ) : (
                agents.map((agent) => {
                  const health = getAgentHealth(agent);
                  const healthColors = {
                    healthy: 'bg-green-500',
                    degraded: 'bg-yellow-500',
                    unhealthy: 'bg-red-500',
                  };
                  return (
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
                        <Badge variant="default" className={healthColors[health]}>
                          {health}
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
                        <span className="text-sm">{getAgentUptime(agent)}</span>
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
                  );
                })
              )}
            </TableBody>
          </Table>
        </div>
      </Card>

      {/* Agent Distribution */}
      <Card className="mt-6 p-6">
        <div className="flex items-center gap-2 mb-4">
          <BarChart3 className="h-5 w-5 text-muted-foreground" />
          <h3 className="text-lg font-semibold">Partition Distribution</h3>
        </div>
        {agents.length === 0 ? (
          <div className="flex h-32 items-center justify-center text-muted-foreground">
            <p>No agents to display</p>
          </div>
        ) : (
          <div className="space-y-3">
            {agents
              .sort((a, b) => b.active_leases - a.active_leases)
              .map((agent) => {
                const pct = totalPartitions > 0
                  ? (agent.active_leases / totalPartitions) * 100
                  : 0;
                return (
                  <div key={agent.agent_id}>
                    <div className="flex items-center justify-between mb-1">
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium font-mono">
                          {agent.agent_id.length > 12
                            ? agent.agent_id.slice(0, 12) + '...'
                            : agent.agent_id}
                        </span>
                        <Badge variant="outline" className="text-xs">
                          {agent.availability_zone}
                        </Badge>
                      </div>
                      <span className="text-sm text-muted-foreground">
                        {agent.active_leases} partitions ({pct.toFixed(0)}%)
                      </span>
                    </div>
                    <div className="h-2.5 bg-secondary rounded-full overflow-hidden">
                      <div
                        className="h-full bg-primary rounded-full transition-all duration-300"
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                  </div>
                );
              })}
          </div>
        )}
      </Card>
    </DashboardLayout>
  );
}
