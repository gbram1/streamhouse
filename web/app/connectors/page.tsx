"use client";

import { useState } from "react";
import { DashboardLayout } from "@/components/layout/dashboard-layout";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { MetricCard } from "@/components/ui/metric-card";
import { DataTable, Column } from "@/components/ui/data-table";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/ui/empty-state";
import { Button } from "@/components/ui/button";
import {
  Plug,
  Play,
  Pause,
  Trash2,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  Activity,
} from "lucide-react";
import {
  useConnectors,
  useDeleteConnector,
  usePauseConnector,
  useResumeConnector,
} from "@/lib/hooks/use-connectors";
import type { Connector } from "@/lib/hooks/use-connectors";

function getStateBadge(state: string) {
  switch (state) {
    case "running":
      return (
        <Badge
          variant="secondary"
          className="bg-green-500/20 text-green-700 dark:text-green-400 border-green-500/30"
        >
          <CheckCircle2 className="mr-1 h-3 w-3" />
          Running
        </Badge>
      );
    case "paused":
      return (
        <Badge
          variant="secondary"
          className="bg-yellow-500/20 text-yellow-700 dark:text-yellow-400 border-yellow-500/30"
        >
          <Pause className="mr-1 h-3 w-3" />
          Paused
        </Badge>
      );
    case "failed":
      return (
        <Badge
          variant="secondary"
          className="bg-red-500/20 text-red-700 dark:text-red-400 border-red-500/30"
        >
          <XCircle className="mr-1 h-3 w-3" />
          Failed
        </Badge>
      );
    case "stopped":
    default:
      return (
        <Badge
          variant="secondary"
          className="bg-gray-500/20 text-gray-700 dark:text-gray-400 border-gray-500/30"
        >
          Stopped
        </Badge>
      );
  }
}

export default function ConnectorsPage() {
  const { data: connectors = [], isLoading: loading } = useConnectors();
  const deleteConnector = useDeleteConnector();
  const pauseConnector = usePauseConnector();
  const resumeConnector = useResumeConnector();

  const totalConnectors = connectors.length;
  const runningCount = connectors.filter((c) => c.state === "running").length;
  const failedCount = connectors.filter((c) => c.state === "failed").length;
  const totalRecords = connectors.reduce(
    (sum, c) => sum + c.recordsProcessed,
    0
  );

  const columns: Column<Connector>[] = [
    {
      id: "name",
      header: "Name",
      accessorKey: "name",
      sortable: true,
      cell: (row) => (
        <code className="text-xs bg-muted px-2 py-1 rounded font-mono">
          {row.name}
        </code>
      ),
    },
    {
      id: "type",
      header: "Type",
      accessorKey: "connectorType",
      sortable: true,
      cell: (row) => (
        <Badge variant="outline" className="text-xs">
          {row.connectorType}
        </Badge>
      ),
    },
    {
      id: "class",
      header: "Class",
      accessorKey: "connectorClass",
      sortable: true,
      cell: (row) => (
        <span className="text-sm text-muted-foreground">
          {row.connectorClass}
        </span>
      ),
    },
    {
      id: "topics",
      header: "Topics",
      cell: (row) => (
        <div className="flex flex-wrap gap-1 max-w-50">
          {row.topics.slice(0, 3).map((topic) => (
            <Badge key={topic} variant="outline" className="text-xs">
              {topic}
            </Badge>
          ))}
          {row.topics.length > 3 && (
            <Badge variant="secondary" className="text-xs">
              +{row.topics.length - 3} more
            </Badge>
          )}
        </div>
      ),
    },
    {
      id: "state",
      header: "State",
      accessorKey: "state",
      sortable: true,
      cell: (row) => getStateBadge(row.state),
    },
    {
      id: "records",
      header: "Records",
      accessorKey: "recordsProcessed",
      sortable: true,
      cell: (row) => (
        <span className="font-mono text-sm">
          {row.recordsProcessed.toLocaleString()}
        </span>
      ),
    },
    {
      id: "actions",
      header: "Actions",
      cell: (row) => (
        <div className="flex items-center gap-1">
          {row.state === "running" ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => pauseConnector.mutate(row.name)}
              disabled={pauseConnector.isPending}
              title="Pause connector"
            >
              <Pause className="h-4 w-4" />
            </Button>
          ) : (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => resumeConnector.mutate(row.name)}
              disabled={resumeConnector.isPending}
              title="Resume connector"
            >
              <Play className="h-4 w-4" />
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={() => deleteConnector.mutate(row.name)}
            disabled={deleteConnector.isPending}
            className="text-red-600 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-950"
            title="Delete connector"
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        </div>
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
      title="Connectors"
      description="Manage source and sink connectors for data integration"
    >
      {/* Stats Overview */}
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
              title="Total Connectors"
              value={totalConnectors.toString()}
              description="Registered connectors"
              icon={Plug}
            />
            <MetricCard
              title="Running"
              value={runningCount.toString()}
              description="Active connectors"
              icon={CheckCircle2}
              className="border-green-500/30"
            />
            <MetricCard
              title="Failed"
              value={failedCount.toString()}
              description="Connectors with errors"
              icon={AlertTriangle}
              className={failedCount > 0 ? "border-red-500/30" : ""}
            />
            <MetricCard
              title="Records Processed"
              value={totalRecords.toLocaleString()}
              description="Total records"
              icon={Activity}
            />
          </>
        )}
      </div>

      {/* Connectors Table */}
      <Card className="mt-6">
        <div className="p-6">
          <DataTable
            data={connectors}
            columns={columns}
            isLoading={loading}
            loadingRows={5}
            searchKey="name"
            searchPlaceholder="Search connectors..."
            pageSize={10}
            pageSizeOptions={[10, 25, 50]}
            getRowId={(row) => row.name}
            emptyState={
              <EmptyState
                icon={Plug}
                title="No connectors"
                description="Connectors enable data integration between StreamHouse and external systems. Create a connector to get started."
              />
            }
          />
        </div>
      </Card>
    </DashboardLayout>
  );
}
