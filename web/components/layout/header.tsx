'use client';

import { useAppStore } from '@/lib/store';
import { Moon, Sun, Monitor, RefreshCw } from 'lucide-react';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { TimeRange } from '@/lib/types';

const timeRanges: { value: TimeRange; label: string }[] = [
  { value: '5m', label: 'Last 5 minutes' },
  { value: '15m', label: 'Last 15 minutes' },
  { value: '1h', label: 'Last hour' },
  { value: '6h', label: 'Last 6 hours' },
  { value: '24h', label: 'Last 24 hours' },
  { value: '7d', label: 'Last 7 days' },
  { value: '30d', label: 'Last 30 days' },
];

interface HeaderProps {
  title: string;
  description?: string;
}

export function Header({ title, description }: HeaderProps) {
  const theme = useAppStore((state) => state.theme);
  const setTheme = useAppStore((state) => state.setTheme);
  const timeRange = useAppStore((state) => state.timeRange);
  const setTimeRange = useAppStore((state) => state.setTimeRange);
  const autoRefresh = useAppStore((state) => state.autoRefresh);
  const toggleAutoRefresh = useAppStore((state) => state.toggleAutoRefresh);

  return (
    <div className="flex h-16 items-center justify-between border-b bg-background px-6">
      {/* Title */}
      <div>
        <h2 className="text-2xl font-bold text-foreground">{title}</h2>
        {description && (
          <p className="text-sm text-muted-foreground">{description}</p>
        )}
      </div>

      {/* Controls */}
      <div className="flex items-center gap-4">
        {/* Time Range Selector */}
        <Select value={timeRange} onValueChange={(value) => setTimeRange(value as TimeRange)}>
          <SelectTrigger className="w-[180px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {timeRanges.map((range) => (
              <SelectItem key={range.value} value={range.value}>
                {range.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {/* Auto Refresh Toggle */}
        <Button
          variant={autoRefresh ? 'default' : 'outline'}
          size="icon"
          onClick={toggleAutoRefresh}
          title={autoRefresh ? 'Disable auto-refresh' : 'Enable auto-refresh'}
        >
          <RefreshCw className={cn('h-4 w-4', autoRefresh && 'animate-spin')} />
        </Button>

        {/* Theme Switcher */}
        <div className="flex items-center gap-1 rounded-md border p-1">
          <Button
            variant={theme === 'light' ? 'default' : 'ghost'}
            size="icon"
            className="h-8 w-8"
            onClick={() => setTheme('light')}
            title="Light mode"
          >
            <Sun className="h-4 w-4" />
          </Button>
          <Button
            variant={theme === 'dark' ? 'default' : 'ghost'}
            size="icon"
            className="h-8 w-8"
            onClick={() => setTheme('dark')}
            title="Dark mode"
          >
            <Moon className="h-4 w-4" />
          </Button>
          <Button
            variant={theme === 'system' ? 'default' : 'ghost'}
            size="icon"
            className="h-8 w-8"
            onClick={() => setTheme('system')}
            title="System theme"
          >
            <Monitor className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}
