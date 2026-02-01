import { cn } from '@/lib/utils';
import { LucideIcon } from 'lucide-react';
import { Card } from './card';

interface MetricCardProps {
  title: string;
  value: string | number;
  description?: string;
  icon?: LucideIcon;
  trend?: {
    value: number;
    label: string;
  };
  className?: string;
}

export function MetricCard({
  title,
  value,
  description,
  icon: Icon,
  trend,
  className,
}: MetricCardProps) {
  return (
    <Card className={cn('p-6', className)}>
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-muted-foreground">{title}</h3>
        {Icon && <Icon className="h-5 w-5 text-muted-foreground" />}
      </div>

      <div className="mt-2">
        <div className="text-3xl font-bold">{value}</div>
        {description && (
          <p className="mt-1 text-sm text-muted-foreground">{description}</p>
        )}
        {trend && (
          <div className="mt-2 flex items-center gap-1">
            <span
              className={cn(
                'text-sm font-medium',
                trend.value > 0
                  ? 'text-green-600 dark:text-green-400'
                  : trend.value < 0
                  ? 'text-red-600 dark:text-red-400'
                  : 'text-muted-foreground'
              )}
            >
              {trend.value > 0 ? '+' : ''}
              {trend.value}%
            </span>
            <span className="text-sm text-muted-foreground">{trend.label}</span>
          </div>
        )}
      </div>
    </Card>
  );
}
