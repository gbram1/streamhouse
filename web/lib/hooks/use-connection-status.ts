/**
 * Shared hook to check if the StreamHouse API server is reachable.
 * Used across dashboard pages to distinguish "no data" from "not connected".
 */

import { useQuery } from '@tanstack/react-query';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

export function useConnectionStatus() {
  const { data: connected, isLoading } = useQuery({
    queryKey: ['connection-status'],
    queryFn: async () => {
      try {
        const response = await fetch(`${API_URL}/health`, {
          method: 'GET',
          signal: AbortSignal.timeout(3000),
        });
        return response.ok;
      } catch {
        // Try fallback endpoint
        try {
          const response = await fetch(`${API_URL}/api/v1/topics`, {
            method: 'GET',
            signal: AbortSignal.timeout(3000),
          });
          return response.ok;
        } catch {
          return false;
        }
      }
    },
    refetchInterval: 10000,
    retry: false,
  });

  return {
    isConnected: connected ?? false,
    isChecking: isLoading,
  };
}
