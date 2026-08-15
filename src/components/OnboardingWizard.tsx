/**
 * Onboarding Wizard Component
 * 
 * Step-by-step wizard that guides users through initial audio device configuration.
 * Automatically starts if no saved configuration exists.
 * 
 * @module components/OnboardingWizard
 * @see Requirement 13.1 - Start wizard if no saved configuration
 * @see Requirement 13.2 - Show capture device selection step
 * @see Requirement 13.3 - Show microphone selection step
 * @see Requirement 13.4 - Show output device selection step
 * @see Requirement 13.5 - Show message if no devices detected
 * @see Requirement 13.8 - Play 3-second test tone for each device
 * @see Requirement 13.9 - Allow selecting alternative device if test fails
 * @see Requirement 13.10 - Save configuration when test completes successfully
 */

import { useState, useEffect, useCallback } from 'react';
import { useAudioEngine } from '../hooks/useAudioEngine';
import { useConfig } from '../hooks/useConfig';
import { AudioTestPanel } from './AudioTestPanel';
import type { AudioDevice, AppConfig, VirtualAudioStatus } from '../ipc/types';

/** Wizard step identifiers - now includes audio test steps */
type WizardStep = 
  | 'welcome' 
  | 'capture' 
  | 'capture_test'    // NEW: Test capture device
  | 'microphone' 
  | 'microphone_test' // NEW: Test microphone
  | 'output' 
  | 'output_test'     // NEW: Test output device
  | 'vbcable'         // Windows: VB-Cable virtual audio driver
  | 'virtual_audio'   // macOS: Virtual Audio Endpoint or BlackHole
  | 'complete';

/** Props for the OnboardingWizard component */
export interface OnboardingWizardProps {
  /** Callback when wizard completes successfully */
  onComplete: (config: AppConfig) => void;
  /** Callback when user skips the wizard */
  onSkip?: () => void;
  /** Whether the wizard is visible */
  isOpen: boolean;
}

/** Step indicator component */
function StepIndicator({ current, total }: { current: number; total: number }) {
  return (
    <div className="flex items-center justify-center gap-2 mb-6">
      {Array.from({ length: total }, (_, i) => (
        <div
          key={i}
          className={`w-2.5 h-2.5 rounded-full transition-all ${
            i < current ? 'bg-primary' : i === current ? 'bg-primary scale-125' : 'bg-surface-hover'
          }`}
        />
      ))}
    </div>
  );
}

/** Device list component with selection */
function DeviceList({
  devices,
  selectedId,
  onSelect,
  emptyMessage,
  deviceTypeLabel,
}: {
  devices: AudioDevice[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  emptyMessage: string;
  deviceTypeLabel: string;
}) {
  if (devices.length === 0) {
    return (
      <div className="p-6 bg-warning/10 border border-warning/20 rounded-xl text-center">
        <svg className="w-12 h-12 mx-auto text-warning mb-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
        </svg>
        <p className="text-warning font-medium mb-1">No se detectaron dispositivos</p>
        <p className="text-warning/70 text-sm">{emptyMessage}</p>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      <p className="text-sm text-text-secondary mb-3">
        Selecciona el {deviceTypeLabel} que deseas usar:
      </p>
      {devices.map((device) => (
        <button
          key={device.id}
          onClick={() => onSelect(device.id)}
          className={`w-full p-4 rounded-xl border text-left transition-all ${
            selectedId === device.id
              ? 'bg-primary/10 border-primary text-text'
              : 'bg-surface-hover border-border hover:border-primary/50 text-text'
          }`}
        >
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className={`w-4 h-4 rounded-full border-2 flex items-center justify-center ${
                selectedId === device.id ? 'border-primary' : 'border-text-secondary'
              }`}>
                {selectedId === device.id && (
                  <div className="w-2 h-2 rounded-full bg-primary" />
                )}
              </div>
              <div>
                <p className="font-medium">{device.name}</p>
                {device.isDefault && (
                  <span className="text-xs text-primary">Dispositivo predeterminado</span>
                )}
              </div>
            </div>
          </div>
        </button>
      ))}
    </div>
  );
}


/** VB-Cable status component */
function VBCableStatus({
  isInstalled,
  onRetry,
  isChecking,
}: {
  isInstalled: boolean;
  onRetry: () => void;
  isChecking: boolean;
}) {
  if (isChecking) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="flex flex-col items-center gap-3">
          <div className="w-10 h-10 border-4 border-primary/30 border-t-primary rounded-full animate-spin" />
          <p className="text-text-secondary">Verificando VB-Cable...</p>
        </div>
      </div>
    );
  }

  if (isInstalled) {
    return (
      <div className="p-6 bg-success/10 border border-success/20 rounded-xl">
        <div className="flex items-center gap-3">
          <svg className="w-8 h-8 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <div>
            <p className="text-success font-medium">VB-Cable detectado</p>
            <p className="text-success/70 text-sm">
              El driver de audio virtual está instalado correctamente.
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="p-6 bg-warning/10 border border-warning/20 rounded-xl">
      <div className="flex items-start gap-3">
        <svg className="w-8 h-8 text-warning flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
        </svg>
        <div className="flex-1">
          <p className="text-warning font-medium mb-2">VB-Cable no detectado</p>
          <p className="text-text-secondary text-sm mb-4">
            VB-Cable es necesario para inyectar tu voz traducida en las aplicaciones de reunión.
            Sin él, solo podrás escuchar traducciones pero no enviar tu voz traducida.
          </p>
          <div className="space-y-3">
            <p className="text-sm text-text-secondary">
              <strong>Para instalar VB-Cable:</strong>
            </p>
            <ol className="text-sm text-text-secondary list-decimal list-inside space-y-1">
              <li>Descarga VB-Cable desde vb-audio.com/Cable</li>
              <li>Ejecuta el instalador como administrador</li>
              <li>Reinicia tu computadora después de instalar</li>
            </ol>
            <button
              onClick={onRetry}
              className="mt-3 px-4 py-2 bg-warning/20 text-warning rounded-lg hover:bg-warning/30 transition text-sm"
            >
              Verificar nuevamente
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}


/** Virtual Audio status component for macOS */
function VirtualAudioStatusComponent({
  status,
  onRetry,
  isChecking,
}: {
  status: VirtualAudioStatus | null;
  onRetry: () => void;
  isChecking: boolean;
}) {
  if (isChecking) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="flex flex-col items-center gap-3">
          <div className="w-10 h-10 border-4 border-primary/30 border-t-primary rounded-full animate-spin" />
          <p className="text-text-secondary">Verificando audio virtual...</p>
        </div>
      </div>
    );
  }

  // macOS 14+ native virtual audio
  if (status?.statusType === 'native' && status.isAvailable) {
    return (
      <div className="p-6 bg-success/10 border border-success/20 rounded-xl">
        <div className="flex items-center gap-3">
          <svg className="w-8 h-8 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <div>
            <p className="text-success font-medium">Audio Virtual Nativo (macOS {status.macosVersion})</p>
            <p className="text-success/70 text-sm">
              Tu versión de macOS soporta audio virtual nativo. Se configurará automáticamente.
            </p>
          </div>
        </div>
      </div>
    );
  }

  // BlackHole fallback available
  if (status?.statusType === 'blackhole' && status.isAvailable) {
    return (
      <div className="p-6 bg-success/10 border border-success/20 rounded-xl">
        <div className="flex items-center gap-3">
          <svg className="w-8 h-8 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <div>
            <p className="text-success font-medium">BlackHole detectado</p>
            <p className="text-success/70 text-sm">
              {status.blackholeDeviceName} está instalado y listo para usar.
            </p>
          </div>
        </div>
      </div>
    );
  }

  // BlackHole required but not installed
  if (status?.statusType === 'requires_blackhole') {
    const instructions = status.installationInstructions;
    return (
      <div className="p-6 bg-warning/10 border border-warning/20 rounded-xl">
        <div className="flex items-start gap-3">
          <svg className="w-8 h-8 text-warning flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          <div className="flex-1">
            <p className="text-warning font-medium mb-2">
              {instructions?.title || 'BlackHole no detectado'}
            </p>
            <p className="text-text-secondary text-sm mb-4">
              {instructions?.description || `Tu versión de macOS (${status.macosVersion}) no soporta audio virtual nativo. BlackHole es necesario para inyectar tu voz traducida en las aplicaciones de reunión.`}
            </p>
            {instructions && (
              <div className="space-y-3">
                <p className="text-sm text-text-secondary">
                  <strong>Para instalar BlackHole:</strong>
                </p>
                <ol className="text-sm text-text-secondary list-decimal list-inside space-y-1">
                  {instructions.steps.map((step, index) => (
                    <li key={index}>{step.replace(/^\d+\.\s*/, '')}</li>
                  ))}
                </ol>
                {instructions.homebrewCommand && (
                  <div className="mt-3 p-3 bg-surface-hover rounded-lg">
                    <p className="text-xs text-text-secondary mb-1">O instala con Homebrew:</p>
                    <code className="text-sm text-primary">{instructions.homebrewCommand}</code>
                  </div>
                )}
                <a
                  href={instructions.downloadUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-block mt-2 text-sm text-primary hover:underline"
                >
                  Descargar BlackHole →
                </a>
              </div>
            )}
            <button
              onClick={onRetry}
              className="mt-3 px-4 py-2 bg-warning/20 text-warning rounded-lg hover:bg-warning/30 transition text-sm"
            >
              Verificar nuevamente
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Not available or error
  return (
    <div className="p-6 bg-error/10 border border-error/20 rounded-xl">
      <div className="flex items-start gap-3">
        <svg className="w-8 h-8 text-error flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <div className="flex-1">
          <p className="text-error font-medium mb-2">Audio virtual no disponible</p>
          <p className="text-text-secondary text-sm mb-4">
            No se pudo detectar un driver de audio virtual. La funcionalidad de inyección de voz traducida no estará disponible.
          </p>
          <button
            onClick={onRetry}
            className="px-4 py-2 bg-error/20 text-error rounded-lg hover:bg-error/30 transition text-sm"
          >
            Verificar nuevamente
          </button>
        </div>
      </div>
    </div>
  );
}


/** Main OnboardingWizard component */
export function OnboardingWizard({ onComplete, onSkip, isOpen }: OnboardingWizardProps) {
  const {
    devices,
    inputDevices,
    outputDevices,
    loopbackDevices,
    vbCableStatus,
    virtualAudioStatus,
    refreshDevices,
    refreshVbCableStatus,
    refreshVirtualAudioStatus,
    loading: devicesLoading,
  } = useAudioEngine();

  const { config, updateConfig, saveConfig } = useConfig();

  // Wizard state
  const [currentStep, setCurrentStep] = useState<WizardStep>('welcome');
  const [selectedCaptureDevice, setSelectedCaptureDevice] = useState<string | null>(null);
  const [selectedMicrophone, setSelectedMicrophone] = useState<string | null>(null);
  const [selectedOutputDevice, setSelectedOutputDevice] = useState<string | null>(null);
  const [isCheckingVbCable, setIsCheckingVbCable] = useState(false);
  const [isCheckingVirtualAudio, setIsCheckingVirtualAudio] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Detect platform (Windows or macOS)
  const isWindows = navigator.userAgent.includes('Windows');
  const isMacOS = navigator.userAgent.includes('Mac');

  // Steps configuration based on platform
  // Windows: uses VB-Cable
  // macOS: uses Virtual Audio Endpoint (native on 14+) or BlackHole (fallback for <14)
  // Audio test steps are added after each device selection (Requirement 13.8)
  const steps: WizardStep[] = isWindows
    ? ['welcome', 'capture', 'capture_test', 'microphone', 'microphone_test', 'output', 'output_test', 'vbcable', 'complete']
    : isMacOS
    ? ['welcome', 'capture', 'capture_test', 'microphone', 'microphone_test', 'output', 'output_test', 'virtual_audio', 'complete']
    : ['welcome', 'capture', 'capture_test', 'microphone', 'microphone_test', 'output', 'output_test', 'complete'];

  const currentStepIndex = steps.indexOf(currentStep);
  const totalSteps = steps.length;

  // Track audio test confirmations for each device
  const [_captureTestConfirmed, setCaptureTestConfirmed] = useState(false);
  const [_microphoneTestConfirmed, setMicrophoneTestConfirmed] = useState(false);
  const [_outputTestConfirmed, setOutputTestConfirmed] = useState(false);

  // Initialize selections with default devices
  useEffect(() => {
    if (devices.length > 0 && !selectedCaptureDevice) {
      const defaultLoopback = loopbackDevices.find(d => d.isDefault) || loopbackDevices[0];
      if (defaultLoopback) setSelectedCaptureDevice(defaultLoopback.id);

      const defaultInput = inputDevices.find(d => d.isDefault) || inputDevices[0];
      if (defaultInput) setSelectedMicrophone(defaultInput.id);

      const defaultOutput = outputDevices.find(d => d.isDefault) || outputDevices[0];
      if (defaultOutput) setSelectedOutputDevice(defaultOutput.id);
    }
  }, [devices, loopbackDevices, inputDevices, outputDevices, selectedCaptureDevice]);


  // Handle VB-Cable check
  const handleCheckVbCable = useCallback(async () => {
    setIsCheckingVbCable(true);
    try {
      await refreshVbCableStatus();
    } finally {
      setIsCheckingVbCable(false);
    }
  }, [refreshVbCableStatus]);

  // Handle Virtual Audio check (macOS)
  const handleCheckVirtualAudio = useCallback(async () => {
    setIsCheckingVirtualAudio(true);
    try {
      await refreshVirtualAudioStatus();
    } finally {
      setIsCheckingVirtualAudio(false);
    }
  }, [refreshVirtualAudioStatus]);

  // Navigation handlers
  const handleNext = useCallback(() => {
    const nextIndex = currentStepIndex + 1;
    if (nextIndex < steps.length) {
      setCurrentStep(steps[nextIndex]);
    }
  }, [currentStepIndex, steps]);

  const handleBack = useCallback(() => {
    const prevIndex = currentStepIndex - 1;
    if (prevIndex >= 0) {
      setCurrentStep(steps[prevIndex]);
    }
  }, [currentStepIndex, steps]);

  // Complete wizard and save configuration
  const handleComplete = useCallback(async () => {
    try {
      setError(null);
      
      // Update config with selected devices
      const newConfig: AppConfig = {
        ...config,
        devices: {
          systemCaptureDevice: selectedCaptureDevice || undefined,
          inputDevice: selectedMicrophone || undefined,
          outputDevice: selectedOutputDevice || undefined,
        },
      };

      updateConfig(newConfig);
      await saveConfig();
      onComplete(newConfig);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error al guardar la configuración');
    }
  }, [config, selectedCaptureDevice, selectedMicrophone, selectedOutputDevice, updateConfig, saveConfig, onComplete]);

  // Check if current step can proceed
  const canProceed = useCallback(() => {
    switch (currentStep) {
      case 'welcome':
        return true;
      case 'capture':
        return selectedCaptureDevice !== null || loopbackDevices.length === 0;
      case 'capture_test':
        // Can always proceed from test (user can skip or confirm)
        return true;
      case 'microphone':
        return selectedMicrophone !== null || inputDevices.length === 0;
      case 'microphone_test':
        return true;
      case 'output':
        return selectedOutputDevice !== null || outputDevices.length === 0;
      case 'output_test':
        return true;
      case 'vbcable':
        return true; // Can proceed even without VB-Cable
      case 'virtual_audio':
        return true; // Can proceed even without virtual audio (macOS)
      case 'complete':
        return true;
      default:
        return false;
    }
  }, [currentStep, selectedCaptureDevice, selectedMicrophone, selectedOutputDevice, loopbackDevices, inputDevices, outputDevices]);

  // Audio test handlers
  const handleCaptureTestConfirm = useCallback(() => {
    setCaptureTestConfirmed(true);
    handleNext();
  }, [handleNext]);

  const handleCaptureTestFailed = useCallback(() => {
    setCaptureTestConfirmed(false);
    // Go back to device selection
    setCurrentStep('capture');
  }, []);

  const handleMicrophoneTestConfirm = useCallback(() => {
    setMicrophoneTestConfirmed(true);
    handleNext();
  }, [handleNext]);

  const handleMicrophoneTestFailed = useCallback(() => {
    setMicrophoneTestConfirmed(false);
    setCurrentStep('microphone');
  }, []);

  const handleOutputTestConfirm = useCallback(() => {
    setOutputTestConfirmed(true);
    handleNext();
  }, [handleNext]);

  const handleOutputTestFailed = useCallback(() => {
    setOutputTestConfirmed(false);
    setCurrentStep('output');
  }, []);

  if (!isOpen) return null;


  // Render step content
  const renderStepContent = () => {
    switch (currentStep) {
      case 'welcome':
        return (
          <div className="text-center">
            <div className="w-20 h-20 mx-auto mb-6 bg-gradient-to-br from-primary to-secondary rounded-2xl flex items-center justify-center shadow-lg">
              <svg className="w-10 h-10 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 5h12M9 3v2m1.048 9.5A18.022 18.022 0 016.412 9m6.088 9h7M11 21l5-10 5 10M12.751 5C11.783 10.77 8.07 15.61 3 18.129" />
              </svg>
            </div>
            <h2 className="text-2xl font-bold text-text mb-3">
              ¡Bienvenido a Traductor Desktop!
            </h2>
            <p className="text-text-secondary mb-6 max-w-md mx-auto">
              Vamos a configurar tus dispositivos de audio para que puedas traducir
              tus reuniones en tiempo real.
            </p>
            <div className="bg-surface-hover rounded-xl p-4 text-left">
              <p className="text-sm text-text-secondary mb-2">Este asistente te ayudará a:</p>
              <ul className="text-sm text-text space-y-2">
                <li className="flex items-center gap-2">
                  <span className="w-5 h-5 rounded-full bg-primary/20 text-primary text-xs flex items-center justify-center">1</span>
                  Seleccionar el dispositivo para capturar audio de reuniones
                </li>
                <li className="flex items-center gap-2">
                  <span className="w-5 h-5 rounded-full bg-primary/20 text-primary text-xs flex items-center justify-center">2</span>
                  Configurar tu micrófono para traducir tu voz
                </li>
                <li className="flex items-center gap-2">
                  <span className="w-5 h-5 rounded-full bg-primary/20 text-primary text-xs flex items-center justify-center">3</span>
                  Elegir dónde escuchar el audio traducido
                </li>
              </ul>
            </div>
          </div>
        );


      case 'capture':
        return (
          <div>
            <h2 className="text-xl font-bold text-text mb-2">Dispositivo de Captura</h2>
            <p className="text-text-secondary mb-6">
              Selecciona el dispositivo para capturar el audio de tus reuniones (Teams, Zoom, Meet).
            </p>
            {devicesLoading ? (
              <div className="flex items-center justify-center py-8">
                <div className="w-8 h-8 border-4 border-primary/30 border-t-primary rounded-full animate-spin" />
              </div>
            ) : (
              <DeviceList
                devices={loopbackDevices}
                selectedId={selectedCaptureDevice}
                onSelect={setSelectedCaptureDevice}
                emptyMessage="Verifica que tu dispositivo de audio esté conectado correctamente. Los dispositivos de captura loopback permiten grabar lo que escuchas en tu computadora."
                deviceTypeLabel="dispositivo de captura"
              />
            )}
            {loopbackDevices.length === 0 && !devicesLoading && (
              <button
                onClick={refreshDevices}
                className="mt-4 w-full px-4 py-2 bg-surface-hover text-text-secondary rounded-lg hover:bg-border transition text-sm"
              >
                Buscar dispositivos nuevamente
              </button>
            )}
          </div>
        );

      case 'capture_test': {
        const selectedDevice = loopbackDevices.find(d => d.id === selectedCaptureDevice);
        if (!selectedDevice) {
          // No device selected, skip test
          return (
            <div className="text-center py-8">
              <p className="text-text-secondary">No hay dispositivo seleccionado para probar.</p>
            </div>
          );
        }
        return (
          <div>
            <h2 className="text-xl font-bold text-text mb-2">Prueba del Dispositivo de Captura</h2>
            <p className="text-text-secondary mb-6">
              Vamos a verificar que el dispositivo de captura funciona correctamente.
            </p>
            <AudioTestPanel
              device={selectedDevice}
              deviceType="capture"
              onConfirm={handleCaptureTestConfirm}
              onFailed={handleCaptureTestFailed}
              alternativeDevices={loopbackDevices.filter(d => d.id !== selectedCaptureDevice)}
              onSelectAlternative={(deviceId) => {
                setSelectedCaptureDevice(deviceId);
              }}
            />
          </div>
        );
      }

      case 'microphone':
        return (
          <div>
            <h2 className="text-xl font-bold text-text mb-2">Micrófono</h2>
            <p className="text-text-secondary mb-6">
              Selecciona el micrófono que usarás para hablar en tus reuniones.
            </p>
            {devicesLoading ? (
              <div className="flex items-center justify-center py-8">
                <div className="w-8 h-8 border-4 border-primary/30 border-t-primary rounded-full animate-spin" />
              </div>
            ) : (
              <DeviceList
                devices={inputDevices}
                selectedId={selectedMicrophone}
                onSelect={setSelectedMicrophone}
                emptyMessage="Verifica que tu micrófono esté conectado y habilitado en la configuración del sistema."
                deviceTypeLabel="micrófono"
              />
            )}
            {inputDevices.length === 0 && !devicesLoading && (
              <button
                onClick={refreshDevices}
                className="mt-4 w-full px-4 py-2 bg-surface-hover text-text-secondary rounded-lg hover:bg-border transition text-sm"
              >
                Buscar dispositivos nuevamente
              </button>
            )}
          </div>
        );

      case 'microphone_test': {
        const selectedDevice = inputDevices.find(d => d.id === selectedMicrophone);
        if (!selectedDevice) {
          return (
            <div className="text-center py-8">
              <p className="text-text-secondary">No hay micrófono seleccionado para probar.</p>
            </div>
          );
        }
        return (
          <div>
            <h2 className="text-xl font-bold text-text mb-2">Prueba del Micrófono</h2>
            <p className="text-text-secondary mb-6">
              Vamos a verificar que el micrófono funciona correctamente.
            </p>
            <AudioTestPanel
              device={selectedDevice}
              deviceType="microphone"
              onConfirm={handleMicrophoneTestConfirm}
              onFailed={handleMicrophoneTestFailed}
              alternativeDevices={inputDevices.filter(d => d.id !== selectedMicrophone)}
              onSelectAlternative={(deviceId) => {
                setSelectedMicrophone(deviceId);
              }}
            />
          </div>
        );
      }


      case 'output':
        return (
          <div>
            <h2 className="text-xl font-bold text-text mb-2">Dispositivo de Salida</h2>
            <p className="text-text-secondary mb-6">
              Selecciona dónde quieres escuchar el audio traducido (tus auriculares o altavoces).
            </p>
            {devicesLoading ? (
              <div className="flex items-center justify-center py-8">
                <div className="w-8 h-8 border-4 border-primary/30 border-t-primary rounded-full animate-spin" />
              </div>
            ) : (
              <DeviceList
                devices={outputDevices}
                selectedId={selectedOutputDevice}
                onSelect={setSelectedOutputDevice}
                emptyMessage="Verifica que tus auriculares o altavoces estén conectados y habilitados."
                deviceTypeLabel="dispositivo de salida"
              />
            )}
            {outputDevices.length === 0 && !devicesLoading && (
              <button
                onClick={refreshDevices}
                className="mt-4 w-full px-4 py-2 bg-surface-hover text-text-secondary rounded-lg hover:bg-border transition text-sm"
              >
                Buscar dispositivos nuevamente
              </button>
            )}
          </div>
        );

      case 'output_test': {
        const selectedDevice = outputDevices.find(d => d.id === selectedOutputDevice);
        if (!selectedDevice) {
          return (
            <div className="text-center py-8">
              <p className="text-text-secondary">No hay dispositivo de salida seleccionado para probar.</p>
            </div>
          );
        }
        return (
          <div>
            <h2 className="text-xl font-bold text-text mb-2">Prueba del Dispositivo de Salida</h2>
            <p className="text-text-secondary mb-6">
              Vamos a verificar que puedes escuchar el audio traducido.
            </p>
            <AudioTestPanel
              device={selectedDevice}
              deviceType="output"
              onConfirm={handleOutputTestConfirm}
              onFailed={handleOutputTestFailed}
              alternativeDevices={outputDevices.filter(d => d.id !== selectedOutputDevice)}
              onSelectAlternative={(deviceId) => {
                setSelectedOutputDevice(deviceId);
              }}
            />
          </div>
        );
      }

      case 'vbcable':
        return (
          <div>
            <h2 className="text-xl font-bold text-text mb-2">VB-Cable (Audio Virtual)</h2>
            <p className="text-text-secondary mb-6">
              VB-Cable es necesario para enviar tu voz traducida a las aplicaciones de reunión.
            </p>
            <VBCableStatus
              isInstalled={vbCableStatus?.isInstalled ?? false}
              onRetry={handleCheckVbCable}
              isChecking={isCheckingVbCable}
            />
          </div>
        );

      case 'virtual_audio':
        return (
          <div>
            <h2 className="text-xl font-bold text-text mb-2">Audio Virtual (macOS)</h2>
            <p className="text-text-secondary mb-6">
              El audio virtual es necesario para enviar tu voz traducida a las aplicaciones de reunión.
            </p>
            <VirtualAudioStatusComponent
              status={virtualAudioStatus}
              onRetry={handleCheckVirtualAudio}
              isChecking={isCheckingVirtualAudio}
            />
          </div>
        );


      case 'complete':
        return (
          <div className="text-center">
            <div className="w-20 h-20 mx-auto mb-6 bg-success/20 rounded-full flex items-center justify-center">
              <svg className="w-10 h-10 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
              </svg>
            </div>
            <h2 className="text-2xl font-bold text-text mb-3">
              ¡Configuración Completa!
            </h2>
            <p className="text-text-secondary mb-6">
              Tu aplicación está lista para traducir reuniones en tiempo real.
            </p>
            
            {/* Summary of selected devices */}
            <div className="bg-surface-hover rounded-xl p-4 text-left mb-6">
              <p className="text-sm text-text-secondary mb-3">Resumen de configuración:</p>
              <div className="space-y-2 text-sm">
                <div className="flex justify-between">
                  <span className="text-text-secondary">Captura:</span>
                  <span className="text-text font-medium truncate ml-2 max-w-[200px]">
                    {loopbackDevices.find(d => d.id === selectedCaptureDevice)?.name || 'No seleccionado'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-text-secondary">Micrófono:</span>
                  <span className="text-text font-medium truncate ml-2 max-w-[200px]">
                    {inputDevices.find(d => d.id === selectedMicrophone)?.name || 'No seleccionado'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-text-secondary">Salida:</span>
                  <span className="text-text font-medium truncate ml-2 max-w-[200px]">
                    {outputDevices.find(d => d.id === selectedOutputDevice)?.name || 'No seleccionado'}
                  </span>
                </div>
                {isWindows && (
                  <div className="flex justify-between">
                    <span className="text-text-secondary">VB-Cable:</span>
                    <span className={`font-medium ${vbCableStatus?.isInstalled ? 'text-success' : 'text-warning'}`}>
                      {vbCableStatus?.isInstalled ? 'Instalado' : 'No instalado'}
                    </span>
                  </div>
                )}
                {isMacOS && (
                  <div className="flex justify-between">
                    <span className="text-text-secondary">Audio Virtual:</span>
                    <span className={`font-medium ${virtualAudioStatus?.isAvailable ? 'text-success' : 'text-warning'}`}>
                      {virtualAudioStatus?.isNative 
                        ? `Nativo (macOS ${virtualAudioStatus.macosVersion})`
                        : virtualAudioStatus?.isAvailable 
                          ? virtualAudioStatus.blackholeDeviceName || 'BlackHole'
                          : 'No disponible'}
                    </span>
                  </div>
                )}
              </div>
            </div>

            {error && (
              <div className="mb-4 p-3 bg-error/10 border border-error/20 rounded-lg">
                <p className="text-error text-sm">{error}</p>
              </div>
            )}
          </div>
        );

      default:
        return null;
    }
  };


  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-surface rounded-2xl shadow-2xl w-full max-w-lg mx-4 border border-border overflow-hidden">
        {/* Header */}
        <div className="px-6 pt-6 pb-2">
          <StepIndicator current={currentStepIndex} total={totalSteps} />
        </div>

        {/* Content */}
        <div className="px-6 py-4 min-h-[350px]">
          {renderStepContent()}
        </div>

        {/* Footer with navigation buttons */}
        <div className="px-6 py-4 bg-surface-hover border-t border-border flex items-center justify-between">
          <div>
            {currentStep !== 'welcome' && currentStep !== 'complete' && (
              <button
                onClick={handleBack}
                className="px-4 py-2 text-text-secondary hover:text-text transition"
              >
                ← Atrás
              </button>
            )}
          </div>
          
          <div className="flex items-center gap-3">
            {onSkip && currentStep !== 'complete' && (
              <button
                onClick={onSkip}
                className="px-4 py-2 text-text-secondary hover:text-text transition text-sm"
              >
                Omitir
              </button>
            )}
            
            {currentStep === 'complete' ? (
              <button
                onClick={handleComplete}
                className="px-6 py-2 bg-primary text-white rounded-lg hover:opacity-90 transition font-medium"
              >
                Comenzar a traducir
              </button>
            ) : (
              <button
                onClick={handleNext}
                disabled={!canProceed()}
                className="px-6 py-2 bg-primary text-white rounded-lg hover:opacity-90 transition font-medium disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Continuar →
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export default OnboardingWizard;
