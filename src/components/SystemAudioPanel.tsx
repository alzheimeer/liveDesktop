/**
 * System Audio Panel Component
 * 
 * Panel for controlling the system audio channel (Meeting → User translation).
 * Captures audio from the system (Teams/Zoom/Meet) and translates it to the user's language.
 * 
 * @module components/SystemAudioPanel
 * @see Requirement 12.1 - Reuse components from web project
 * @see Requirement 12.2 - Replace fetch/WebSocket with Tauri IPC
 * @see Requirement 12.3 - Show two main panels with start/pause button and status indicator
 * @see Requirement 12.4 - Show volume indicators updated every 100ms
 * @see Requirement 12.5 - Show latency in ms while channel active
 * @see Requirement 12.6 - Show "--" when channel inactive
 */

import { useState, useCallback } from 'react';
import { useAudioEngine } from '../hooks/useAudioEngine';
import { useConfig } from '../hooks/useConfig';
import { useAuth } from '../hooks/useAuth';
import type { ChannelConfig } from '../ipc/types';

/** Volume meter component */
function VolumeMeter({ level, label }: { level: number; label: string }) {
  // Normalize level from dB (-60 to 0) to percentage (0 to 100)
  const percentage = Math.max(0, Math.min(100, ((level + 60) / 60) * 100));
  
  // Color based on level
  const getColor = () => {
    if (percentage > 80) return 'bg-level-high';
    if (percentage > 50) return 'bg-level-mid';
    return 'bg-level-low';
  };
  
  return (
    <div className="flex items-center gap-2">
      <span className="text-xs text-text-secondary w-12">{label}</span>
      <div className="flex-1 h-2 bg-surface-hover rounded-full overflow-hidden">
        <div 
          className={`h-full ${getColor()} transition-all duration-100`}
          style={{ width: `${percentage}%` }}
        />
      </div>
      <span className="text-xs text-text-secondary w-10 text-right">
        {level.toFixed(0)} dB
      </span>
    </div>
  );
}

/** Status indicator component */
function StatusIndicator({ state }: { state: 'inactive' | 'active' | 'error' | 'paused' }) {
  const colors = {
    inactive: 'bg-gray-500',
    active: 'bg-success',
    error: 'bg-error',
    paused: 'bg-warning',
  };
  
  const labels = {
    inactive: 'Inactivo',
    active: 'Activo',
    error: 'Error',
    paused: 'Pausado',
  };
  
  return (
    <div className="flex items-center gap-2">
      <div className={`w-3 h-3 rounded-full ${colors[state]} ${state === 'active' ? 'animate-pulse' : ''}`} />
      <span className="text-sm text-text-secondary">{labels[state]}</span>
    </div>
  );
}

export function SystemAudioPanel() {
  const { 
    systemChannelState, 
    metrics, 
    loopbackDevices,
    outputDevices,
    startSystem, 
    stop, 
    error: audioError,
    clearError,
    loading: audioLoading 
  } = useAudioEngine();
  
  const { config } = useConfig();
  const { user, hasByokKey } = useAuth();
  
  const [isStarting, setIsStarting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  
  // Determine status for indicator
  const status = systemChannelState.type === 'error' 
    ? 'error' 
    : systemChannelState.type;
  
  // Check if user can translate (has subscription or BYOK key)
  const canTranslate = user || hasByokKey;
  
  const handleToggle = useCallback(async () => {
    if (systemChannelState.type === 'active' || systemChannelState.type === 'paused') {
      // Stop channel
      try {
        await stop('system');
      } catch (err) {
        setLocalError(err instanceof Error ? err.message : 'Error al detener canal');
      }
    } else {
      // Start channel
      if (!canTranslate) {
        setLocalError('Necesitas una suscripción activa o API key BYOK para traducir');
        return;
      }
      
      setIsStarting(true);
      setLocalError(null);
      clearError();
      
      try {
        const channelConfig: ChannelConfig = {
          sourceLang: config.languages.systemSourceLang,
          targetLang: config.languages.systemTargetLang,
          inputDevice: config.devices.systemCaptureDevice || loopbackDevices[0]?.id || 'default',
          outputDevice: config.devices.outputDevice || outputDevices[0]?.id || 'default',
        };
        
        await startSystem(channelConfig);
      } catch (err) {
        setLocalError(err instanceof Error ? err.message : 'Error al iniciar canal');
      } finally {
        setIsStarting(false);
      }
    }
  }, [systemChannelState, canTranslate, config, loopbackDevices, outputDevices, startSystem, stop, clearError]);
  
  const displayError = localError || audioError;
  const isActive = systemChannelState.type === 'active';
  
  console.log('SystemAudioPanel disabled state:', { 
    isStarting, 
    audioLoading, 
    canTranslate, 
    hasUser: !!user, 
    hasByokKey 
  });
  
  return (
    <div className="bg-surface rounded-xl p-6 border border-border">
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <div>
          <h2 className="text-lg font-medium text-text">
            Canal Sistema
          </h2>
          <p className="text-sm text-text-secondary">
            Reunión → Usuario
          </p>
        </div>
        <StatusIndicator state={status} />
      </div>
      
      {/* Description */}
      <p className="text-text-secondary mb-4">
        Captura el audio de la reunión (Teams, Zoom, Meet) y lo traduce al idioma del usuario.
      </p>
      
      {/* Language pair */}
      <div className="flex items-center gap-2 mb-4 text-sm">
        <span className="px-2 py-1 bg-surface-hover rounded text-text">
          {config.languages.systemSourceLang.toUpperCase()}
        </span>
        <span className="text-text-secondary">→</span>
        <span className="px-2 py-1 bg-primary/20 text-primary rounded">
          {config.languages.systemTargetLang.toUpperCase()}
        </span>
      </div>
      
      {/* Metrics section - visible when active */}
      {isActive && metrics && (
        <div className="mb-4 space-y-2 bg-surface-hover rounded-lg p-3">
          <VolumeMeter level={metrics.inputLevelDb} label="Entrada" />
          <VolumeMeter level={metrics.outputLevelDb} label="Salida" />
          <div className="flex items-center justify-between text-sm pt-2 border-t border-border">
            <span className="text-text-secondary">Latencia</span>
            <span className="text-text font-medium">{metrics.latencyMs} ms</span>
          </div>
        </div>
      )}
      
      {/* Latency placeholder when inactive */}
      {!isActive && (
        <div className="mb-4 bg-surface-hover rounded-lg p-3">
          <div className="flex items-center justify-between text-sm">
            <span className="text-text-secondary">Latencia</span>
            <span className="text-text-secondary">--</span>
          </div>
        </div>
      )}
      
      {/* Error display */}
      {displayError && (
        <div className="mb-4 p-3 bg-error/10 border border-error/20 rounded-lg">
          <p className="text-error text-sm">{displayError}</p>
          {systemChannelState.type === 'error' && 'message' in systemChannelState && (
            <p className="text-error/70 text-xs mt-1">{systemChannelState.message}</p>
          )}
        </div>
      )}
      
      {/* Action button */}
      <button 
        onClick={handleToggle}
        disabled={isStarting || audioLoading || !canTranslate}
        className={`w-full px-4 py-3 rounded-lg font-medium transition-all
          ${isActive 
            ? 'bg-error/20 text-error hover:bg-error/30 border border-error/30' 
            : 'bg-primary text-white hover:opacity-90'
          }
          disabled:opacity-50 disabled:cursor-not-allowed
        `}
      >
        {isStarting ? (
          <span className="flex items-center justify-center gap-2">
            <svg className="animate-spin h-5 w-5" viewBox="0 0 24 24">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
            </svg>
            Conectando...
          </span>
        ) : isActive ? (
          'Detener Canal Sistema'
        ) : (
          'Iniciar Canal Sistema'
        )}
      </button>
      
      {/* Auth warning */}
      {!canTranslate && (
        <p className="mt-2 text-xs text-warning text-center">
          Inicia sesión o configura tu API key BYOK para traducir
        </p>
      )}
    </div>
  );
}

export default SystemAudioPanel;
