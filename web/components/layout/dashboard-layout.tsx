'use client';

import { Sidebar } from './sidebar';
import { Header } from './header';
import { useConnectionStatus } from '@/lib/hooks/use-connection-status';
import { WifiOff } from 'lucide-react';

interface DashboardLayoutProps {
  children: React.ReactNode;
  title: React.ReactNode;
  description?: string;
  actions?: React.ReactNode;
}

export function DashboardLayout({
  children,
  title,
  description,
  actions,
}: DashboardLayoutProps) {
  const { isConnected, isChecking } = useConnectionStatus();

  return (
    <div className="flex h-screen overflow-hidden">
      <Sidebar />
      <div className="flex flex-1 flex-col overflow-hidden">
        <Header title={title} description={description} actions={actions} />
        <main className="flex-1 overflow-y-auto bg-background p-6">
          {!isChecking && !isConnected && (
            <div className="mb-6 flex items-center gap-3 rounded-lg border border-yellow-500/30 bg-yellow-500/5 p-4">
              <WifiOff className="h-5 w-5 text-yellow-600 shrink-0" />
              <div>
                <span className="text-sm font-medium text-yellow-700 dark:text-yellow-400">
                  Not connected to StreamHouse server
                </span>
                <p className="text-xs text-muted-foreground">
                  Metrics and data below may be unavailable. Start the server or check your connection to{' '}
                  {process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080'}
                </p>
              </div>
            </div>
          )}
          {children}
        </main>
      </div>
    </div>
  );
}
