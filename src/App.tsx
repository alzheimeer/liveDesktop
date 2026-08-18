/**
 * Main Application Component
 * 
 * Root component that composes the main UI with Header and audio channel panels.
 * Uses Tauri IPC for all backend communication.
 * 
 * @module App
 * @see Requirement 12.1 - Reuse components from web project
 * @see Requirement 12.2 - Replace fetch/WebSocket with Tauri IPC
 * @see Requirement 12.3 - Show two main panels
 * @see Requirement 12.7 - Dark theme with indigo/cyan accents
 * @see Requirement 13.1 - Start wizard if no saved configuration
 */

import { useState, useCallback, useEffect } from "react";
import { Header } from "./components/Header";
import { SystemAudioPanel } from "./components/SystemAudioPanel";
import { UserMicPanel } from "./components/UserMicPanel";
import { OnboardingWizard } from "./components/OnboardingWizard";
import { useAudioEngine } from "./hooks/useAudioEngine";
import { useConfig } from "./hooks/useConfig";
import { useAuth } from "./hooks/useAuth";
import { configExists } from "./ipc/commands";
import type { AppConfig } from "./ipc/types";

function App() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [checkingConfig, setCheckingConfig] = useState(true);
  const { loading, error, clearError, isTranslating } = useAudioEngine();
  const { updateConfig } = useConfig();
  const { saveByokKey, hasByokKey, error: authError, clearError: clearAuthError } = useAuth();
  const [apiKeyInput, setApiKeyInput] = useState('');
  const [savingKey, setSavingKey] = useState(false);
  
  // Check if configuration exists on mount (Requirement 13.1)
  useEffect(() => {
    async function checkConfig() {
      try {
        const exists = await configExists();
        if (!exists) {
          setShowOnboarding(true);
        }
      } catch (err) {
        console.error('Error checking config:', err);
        // If check fails, show onboarding to be safe
        setShowOnboarding(true);
      } finally {
        setCheckingConfig(false);
      }
    }
    checkConfig();
  }, []);
  
  const handleSettingsOpen = useCallback(() => {
    setIsSettingsOpen(true);
  }, []);
  
  const handleSettingsClose = useCallback(() => {
    setIsSettingsOpen(false);
  }, []);
  
  const handleOnboardingComplete = useCallback((config: AppConfig) => {
    updateConfig(config);
    setShowOnboarding(false);
  }, [updateConfig]);
  
  const handleOnboardingSkip = useCallback(() => {
    setShowOnboarding(false);
  }, []);
  
  // Show loading while checking configuration
  if (checkingConfig) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="flex flex-col items-center gap-4">
          <div className="w-12 h-12 border-4 border-primary/30 border-t-primary rounded-full animate-spin" />
          <p className="text-text-secondary">Cargando configuración...</p>
        </div>
      </div>
    );
  }
  
  return (
    <div className="min-h-screen bg-background flex flex-col">
      <Header onSettingsClick={handleSettingsOpen} />
      
      <main className="flex-1 p-6">
        <div className="max-w-4xl mx-auto space-y-6">
          {/* Loading state */}
          {loading && (
            <div className="flex items-center justify-center py-12">
              <div className="flex flex-col items-center gap-4">
                <div className="w-12 h-12 border-4 border-primary/30 border-t-primary rounded-full animate-spin" />
                <p className="text-text-secondary">Inicializando audio...</p>
              </div>
            </div>
          )}
          
          {/* Global error display */}
          {error && !loading && (
            <div className="p-4 bg-error/10 border border-error/20 rounded-xl">
              <div className="flex items-start justify-between">
                <div className="flex items-start gap-3">
                  <svg className="w-5 h-5 text-error flex-shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                  </svg>
                  <div>
                    <p className="text-error font-medium">Error del sistema de audio</p>
                    <p className="text-error/70 text-sm mt-1">{error}</p>
                  </div>
                </div>
                <button
                  onClick={clearError}
                  className="text-error/60 hover:text-error transition-colors p-1"
                  title="Cerrar"
                >
                  <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </div>
          )}
          
          {/* Translation status banner */}
          {isTranslating && !loading && (
            <div className="bg-success/10 border border-success/20 rounded-xl p-4">
              <div className="flex items-center gap-3">
                <div className="w-3 h-3 rounded-full bg-success animate-pulse" />
                <span className="text-success font-medium">Traducción en curso</span>
              </div>
            </div>
          )}
          
          {/* Audio channel panels */}
          {!loading && (
            <div className="space-y-6">
              <SystemAudioPanel />
              <UserMicPanel />
            </div>
          )}
          
          {/* Quick tips */}
          {!loading && (
            <div className="bg-surface/50 rounded-xl p-4 border border-border">
              <h3 className="text-sm font-medium text-text mb-2">💡 Consejos rápidos</h3>
              <ul className="text-sm text-text-secondary space-y-1">
                <li>• <strong>Canal Sistema:</strong> Traduce lo que escuchas de la reunión a tu idioma.</li>
                <li>• <strong>Canal Usuario:</strong> Traduce tu voz para que la reunión te entienda.</li>
                <li>• Puedes usar ambos canales simultáneamente para traducción bidireccional.</li>
              </ul>
            </div>
          )}
        </div>
      </main>
      
      {/* Footer with status */}
      <footer className="border-t border-border px-6 py-3">
        <div className="max-w-4xl mx-auto flex items-center justify-between text-xs text-text-secondary">
          <span>
            {isTranslating 
              ? 'Conexión activa con Gemini Live' 
              : 'Listo para traducir'
            }
          </span>
          <span>Traductor Desktop v1.0.0</span>
        </div>
      </footer>
      
      {/* Settings modal placeholder - to be implemented in task 21 */}
      {isSettingsOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-surface rounded-xl p-6 max-w-md w-full mx-4 border border-border">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold text-text">Configuración</h2>
              <button
                onClick={() => {
                  clearAuthError();
                  handleSettingsClose();
                }}
                className="text-text-secondary hover:text-text transition-colors p-1"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <p className="text-text-secondary text-sm mb-4">
              El panel de configuración completo se implementará en la tarea 21 (Onboarding Wizard).
            </p>
            <div className="mt-4 border-t border-border pt-4">
              <h3 className="text-md font-medium text-text mb-2">API Key de Gemini (BYOK)</h3>
              <p className="text-xs text-text-secondary mb-3">
                Ingresa tu VITE_GEMINI_API_KEY para habilitar la traducción local.
              </p>
              <div className="flex gap-2">
                <input
                  type="password"
                  value={apiKeyInput}
                  onChange={(e) => setApiKeyInput(e.target.value)}
                  placeholder={hasByokKey ? "••••••••••••••••" : "AIzaSy..."}
                  className="flex-1 bg-surface-hover border border-border rounded-lg px-3 py-2 text-sm text-text focus:outline-none focus:border-primary"
                />
                <button
                  onClick={async () => {
                    if (!apiKeyInput) return;
                    setSavingKey(true);
                    clearAuthError();
                    try {
                      const success = await saveByokKey(apiKeyInput);
                      if (success) {
                        setApiKeyInput('');
                        handleSettingsClose();
                      }
                    } catch (e) {
                      console.error("Failed to save API key", e);
                    } finally {
                      setSavingKey(false);
                    }
                  }}
                  disabled={savingKey || !apiKeyInput}
                  className="bg-primary hover:bg-primary/90 text-white px-4 py-2 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
                >
                  {savingKey ? 'Guardando...' : 'Guardar'}
                </button>
              </div>
              {authError && (
                <div className="mt-2 text-xs text-error">
                  {authError}
                </div>
              )}
            </div>
          </div>
        </div>
      )}
      
      {/* Onboarding Wizard - starts automatically if no config saved */}
      <OnboardingWizard
        isOpen={showOnboarding}
        onComplete={handleOnboardingComplete}
        onSkip={handleOnboardingSkip}
      />
    </div>
  );
}

export default App;
