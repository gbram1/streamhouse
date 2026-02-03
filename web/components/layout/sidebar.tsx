'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { cn } from '@/lib/utils';
import {
  LayoutDashboard,
  MessageSquare,
  Users,
  Activity,
  Database,
  Server,
  FileCode2,
  BarChart3,
  HardDrive,
  Bell,
  Terminal,
} from 'lucide-react';

const navigation = [
  {
    name: 'Overview',
    href: '/',
    icon: LayoutDashboard,
  },
  {
    name: 'Topics',
    href: '/topics',
    icon: MessageSquare,
  },
  {
    name: 'Consumers',
    href: '/consumers',
    icon: Users,
  },
  {
    name: 'Producers',
    href: '/producers',
    icon: Activity,
  },
  {
    name: 'Partitions',
    href: '/partitions',
    icon: Database,
  },
  {
    name: 'Agents',
    href: '/agents',
    icon: Server,
  },
  {
    name: 'Schemas',
    href: '/schemas',
    icon: FileCode2,
  },
  {
    name: 'SQL Workbench',
    href: '/sql',
    icon: Terminal,
  },
  {
    name: 'Performance',
    href: '/performance',
    icon: BarChart3,
  },
  {
    name: 'Storage',
    href: '/storage',
    icon: HardDrive,
  },
  {
    name: 'Monitoring',
    href: '/monitoring',
    icon: Bell,
  },
];

export function Sidebar() {
  const pathname = usePathname();

  return (
    <div className="flex h-full w-64 flex-col border-r bg-sidebar">
      {/* Logo */}
      <div className="flex h-16 items-center border-b px-6">
        <h1 className="text-xl font-bold text-sidebar-foreground">
          StreamHouse
        </h1>
      </div>

      {/* Navigation */}
      <nav className="flex-1 space-y-1 overflow-y-auto p-4">
        {navigation.map((item) => {
          const isActive = pathname === item.href;
          const Icon = item.icon;

          return (
            <Link
              key={item.name}
              href={item.href}
              className={cn(
                'flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors',
                isActive
                  ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                  : 'text-sidebar-foreground hover:bg-sidebar-accent/50'
              )}
            >
              <Icon className="h-5 w-5" />
              <span>{item.name}</span>
            </Link>
          );
        })}
      </nav>

      {/* Footer */}
      <div className="border-t p-4">
        <div className="flex items-center gap-3">
          <div className="h-2 w-2 rounded-full bg-green-500" />
          <span className="text-xs text-sidebar-foreground">Connected</span>
        </div>
      </div>
    </div>
  );
}
