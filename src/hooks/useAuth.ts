/**
 * Authentication Hook
 * 
 * React hook for authentication state and actions via Tauri IPC.
 * Manages user sessions, BYOK key management, and session expiration.
 * 
 * @module hooks/useAuth
 * @see Requirement 12.2 - Replace fetch/WebSocket with Tauri IPC
 * @see Requirement 9.6 - Token with 7-day expiration
 * @see Requirement 9.10 - Request re-authentication when token expires
 * @see Requirement 8.3 - Store API key in OS Keyring
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import {
  loginWithGoogle,
  loginWithEmail,
  registerWithEmail,
  logout as logoutCommand,
  getSession,
  restoreSession,
  startSessionExpirationChecker,
  stopSessionExpirationChecker,
  setByokKey,
  getByokKeyExists,
  deleteByokKey,
  validateByokKey,
  validateByokKeyFull
} from '../ipc/commands';
import {
  onSessionExpired,
  onSessionExpiringSoon,
  onSessionRestored,
  onSessionCleared
} from '../ipc/events';
import type { 
  UserInfo, 
  SessionInfo,
  LoginResponse,
  ValidationResult,
  SessionExpirationEvent,
  SessionRestoredEvent
} from '../ipc/types';

/** State returned by the useAuth hook */
export interface AuthState {
  /** Current user information (null if not authenticated) */
  user: UserInfo | null;
  /** Whether the user has an active session */
  isAuthenticated: boolean;
  /** Whether the user has a BYOK API key configured */
  hasByokKey: boolean;
  /** Whether BYOK mode is active (has key and preferred auth method) */
  isByokMode: boolean;
  /** Whether initial data is loading */
  loading: boolean;
  /** Error message if any operation failed */
  error: string | null;
  /** Session expiration date (ISO 8601) */
  expiresAt: string | null;
  /** Whether session is about to expire (within 24 hours) */
  isExpiringSoon: boolean;
  /** Whether session has expired */
  isExpired: boolean;
}

/** Actions available from the useAuth hook */
export interface AuthActions {
  /** Login with Google OAuth */
  loginGoogle: () => Promise<boolean>;
  /** Login with email and password */
  loginEmail: (email: string, password: string) => Promise<boolean>;
  /** Register a new account with email and password */
  register: (email: string, password: string, name?: string) => Promise<boolean>;
  /** Logout current user */
  logout: () => Promise<void>;
  /** Refresh session status */
  refreshSession: () => Promise<void>;
  /** Save BYOK API key to OS keyring */
  saveByokKey: (apiKey: string) => Promise<boolean>;
  /** Validate BYOK API key completely (format + Gemini test) */
  validateByokKeyComplete: (apiKey: string) => Promise<ValidationResult>;
  /** Remove BYOK API key from OS keyring */
  removeByokKey: () => Promise<void>;
  /** Clear the current error */
  clearError: () => void;
}

export type UseAuthReturn = AuthState & AuthActions;

/**
 * Hook for managing authentication state and actions.
 * 
 * Provides:
 * - Session management (login, logout, registration)
 * - BYOK key management (save, validate, remove)
 * - Session expiration tracking and events
 * - Automatic session restoration on mount
 * - Session expiration checker background task
 * 
 * @example
 * ```tsx
 * function AuthPage() {
 *   const { 
 *     user, 
 *     isAuthenticated,
 *     isExpiringSoon,
 *     loginEmail,
 *     logout,
 *     error 
 *   } = useAuth();
 *   
 *   if (isExpiringSoon) {
 *     return <SessionExpirationWarning onRelogin={refreshSession} />;
 *   }
 *   
 *   if (!isAuthenticated) {
 *     return <LoginForm onLogin={loginEmail} error={error} />;
 *   }
 *   
 *   return <UserProfile user={user} onLogout={logout} />;
 * }
 * ```
 */
export function useAuth(): UseAuthReturn {
  const [user, setUser] = useState<UserInfo | null>(null);
  const [hasByokKey, setHasByokKey] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expiresAt, setExpiresAt] = useState<string | null>(null);
  const [isExpiringSoon, setIsExpiringSoon] = useState(false);
  const [isExpired, setIsExpired] = useState(false);

  // Track mounted state to avoid state updates after unmount
  const mountedRef = useRef(true);
  
  // React 18 Strict Mode compatibility: reset to true on mount
  useEffect(() => {
    mountedRef.current = true;
    
    const onByokChanged = async (e: Event) => {
      try {
        if (e instanceof CustomEvent && typeof e.detail?.hasKey === 'boolean') {
          if (mountedRef.current) {
            setHasByokKey(e.detail.hasKey);
          }
          return;
        }
        
        const byokExists = await getByokKeyExists();
        if (mountedRef.current) {
          setHasByokKey(byokExists);
        }
      } catch (err) {
        console.warn("Could not check BYOK key status:", err);
      }
    };
    
    window.addEventListener('byok_changed', onByokChanged);
    
    return () => {
      window.removeEventListener('byok_changed', onByokChanged);
    };
  }, []);
  
  // Track if session checker was started
  const checkerStartedRef = useRef(false);

  // Derived state
  const isAuthenticated = user !== null;
  const isByokMode = hasByokKey && !user; // BYOK mode when has key but no subscription session

  // Initialize: check session state and restore if needed
  useEffect(() => {
    async function init() {
      try {
        // Try to restore session from storage
        const sessionInfo = await restoreSession();
        
        if (!mountedRef.current) return;
        
        if (sessionInfo.is_authenticated && sessionInfo.user) {
          setUser(sessionInfo.user);
          setExpiresAt(sessionInfo.expires_at);
          setIsExpired(false);
        }
        
        // Check BYOK key existence
        const byokExists = await getByokKeyExists();
        if (mountedRef.current) {
          setHasByokKey(byokExists);
        }
        
        // Start session expiration checker (every 60 seconds)
        if (!checkerStartedRef.current) {
          await startSessionExpirationChecker(60);
          checkerStartedRef.current = true;
        }
      } catch (err) {
        if (mountedRef.current) {
          setError(err instanceof Error ? err.message : 'Error al verificar sesión');
        }
      } finally {
        if (mountedRef.current) {
          setLoading(false);
        }
      }
    }
    init();

    return () => {
      mountedRef.current = false;
      // Stop the session checker on unmount
      if (checkerStartedRef.current) {
        stopSessionExpirationChecker().catch(() => {});
        checkerStartedRef.current = false;
      }
    };
  }, []);

  // Subscribe to session events
  useEffect(() => {
    const unsubscribers: Array<() => void> = [];

    // Session expired event
    onSessionExpired((event: SessionExpirationEvent) => {
      if (!mountedRef.current) return;
      setIsExpired(true);
      setIsExpiringSoon(false);
      setUser(null);
      setExpiresAt(null);
      setError(event.message);
    }).then(unsub => unsubscribers.push(unsub));

    // Session expiring soon event (24 hours before)
    onSessionExpiringSoon((_event: SessionExpirationEvent) => {
      if (!mountedRef.current) return;
      setIsExpiringSoon(true);
    }).then(unsub => unsubscribers.push(unsub));

    // Session restored event
    onSessionRestored((event: SessionRestoredEvent) => {
      if (!mountedRef.current) return;
      setUser({
        user_id: '', // Will be populated on next getSession call
        email: event.email,
        name: event.name,
        plan: event.plan
      });
      setExpiresAt(event.expiresAt);
      setIsExpired(false);
      setIsExpiringSoon(false);
    }).then(unsub => unsubscribers.push(unsub));

    // Session cleared event (logout)
    onSessionCleared(() => {
      if (!mountedRef.current) return;
      setUser(null);
      setExpiresAt(null);
      setIsExpired(false);
      setIsExpiringSoon(false);
    }).then(unsub => unsubscribers.push(unsub));

    return () => {
      unsubscribers.forEach(unsub => unsub());
    };
  }, []);

  const loginGoogle = useCallback(async (): Promise<boolean> => {
    try {
      setLoading(true);
      setError(null);
      const response: LoginResponse = await loginWithGoogle();
      
      if (!mountedRef.current) return false;
      
      if (response.success && response.user) {
        setUser(response.user);
        setIsExpired(false);
        setIsExpiringSoon(false);
        return true;
      } else {
        setError(response.error || 'Error al iniciar sesión con Google');
        return false;
      }
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : 'Error al iniciar sesión con Google');
      }
      return false;
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, []);

  const loginEmail = useCallback(async (email: string, password: string): Promise<boolean> => {
    try {
      setLoading(true);
      setError(null);
      const response: LoginResponse = await loginWithEmail(email, password);
      
      if (!mountedRef.current) return false;
      
      if (response.success && response.user) {
        setUser(response.user);
        setIsExpired(false);
        setIsExpiringSoon(false);
        return true;
      } else {
        setError(response.error || 'Credenciales inválidas');
        return false;
      }
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : 'Error al iniciar sesión');
      }
      return false;
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, []);

  const register = useCallback(async (email: string, password: string, name?: string): Promise<boolean> => {
    try {
      setLoading(true);
      setError(null);
      const response: LoginResponse = await registerWithEmail(email, password, name);
      
      if (!mountedRef.current) return false;
      
      if (response.success && response.user) {
        setUser(response.user);
        setIsExpired(false);
        setIsExpiringSoon(false);
        return true;
      } else {
        setError(response.error || 'Error al registrar cuenta');
        return false;
      }
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : 'Error al registrar cuenta');
      }
      return false;
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, []);

  const logout = useCallback(async () => {
    try {
      setLoading(true);
      await logoutCommand();
      if (mountedRef.current) {
        setUser(null);
        setExpiresAt(null);
        setIsExpired(false);
        setIsExpiringSoon(false);
      }
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : 'Error al cerrar sesión');
      }
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, []);

  const refreshSession = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const sessionInfo: SessionInfo = await getSession();
      
      if (!mountedRef.current) return;
      
      if (sessionInfo.is_authenticated && sessionInfo.user) {
        setUser(sessionInfo.user);
        setExpiresAt(sessionInfo.expires_at);
        setIsExpired(false);
      } else {
        setUser(null);
        setExpiresAt(null);
      }
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : 'Error al actualizar sesión');
      }
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, []);

  const saveByokKey = useCallback(async (apiKey: string): Promise<boolean> => {
    try {
      setLoading(true);
      setError(null);
      
      // First validate format
      const isValidFormat = await validateByokKey(apiKey);
      if (!isValidFormat) {
        setError('Formato de API key inválido');
        return false;
      }
      
      // Save to keyring
      await setByokKey(apiKey);
      
      if (mountedRef.current) {
        setHasByokKey(true);
      }
      window.dispatchEvent(new CustomEvent('byok_changed', { detail: { hasKey: true } }));
      return true;
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : 'Error al guardar API key');
      }
      return false;
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, []);

  const validateByokKeyComplete = useCallback(async (apiKey: string): Promise<ValidationResult> => {
    try {
      setLoading(true);
      setError(null);
      return await validateByokKeyFull(apiKey);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Error al validar API key';
      if (mountedRef.current) {
        setError(errorMessage);
      }
      return {
        valid: false,
        error_message: errorMessage,
        suggestion: 'Verifica tu conexión a internet e intenta de nuevo'
      };
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, []);

  const removeByokKey = useCallback(async () => {
    try {
      setLoading(true);
      await deleteByokKey();
      if (mountedRef.current) {
        setHasByokKey(false);
      }
      window.dispatchEvent(new CustomEvent('byok_changed', { detail: { hasKey: false } }));
      return true;
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : 'Error al eliminar API key');
      }
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, []);

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  return {
    // State
    user,
    isAuthenticated,
    hasByokKey,
    isByokMode,
    loading,
    error,
    expiresAt,
    isExpiringSoon,
    isExpired,
    // Actions
    loginGoogle,
    loginEmail,
    register,
    logout,
    refreshSession,
    saveByokKey,
    validateByokKeyComplete,
    removeByokKey,
    clearError,
  };
}
