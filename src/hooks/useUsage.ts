// Usage tracking hook
// React hook for usage statistics

import { useState, useEffect, useCallback } from 'react';
import { getUsageStats, getUsageHistory } from '../ipc/commands';
import { onUsageLimitReached } from '../ipc/events';
import type { UsageStats, DailyUsage } from '../ipc/types';

export function useUsage() {
  const [stats, setStats] = useState<UsageStats | null>(null);
  const [history, setHistory] = useState<DailyUsage[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [limitReached, setLimitReached] = useState(false);

  // Load initial stats
  useEffect(() => {
    async function init() {
      try {
        const [usageStats, usageHistory] = await Promise.all([
          getUsageStats(),
          getUsageHistory(30)
        ]);
        setStats(usageStats);
        setHistory(usageHistory);
        setLimitReached(usageStats.currentMonth.percentage >= 100);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load usage');
      } finally {
        setLoading(false);
      }
    }
    init();
  }, []);

  // Subscribe to limit reached event
  useEffect(() => {
    let unsub: (() => void) | undefined;
    
    onUsageLimitReached((event) => {
      setLimitReached(true);
      setStats(prev => prev ? {
        ...prev,
        currentMonth: { 
          ...prev.currentMonth, 
          used: event.minutesUsed, 
          limit: event.minutesLimit, 
          percentage: 100 
        }
      } : prev);
    }).then(fn => { unsub = fn; });

    return () => { unsub?.(); };
  }, []);

  const refresh = useCallback(async () => {
    try {
      setLoading(true);
      const [usageStats, usageHistory] = await Promise.all([
        getUsageStats(),
        getUsageHistory(30)
      ]);
      setStats(usageStats);
      setHistory(usageHistory);
      setLimitReached(usageStats.currentMonth.percentage >= 100);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to refresh usage');
    } finally {
      setLoading(false);
    }
  }, []);

  return {
    stats,
    history,
    loading,
    error,
    limitReached,
    refresh,
    clearError: () => setError(null),
  };
}
