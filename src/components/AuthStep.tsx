import React, { useState } from 'react';
import { useAuth } from '../hooks/useAuth';

export interface AuthStepProps {
  onSuccess: () => void;
}

type AuthMode = 'selection' | 'byok' | 'login' | 'register';

export function AuthStep({ onSuccess }: AuthStepProps) {
  const { 
    loginEmail, 
    loginGoogle, 
    register, 
    saveByokKey, 
    validateByokKeyComplete,
    hasByokKey,
    isAuthenticated,
    loading,
    error: authError,
    clearError
  } = useAuth();

  const [mode, setMode] = useState<AuthMode>('selection');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [name, setName] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [localError, setLocalError] = useState<string | null>(null);

  // If already authenticated or has key, move on immediately
  React.useEffect(() => {
    if (isAuthenticated || hasByokKey) {
      onSuccess();
    }
  }, [isAuthenticated, hasByokKey, onSuccess]);

  const handleByokSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLocalError(null);
    clearError();

    if (!apiKey.trim()) {
      setLocalError('Por favor ingresa tu API Key');
      return;
    }

    const result = await validateByokKeyComplete(apiKey);
    if (result.valid) {
      await saveByokKey(apiKey);
      onSuccess();
    } else {
      setLocalError(result.error_message || 'Clave inválida');
    }
  };

  const handleAuthSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLocalError(null);
    clearError();

    try {
      if (mode === 'login') {
        const result = await loginEmail(email, password);
        if (result) onSuccess();
      } else if (mode === 'register') {
        const result = await register(email, password, name || undefined);
        if (result) onSuccess();
      }
    } catch (err) {
      setLocalError(err instanceof Error ? err.message : 'Error en autenticación');
    }
  };

  const handleGoogleLogin = async () => {
    try {
      const result = await loginGoogle();
      if (result) onSuccess();
    } catch (err) {
      setLocalError(err instanceof Error ? err.message : 'Error en login con Google');
    }
  };

  const errorMsg = localError || authError;

  if (mode === 'selection') {
    return (
      <div className="flex flex-col gap-4 max-w-md mx-auto">
        <div className="text-center mb-6">
          <h2 className="text-2xl font-semibold mb-2">Bienvenido a Traductor Desktop</h2>
          <p className="text-text-secondary">Selecciona cómo deseas utilizar la aplicación</p>
        </div>

        <button
          onClick={() => setMode('register')}
          className="flex flex-col items-center p-6 bg-surface border border-border hover:border-primary/50 rounded-xl transition-all"
        >
          <span className="text-lg font-medium mb-1">Crear Cuenta</span>
          <span className="text-sm text-text-secondary text-center">Únete y elige entre planes gratuitos y de pago con traducción ilimitada.</span>
        </button>

        <button
          onClick={() => setMode('login')}
          className="flex flex-col items-center p-6 bg-surface border border-border hover:border-primary/50 rounded-xl transition-all"
        >
          <span className="text-lg font-medium mb-1">Iniciar Sesión</span>
          <span className="text-sm text-text-secondary text-center">Ya tengo una cuenta registrada.</span>
        </button>

        <div className="relative py-4">
          <div className="absolute inset-0 flex items-center">
            <div className="w-full border-t border-border"></div>
          </div>
          <div className="relative flex justify-center text-sm">
            <span className="px-2 bg-background text-text-secondary">O para usuarios avanzados</span>
          </div>
        </div>

        <button
          onClick={() => setMode('byok')}
          className="flex flex-col items-center p-6 bg-surface border border-border hover:border-primary/50 rounded-xl transition-all"
        >
          <span className="text-lg font-medium mb-1">Usar mi propia API Key (BYOK)</span>
          <span className="text-sm text-text-secondary text-center">Conecta tu cuenta de Google Cloud y paga directamente a Google.</span>
        </button>
      </div>
    );
  }

  if (mode === 'byok') {
    return (
      <div className="flex flex-col max-w-md mx-auto">
        <button 
          onClick={() => { setMode('selection'); setLocalError(null); }}
          className="self-start text-sm text-text-secondary hover:text-text mb-6"
        >
          ← Volver
        </button>
        
        <h2 className="text-xl font-semibold mb-2">Configurar BYOK</h2>
        <p className="text-sm text-text-secondary mb-6">
          Ingresa tu API Key de Gemini Live. Esta clave se guardará de forma segura en tu sistema.
        </p>

        <form onSubmit={handleByokSubmit} className="space-y-4">
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="AIzaSy..."
            className="w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary/50"
            required
          />

          {errorMsg && <p className="text-sm text-error">{errorMsg}</p>}

          <button
            type="submit"
            disabled={loading}
            className="w-full py-2 bg-primary text-white rounded-lg hover:bg-primary-hover disabled:opacity-50"
          >
            {loading ? 'Validando...' : 'Guardar y Continuar'}
          </button>
        </form>
      </div>
    );
  }

  return (
    <div className="flex flex-col max-w-md mx-auto">
      <button 
        onClick={() => { setMode('selection'); setLocalError(null); }}
        className="self-start text-sm text-text-secondary hover:text-text mb-6"
      >
        ← Volver
      </button>

      <h2 className="text-xl font-semibold mb-6">
        {mode === 'login' ? 'Iniciar Sesión' : 'Crear Cuenta'}
      </h2>

      <button
        onClick={handleGoogleLogin}
        disabled={loading}
        className="w-full py-2 bg-white text-black border border-gray-300 rounded-lg hover:bg-gray-50 flex items-center justify-center gap-2 mb-6 disabled:opacity-50"
      >
        <svg className="w-5 h-5" viewBox="0 0 24 24">
          <path fill="currentColor" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" />
          <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
          <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" />
          <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" />
        </svg>
        Continuar con Google
      </button>

      <div className="relative py-2 mb-6">
        <div className="absolute inset-0 flex items-center">
          <div className="w-full border-t border-border"></div>
        </div>
        <div className="relative flex justify-center text-sm">
          <span className="px-2 bg-background text-text-secondary">O con correo electrónico</span>
        </div>
      </div>

      <form onSubmit={handleAuthSubmit} className="space-y-4">
        {mode === 'register' && (
          <div className="space-y-1">
            <label className="text-sm text-text-secondary">Nombre (opcional)</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>
        )}
        <div className="space-y-1">
          <label className="text-sm text-text-secondary">Correo electrónico</label>
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className="w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary/50"
            required
          />
        </div>
        <div className="space-y-1">
          <label className="text-sm text-text-secondary">Contraseña</label>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary/50"
            required
            minLength={8}
          />
        </div>

        {errorMsg && <p className="text-sm text-error">{errorMsg}</p>}

        <button
          type="submit"
          disabled={loading}
          className="w-full py-2 bg-primary text-white rounded-lg hover:bg-primary-hover mt-4 disabled:opacity-50"
        >
          {loading ? 'Procesando...' : (mode === 'login' ? 'Iniciar Sesión' : 'Crear Cuenta')}
        </button>
      </form>
    </div>
  );
}
