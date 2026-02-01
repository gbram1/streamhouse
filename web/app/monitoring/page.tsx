'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Bell, AlertTriangle, AlertCircle, Info, Plus } from 'lucide-react';
import { formatRelativeTime } from '@/lib/utils';

// Mock alerts data
const mockAlerts = [
  {
    id: '1',
    severity: 'warning' as const,
    title: 'High consumer lag detected',
    message: 'Consumer group "analytics-workers" has lag > 10000 messages',
    timestamp: Date.now() - 300000,
    status: 'active',
  },
  {
    id: '2',
    severity: 'info' as const,
    title: 'New schema registered',
    message: 'Schema "orders-value" v2 registered successfully',
    timestamp: Date.now() - 600000,
    status: 'resolved',
  },
];

export default function MonitoringPage() {
  const activeAlerts = mockAlerts.filter((a) => a.status === 'active');

  return (
    <DashboardLayout
      title="Monitoring & Alerts"
      description="System alerts and custom monitoring rules"
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Active Alerts</h3>
            <Bell className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold text-red-600">
            {activeAlerts.length}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Require attention</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Alert Rules</h3>
            <AlertTriangle className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0</div>
          <p className="mt-1 text-xs text-muted-foreground">Configured rules</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Silenced</h3>
            <Bell className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0</div>
          <p className="mt-1 text-xs text-muted-foreground">Muted alerts</p>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Channels</h3>
            <Info className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">0</div>
          <p className="mt-1 text-xs text-muted-foreground">Notification channels</p>
        </Card>
      </div>

      {/* Active Alerts */}
      <Card className="mt-6">
        <div className="p-6">
          <div className="flex items-center justify-between mb-6">
            <h3 className="text-lg font-semibold">Active Alerts</h3>
            <Button>
              <Plus className="mr-2 h-4 w-4" />
              Create Alert Rule
            </Button>
          </div>

          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Severity</TableHead>
                <TableHead>Alert</TableHead>
                <TableHead>Message</TableHead>
                <TableHead>Time</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {mockAlerts.map((alert) => (
                <TableRow key={alert.id}>
                  <TableCell>
                    <Badge
                      variant={
                        alert.severity === 'critical'
                          ? 'destructive'
                          : alert.severity === 'warning'
                          ? 'secondary'
                          : 'default'
                      }
                    >
                      {alert.severity === 'critical' && (
                        <AlertCircle className="mr-1 h-3 w-3" />
                      )}
                      {alert.severity === 'warning' && (
                        <AlertTriangle className="mr-1 h-3 w-3" />
                      )}
                      {alert.severity === 'info' && <Info className="mr-1 h-3 w-3" />}
                      {alert.severity}
                    </Badge>
                  </TableCell>
                  <TableCell className="font-medium">{alert.title}</TableCell>
                  <TableCell className="text-muted-foreground">{alert.message}</TableCell>
                  <TableCell>{formatRelativeTime(alert.timestamp)}</TableCell>
                  <TableCell>
                    <div className="flex gap-2">
                      <Button variant="ghost" size="sm">
                        View
                      </Button>
                      {alert.status === 'active' && (
                        <Button variant="ghost" size="sm">
                          Silence
                        </Button>
                      )}
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </Card>

      {/* Alert Rules */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Alert Rules</h3>
        <div className="flex h-48 items-center justify-center text-muted-foreground">
          <div className="text-center">
            <AlertTriangle className="mx-auto h-12 w-12 opacity-50 mb-4" />
            <p>No alert rules configured</p>
            <Button className="mt-4" variant="outline">
              <Plus className="mr-2 h-4 w-4" />
              Create Your First Rule
            </Button>
          </div>
        </div>
      </Card>

      {/* System Logs */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">System Logs</h3>
        <div className="flex h-64 items-center justify-center text-muted-foreground">
          <p>Real-time log viewer will be rendered here</p>
        </div>
      </Card>
    </DashboardLayout>
  );
}
