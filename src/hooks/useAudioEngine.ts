/**
 * Audio Engine Hook
 * 
 * React hook for interacting with the audio engine via Tauri IPC.
 * Manages audio device enumeration, channel state, metrics, and VB-Cable status.
 * 
 * @module hooks/useAudioEngine
 * @see Requirement 12.2 - Replace fetch/WebSocket with Tauri IPC
 * @see Requirement 14.1 - Enumerate dynamically all audio devices
 * @see Requirement 4.1 - Detect VB-Cable installation
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { 
  enumerateAudioDevices, 
  startSystemChannel, 
  startUserChannel,
  stopChannel,
  changeAudioDevice,
  getAudioState,
  getVbCableStatus,
  getVirtualAudioStatus
} from '../ipc/commands';
import { 
  onAudioMetrics, 
  onChannelStateChanged, 
  onDeviceChanged,
  onGeminiError
} from '../ipc/events';
import type { 
  AudioDevice, 
  AudioMetrics, 
  ChannelState, 
  ChannelConfig, 
  ChannelType,
  VBCableStatus,
  VirtualAudioStatus,
  PauseReason
} from '../ipc/types';

/** State returned by the useAudioEngine hook */
export interface AudioEngineState {
  /** List of available audio devices */
  devices: AudioDevice[];
  /** State of the system audio channel (meeting → user) */
  systemChannelState: ChannelState;
  /** State of the user microphone channel (user → meeting) */
  userChannelState: ChannelState;
  /** Current audio metrics (null if no channel active) */
  metrics: AudioMetrics | null;
  /** Reason for pause if channel is paused */
  pauseReason: PauseReason | null;
  /** VB-Cable installation status (Windows only) */
  vbCableStatus: VBCableStatus | null;
  /** Virtual audio status (macOS only) */
  virtualAudioStatus: VirtualAudioStatus | null;
  /** Whether initial data is loading */
  loading: boolean;
  /** Error message if any operation failed */
  error: string | null;
  /** Whether at least one channel is active */
  isTranslating: boolean;
  /** Input devices (microphones) */
  inputDevices: AudioDevice[];
  /** Output devices (speakers/headphones) */
  outputDevices: AudioDevice[];
  /** Loopback devices (for system audio capture) */
  loopbackDevices: AudioDevice[];
}

/** Actions available from the useAudioEngine hook */
export interface AudioEngineActions {
  /** Start the system audio channel (meeting → user translation) */
  startSystem: (config: ChannelConfig, token?: string) => Promise<void>;
  /** Start the user microphone channel (user → meeting translation) */
  startUser: (config: ChannelConfig, token?: string) => Promise<void>;
  /** Stop a specific channel */
  stop: (channel: ChannelType) => Promise<void>;
  /** Stop all active channels */
  stopAll: () => Promise<void>;
  /** Change audio device for a channel without stopping translation */
  changeDevice: (channel: ChannelType, deviceId: string) => Promise<void>;
  /** Refresh the list of available devices */
  refreshDevices: () => Promise<void>;
  /** Refresh VB-Cable status (Windows) */
  refreshVbCableStatus: () => Promise<void>;
  /** Refresh Virtual Audio status (macOS) */
  refreshVirtualAudioStatus: () => Promise<void>;
  /** Clear the current error */
  clearError: () => void;
}

export type UseAudioEngineReturn = AudioEngineState & AudioEngineActions;

/**
 * Hook for managing audio engine state and actions.
 * 
 * Provides:
 * - Device enumeration with categorization (input/output/loopback)
 * - Channel state management (system and user channels)
 * - Real-time audio metrics subscription
 * - VB-Cable status detection (Windows)
 * - Automatic event subscription cleanup
 * 
 * @example
 * ```tsx
 * function AudioPanel() {
 *   const { 
 *     devices, 
 *     systemChannelState, 
 *     metrics,
 *     vbCableStatus,
 *     startSystem, 
 *     stop 
 *   } = useAudioEngine();
 *   
 *   if (!vbCableStatus?.isInstalled) {
 *     return <VbCableInstallPrompt />;
 *   }
 *   
 *   return (
 *     <div>
 *       <ChannelStatus state={systemChannelState} />
 *       {metrics && <MetricsDisplay metrics={metrics} />}
 *     </div>
 *   );
 * }
 * ```
 */
export function useAudioEngine(): UseAudioEngineReturn {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [systemChannelState, setSystemChannelState] = useState<ChannelState>({ type: 'inactive' });
  const [userChannelState, setUserChannelState] = useState<ChannelState>({ type: 'inactive' });
  const [metrics, setMetrics] = useState<AudioMetrics | null>(null);
  const [pauseReason, setPauseReason] = useState<PauseReason | null>(null);
  const [vbCableStatus, setVbCableStatus] = useState<VBCableStatus | null>(null);
  const [virtualAudioStatus, setVirtualAudioStatus] = useState<VirtualAudioStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  // Track mounted state to avoid state updates after unmount
  const mountedRef = useRef(true);
  
  // React 18 Strict Mode compatibility: reset to true on mount
  useEffect(() => {
    mountedRef.current = true;
  }, []);

  // Derived state
  const isTranslating = systemChannelState.type === 'active' || userChannelState.type === 'active';
  
  // Categorize devices
  const inputDevices = devices.filter(d => d.deviceType === 'input');
  const outputDevices = devices.filter(d => d.deviceType === 'output');
  const loopbackDevices = devices.filter(d => d.deviceType === 'loopback');

  // Load initial state, devices, and VB-Cable/Virtual Audio status
  useEffect(() => {
    async function init() {
      try {
        console.log('Fetching audio engine initial state...');
        const p1 = enumerateAudioDevices().then(res => { console.log('enumerateAudioDevices DONE'); return res; });
        const p2 = getAudioState().then(res => { console.log('getAudioState DONE'); return res; });
        const p3 = getVbCableStatus().then(res => { console.log('getVbCableStatus DONE'); return res; }).catch(() => { console.log('getVbCableStatus FAILED'); return null; });
        const p4 = getVirtualAudioStatus().then(res => { console.log('getVirtualAudioStatus DONE'); return res; }).catch(() => { console.log('getVirtualAudioStatus FAILED'); return null; });

        const [deviceList, state, vbStatus, virtualStatus] = await Promise.all([p1, p2, p3, p4]);
        console.log('All audio engine fetches complete!');
        
        // removed mountedRef check
        setDevices(deviceList);
        setSystemChannelState(state.systemChannel);
        setUserChannelState(state.userChannel);
        setMetrics(state.metrics);
        setPauseReason(state.pauseReason);
        setVbCableStatus(vbStatus);
        setVirtualAudioStatus(virtualStatus);
      } catch (err) {
        if (!mountedRef.current) return;
        setError(err instanceof Error ? err.message : 'Error al inicializar el audio');
      } finally {
        if (mountedRef.current) {
          setLoading(false);
        }
      }
    }
    init();

    return () => {
      mountedRef.current = false;
    };
  }, []);

  // Subscribe to events
  useEffect(() => {
    const unsubscribers: Array<() => void> = [];

    // Audio metrics (updated every 100ms during active capture)
    onAudioMetrics((m) => {
      if (mountedRef.current) setMetrics(m);
    }).then(unsub => unsubscribers.push(unsub));

    // Channel state changes
    onChannelStateChanged((channel, state) => {
      if (!mountedRef.current) return;
      if (channel === 'system') setSystemChannelState(state);
      if (channel === 'user') setUserChannelState(state);
      
      // Clear metrics when both channels become inactive
      if (state.type === 'inactive') {
        // Check if the other channel is also inactive
        if (channel === 'system') {
          setUserChannelState(prev => {
            if (prev.type === 'inactive') setMetrics(null);
            return prev;
          });
        } else {
          setSystemChannelState(prev => {
            if (prev.type === 'inactive') setMetrics(null);
            return prev;
          });
        }
      }
    }).then(unsub => unsubscribers.push(unsub));

    // Device connection/disconnection events
    onDeviceChanged((action, device) => {
      if (!mountedRef.current) return;
      setDevices(prev => {
        if (action === 'connected') {
          // Avoid duplicates
          if (prev.some(d => d.id === device.id)) return prev;
          return [...prev, device];
        }
        return prev.filter(d => d.id !== device.id);
      });
    }).then(unsub => unsubscribers.push(unsub));

    // Gemini errors
    onGeminiError((channel, errorMsg) => {
      if (!mountedRef.current) return;
      setError(`Error Gemini (${channel}): ${errorMsg}`);
    }).then(unsub => unsubscribers.push(unsub));

    return () => {
      unsubscribers.forEach(unsub => unsub());
    };
  }, []);

  const startSystem = useCallback(async (config: ChannelConfig, token?: string) => {
    try {
      setError(null);
      await startSystemChannel(config, token);
    } catch (err) {
      const message = err instanceof Error ? err.message : typeof err === 'string' ? err : 'Error al iniciar canal de sistema';
      setError(message);
      throw new Error(message);
    }
  }, []);

  const startUser = useCallback(async (config: ChannelConfig, token?: string) => {
    try {
      setError(null);
      await startUserChannel(config, token);
    } catch (err) {
      const message = err instanceof Error ? err.message : typeof err === 'string' ? err : 'Error al iniciar canal de usuario';
      setError(message);
      throw new Error(message);
    }
  }, []);

  const stop = useCallback(async (channel: ChannelType) => {
    try {
      setError(null);
      await stopChannel(channel);
    } catch (err) {
      const message = err instanceof Error ? err.message : typeof err === 'string' ? err : 'Error al detener canal';
      setError(message);
      throw new Error(message);
    }
  }, []);

  const stopAll = useCallback(async () => {
    try {
      setError(null);
      await Promise.all([
        stopChannel('system').catch(() => {}),
        stopChannel('user').catch(() => {})
      ]);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Error al detener canales';
      setError(message);
    }
  }, []);

  const changeDevice = useCallback(async (channel: ChannelType, deviceId: string) => {
    try {
      setError(null);
      await changeAudioDevice(channel, deviceId);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Error al cambiar dispositivo';
      setError(message);
      throw new Error(message);
    }
  }, []);

  const refreshDevices = useCallback(async () => {
    try {
      setError(null);
      const deviceList = await enumerateAudioDevices();
      if (mountedRef.current) {
        setDevices(deviceList);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Error al actualizar dispositivos';
      setError(message);
    }
  }, []);

  const refreshVbCableStatus = useCallback(async () => {
    try {
      const status = await getVbCableStatus();
      if (mountedRef.current) {
        setVbCableStatus(status);
      }
    } catch (err) {
      // VB-Cable not available (likely macOS)
      if (mountedRef.current) {
        setVbCableStatus(null);
      }
    }
  }, []);

  const refreshVirtualAudioStatus = useCallback(async () => {
    try {
      const status = await getVirtualAudioStatus();
      if (mountedRef.current) {
        setVirtualAudioStatus(status);
      }
    } catch (err) {
      // Virtual Audio not available (likely Windows)
      if (mountedRef.current) {
        setVirtualAudioStatus(null);
      }
    }
  }, []);

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  return {
    // State
    devices,
    systemChannelState,
    userChannelState,
    metrics,
    pauseReason,
    vbCableStatus,
    virtualAudioStatus,
    loading,
    error,
    isTranslating,
    inputDevices,
    outputDevices,
    loopbackDevices,
    // Actions
    startSystem,
    startUser,
    stop,
    stopAll,
    changeDevice,
    refreshDevices,
    refreshVbCableStatus,
    refreshVirtualAudioStatus,
    clearError,
  };
}
