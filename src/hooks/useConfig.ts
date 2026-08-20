// Configuration hook
// React hook for app configuration

import { useState, useEffect, useCallback } from 'react';
import { getConfig, saveConfig as saveConfigCommand, exportConfig, importConfig } from '../ipc/commands';
import type { AppConfig } from '../ipc/types';

const defaultConfig: AppConfig = {
  languages: {
    systemSourceLang: 'en',
    systemTargetLang: 'es',
    userSourceLang: 'es',
    userTargetLang: 'en',
  },
  devices: {},
  preferences: {
    startMinimized: false,
    autoStart: false,
    theme: 'dark',
    enableSentry: false,
  },
};

const configEventEmitter = new EventTarget();

export function useConfig() {
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [hasChanges, setHasChanges] = useState(false);

  // Listen for global config updates
  useEffect(() => {
    const handleConfigUpdate = (e: Event) => {
      const customEvent = e as CustomEvent<AppConfig>;
      setConfig(customEvent.detail);
    };
    configEventEmitter.addEventListener('configUpdated', handleConfigUpdate);
    return () => {
      configEventEmitter.removeEventListener('configUpdated', handleConfigUpdate);
    };
  }, []);

  // Load initial config
  useEffect(() => {
    async function init() {
      try {
        const loadedConfig = await getConfig();
        setConfig(loadedConfig);
      } catch (err) {
        console.warn('Failed to load config, using defaults:', err);
      } finally {
        setLoading(false);
      }
    }
    init();
  }, []);

  const updateConfig = useCallback((updates: Partial<AppConfig>) => {
    setConfig(prev => {
      const newConfig = { ...prev, ...updates };
      setHasChanges(true);
      return newConfig;
    });
  }, []);

  const saveConfig = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      await saveConfigCommand(config);
      setHasChanges(false);
      // Notify other hooks of the saved config
      configEventEmitter.dispatchEvent(new CustomEvent('configUpdated', { detail: config }));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save config');
    } finally {
      setLoading(false);
    }
  }, [config]);

  const exportToFile = useCallback(async (path: string) => {
    try {
      setLoading(true);
      await exportConfig(path);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to export config');
    } finally {
      setLoading(false);
    }
  }, []);

  const importFromFile = useCallback(async (path: string) => {
    try {
      setLoading(true);
      const imported = await importConfig(path);
      setConfig(imported);
      setHasChanges(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to import config');
    } finally {
      setLoading(false);
    }
  }, []);

  return {
    config,
    loading,
    error,
    hasChanges,
    updateConfig,
    saveConfig,
    exportToFile,
    importFromFile,
    clearError: () => setError(null),
  };
}
