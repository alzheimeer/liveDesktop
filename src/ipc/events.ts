// Tauri event listeners
// Listen to events emitted from Rust backend

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { 
  AudioMetrics, 
  ChannelType, 
  ChannelState, 
  AudioDevice,
  SessionExpirationEvent,
  SessionRestoredEvent,
  UsageWarningEvent,
  UsageLimitReachedEvent,
  UsageBlockedEvent,
} from './types';

// ==================== Audio Event Listeners ====================

export async function onAudioMetrics(
  callback: (metrics: AudioMetrics) => void
): Promise<UnlistenFn> {
  return listen<AudioMetrics>('audio-metrics', (event) => {
    callback(event.payload);
  });
}

export async function onChannelStateChanged(
  callback: (channel: ChannelType, state: ChannelState) => void
): Promise<UnlistenFn> {
  return listen<{ channel: ChannelType; state: ChannelState }>('channel-state', (event) => {
    callback(event.payload.channel, event.payload.state);
  });
}

export async function onDeviceChanged(
  callback: (action: 'connected' | 'disconnected', device: AudioDevice) => void
): Promise<UnlistenFn> {
  return listen<{ action: 'connected' | 'disconnected'; device: AudioDevice }>('device-changed', (event) => {
    callback(event.payload.action, event.payload.device);
  });
}

// ==================== Token Event Listeners ====================

export async function onTokenExpiringSoon(
  callback: () => void
): Promise<UnlistenFn> {
  return listen('token-expiring', () => {
    callback();
  });
}

// ==================== Gemini Event Listeners ====================

export async function onGeminiError(
  callback: (channel: ChannelType, error: string) => void
): Promise<UnlistenFn> {
  return listen<{ channel: ChannelType; error: string }>('gemini-error', (event) => {
    callback(event.payload.channel, event.payload.error);
  });
}

// ==================== Usage Event Listeners (Requirements 10.6, 10.7, 11.8) ====================

/**
 * Listen for usage warning events (80% threshold).
 * 
 * Emitted when the user has consumed 80% or more of their monthly minutes.
 * Use this to show a warning notification to the user.
 * 
 * Requirement: 10.6
 * 
 * @param callback - Function to call when usage reaches 80%
 * @returns Unlisten function to stop listening
 * 
 * @example
 * const unlisten = await onUsageWarning((event) => {
 *   showNotification({
 *     title: 'Advertencia de uso',
 *     message: event.message,
 *     type: 'warning'
 *   });
 * });
 */
export async function onUsageWarning(
  callback: (event: UsageWarningEvent) => void
): Promise<UnlistenFn> {
  return listen<UsageWarningEvent>('usage-warning', (event) => {
    callback(event.payload);
  });
}

/**
 * Listen for usage limit reached events (100% threshold).
 * 
 * Emitted when the user has reached their monthly limit.
 * Use this to show upgrade options and block new translations.
 * 
 * Requirements: 10.7, 11.8
 * 
 * @param callback - Function to call when usage reaches 100%
 * @returns Unlisten function to stop listening
 * 
 * @example
 * const unlisten = await onUsageLimitReached((event) => {
 *   showModal({
 *     title: 'Límite alcanzado',
 *     message: event.message,
 *     upgradeOptions: event.upgradeOptions,
 *     upgradeUrl: event.upgradeUrl
 *   });
 * });
 */
export async function onUsageLimitReached(
  callback: (event: UsageLimitReachedEvent) => void
): Promise<UnlistenFn> {
  return listen<UsageLimitReachedEvent>('usage-limit-reached', (event) => {
    callback(event.payload);
  });
}

/**
 * Listen for usage blocked events.
 * 
 * Emitted when a translation attempt is blocked because the user
 * has exceeded their monthly limit.
 * 
 * Requirement: 10.7
 * 
 * @param callback - Function to call when translation is blocked
 * @returns Unlisten function to stop listening
 * 
 * @example
 * const unlisten = await onUsageBlocked((event) => {
 *   showError({
 *     title: 'Traducción bloqueada',
 *     message: event.message,
 *     suggestion: event.suggestion,
 *     action: { label: 'Actualizar plan', url: event.upgradeUrl }
 *   });
 * });
 */
export async function onUsageBlocked(
  callback: (event: UsageBlockedEvent) => void
): Promise<UnlistenFn> {
  return listen<UsageBlockedEvent>('usage-blocked', (event) => {
    callback(event.payload);
  });
}

// ==================== Update Event Listeners ====================

export async function onUpdateAvailable(
  callback: (version: string, changelog: string) => void
): Promise<UnlistenFn> {
  return listen<{ version: string; changelog: string }>('update-available', (event) => {
    callback(event.payload.version, event.payload.changelog);
  });
}

// ==================== Session Event Listeners ====================

/**
 * Listen for session expired events.
 * 
 * Emitted when the user's session has expired and they need to re-authenticate.
 * The callback receives details about the expiration including a Spanish message.
 * 
 * @param callback - Function to call when session expires
 * @returns Unlisten function to stop listening
 * 
 * @example
 * const unlisten = await onSessionExpired((event) => {
 *   console.log(event.message); // "Tu sesión ha expirado..."
 *   // Navigate to login page
 * });
 */
export async function onSessionExpired(
  callback: (event: SessionExpirationEvent) => void
): Promise<UnlistenFn> {
  return listen<SessionExpirationEvent>('session:expired', (event) => {
    callback(event.payload);
  });
}

/**
 * Listen for session expiring soon events.
 * 
 * Emitted when the session will expire within 24 hours.
 * Use this to warn the user or refresh the session proactively.
 * 
 * @param callback - Function to call when session is about to expire
 * @returns Unlisten function to stop listening
 * 
 * @example
 * const unlisten = await onSessionExpiringSoon((event) => {
 *   const hours = Math.floor(event.secondsUntilExpiry / 3600);
 *   showWarning(`Tu sesión expirará en ${hours} horas`);
 * });
 */
export async function onSessionExpiringSoon(
  callback: (event: SessionExpirationEvent) => void
): Promise<UnlistenFn> {
  return listen<SessionExpirationEvent>('session:expiring_soon', (event) => {
    callback(event.payload);
  });
}

/**
 * Listen for session restored events.
 * 
 * Emitted when a session is successfully restored from SQLite storage,
 * typically during app startup.
 * 
 * @param callback - Function to call when session is restored
 * @returns Unlisten function to stop listening
 * 
 * @example
 * const unlisten = await onSessionRestored((event) => {
 *   console.log(`Welcome back, ${event.name}!`);
 *   // Update UI to show logged-in state
 * });
 */
export async function onSessionRestored(
  callback: (event: SessionRestoredEvent) => void
): Promise<UnlistenFn> {
  return listen<SessionRestoredEvent>('session:restored', (event) => {
    callback(event.payload);
  });
}

/**
 * Listen for session cleared events.
 * 
 * Emitted when the user logs out and the session is cleared
 * from both memory and SQLite storage.
 * 
 * @param callback - Function to call when session is cleared
 * @returns Unlisten function to stop listening
 * 
 * @example
 * const unlisten = await onSessionCleared(() => {
 *   // Navigate to login page
 *   // Clear any cached user data
 * });
 */
export async function onSessionCleared(
  callback: () => void
): Promise<UnlistenFn> {
  return listen('session:cleared', () => {
    callback();
  });
}
