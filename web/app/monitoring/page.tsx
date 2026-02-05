'use client';

import { useState, useEffect } from 'react';
import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { MetricCard } from '@/components/ui/metric-card';
import { DataTable, Column } from '@/components/ui/data-table';
import { Skeleton, SkeletonChart } from '@/components/ui/skeleton';
import { EmptyAlerts } from '@/components/ui/empty-state';
import { AreaChart } from '@/components/charts/area-chart';
import {
  Bell,
  AlertTriangle,
  AlertCircle,
  Info,
  Plus,
  CheckCircle2,
  XCircle,
  VolumeX,
  Activity,
  RefreshCw,
} from 'lucide-react';
import { formatRelativeTime, cn } from '@/lib/utils';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

interface Alert {
  id: string;
  severity: 'critical' | 'warning' | 'info';
  title: string;
  message: string;
  timestamp: number;
  status: 'active' | 'resolved' | 'silenced';
  source?: string;
}

interface SystemMetrics {
  cpuUsage: number;
  memoryUsage: number;
  diskUsage: number;
  activeConnections: number;
}

export default function MonitoringPage() {
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [metrics, setMetrics] = useState<SystemMetrics | null>(null);
  const [metricsHistory, setMetricsHistory] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchData = async () => {
      try {
        // Try to fetch alerts from API
        const [alertsRes, metricsRes] = await Promise.all([
          fetch(`${API_URL}/api/v1/alerts`).then(r => r.json()).catch(() => []),
          fetch(`${API_URL}/api/v1/metrics/system`).then(r => r.json()).catch(() => null),
        ]);

        setAlerts(alertsRes || []);
        setMetrics(metricsRes);

        // Generate sample metrics history for chart if no real data
        if (!metricsRes) {
          const now = Date.now();
          const history = Array.from({ length: 20 }, (_, i) => ({
            time: new Date(now - (19 - i) * 30000).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' }),
            cpu: Math.random() * 30 + 20,
            memory: Math.random() * 20 + 40,
          }));
          setMetricsHistory(history);
        }
      } catch (error) {
        console.error('Failed to fetch monitoring data:', error);
      } finally {
        setLoading(false);
      }
    };

    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, []);

  const activeAlerts = alerts.filter((a) => a.status === 'active');
  const criticalCount = alerts.filter((a) => a.severity === 'critical' && a.status === 'active').length;
  const warningCount = alerts.filter((a) => a.severity === 'warning' && a.status === 'active').length;
  const silencedCount = alerts.filter((a) => a.status === 'silenced').length;

  const alertColumns: Column<Alert>[] = [
    {
      id: 'severity',
      header: 'Severity',
      accessorKey: 'severity',
      sortable: true,
      cell: (row) => {
        const colors = {
          critical: 'bg-red-500/20 text-red-700 dark:text-red-400 border-red-500/30',
          warning: 'bg-yellow-500/20 text-yellow-700 dark:text-yellow-400 border-yellow-500/30',
          info: 'bg-blue-500/20 text-blue-700 dark:text-blue-400 border-blue-500/30',
        };
        const icons = {
          critical: <XCircle className="mr-1 h-3 w-3" />,
          warning: <AlertTriangle className="mr-1 h-3 w-3" />,
          info: <Info className="mr-1 h-3 w-3" />,
        };
        return (
          <Badge variant="secondary" className={colors[row.severity]}>
            {icons[row.severity]}
            {row.severity}
          </Badge>
        );
      },
    },
    {
      id: 'title',
      header: 'Alert',
      accessorKey: 'title',
      sortable: true,
      cell: (row) => <span className="font-medium">{row.title}</span>,
    },
    {
      id: 'message',
      header: 'Message',
      accessorKey: 'message',
      cell: (row) => (
        <span className="text-muted-foreground text-sm max-w-xs truncate block">
          {row.message}
        </span>
      ),
    },
    {
      id: 'status',
      header: 'Status',
      accessorKey: 'status',
      sortable: true,
      cell: (row) => {
        const statusConfig = {
          active: { icon: <AlertCircle className="mr-1 h-3 w-3" />, class: 'bg-red-500/20 text-red-700 dark:text-red-400' },
          resolved: { icon: <CheckCircle2 className="mr-1 h-3 w-3" />, class: 'bg-green-500/20 text-green-700 dark:text-green-400' },
          silenced: { icon: <VolumeX className="mr-1 h-3 w-3" />, class: 'bg-gray-500/20 text-gray-700 dark:text-gray-400' },
        };
        const config = statusConfig[row.status];
        return (
          <Badge variant="secondary" className={config.class}>
            {config.icon}
            {row.status}
          </Badge>
        );
      },
    },
    {
      id: 'timestamp',
      header: 'Time',
      accessorKey: 'timestamp',
      sortable: true,
      cell: (row) => (
        <span className="text-sm text-muted-foreground">
          {formatRelativeTime(row.timestamp)}
        </span>
      ),
    },
  ];

  const MetricSkeleton = () => (
    <Card className="p-6">
      <div className="flex items-center justify-between">
        <Skeleton className="h-4 w-24" />
        <Skeleton className="h-5 w-5 rounded" />
      </div>
      <div className="mt-3">
        <Skeleton className="h-8 w-16" />
        <Skeleton className="mt-2 h-3 w-20" />
      </div>
    </Card>
  );

  return (
    <DashboardLayout
      title="Monitoring & Alerts"
      description="System health, alerts, and monitoring rules"
      actions={
        <Button>
          <Plus className="mr-2 h-4 w-4" />
          Create Alert Rule
        </Button>
      }
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {loading ? (
          <>
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
            <MetricSkeleton />
          </>
        ) : (
          <>
            <MetricCard
              title="Active Alerts"
              value={activeAlerts.length.toString()}
              description="Require attention"
              icon={Bell}
              className={activeAlerts.length > 0 ? 'border-red-500/30' : ''}
            />
            <MetricCard
              title="Critical"
              value={criticalCount.toString()}
              description="High priority"
              icon={XCircle}
              className={criticalCount > 0 ? 'border-red-500/30 bg-red-500/5' : ''}
            />
            <MetricCard
              title="Warnings"
              value={warningCount.toString()}
              description="Medium priority"
              icon={AlertTriangle}
              className={warningCount > 0 ? 'border-yellow-500/30 bg-yellow-500/5' : ''}
            />
            <MetricCard
              title="Silenced"
              value={silencedCount.toString()}
              description="Muted alerts"
              icon={VolumeX}
            />
          </>
        )}
      </div>

      {/* System Metrics Chart */}
      <Card className="mt-6 p-6">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h3 className="text-lg font-semibold">System Metrics</h3>
            <p className="text-sm text-muted-foreground">CPU and memory usage over time</p>
          </div>
          <Button variant="ghost" size="sm">
            <RefreshCw className="mr-2 h-4 w-4" />
            Refresh
          </Button>
        </div>
        {loading ? (
          <SkeletonChart height={250} />
        ) : metricsHistory.length > 0 ? (
          <AreaChart
            data={metricsHistory}
            xKey="time"
            areas={[
              { key: 'cpu', color: '#3b82f6', name: 'CPU %' },
              { key: 'memory', color: '#10b981', name: 'Memory %' },
            ]}
            height={250}
          />
        ) : (
          <div className="flex h-64 items-center justify-center text-muted-foreground">
            <div className="text-center">
              <Activity className="mx-auto h-12 w-12 opacity-50 mb-4" />
              <p>No metrics data available</p>
              <p className="text-sm mt-1">System metrics will appear here once collected</p>
            </div>
          </div>
        )}
      </Card>

      {/* Active Alerts Table */}
      <Card className="mt-6">
        <div className="p-6">
          <div className="mb-4">
            <h3 className="text-lg font-semibold">Alert History</h3>
            <p className="text-sm text-muted-foreground">Recent alerts and their status</p>
          </div>
          <DataTable
            data={alerts}
            columns={alertColumns}
            isLoading={loading}
            loadingRows={5}
            searchKey="title"
            searchPlaceholder="Search alerts..."
            pageSize={10}
            getRowId={(row) => row.id}
            emptyState={<EmptyAlerts />}
            rowActions={(row) => [
              {
                label: 'View Details',
                icon: <Info className="mr-2 h-4 w-4" />,
                onClick: () => console.log('View alert:', row.id),
              },
              ...(row.status === 'active' ? [{
                label: 'Silence',
                icon: <VolumeX className="mr-2 h-4 w-4" />,
                onClick: () => console.log('Silence alert:', row.id),
              }] : []),
              ...(row.status === 'active' ? [{
                label: 'Resolve',
                icon: <CheckCircle2 className="mr-2 h-4 w-4" />,
                onClick: () => console.log('Resolve alert:', row.id),
              }] : []),
            ]}
          />
        </div>
      </Card>

      {/* Quick Links */}
      <div className="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-3">
        <Card className="p-5 hover:bg-accent cursor-pointer transition-all hover:shadow-md">
          <div className="flex items-center gap-3">
            <div className="rounded-lg bg-blue-500/10 p-2.5 text-blue-500">
              <Bell className="h-5 w-5" />
            </div>
            <div>
              <h4 className="font-medium">Alert Rules</h4>
              <p className="text-sm text-muted-foreground">Configure monitoring rules</p>
            </div>
          </div>
        </Card>

        <Card className="p-5 hover:bg-accent cursor-pointer transition-all hover:shadow-md">
          <div className="flex items-center gap-3">
            <div className="rounded-lg bg-purple-500/10 p-2.5 text-purple-500">
              <Activity className="h-5 w-5" />
            </div>
            <div>
              <h4 className="font-medium">Metrics Explorer</h4>
              <p className="text-sm text-muted-foreground">Query system metrics</p>
            </div>
          </div>
        </Card>

        <Card className="p-5 hover:bg-accent cursor-pointer transition-all hover:shadow-md">
          <div className="flex items-center gap-3">
            <div className="rounded-lg bg-green-500/10 p-2.5 text-green-500">
              <CheckCircle2 className="h-5 w-5" />
            </div>
            <div>
              <h4 className="font-medium">Health Checks</h4>
              <p className="text-sm text-muted-foreground">View system health</p>
            </div>
          </div>
        </Card>
      </div>
    </DashboardLayout>
  );
}
