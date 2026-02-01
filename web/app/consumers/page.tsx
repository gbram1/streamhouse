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
import { useConsumerGroups } from '@/lib/hooks/use-consumer-groups';
import { formatCompactNumber, getLagColor } from '@/lib/utils';
import { Users, TrendingUp, TrendingDown, Minus, AlertTriangle } from 'lucide-react';
import Link from 'next/link';

export default function ConsumersPage() {
  const { data: consumerGroups, isLoading } = useConsumerGroups();

  const totalLag = consumerGroups?.reduce((acc, g) => acc + g.totalLag, 0) || 0;
  const groupsWithLag = consumerGroups?.filter((g) => g.totalLag > 0).length || 0;

  return (
    <DashboardLayout
      title="Consumer Groups"
      description="Monitor consumer lag and group health"
    >
      {/* Quick Stats */}
      <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Active Groups</h3>
            <Users className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : consumerGroups?.length || 0}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Lag</h3>
            <AlertTriangle className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className={`mt-2 text-3xl font-bold ${getLagColor(totalLag)}`}>
            {isLoading ? '...' : formatCompactNumber(totalLag)}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Groups with Lag</h3>
            <TrendingUp className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading ? '...' : groupsWithLag}
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-muted-foreground">Total Members</h3>
            <Users className="h-5 w-5 text-muted-foreground" />
          </div>
          <div className="mt-2 text-3xl font-bold">
            {isLoading
              ? '...'
              : consumerGroups?.reduce((acc, g) => acc + g.memberCount, 0) || 0}
          </div>
        </Card>
      </div>

      {/* Consumer Groups Table */}
      <Card className="mt-6">
        <div className="p-6">
          <div className="flex items-center justify-between mb-6">
            <h3 className="text-lg font-semibold">Consumer Groups</h3>
          </div>

          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Group ID</TableHead>
                <TableHead>State</TableHead>
                <TableHead>Members</TableHead>
                <TableHead>Total Lag</TableHead>
                <TableHead>Lag Trend</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableRow>
                  <TableCell colSpan={6} className="text-center">
                    Loading consumer groups...
                  </TableCell>
                </TableRow>
              ) : consumerGroups?.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={6} className="text-center text-muted-foreground">
                    No consumer groups found. Start a consumer to see it here.
                  </TableCell>
                </TableRow>
              ) : (
                consumerGroups?.map((group) => (
                  <TableRow key={group.id}>
                    <TableCell className="font-medium">
                      <Link
                        href={`/consumers/${group.id}`}
                        className="hover:text-primary"
                      >
                        {group.id}
                      </Link>
                    </TableCell>
                    <TableCell>
                      <Badge
                        variant={
                          group.state === 'stable'
                            ? 'default'
                            : group.state === 'rebalancing'
                            ? 'secondary'
                            : 'destructive'
                        }
                      >
                        {group.state}
                      </Badge>
                    </TableCell>
                    <TableCell>{group.memberCount}</TableCell>
                    <TableCell>
                      <span className={getLagColor(group.totalLag)}>
                        {formatCompactNumber(group.totalLag)}
                      </span>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        {group.lagTrend === 'increasing' ? (
                          <>
                            <TrendingUp className="h-4 w-4 text-red-600" />
                            <span className="text-sm text-red-600">Increasing</span>
                          </>
                        ) : group.lagTrend === 'decreasing' ? (
                          <>
                            <TrendingDown className="h-4 w-4 text-green-600" />
                            <span className="text-sm text-green-600">Decreasing</span>
                          </>
                        ) : (
                          <>
                            <Minus className="h-4 w-4 text-muted-foreground" />
                            <span className="text-sm text-muted-foreground">Stable</span>
                          </>
                        )}
                      </div>
                    </TableCell>
                    <TableCell>
                      <Link href={`/consumers/${group.id}`}>
                        <Button variant="ghost" size="sm">
                          View Details
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

      {/* Lag Heatmap Placeholder */}
      <Card className="mt-6 p-6">
        <h3 className="text-lg font-semibold mb-4">Lag Heatmap</h3>
        <div className="flex h-64 items-center justify-center text-muted-foreground">
          <p>Visual lag heatmap will be rendered here</p>
        </div>
      </Card>
    </DashboardLayout>
  );
}
