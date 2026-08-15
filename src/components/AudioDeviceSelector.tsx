/**
 * Audio Device Selector Component
 * 
 * Allows users to select and manage audio devices for translation channels.
 * Provides dynamic enumeration, hot-swap during translation, and disconnection handling.
 * 
 * @module components/AudioDeviceSelector
 * @see Requirement 14.1 - Enumerate dynamically all audio devices (input, output, loopback)
 * @see Requirement 14.2 - Detect device connection/disconnection and update list
 * @see Requirement 14.3 - Allow selection of microphone, system output, translated output
 * @see Requirement 14.4 - Apply device change without restarting translation session
 * @see Requirement 14.5 - Pause and notify if device disconnects during translation
 */

import { useState, useCallback, useEffect } from 'react';
import { useAudioEngine } from '../hooks/useAudioEngine';
import { useConfig } from '../hooks/useConfig';
import { onDeviceChanged } from '../ipc/events';
import type { AudioDevice, ChannelType } from '../ipc/types';

/** Props for the AudioDeviceSelector component */
export interface AudioDeviceSelectorProps {
  /** Device category to select */
  deviceType: 'input' | 'output' | 'loopback';
  /** Channel this selector is associated with (for hot-swap) */
  channel?: ChannelType;
  /** Currently selected device ID */
  selectedDeviceId?: string;
  /** Callback when a device is selected */
  onDeviceSelect?: (device: AudioDevice) => void;
  /** Whether the selector is disabled */
  disabled?: boolean;
  /** Label for the selector */
  label?: string;
  /** Description text */
  description?: string;
  /** Show compact version without description */
  compact?: boolean;
}

/** Device type labels in Spanish */
const DEVICE_TYPE_LABELS: Record<AudioDeviceSelectorProps['deviceType'], string> = {
  input: 'Micrófono',
  output: 'Salida de audio',
  loopback: 'Captura del sistema',
};

/** Device type icons */
function DeviceIcon({ type }: { type: AudioDeviceSelectorProps['deviceType'] }) {
  if (type === 'input') {
    return (
      <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} 
          d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z" />
      </svg>
    );
  }
  if (type === 'output') {
    return (
      <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} 
          d="M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z" />
      </svg>
    );
  }
  // loopback
  return (
    <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} 
        d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
    </svg>
  );
}

/** Connection status indicator */
function ConnectionStatus({ 
  connected, 
  isDefault 
}: { 
  connected: boolean; 
  isDefault: boolean;
}) {
  if (!connected) {
    return (
      <span className="flex items-center gap-1 text-xs text-error">
        <span className="w-2 h-2 rounded-full bg-error" />
        Desconectado
      </span>
    );
  }
  
  if (isDefault) {
    return (
      <span className="flex items-center gap-1 text-xs text-success">
        <span className="w-2 h-2 rounded-full bg-success" />
        Por defecto
      </span>
    );
  }
  
  return (
    <span className="flex items-center gap-1 text-xs text-text-secondary">
      <span className="w-2 h-2 rounded-full bg-gray-500" />
      Conectado
    </span>
  );
}

/** Device disconnection notification */
function DisconnectionNotice({ 
  deviceName, 
  onSelectAlternative 
}: { 
  deviceName: string;
  onSelectAlternative: () => void;
}) {
  return (
    <div className="mt-2 p-3 bg-warning/10 border border-warning/30 rounded-lg">
      <div className="flex items-start gap-2">
        <svg className="w-5 h-5 text-warning flex-shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} 
            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
        </svg>
        <div className="flex-1">
          <p className="text-sm font-medium text-warning">
            Dispositivo desconectado
          </p>
          <p className="text-xs text-text-secondary mt-1">
            "{deviceName}" ya no está disponible. La traducción está pausada.
          </p>
          <button
            onClick={onSelectAlternative}
            className="mt-2 text-xs text-primary hover:underline"
          >
            Seleccionar dispositivo alternativo
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * Audio Device Selector Component
 * 
 * Provides a dropdown selector for audio devices with:
 * - Dynamic enumeration of input, output, and loopback devices
 * - Real-time updates when devices connect/disconnect
 * - Hot-swap capability during active translation
 * - Visual feedback for connection status and defaults
 * 
 * @example
 * // Basic usage for microphone selection
 * <AudioDeviceSelector
 *   deviceType="input"
 *   label="Micrófono"
 *   selectedDeviceId={config.devices.inputDevice}
 *   onDeviceSelect={(device) => updateConfig({ devices: { inputDevice: device.id }})}
 * />
 * 
 * @example
 * // With hot-swap for active channel
 * <AudioDeviceSelector
 *   deviceType="output"
 *   channel="user"
 *   label="Salida para voz traducida"
 *   selectedDeviceId={selectedOutputId}
 *   onDeviceSelect={handleOutputChange}
 * />
 */
export function AudioDeviceSelector({
  deviceType,
  channel,
  selectedDeviceId,
  onDeviceSelect,
  disabled = false,
  label,
  description,
  compact = false,
}: AudioDeviceSelectorProps) {
  const { 
    inputDevices, 
    outputDevices, 
    loopbackDevices,
    refreshDevices,
    changeDevice,
    systemChannelState,
    userChannelState,
    pauseReason,
    loading,
    error: audioError,
  } = useAudioEngine();
  
  const { config, updateConfig } = useConfig();
  
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [disconnectedDevice, setDisconnectedDevice] = useState<{ id: string; name: string } | null>(null);
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  
  // Get devices based on type
  const availableDevices = deviceType === 'input' 
    ? inputDevices 
    : deviceType === 'output' 
      ? outputDevices 
      : loopbackDevices;
  
  // Find selected device
  const selectedDevice = selectedDeviceId 
    ? availableDevices.find(d => d.id === selectedDeviceId)
    : availableDevices.find(d => d.isDefault) || availableDevices[0];
  
  // Check if channel is active (for hot-swap)
  const isChannelActive = channel === 'system' 
    ? systemChannelState.type === 'active'
    : channel === 'user'
      ? userChannelState.type === 'active'
      : false;
  
  // Check if current device is disconnected
  const isDeviceDisconnected = pauseReason?.type === 'deviceDisconnected' 
    && pauseReason.deviceId === selectedDeviceId;
  
  // Listen for device changes
  useEffect(() => {
    let unsubscribe: (() => void) | null = null;
    
    const setupListener = async () => {
      unsubscribe = await onDeviceChanged((action, device) => {
        // Device disconnected
        if (action === 'disconnected') {
          // Check if this was our selected device
          if (device.id === selectedDeviceId) {
            setDisconnectedDevice({ id: device.id, name: device.name });
            setLocalError(`Dispositivo "${device.name}" desconectado`);
          }
        }
        
        // Device connected - clear disconnection notice if it's the same device
        if (action === 'connected') {
          if (device.id === disconnectedDevice?.id) {
            setDisconnectedDevice(null);
            setLocalError(null);
          }
        }
      });
    };
    
    setupListener();
    
    return () => {
      if (unsubscribe) {
        unsubscribe();
      }
    };
  }, [selectedDeviceId, disconnectedDevice?.id]);
  
  // Clear disconnection notice when device becomes available again
  useEffect(() => {
    if (disconnectedDevice) {
      const isStillDisconnected = !availableDevices.some(d => d.id === disconnectedDevice.id);
      if (!isStillDisconnected) {
        setDisconnectedDevice(null);
        setLocalError(null);
      }
    }
  }, [availableDevices, disconnectedDevice]);
  
  // Handle device selection
  const handleSelectDevice = useCallback(async (device: AudioDevice) => {
    setLocalError(null);
    setDisconnectedDevice(null);
    setIsDropdownOpen(false);
    
    try {
      // If channel is active, perform hot-swap
      if (isChannelActive && channel) {
        await changeDevice(channel, device.id);
      }
      
      // Notify parent
      onDeviceSelect?.(device);
      
      // Update config based on device type
      if (deviceType === 'input') {
        updateConfig({ devices: { ...config.devices, inputDevice: device.id } });
      } else if (deviceType === 'output') {
        updateConfig({ devices: { ...config.devices, outputDevice: device.id } });
      } else if (deviceType === 'loopback') {
        updateConfig({ devices: { ...config.devices, systemCaptureDevice: device.id } });
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Error al cambiar dispositivo';
      setLocalError(message);
    }
  }, [isChannelActive, channel, changeDevice, onDeviceSelect, deviceType, config.devices, updateConfig]);
  
  // Handle refresh
  const handleRefresh = useCallback(async () => {
    setIsRefreshing(true);
    setLocalError(null);
    try {
      await refreshDevices();
    } catch (err) {
      setLocalError('Error al actualizar dispositivos');
    } finally {
      setIsRefreshing(false);
    }
  }, [refreshDevices]);
  
  // Handle select alternative (when device disconnected)
  const handleSelectAlternative = useCallback(() => {
    setIsDropdownOpen(true);
  }, []);
  
  const displayLabel = label || DEVICE_TYPE_LABELS[deviceType];
  const displayError = localError || audioError;
  
  return (
    <div className="space-y-2">
      {/* Label and description */}
      {!compact && (
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <DeviceIcon type={deviceType} />
            <label className="text-sm font-medium text-text">{displayLabel}</label>
          </div>
          <button
            onClick={handleRefresh}
            disabled={isRefreshing || disabled}
            className="p-1 text-text-secondary hover:text-text rounded transition disabled:opacity-50"
            title="Actualizar lista de dispositivos"
          >
            <svg 
              className={`w-4 h-4 ${isRefreshing ? 'animate-spin' : ''}`} 
              fill="none" 
              viewBox="0 0 24 24" 
              stroke="currentColor"
            >
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} 
                d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </button>
        </div>
      )}
      
      {description && !compact && (
        <p className="text-xs text-text-secondary">{description}</p>
      )}
      
      {/* Dropdown selector */}
      <div className="relative">
        <button
          onClick={() => setIsDropdownOpen(!isDropdownOpen)}
          disabled={disabled || loading}
          className={`w-full flex items-center justify-between px-3 py-2.5 
            bg-surface-hover border rounded-lg transition
            ${displayError || isDeviceDisconnected 
              ? 'border-error/50 focus:ring-error/50' 
              : 'border-border focus:ring-primary/50'
            }
            ${disabled ? 'opacity-50 cursor-not-allowed' : 'hover:bg-surface-hover/80 cursor-pointer'}
            focus:outline-none focus:ring-2
          `}
        >
          <div className="flex items-center gap-2 min-w-0">
            {compact && <DeviceIcon type={deviceType} />}
            <span className="text-text truncate">
              {selectedDevice?.name || 'Seleccionar dispositivo...'}
            </span>
          </div>
          <div className="flex items-center gap-2 flex-shrink-0">
            {selectedDevice && (
              <ConnectionStatus 
                connected={!isDeviceDisconnected} 
                isDefault={selectedDevice.isDefault} 
              />
            )}
            <svg 
              className={`w-4 h-4 text-text-secondary transition-transform ${isDropdownOpen ? 'rotate-180' : ''}`} 
              fill="none" 
              viewBox="0 0 24 24" 
              stroke="currentColor"
            >
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
            </svg>
          </div>
        </button>
        
        {/* Dropdown menu */}
        {isDropdownOpen && (
          <>
            {/* Backdrop to close dropdown */}
            <div 
              className="fixed inset-0 z-10" 
              onClick={() => setIsDropdownOpen(false)} 
            />
            
            <div className="absolute z-20 w-full mt-1 bg-surface border border-border rounded-lg shadow-lg max-h-60 overflow-y-auto">
              {availableDevices.length === 0 ? (
                <div className="px-3 py-4 text-center text-text-secondary">
                  <p className="text-sm">No hay dispositivos disponibles</p>
                  <button
                    onClick={handleRefresh}
                    className="mt-2 text-xs text-primary hover:underline"
                  >
                    Actualizar lista
                  </button>
                </div>
              ) : (
                availableDevices.map((device) => (
                  <button
                    key={device.id}
                    onClick={() => handleSelectDevice(device)}
                    className={`w-full flex items-center justify-between px-3 py-2.5 
                      text-left transition hover:bg-surface-hover
                      ${device.id === selectedDeviceId ? 'bg-primary/10' : ''}
                    `}
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="text-text truncate">{device.name}</span>
                    </div>
                    <div className="flex items-center gap-2 flex-shrink-0">
                      {device.isDefault && (
                        <span className="text-xs text-success px-1.5 py-0.5 bg-success/10 rounded">
                          Por defecto
                        </span>
                      )}
                      {device.id === selectedDeviceId && (
                        <svg className="w-4 h-4 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                        </svg>
                      )}
                    </div>
                  </button>
                ))
              )}
            </div>
          </>
        )}
      </div>
      
      {/* Error display */}
      {displayError && !isDeviceDisconnected && (
        <p className="text-xs text-error">{displayError}</p>
      )}
      
      {/* Disconnection notice */}
      {isDeviceDisconnected && disconnectedDevice && (
        <DisconnectionNotice 
          deviceName={disconnectedDevice.name}
          onSelectAlternative={handleSelectAlternative}
        />
      )}
      
      {/* Hot-swap indicator */}
      {isChannelActive && !compact && (
        <p className="text-xs text-text-secondary flex items-center gap-1">
          <span className="w-2 h-2 rounded-full bg-success animate-pulse" />
          Cambiar dispositivo sin interrumpir la traducción
        </p>
      )}
    </div>
  );
}

/**
 * Audio Device Selector Group
 * 
 * Pre-configured group of selectors for common translation setups.
 * Includes input (microphone), system capture (loopback), and output devices.
 */
export interface AudioDeviceSelectorGroupProps {
  /** Show selectors in a compact grid layout */
  compact?: boolean;
  /** Disable all selectors */
  disabled?: boolean;
}

export function AudioDeviceSelectorGroup({ 
  compact = false,
  disabled = false,
}: AudioDeviceSelectorGroupProps) {
  const { config } = useConfig();
  
  return (
    <div className={compact ? 'grid grid-cols-1 md:grid-cols-3 gap-4' : 'space-y-6'}>
      <AudioDeviceSelector
        deviceType="input"
        channel="user"
        label="Micrófono"
        description="Dispositivo para capturar tu voz"
        selectedDeviceId={config.devices.inputDevice}
        disabled={disabled}
        compact={compact}
      />
      
      <AudioDeviceSelector
        deviceType="loopback"
        channel="system"
        label="Captura del sistema"
        description="Dispositivo para capturar audio de reuniones"
        selectedDeviceId={config.devices.systemCaptureDevice}
        disabled={disabled}
        compact={compact}
      />
      
      <AudioDeviceSelector
        deviceType="output"
        label="Salida de audio"
        description="Dispositivo para reproducir audio traducido"
        selectedDeviceId={config.devices.outputDevice}
        disabled={disabled}
        compact={compact}
      />
    </div>
  );
}

export default AudioDeviceSelector;
