'use client';

import * as React from 'react';
import { useAppStore } from '@/lib/store';

type ThemeProviderProps = {
  children: React.ReactNode;
  attribute?: string;
  defaultTheme?: string;
  enableSystem?: boolean;
  disableTransitionOnChange?: boolean;
};

export function ThemeProvider({ children }: ThemeProviderProps) {
  const theme = useAppStore((state) => state.theme);

  React.useEffect(() => {
    const root = window.document.documentElement;

    root.classList.remove('light', 'dark');

    if (theme === 'system') {
      const systemTheme = window.matchMedia('(prefers-color-scheme: dark)').matches
        ? 'dark'
        : 'light';

      root.classList.add(systemTheme);
      return;
    }

    root.classList.add(theme);
  }, [theme]);

  return <>{children}</>;
}
