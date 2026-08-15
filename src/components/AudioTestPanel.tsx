/**
 * Audio Test Panel Component
 * 
 * Panel for testing audio devices during onboarding.
 * Plays a 3-second test tone on the selected device and asks the user
 * to confirm they heard it.
 * 
 * @module components/AudioTestPanel
 * @see Requirement 13.8 - Play 3-second test tone for each device
 * @see Requirement 13.9 - Allow selecting alternative device if test fails
 * @see Requirement 13.10 - Save configuration when test completes successfully
 */

import { useState, useCallback, useEffect, useRef } from 'react';
import { playAudioTest, stopAudioTest, isAudioTestPlaying } from '../ipc/commands';
import type { AudioDevice, AudioTestStatus } from '../ipc/types';

/** Props for the AudioTestPanel component */
export interface AudioTestPanelProps {
  /** The device to test */
  device: AudioDevice;
  /** Type of device being tested */
  deviceType: 'capture' | 'microphone' | 'output';
  /** Called when user confirms they heard the test tone */
  onConfirm: () => void;
  /** Called when user indicates they didn't hear the tone */
  onFailed: () => void;
  /** List of alternative devices to select from */
  alternativeDevices: AudioDevice[];
  /** Called when user selects an alternative device */
  onSelectAlternative: (deviceId: string) => void;
  /** Optional custom duration in milliseconds (default: 3000) */
  durationMs?: number;
}

/** Device type labels in Spanish */
const deviceTypeLabels: Record<string, string> = {
  capture: 'dispositivo de captura',
  microphone: 'micrófono',
  output: 'dispositivo de salida',
};

/**
 * AudioTestPanel Component
 * 
 * Provides UI for testing audio devices during onboarding.
 * Includes:
 * - Play test button
 * - Progress indicator during playback
 * - Confirmation buttons
 * - Alternative device selection if test fails
 */
export function AudioTestPanel({
  device,
  deviceType,
  onConfirm,
  onFailed,
  alternativeDevices,
  onSelectAlternative,
  durationMs = 3000,
}: AudioTestPanelProps) {
  const [status, setStatus] = useState<AudioTestStatus>({ type: 'idle' });
  const [showAlternatives, setShowAlternatives] = useState(false);
  const [progress, setProgress] = useState(0);
  const progressIntervalRef = useRef<number | null>(null);

  // Clean up on unmount
  useEffect(() => {
    return () => {
      if (progressIntervalRef.current) {
        clearInterval(progressIntervalRef.current);
      }
      // Stop any playing test when unmounting
      isAudioTestPlaying().then((isPlaying) => {
        if (isPlaying) {
          stopAudioTest().catch(() => {});
        }
      });
    };
  }, []);

  // Start progress animation
  const startProgressAnimation = useCallback(() => {
    setProgress(0);
    const startTime = Date.now();
    
    progressIntervalRef.current = window.setInterval(() => {
      const elapsed = Date.now() - startTime;
      const newProgress = Math.min((elapsed / durationMs) * 100, 100);
      setProgress(newProgress);
      
      if (newProgress >= 100 && progressIntervalRef.current) {
        clearInterval(progressIntervalRef.current);
        progressIntervalRef.current = null;
      }
    }, 50) as unknown as number;
  }, [durationMs]);

  // Stop progress animation
  const stopProgressAnimation = useCallback(() => {
    if (progressIntervalRef.current) {
      clearInterval(progressIntervalRef.current);
      progressIntervalRef.current = null;
    }
    setProgress(0);
  }, []);

  // Play test tone
  const handlePlayTest = useCallback(async () => {
    try {
      setStatus({ type: 'playing', deviceId: device.id, deviceName: device.name });
      setShowAlternatives(false);
      startProgressAnimation();
      
      const result = await playAudioTest(device.id, device.name, { durationMs });
      
      stopProgressAnimation();
      setStatus({ type: 'completed', result });
    } catch (err) {
      stopProgressAnimation();
      setStatus({ 
        type: 'error', 
        message: err instanceof Error ? err.message : 'Error desconocido al reproducir el audio'
      });
    }
  }, [device, durationMs, startProgressAnimation, stopProgressAnimation]);

  // Stop current test
  const handleStopTest = useCallback(async () => {
    try {
      await stopAudioTest();
      stopProgressAnimation();
      setStatus({ type: 'idle' });
    } catch (err) {
      // Ignore errors when stopping
      stopProgressAnimation();
      setStatus({ type: 'idle' });
    }
  }, [stopProgressAnimation]);

  // User confirms they heard the tone
  const handleConfirmHeard = useCallback(() => {
    setStatus({ type: 'idle' });
    onConfirm();
  }, [onConfirm]);

  // User didn't hear the tone
  const handleDidNotHear = useCallback(() => {
    setShowAlternatives(true);
    onFailed();
  }, [onFailed]);

  // Select alternative device
  const handleSelectAlternative = useCallback((deviceId: string) => {
    setShowAlternatives(false);
    setStatus({ type: 'idle' });
    onSelectAlternative(deviceId);
  }, [onSelectAlternative]);

  // Retry with current device
  const handleRetry = useCallback(() => {
    setShowAlternatives(false);
    setStatus({ type: 'idle' });
  }, []);

  const isPlaying = status.type === 'playing';
  const isCompleted = status.type === 'completed';
  const hasError = status.type === 'error';

  return (
    <div className="p-4 bg-surface-hover rounded-xl border border-border">
      {/* Header */}
      <div className="mb-4">
        <h3 className="text-lg font-semibold text-text mb-1">
          Prueba de Audio
        </h3>
        <p className="text-sm text-text-secondary">
          Vamos a verificar que el {deviceTypeLabels[deviceType]} funciona correctamente.
        </p>
      </div>

      {/* Device info */}
      <div className="mb-4 p-3 bg-surface rounded-lg">
        <p className="text-sm text-text-secondary">Dispositivo seleccionado:</p>
        <p className="font-medium text-text">{device.name}</p>
      </div>

      {/* Status display */}
      {isPlaying && (
        <div className="mb-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="w-5 h-5 border-3 border-primary/30 border-t-primary rounded-full animate-spin" />
            <span className="text-text">Reproduciendo tono de prueba...</span>
          </div>
          {/* Progress bar */}
          <div className="h-2 bg-surface rounded-full overflow-hidden">
            <div 
              className="h-full bg-primary transition-all duration-100"
              style={{ width: `${progress}%` }}
            />
          </div>
          <p className="mt-2 text-sm text-text-secondary text-center">
            {Math.ceil((durationMs - (progress * durationMs / 100)) / 1000)} segundos restantes
          </p>
        </div>
      )}

      {hasError && (
        <div className="mb-4 p-3 bg-error/10 border border-error/20 rounded-lg">
          <p className="text-error text-sm">{status.message}</p>
        </div>
      )}

      {/* Action buttons */}
      {!isPlaying && !isCompleted && !showAlternatives && (
        <button
          onClick={handlePlayTest}
          className="w-full py-3 bg-primary text-white rounded-lg hover:opacity-90 transition font-medium flex items-center justify-center gap-2"
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M9 12a3 3 0 116 0 3 3 0 01-6 0z" />
          </svg>
          Reproducir tono de prueba
        </button>
      )}

      {isPlaying && (
        <button
          onClick={handleStopTest}
          className="w-full py-3 bg-surface text-text-secondary border border-border rounded-lg hover:bg-border transition font-medium"
        >
          Detener prueba
        </button>
      )}

      {/* Confirmation after test completes */}
      {isCompleted && status.result.success && !showAlternatives && (
        <div className="space-y-3">
          <div className="p-3 bg-success/10 border border-success/20 rounded-lg text-center">
            <svg className="w-8 h-8 mx-auto text-success mb-2" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <p className="text-success font-medium">Prueba completada</p>
          </div>
          
          <p className="text-center text-text-secondary">¿Escuchaste el tono de prueba?</p>
          
          <div className="flex gap-3">
            <button
              onClick={handleConfirmHeard}
              className="flex-1 py-3 bg-success text-white rounded-lg hover:opacity-90 transition font-medium"
            >
              Sí, lo escuché
            </button>
            <button
              onClick={handleDidNotHear}
              className="flex-1 py-3 bg-surface text-text border border-border rounded-lg hover:bg-border transition font-medium"
            >
              No escuché nada
            </button>
          </div>
        </div>
      )}

      {/* Error after test */}
      {isCompleted && !status.result.success && !showAlternatives && (
        <div className="space-y-3">
          <div className="p-3 bg-warning/10 border border-warning/20 rounded-lg">
            <p className="text-warning text-sm">
              {status.result.error || 'La prueba no se completó correctamente.'}
            </p>
          </div>
          
          <div className="flex gap-3">
            <button
              onClick={handlePlayTest}
              className="flex-1 py-3 bg-primary text-white rounded-lg hover:opacity-90 transition font-medium"
            >
              Reintentar
            </button>
            <button
              onClick={() => setShowAlternatives(true)}
              className="flex-1 py-3 bg-surface text-text border border-border rounded-lg hover:bg-border transition font-medium"
            >
              Cambiar dispositivo
            </button>
          </div>
        </div>
      )}

      {/* Alternative device selection */}
      {showAlternatives && (
        <div className="space-y-3">
          <p className="text-sm text-text-secondary">
            Selecciona otro {deviceTypeLabels[deviceType]} y prueba de nuevo:
          </p>
          
          {alternativeDevices.length > 0 ? (
            <div className="space-y-2">
              {alternativeDevices.map((altDevice) => (
                <button
                  key={altDevice.id}
                  onClick={() => handleSelectAlternative(altDevice.id)}
                  className={`w-full p-3 rounded-lg border text-left transition ${
                    altDevice.id === device.id
                      ? 'bg-primary/10 border-primary text-text'
                      : 'bg-surface border-border hover:border-primary/50 text-text'
                  }`}
                >
                  <p className="font-medium">{altDevice.name}</p>
                  {altDevice.isDefault && (
                    <span className="text-xs text-primary">Predeterminado</span>
                  )}
                </button>
              ))}
            </div>
          ) : (
            <p className="text-sm text-warning">
              No hay otros dispositivos disponibles.
            </p>
          )}
          
          <div className="flex gap-3 pt-2">
            <button
              onClick={handleRetry}
              className="flex-1 py-2 bg-primary text-white rounded-lg hover:opacity-90 transition font-medium text-sm"
            >
              Reintentar con el actual
            </button>
            <button
              onClick={handleConfirmHeard}
              className="flex-1 py-2 bg-surface text-text-secondary border border-border rounded-lg hover:bg-border transition font-medium text-sm"
            >
              Continuar sin probar
            </button>
          </div>
        </div>
      )}

      {/* Help text */}
      <div className="mt-4 text-xs text-text-secondary">
        <p>💡 Asegúrate de que el volumen de tu sistema no esté en silencio y que el dispositivo esté correctamente conectado.</p>
      </div>
    </div>
  );
}

export default AudioTestPanel;
