/**
 * Settings Modal Component
 * 
 * Modal dialog for application settings including audio devices, languages,
 * BYOK key management, and preferences.
 * 
 * @module components/SettingsModal
 * @see Requirement 12.1 - Reuse components from web project
 * @see Requirement 12.2 - Replace fetch/WebSocket with Tauri IPC
 * @see Requirement 8.1 - UI to enter Gemini API key
 * @see Requirement 8.7 - Options to modify or delete API key
 * @see Requirement 14.3 - Allow device selection
 * @see Requirement 15.4 - Persist language selection
 */

import { useState, useEffect, useCallback } from 'react';
import { useConfig } from '../hooks/useConfig';
import { useAuth } from '../hooks/useAuth';
import { useAudioEngine } from '../hooks/useAudioEngine';
import { DualLanguageSelector } from './LanguageSelector';
import { AuthStep } from './AuthStep';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onOpenSubscription?: () => void;
}

type Tab = 'devices' | 'languages' | 'account' | 'byok' | 'preferences';

/** Tab button component */
function TabButton({ 
  active, 
  onClick, 
  children 
}: { 
  active: boolean; 
  onClick: () => void; 
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={`px-4 py-2 text-sm font-medium rounded-lg transition-colors
        ${active 
          ? 'bg-primary/20 text-primary' 
          : 'text-text-secondary hover:text-text hover:bg-surface-hover'
        }`}
    >
      {children}
    </button>
  );
}

/** Select input component */
function Select({ 
  label, 
  value, 
  options, 
  onChange,
  disabled = false,
}: { 
  label: string; 
  value: string; 
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  return (
    <div className="space-y-1">
      <label className="block text-sm text-text-secondary">{label}</label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        className="w-full px-3 py-2 bg-surface-hover border border-border rounded-lg text-text 
                   focus:outline-none focus:ring-2 focus:ring-primary/50 
                   disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {options.map(opt => (
          <option key={opt.value} value={opt.value}>{opt.label}</option>
        ))}
      </select>
    </div>
  );
}

/** Toggle switch component */
function Toggle({ 
  label, 
  description, 
  checked, 
  onChange 
}: { 
  label: string; 
  description?: string;
  checked: boolean; 
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between py-3">
      <div>
        <p className="text-text font-medium">{label}</p>
        {description && <p className="text-sm text-text-secondary">{description}</p>}
      </div>
      <button
        onClick={() => onChange(!checked)}
        className={`relative w-11 h-6 rounded-full transition-colors
          ${checked ? 'bg-primary' : 'bg-surface-hover'}`}
      >
        <span 
          className={`absolute top-1 w-4 h-4 rounded-full bg-white transition-transform
            ${checked ? 'translate-x-6' : 'translate-x-1'}`}
        />
      </button>
    </div>
  );
}

export function SettingsModal({ isOpen, onClose, onOpenSubscription }: SettingsModalProps) {
  const { config, updateConfig, saveConfig, hasChanges, loading: configLoading } = useConfig();
  const { user, isAuthenticated, logout, hasByokKey, saveByokKey, removeByokKey, validateByokKeyComplete, loading: authLoading } = useAuth();
  const { inputDevices, outputDevices, loopbackDevices, refreshDevices } = useAudioEngine();
  
  const [activeTab, setActiveTab] = useState<Tab>('devices');
  const [byokKey, setByokKey] = useState('');
  const [byokError, setByokError] = useState<string | null>(null);
  const [byokSuccess, setByokSuccess] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  
  // Refresh devices when modal opens
  useEffect(() => {
    if (isOpen) {
      refreshDevices();
    }
  }, [isOpen, refreshDevices]);
  
  // Handle save
  const handleSave = useCallback(async () => {
    setIsSaving(true);
    try {
      await saveConfig();
      onClose();
    } catch (err) {
      console.error('Error saving config:', err);
    } finally {
      setIsSaving(false);
    }
  }, [saveConfig, onClose]);
  
  // Handle BYOK key save
  const handleByokSave = useCallback(async () => {
    if (!byokKey.trim()) {
      setByokError('Ingresa una API key');
      return;
    }
    
    setByokError(null);
    setByokSuccess(false);
    
    // First validate the key
    const result = await validateByokKeyComplete(byokKey);
    
    if (!result.valid) {
      setByokError(result.error_message || 'API key inválida');
      return;
    }
    
    // Save to keyring
    const saved = await saveByokKey(byokKey);
    if (saved) {
      setByokSuccess(true);
      setByokKey('');
      setTimeout(() => setByokSuccess(false), 3000);
    } else {
      setByokError('Error al guardar la API key');
    }
  }, [byokKey, validateByokKeyComplete, saveByokKey]);
  
  // Handle BYOK key removal
  const handleByokRemove = useCallback(async () => {
    if (confirm('¿Estás seguro de que quieres eliminar tu API key?')) {
      await removeByokKey();
    }
  }, [removeByokKey]);
  
  if (!isOpen) return null;
  
  // Device options
  const inputOptions = [
    { value: 'default', label: 'Por defecto' },
    ...inputDevices.map(d => ({ value: d.id, label: d.name }))
  ];
  
  const outputOptions = [
    { value: 'default', label: 'Por defecto' },
    ...outputDevices.map(d => ({ value: d.id, label: d.name }))
  ];
  
  const loopbackOptions = [
    { value: 'default', label: 'Por defecto' },
    ...loopbackDevices.map(d => ({ value: d.id, label: d.name }))
  ];
  
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div 
        className="absolute inset-0 bg-black/50 backdrop-blur-sm"
        onClick={onClose}
      />
      
      {/* Modal */}
      <div className="relative w-full max-w-lg bg-surface border border-border rounded-xl shadow-xl">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <h2 className="text-lg font-medium text-text">Configuración</h2>
          <button 
            onClick={onClose}
            className="p-1 text-text-secondary hover:text-text rounded-lg hover:bg-surface-hover transition"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        
        {/* Tabs */}
        <div className="flex gap-1 px-6 py-3 border-b border-border">
          <TabButton active={activeTab === 'devices'} onClick={() => setActiveTab('devices')}>
            Dispositivos
          </TabButton>
          <TabButton active={activeTab === 'languages'} onClick={() => setActiveTab('languages')}>
            Idiomas
          </TabButton>
          <TabButton active={activeTab === 'account'} onClick={() => setActiveTab('account')}>
            Cuenta
          </TabButton>
          <TabButton active={activeTab === 'byok'} onClick={() => setActiveTab('byok')}>
            BYOK
          </TabButton>
          <TabButton active={activeTab === 'preferences'} onClick={() => setActiveTab('preferences')}>
            Preferencias
          </TabButton>
        </div>
        
        {/* Content */}
        <div className="p-6 max-h-[60vh] overflow-y-auto">
          {/* Devices Tab */}
          {activeTab === 'devices' && (
            <div className="space-y-4">
              <Select
                label="Micrófono de entrada"
                value={config.devices.inputDevice || 'default'}
                options={inputOptions}
                onChange={(value) => updateConfig({ 
                  devices: { ...config.devices, inputDevice: value } 
                })}
              />
              
              <Select
                label="Dispositivo de captura del sistema (Loopback)"
                value={config.devices.systemCaptureDevice || 'default'}
                options={loopbackOptions}
                onChange={(value) => updateConfig({ 
                  devices: { ...config.devices, systemCaptureDevice: value } 
                })}
              />
              
              <Select
                label="Salida de audio traducido"
                value={config.devices.outputDevice || 'default'}
                options={outputOptions}
                onChange={(value) => updateConfig({ 
                  devices: { ...config.devices, outputDevice: value } 
                })}
              />
              
              <button
                onClick={() => refreshDevices()}
                className="text-sm text-primary hover:underline"
              >
                Actualizar lista de dispositivos
              </button>
            </div>
          )}
          
          {/* Languages Tab */}
          {activeTab === 'languages' && (
            <div className="space-y-4">
              <p className="text-sm text-text-secondary mb-4">
                Configura los idiomas de traducción para cada canal. Los cambios se aplicarán 
                en la siguiente sesión de traducción.
              </p>
              <DualLanguageSelector showSource={true} />
            </div>
          )}
          
          {/* Account Tab */}
          {activeTab === 'account' && (
            <div className="space-y-4">
              <div className="p-4 bg-surface-hover rounded-lg">
                <h3 className="text-sm font-medium text-text mb-4">
                  Tu Cuenta
                </h3>
                
                {isAuthenticated && user ? (
                  <div className="space-y-4">
                    <div className="flex items-center gap-4">
                      <div className="w-12 h-12 bg-primary/20 rounded-full flex items-center justify-center">
                        <span className="text-primary text-xl font-medium">
                          {user.name?.charAt(0).toUpperCase() || user.email?.charAt(0).toUpperCase() || 'U'}
                        </span>
                      </div>
                      <div>
                        <p className="font-medium">{user.name || 'Usuario'}</p>
                        <p className="text-sm text-text-secondary">{user.email}</p>
                      </div>
                    </div>
                    
                    <div className="flex gap-3">
                      {onOpenSubscription && (
                        <button
                          onClick={() => {
                            onClose();
                            onOpenSubscription();
                          }}
                          className="px-4 py-2 bg-primary/10 text-primary rounded-lg hover:bg-primary/20 transition text-sm"
                        >
                          Administrar Suscripción
                        </button>
                      )}
                      <button
                        onClick={() => logout()}
                        className="px-4 py-2 bg-error/10 text-error rounded-lg hover:bg-error/20 transition text-sm"
                      >
                        Cerrar Sesión
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="mt-2">
                    <AuthStep onSuccess={() => {}} />
                  </div>
                )}
              </div>
            </div>
          )}
          
          {/* BYOK Tab */}
          {activeTab === 'byok' && (
            <div className="space-y-4">
              <div className="p-4 bg-surface-hover rounded-lg">
                <h3 className="text-sm font-medium text-text mb-2">
                  Bring Your Own Key (BYOK)
                </h3>
                <p className="text-sm text-text-secondary mb-4">
                  Usa tu propia API key de Gemini. Sin límites de minutos mensuales. 
                  El costo se factura directamente por Google.
                </p>
                
                {hasByokKey ? (
                  <div className="space-y-3">
                    <div className="flex items-center gap-2 text-success">
                      <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                      </svg>
                      <span className="text-sm font-medium">API key configurada</span>
                    </div>
                    <button
                      onClick={handleByokRemove}
                      disabled={authLoading}
                      className="text-sm text-error hover:underline disabled:opacity-50"
                    >
                      Eliminar API key
                    </button>
                  </div>
                ) : (
                  <div className="space-y-3">
                    <input
                      type="password"
                      value={byokKey}
                      onChange={(e) => setByokKey(e.target.value)}
                      placeholder="Ingresa tu API key de Gemini"
                      className="w-full px-3 py-2 bg-background border border-border rounded-lg text-text 
                                placeholder:text-text-secondary focus:outline-none focus:ring-2 focus:ring-primary/50"
                    />
                    
                    {byokError && (
                      <p className="text-sm text-error">{byokError}</p>
                    )}
                    
                    {byokSuccess && (
                      <p className="text-sm text-success">API key guardada correctamente</p>
                    )}
                    
                    <button
                      onClick={handleByokSave}
                      disabled={authLoading || !byokKey.trim()}
                      className="px-4 py-2 bg-primary text-white rounded-lg hover:opacity-90 
                                transition disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {authLoading ? 'Validando...' : 'Guardar API key'}
                    </button>
                  </div>
                )}
              </div>
              
              <p className="text-xs text-text-secondary">
                Tu API key se almacena de forma segura en el llavero del sistema operativo 
                (Windows Credential Manager / macOS Keychain) y nunca se envía a nuestros servidores.
              </p>
            </div>
          )}
          
          {/* Preferences Tab */}
          {activeTab === 'preferences' && (
            <div className="divide-y divide-border">
              <Toggle
                label="Iniciar minimizado"
                description="Iniciar la aplicación en la bandeja del sistema"
                checked={config.preferences.startMinimized}
                onChange={(checked) => updateConfig({
                  preferences: { ...config.preferences, startMinimized: checked }
                })}
              />
              
              <Toggle
                label="Inicio automático"
                description="Iniciar la aplicación al arrancar el sistema"
                checked={config.preferences.autoStart}
                onChange={(checked) => updateConfig({
                  preferences: { ...config.preferences, autoStart: checked }
                })}
              />
              
              <Toggle
                label="Enviar reportes de errores"
                description="Ayúdanos a mejorar enviando reportes anónimos de errores"
                checked={config.preferences.enableSentry}
                onChange={(checked) => updateConfig({
                  preferences: { ...config.preferences, enableSentry: checked }
                })}
              />
              
              <div className="py-3">
                <label className="block text-text font-medium mb-2">Tema</label>
                <div className="flex gap-2">
                  {(['dark', 'light', 'system'] as const).map((theme) => (
                    <button
                      key={theme}
                      onClick={() => updateConfig({
                        preferences: { ...config.preferences, theme }
                      })}
                      className={`px-4 py-2 rounded-lg text-sm capitalize transition
                        ${config.preferences.theme === theme
                          ? 'bg-primary/20 text-primary border border-primary/30'
                          : 'bg-surface-hover text-text-secondary hover:text-text'
                        }`}
                    >
                      {theme === 'dark' ? 'Oscuro' : theme === 'light' ? 'Claro' : 'Sistema'}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          )}
        </div>
        
        {/* Footer */}
        <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-border">
          <button
            onClick={onClose}
            className="px-4 py-2 text-text-secondary hover:text-text rounded-lg hover:bg-surface-hover transition"
          >
            Cancelar
          </button>
          <button
            onClick={handleSave}
            disabled={isSaving || configLoading || !hasChanges}
            className="px-4 py-2 bg-primary text-white rounded-lg hover:opacity-90 
                      transition disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {isSaving ? 'Guardando...' : 'Guardar cambios'}
          </button>
        </div>
      </div>
    </div>
  );
}
