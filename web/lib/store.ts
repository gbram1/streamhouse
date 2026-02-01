/**
 * Global State Management with Zustand
 */

import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { DashboardPreferences, TimeRange } from './types';

interface AppState {
  // Theme
  theme: 'light' | 'dark' | 'system';
  setTheme: (theme: 'light' | 'dark' | 'system') => void;

  // Auto-refresh
  autoRefresh: boolean;
  toggleAutoRefresh: () => void;
  refreshInterval: number;
  setRefreshInterval: (interval: number) => void;

  // Time range
  timeRange: TimeRange;
  setTimeRange: (range: TimeRange) => void;

  // Favorites
  favoriteTopics: string[];
  toggleFavoriteTopic: (topic: string) => void;
  favoriteConsumerGroups: string[];
  toggleFavoriteConsumerGroup: (groupId: string) => void;

  // Sidebar
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;

  // Notifications
  notifications: Notification[];
  addNotification: (notification: Omit<Notification, 'id' | 'timestamp'>) => void;
  removeNotification: (id: string) => void;
}

interface Notification {
  id: string;
  type: 'info' | 'success' | 'warning' | 'error';
  title: string;
  message?: string;
  timestamp: number;
}

export const useAppStore = create<AppState>()(
  persist(
    (set) => ({
      // Theme
      theme: 'system',
      setTheme: (theme) => set({ theme }),

      // Auto-refresh
      autoRefresh: true,
      toggleAutoRefresh: () => set((state) => ({ autoRefresh: !state.autoRefresh })),
      refreshInterval: 10000,
      setRefreshInterval: (interval) => set({ refreshInterval: interval }),

      // Time range
      timeRange: '1h',
      setTimeRange: (range) => set({ timeRange: range }),

      // Favorites
      favoriteTopics: [],
      toggleFavoriteTopic: (topic) =>
        set((state) => ({
          favoriteTopics: state.favoriteTopics.includes(topic)
            ? state.favoriteTopics.filter((t) => t !== topic)
            : [...state.favoriteTopics, topic],
        })),
      favoriteConsumerGroups: [],
      toggleFavoriteConsumerGroup: (groupId) =>
        set((state) => ({
          favoriteConsumerGroups: state.favoriteConsumerGroups.includes(groupId)
            ? state.favoriteConsumerGroups.filter((g) => g !== groupId)
            : [...state.favoriteConsumerGroups, groupId],
        })),

      // Sidebar
      sidebarOpen: true,
      setSidebarOpen: (open) => set({ sidebarOpen: open }),

      // Notifications
      notifications: [],
      addNotification: (notification) =>
        set((state) => ({
          notifications: [
            ...state.notifications,
            {
              ...notification,
              id: Math.random().toString(36).substring(7),
              timestamp: Date.now(),
            },
          ],
        })),
      removeNotification: (id) =>
        set((state) => ({
          notifications: state.notifications.filter((n) => n.id !== id),
        })),
    }),
    {
      name: 'streamhouse-preferences',
      partialize: (state) => ({
        theme: state.theme,
        autoRefresh: state.autoRefresh,
        refreshInterval: state.refreshInterval,
        timeRange: state.timeRange,
        favoriteTopics: state.favoriteTopics,
        favoriteConsumerGroups: state.favoriteConsumerGroups,
        sidebarOpen: state.sidebarOpen,
      }),
    }
  )
);
