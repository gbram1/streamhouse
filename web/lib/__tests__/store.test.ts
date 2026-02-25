import { describe, it, expect, beforeEach } from 'vitest';
import { useAppStore } from '../store';

describe('useAppStore', () => {
  beforeEach(() => {
    // Reset store to defaults between tests
    useAppStore.setState({
      theme: 'system',
      autoRefresh: true,
      refreshInterval: 10000,
      timeRange: '1h',
      favoriteTopics: [],
      favoriteConsumerGroups: [],
      sidebarOpen: true,
      notifications: [],
    });
  });

  describe('theme', () => {
    it('defaults to system', () => {
      expect(useAppStore.getState().theme).toBe('system');
    });

    it('sets theme', () => {
      useAppStore.getState().setTheme('dark');
      expect(useAppStore.getState().theme).toBe('dark');
    });
  });

  describe('autoRefresh', () => {
    it('defaults to true', () => {
      expect(useAppStore.getState().autoRefresh).toBe(true);
    });

    it('toggles auto refresh', () => {
      useAppStore.getState().toggleAutoRefresh();
      expect(useAppStore.getState().autoRefresh).toBe(false);
      useAppStore.getState().toggleAutoRefresh();
      expect(useAppStore.getState().autoRefresh).toBe(true);
    });

    it('sets refresh interval', () => {
      useAppStore.getState().setRefreshInterval(5000);
      expect(useAppStore.getState().refreshInterval).toBe(5000);
    });
  });

  describe('timeRange', () => {
    it('defaults to 1h', () => {
      expect(useAppStore.getState().timeRange).toBe('1h');
    });

    it('sets time range', () => {
      useAppStore.getState().setTimeRange('24h');
      expect(useAppStore.getState().timeRange).toBe('24h');
    });
  });

  describe('favoriteTopics', () => {
    it('starts empty', () => {
      expect(useAppStore.getState().favoriteTopics).toEqual([]);
    });

    it('adds a favorite topic', () => {
      useAppStore.getState().toggleFavoriteTopic('my-topic');
      expect(useAppStore.getState().favoriteTopics).toEqual(['my-topic']);
    });

    it('removes a favorite topic on second toggle', () => {
      useAppStore.getState().toggleFavoriteTopic('my-topic');
      useAppStore.getState().toggleFavoriteTopic('my-topic');
      expect(useAppStore.getState().favoriteTopics).toEqual([]);
    });

    it('handles multiple favorites', () => {
      useAppStore.getState().toggleFavoriteTopic('topic-a');
      useAppStore.getState().toggleFavoriteTopic('topic-b');
      expect(useAppStore.getState().favoriteTopics).toEqual(['topic-a', 'topic-b']);
    });
  });

  describe('favoriteConsumerGroups', () => {
    it('toggles consumer group favorites', () => {
      useAppStore.getState().toggleFavoriteConsumerGroup('group-1');
      expect(useAppStore.getState().favoriteConsumerGroups).toEqual(['group-1']);

      useAppStore.getState().toggleFavoriteConsumerGroup('group-1');
      expect(useAppStore.getState().favoriteConsumerGroups).toEqual([]);
    });
  });

  describe('sidebar', () => {
    it('defaults to open', () => {
      expect(useAppStore.getState().sidebarOpen).toBe(true);
    });

    it('sets sidebar state', () => {
      useAppStore.getState().setSidebarOpen(false);
      expect(useAppStore.getState().sidebarOpen).toBe(false);
    });
  });

  describe('notifications', () => {
    it('starts empty', () => {
      expect(useAppStore.getState().notifications).toEqual([]);
    });

    it('adds a notification', () => {
      useAppStore.getState().addNotification({
        type: 'info',
        title: 'Test',
        message: 'Hello',
      });
      const notifications = useAppStore.getState().notifications;
      expect(notifications).toHaveLength(1);
      expect(notifications[0].type).toBe('info');
      expect(notifications[0].title).toBe('Test');
      expect(notifications[0].message).toBe('Hello');
      expect(notifications[0].id).toBeDefined();
      expect(notifications[0].timestamp).toBeDefined();
    });

    it('removes a notification by id', () => {
      useAppStore.getState().addNotification({
        type: 'success',
        title: 'Done',
      });
      const id = useAppStore.getState().notifications[0].id;
      useAppStore.getState().removeNotification(id);
      expect(useAppStore.getState().notifications).toHaveLength(0);
    });

    it('removes only the target notification', () => {
      useAppStore.getState().addNotification({ type: 'info', title: 'A' });
      useAppStore.getState().addNotification({ type: 'error', title: 'B' });
      const idA = useAppStore.getState().notifications[0].id;
      useAppStore.getState().removeNotification(idA);
      expect(useAppStore.getState().notifications).toHaveLength(1);
      expect(useAppStore.getState().notifications[0].title).toBe('B');
    });
  });
});
