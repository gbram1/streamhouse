"use client"

import * as React from "react"
import { cn } from "@/lib/utils"
import { Button } from "./button"
import {
  Database,
  Users,
  FileCode2,
  Bell,
  Search,
  Plus,
  AlertTriangle,
  Inbox,
  Server,
  Activity,
  type LucideIcon
} from "lucide-react"

interface EmptyStateProps {
  icon?: LucideIcon
  title: string
  description?: string
  action?: {
    label: string
    onClick?: () => void
    href?: string
  }
  secondaryAction?: {
    label: string
    onClick?: () => void
    href?: string
  }
  className?: string
  variant?: "default" | "compact" | "minimal"
}

const presetIcons: Record<string, LucideIcon> = {
  topics: Database,
  consumers: Users,
  schemas: FileCode2,
  alerts: Bell,
  search: Search,
  agents: Server,
  activity: Activity,
  empty: Inbox,
  warning: AlertTriangle,
}

export function EmptyState({
  icon: Icon = Inbox,
  title,
  description,
  action,
  secondaryAction,
  className,
  variant = "default"
}: EmptyStateProps) {
  const ActionButton = ({ actionConfig, variant: btnVariant = "default" }: {
    actionConfig: NonNullable<EmptyStateProps['action']>
    variant?: "default" | "outline" | "ghost"
  }) => {
    if (actionConfig.href) {
      return (
        <a href={actionConfig.href}>
          <Button variant={btnVariant}>
            {btnVariant === "default" && <Plus className="mr-2 h-4 w-4" />}
            {actionConfig.label}
          </Button>
        </a>
      )
    }
    return (
      <Button variant={btnVariant} onClick={actionConfig.onClick}>
        {btnVariant === "default" && <Plus className="mr-2 h-4 w-4" />}
        {actionConfig.label}
      </Button>
    )
  }

  if (variant === "minimal") {
    return (
      <div className={cn("text-center py-6", className)}>
        <p className="text-sm text-muted-foreground">{title}</p>
      </div>
    )
  }

  if (variant === "compact") {
    return (
      <div className={cn("flex flex-col items-center justify-center py-8 text-center", className)}>
        <div className="rounded-full bg-muted p-3 mb-3">
          <Icon className="h-5 w-5 text-muted-foreground" />
        </div>
        <p className="text-sm font-medium">{title}</p>
        {description && (
          <p className="text-xs text-muted-foreground mt-1 max-w-[250px]">{description}</p>
        )}
        {action && (
          <div className="mt-3">
            <ActionButton actionConfig={action} />
          </div>
        )}
      </div>
    )
  }

  return (
    <div className={cn(
      "flex flex-col items-center justify-center py-12 px-4 text-center",
      className
    )}>
      <div className="rounded-full bg-muted p-4 mb-4">
        <Icon className="h-8 w-8 text-muted-foreground" />
      </div>
      <h3 className="text-lg font-semibold mb-1">{title}</h3>
      {description && (
        <p className="text-sm text-muted-foreground max-w-[400px] mb-4">{description}</p>
      )}
      {(action || secondaryAction) && (
        <div className="flex items-center gap-3 mt-2">
          {action && <ActionButton actionConfig={action} />}
          {secondaryAction && <ActionButton actionConfig={secondaryAction} variant="outline" />}
        </div>
      )}
    </div>
  )
}

// Preset empty states for common scenarios
export function EmptyTopics({ onCreateTopic }: { onCreateTopic?: () => void }) {
  return (
    <EmptyState
      icon={Database}
      title="No topics yet"
      description="Topics are event streams that store messages. Create your first topic to start streaming data."
      action={{
        label: "Create Topic",
        onClick: onCreateTopic,
        href: onCreateTopic ? undefined : "/topics/new"
      }}
    />
  )
}

export function EmptyConsumerGroups() {
  return (
    <EmptyState
      icon={Users}
      title="No consumer groups"
      description="Consumer groups will appear here when applications start consuming from topics."
    />
  )
}

export function EmptySchemas({ onCreateSchema }: { onCreateSchema?: () => void }) {
  return (
    <EmptyState
      icon={FileCode2}
      title="No schemas registered"
      description="Register Avro, Protobuf, or JSON schemas to ensure data compatibility across producers and consumers."
      action={{
        label: "Register Schema",
        onClick: onCreateSchema
      }}
    />
  )
}

export function EmptyAlerts({ onCreateAlert }: { onCreateAlert?: () => void }) {
  return (
    <EmptyState
      icon={Bell}
      title="No alerts configured"
      description="Set up alert rules to get notified about important events like consumer lag, errors, or throughput changes."
      action={{
        label: "Create Alert Rule",
        onClick: onCreateAlert
      }}
    />
  )
}

export function EmptySearchResults({ query }: { query: string }) {
  return (
    <EmptyState
      icon={Search}
      title="No results found"
      description={`No items match "${query}". Try adjusting your search or filters.`}
      variant="compact"
    />
  )
}

export function NoDataAvailable({ message, description }: { message?: string; description?: string }) {
  return (
    <EmptyState
      icon={Activity}
      title={message || "No data available"}
      description={description || "Data will appear here once it becomes available."}
      variant="compact"
    />
  )
}
