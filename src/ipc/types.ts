/**
 * Shared TypeScript types for Tauri IPC communication.
 * 
 * This file defines all interfaces and types used for communication between
 * the React frontend and Rust backend via Tauri's IPC system.
 * 
 * @module ipc/types
 * @see Requirements 22.3, 22.5
 * 
 * ## Available IPC Commands
 * 
 * ### Audio Commands
 * - `enumerate_audio_devices` - List available audio devices
 * - `start_system_channel` - Start system audio capture (meeting → user)
 * - `start_user_channel` - Start user microphone capture (user → meeting)
 * - `stop_channel` - Stop a specific audio channel
 * - `change_audio_device` - Change device without stopping translation
 * - `get_audio_state` - Get current audio engine state
 * - `get_vbcable_status` - Get VB-Cable installation status (Windows)
 * 
 * ### Auth Commands
 * - `login_with_google` - Start Google OAuth flow
 * - `login_with_email` - Login with email/password
 * - `register_with_email` - Create new account
 * - `logout` - Clear session and logout
 * - `get_session` - Get current session info
 * - `restore_session` - Restore session from storage
 * - `is_authenticated` - Check if user is authenticated
 * 
 * ### BYOK Commands (Bring Your Own Key)
 * - `set_byok_key` - Store API key in OS keyring
 * - `get_byok_key_exists` - Check if BYOK key exists
 * - `delete_byok_key` - Remove BYOK key from keyring
 * - `validate_byok_key` - Validate key format only
 * - `validate_byok_key_full` - Validate format + test with Gemini
 * 
 * ### Config Commands
 * - `get_config` - Get current app configuration
 * - `save_config` - Save configuration to storage
 * - `export_config` - Export config to JSON file
 * - `import_config` - Import config from JSON file
 * - `export_config_string` - Export config as JSON string
 * - `import_config_string` - Import config from JSON string
 * - `config_exists` - Check if config file exists
 * - `reset_config` - Reset to default configuration
 * 
 * ### Usage Commands
 * - `get_usage_stats` - Get current month usage statistics
 * - `get_usage_history` - Get daily usage for past N days
 * - `can_start_translation` - Check if user can start (under limit)
 * - `get_upgrade_options` - Get available plan upgrades
 * - `check_usage_limits` - Emit notifications if thresholds crossed
 */

// ============================================================================
// AUDIO TYPES
// ============================================================================

/**
 * Audio channel type identifier.
 * 
 * - `system`: Captures meeting audio (Teams/Zoom/Meet) and translates to user's language
 * - `user`: Captures user's microphone and translates to meeting language
 */
export type ChannelType = 'system' | 'user';

/**
 * Current state of an audio channel.
 * 
 * @example
 * // Check if channel is active
 * if (state.type === 'active') {
 *   console.log('Channel is translating');
 * }
 * 
 * // Handle errors
 * if (state.type === 'error') {
 *   showError(state.message);
 * }
 */
export type ChannelState = 
  | { type: 'inactive' }
  | { type: 'active' }
  | { type: 'paused' }
  | { type: 'error'; message: string };

/**
 * Reason why a channel is paused.
 * Matches Rust PauseReason enum serialization.
 */
export type PauseReason = 
  | { type: 'userRequested' }
  | { type: 'deviceDisconnected'; deviceId: string; deviceName: string }
  | { type: 'networkError' }
  | { type: 'geminiDisconnected' };

/**
 * Audio device information returned by enumerate_audio_devices.
 * 
 * @see Requirement 2.2 - Enumerate devices within 2 seconds
 * @see Requirement 14.1 - Enumerate dynamically all audio devices
 */
export interface AudioDevice {
  /** Unique device identifier (platform-specific) */
  id: string;
  /** Human-readable device name */
  name: string;
  /** Type of audio device */
  deviceType: 'input' | 'output' | 'loopback';
  /** Whether this is the system default device */
  isDefault: boolean;
}

/**
 * Configuration for starting an audio channel.
 * 
 * @example
 * const config: ChannelConfig = {
 *   sourceLang: 'en',    // English source
 *   targetLang: 'es',    // Spanish target
 *   inputDevice: 'default',
 *   outputDevice: 'default'
 * };
 */
export interface ChannelConfig {
  /** Source language code (ISO 639-1, e.g., "en", "es") */
  sourceLang: string;
  /** Target language code (ISO 639-1) */
  targetLang: string;
  /** Input device ID for audio capture */
  inputDevice: string;
  /** Output device ID for audio playback */
  outputDevice: string;
}

/**
 * Real-time audio metrics emitted every 100ms during active capture.
 * 
 * @see Requirement 12.4 - Show volume indicators updated every 100ms
 * @see Requirement 12.5 - Show latency in milliseconds
 */
export interface AudioMetrics {
  /** Input audio level in decibels (-60 to 0) */
  inputLevelDb: number;
  /** Output audio level in decibels (-60 to 0) */
  outputLevelDb: number;
  /** End-to-end translation latency in milliseconds */
  latencyMs: number;
  /** Number of audio packets sent to Gemini */
  packetsSent: number;
  /** Number of audio packets received from Gemini */
  packetsReceived: number;
}

/**
 * Complete state of the audio engine.
 * Returned by get_audio_state command.
 */
export interface EngineState {
  /** State of the system audio channel (meeting → user) */
  systemChannel: ChannelState;
  /** State of the user microphone channel (user → meeting) */
  userChannel: ChannelState;
  /** Current audio metrics (null if no channel is active) */
  metrics: AudioMetrics | null;
  /** Reason for pause (null if not paused) */
  pauseReason: PauseReason | null;
}

// ============================================================================
// VB-CABLE TYPES (Windows)
// ============================================================================

/**
 * VB-Cable installation status information.
 * Used to determine if the virtual audio driver is available for audio injection.
 * 
 * @see Requirement 4.1 - Detect VB-Cable installation
 * @see Requirement 4.5 - Route translated audio to VB-Cable Output
 * 
 * @example
 * const status = await getVbCableStatus();
 * if (status.isInstalled && status.outputDeviceId) {
 *   console.log('VB-Cable ready for audio injection');
 * } else {
 *   showVbCableInstallPrompt();
 * }
 */
export interface VBCableStatus {
  /** Whether VB-Cable is installed (either input or output detected) */
  isInstalled: boolean;
  /** Whether CABLE Input (virtual microphone) is available */
  inputAvailable: boolean;
  /** Whether CABLE Output (virtual speaker) is available */
  outputAvailable: boolean;
  /** Device ID of CABLE Output if found (for routing translated audio) */
  outputDeviceId: string | null;
  /** Friendly name of CABLE Output if found */
  outputDeviceName: string | null;
}

// ============================================================================
// VIRTUAL AUDIO TYPES (macOS)
// ============================================================================

/**
 * Virtual audio installation status information for macOS.
 * Used to determine if virtual audio is available for audio injection.
 * 
 * On macOS 14+ (Sonoma): Native Virtual Audio Endpoint is used
 * On macOS <14: BlackHole driver is used as fallback
 * 
 * @see Requirement 5.1 - Create Virtual_Audio_Endpoint using AudioDriverKit/CoreAudio
 * @see Requirement 5.7 - Show BlackHole installation instructions if Virtual_Audio_Endpoint fails
 * @see Requirement 13.7 - Verify Virtual_Audio_Endpoint on macOS
 * 
 * @example
 * const status = await getVirtualAudioStatus();
 * if (status.isAvailable) {
 *   if (status.isNative) {
 *     console.log('Using native macOS 14+ virtual audio');
 *   } else {
 *     console.log('Using BlackHole fallback');
 *   }
 * } else if (status.statusType === 'requires_blackhole') {
 *   showBlackHoleInstallPrompt(status.installationInstructions);
 * }
 */
export interface VirtualAudioStatus {
  /** 
   * Type of virtual audio status
   * - "native" - macOS 14+ native virtual audio endpoint
   * - "blackhole" - BlackHole driver installed (fallback for macOS <14)
   * - "not_available" - No virtual audio available
   * - "requires_blackhole" - macOS <14 and BlackHole not installed
   * - "not_applicable" - Not on macOS (Windows/Linux)
   */
  statusType: 'native' | 'blackhole' | 'not_available' | 'requires_blackhole' | 'not_applicable';
  /** Whether virtual audio is available and ready */
  isAvailable: boolean;
  /** macOS version string (e.g., "14.0.0") */
  macosVersion: string | null;
  /** Whether this is native macOS 14+ virtual audio */
  isNative: boolean;
  /** BlackHole device ID if using fallback */
  blackholeDeviceId: string | null;
  /** BlackHole device name if using fallback */
  blackholeDeviceName: string | null;
  /** Installation instructions if BlackHole is required but not installed */
  installationInstructions: VirtualAudioInstructions | null;
}

/**
 * Installation instructions for virtual audio drivers (e.g., BlackHole).
 * Shown when the user needs to install a virtual audio driver.
 * 
 * @see Requirement 5.7 - Show instructions for installing BlackHole as alternative
 */
export interface VirtualAudioInstructions {
  /** Title of the instructions (e.g., "Instalar BlackHole Virtual Audio Driver") */
  title: string;
  /** Description of what the driver does */
  description: string;
  /** Step-by-step installation instructions */
  steps: string[];
  /** Download URL for the driver */
  downloadUrl: string;
  /** Homebrew command for installation (optional) */
  homebrewCommand: string | null;
}

// ============================================================================
// AUTH TYPES
// ============================================================================

/**
 * Response from login commands (login_with_email, login_with_google, register_with_email).
 * 
 * @see Requirement 9.6 - Return token with 7-day expiration
 */
export interface LoginResponse {
  /** Whether login was successful */
  success: boolean;
  /** User information if login succeeded */
  user: UserInfo | null;
  /** Error message if login failed */
  error: string | null;
}

/**
 * User information structure.
 */
export interface UserInfo {
  /** User's unique identifier */
  user_id: string;
  /** User's email address */
  email: string;
  /** User's display name */
  name: string;
  /** User's subscription plan */
  plan: string;
}

/**
 * Session information structure returned by get_session command.
 * 
 * @see Requirement 9.10 - Request re-authentication when token expires
 */
export interface SessionInfo {
  /** Whether user is authenticated */
  is_authenticated: boolean;
  /** User information if authenticated */
  user: UserInfo | null;
  /** Session expiration time in ISO 8601 format */
  expires_at: string | null;
}

/**
 * Complete user session with all authentication details.
 * 
 * @see Requirement 9.6 - Token expiration of 7 days
 * @see Requirement 9.8 - Store tokens in encrypted SQLite
 */
export interface UserSession {
  /** User's unique identifier */
  userId: string;
  /** User's email address */
  email: string;
  /** User's display name */
  name: string;
  /** URL to user's avatar image (optional) */
  avatarUrl?: string;
  /** User's subscription plan */
  plan: SubscriptionPlan;
  /** Session expiration time (ISO 8601) */
  expiresAt: string;
  /** Session token for API authentication */
  sessionToken: string;
}

/**
 * Subscription plan types.
 * 
 * - `byok_free`: $0/month, requires user's own Gemini API key, unlimited usage
 * - `starter`: $14.99/month, 600 minutes of translation
 * - `pro`: $39.99/month, 2000 minutes of translation
 * 
 * @see Requirement 10.1 - Three subscription plans
 */
export type SubscriptionPlan = 'byok_free' | 'starter' | 'pro';

/**
 * Detailed information about a subscription plan.
 * 
 * @see Requirement 10.2 - Show comparative table of plans
 */
export interface PlanDetails {
  /** Plan display name */
  name: string;
  /** Monthly price in USD */
  price: number;
  /** Monthly minutes limit (0 = unlimited for BYOK) */
  minutesLimit: number;
  /** List of features included in this plan */
  features: string[];
}

// ============================================================================
// BYOK (Bring Your Own Key) TYPES
// ============================================================================

/**
 * Result of BYOK API key validation.
 * Returned by validate_byok_key_full command.
 * 
 * @see Requirement 8.2 - Validate key format before storage
 * @see Requirement 8.6 - Show error if Gemini rejects key
 * 
 * @example
 * const result = await validateByokKeyFull('AIzaSy...');
 * if (!result.valid) {
 *   console.error(result.error_message);
 *   console.log('Suggestion:', result.suggestion);
 * }
 */
export interface ValidationResult {
  /** Whether the API key is valid */
  valid: boolean;
  /** Error message if validation failed (null if valid) */
  error_message: string | null;
  /** Suggestion for fixing the issue (null if valid) */
  suggestion: string | null;
}

/**
 * BYOK configuration status.
 * 
 * @see Requirement 8.3 - Store API key in OS Keyring
 * @see Requirement 8.8 - Show indication of no limits in BYOK mode
 */
export interface ByokStatus {
  /** Whether a BYOK API key is configured */
  hasKey: boolean;
  /** Whether BYOK mode is active (has key and is primary auth method) */
  isActive: boolean;
  /** Last validation result (if key has been validated) */
  lastValidation?: ValidationResult;
}

// ============================================================================
// AUTH EVENT TYPES
// ============================================================================

/**
 * Auth event names for Tauri event listeners.
 */
export const AUTH_EVENTS = {
  /** Emitted when user logs in successfully */
  LOGIN_SUCCESS: 'auth:login_success',
  /** Emitted when login fails */
  LOGIN_ERROR: 'auth:login_error',
  /** Emitted when user logs out */
  LOGOUT: 'auth:logout',
  /** Emitted when session changes */
  SESSION_CHANGED: 'auth:session_changed',
} as const;

// ============================================================================
// SESSION EVENT TYPES
// ============================================================================

/**
 * Event payload when session expires or is about to expire.
 * Emitted by the session expiration checker background task.
 * 
 * @see Requirement 9.10 - Request re-authentication when token expires
 */
export interface SessionExpirationEvent {
  /** Whether the session has expired */
  expired: boolean;
  /** Time until expiration in seconds (negative if already expired) */
  secondsUntilExpiry: number;
  /** Human-readable message in Spanish */
  message: string;
}

/**
 * Event payload when a session is restored from storage.
 */
export interface SessionRestoredEvent {
  /** User email */
  email: string;
  /** User display name */
  name: string;
  /** Subscription plan */
  plan: string;
  /** Expiration time in ISO 8601 format */
  expiresAt: string;
}

/**
 * Session event names for Tauri event listeners.
 */
export const SESSION_EVENTS = {
  /** Emitted when session has expired */
  SESSION_EXPIRED: 'session:expired',
  /** Emitted when session is about to expire (24 hours before) */
  SESSION_EXPIRING_SOON: 'session:expiring_soon',
  /** Emitted when session is restored from storage */
  SESSION_RESTORED: 'session:restored',
  /** Emitted when session is cleared (logout) */
  SESSION_CLEARED: 'session:cleared',
} as const;

// ============================================================================
// USAGE TYPES
// ============================================================================

/**
 * Current month's usage statistics.
 * Returned by get_usage_stats command.
 * 
 * @see Requirement 11.4 - Show dashboard with usage vs limit
 */
export interface UsageStats {
  currentMonth: {
    /** Minutes used this billing period */
    used: number;
    /** Plan's minute limit (0 = unlimited for BYOK) */
    limit: number;
    /** Percentage of limit used (0-100) */
    percentage: number;
  };
  /** User's current plan */
  plan: SubscriptionPlan;
  /** Date when billing period renews (ISO 8601) */
  renewalDate: string;
}

/**
 * Daily usage breakdown.
 * Returned by get_usage_history command.
 * 
 * @see Requirement 11.4 - Show graph of daily usage for last 30 days
 */
export interface DailyUsage {
  /** Date in YYYY-MM-DD format */
  date: string;
  /** Minutes used for system channel (meeting → user) */
  systemMinutes: number;
  /** Minutes used for user channel (user → meeting) */
  userMinutes: number;
  /** Total minutes for this day */
  totalMinutes: number;
}

/**
 * Upgrade option for higher usage limits.
 * Displayed when user reaches usage limit and needs to upgrade.
 * 
 * @see Requirement 10.7 - Show upgrade options when limit reached
 */
export interface UpgradeOption {
  /** Plan identifier */
  planId: string;
  /** Plan display name */
  planName: string;
  /** Monthly price in USD */
  priceUsd: number;
  /** Monthly minute limit (0 = unlimited) */
  minutesLimit: number;
  /** Whether this is the recommended upgrade */
  isRecommended: boolean;
  /** Additional features of this plan */
  features: string[];
}

// ============================================================================
// USAGE NOTIFICATION TYPES
// ============================================================================

/**
 * Event payload when usage reaches 80% threshold.
 * 
 * @see Requirement 10.6 - Notify at 80% usage
 */
export interface UsageWarningEvent {
  /** Percentage of plan limit used (e.g., 80.5) */
  percentageUsed: number;
  /** Total minutes used in current billing period */
  minutesUsed: number;
  /** Minutes remaining until limit */
  minutesRemaining: number;
  /** Monthly minute limit from plan */
  minutesLimit: number;
  /** User-friendly message in Spanish */
  message: string;
}

/**
 * Event payload when usage reaches 100% threshold.
 * 
 * @see Requirement 10.7 - Block translation at 100%
 * @see Requirement 11.8 - Show notification at 100%
 */
export interface UsageLimitReachedEvent {
  /** Total minutes used (equals or exceeds limit) */
  minutesUsed: number;
  /** Monthly minute limit from plan */
  minutesLimit: number;
  /** Current plan name */
  currentPlan: string;
  /** Available upgrade options */
  upgradeOptions: UpgradeOption[];
  /** URL to subscription/upgrade page */
  upgradeUrl: string;
  /** User-friendly message in Spanish */
  message: string;
}

/**
 * Event payload when translation is blocked due to limit.
 * 
 * @see Requirement 10.7 - Block translation when limit reached
 */
export interface UsageBlockedEvent {
  /** Reason for blocking */
  reason: string;
  /** URL to subscription/upgrade page */
  upgradeUrl: string;
  /** Current plan name */
  currentPlan: string;
  /** User-friendly message in Spanish */
  message: string;
  /** Suggested action */
  suggestion: string;
}

/**
 * Usage notification event names for Tauri event listeners.
 */
export const USAGE_EVENTS = {
  /** Emitted when usage reaches 80% threshold (Requirement 10.6) */
  USAGE_WARNING: 'usage-warning',
  /** Emitted when usage reaches 100% limit (Requirements 10.7, 11.8) */
  USAGE_LIMIT_REACHED: 'usage-limit-reached',
  /** Emitted when translation is blocked (Requirement 10.7) */
  USAGE_BLOCKED: 'usage-blocked',
} as const;

// ============================================================================
// CONFIG TYPES
// ============================================================================

/**
 * Application configuration structure.
 * Stored in encrypted SQLite and persisted between sessions.
 * 
 * @see Requirement 25.1 - Save configuration
 * @see Requirement 25.2 - Load configuration on startup
 */
export interface AppConfig {
  /** Language settings for translation channels */
  languages: {
    /** Source language for system channel (what meeting speaks) */
    systemSourceLang: string;
    /** Target language for system channel (what user hears) */
    systemTargetLang: string;
    /** Source language for user channel (what user speaks) */
    userSourceLang: string;
    /** Target language for user channel (what meeting hears) */
    userTargetLang: string;
  };
  /** Audio device selections */
  devices: {
    /** Selected input device ID (microphone) */
    inputDevice?: string;
    /** Selected system capture device ID (for WASAPI loopback) */
    systemCaptureDevice?: string;
    /** Selected output device ID (for translated audio playback) */
    outputDevice?: string;
  };
  /** User preferences */
  preferences: {
    /** Start app minimized to system tray */
    startMinimized: boolean;
    /** Auto-start app when OS boots */
    autoStart: boolean;
    /** UI theme setting */
    theme: 'dark' | 'light' | 'system';
    /** Enable Sentry error reporting */
    enableSentry: boolean;
  };
}

/**
 * Default configuration values.
 */
export const DEFAULT_CONFIG: AppConfig = {
  languages: {
    systemSourceLang: 'en',
    systemTargetLang: 'es',
    userSourceLang: 'es',
    userTargetLang: 'en',
  },
  devices: {},
  preferences: {
    startMinimized: false,
    autoStart: false,
    theme: 'dark',
    enableSentry: true,
  },
};

// ============================================================================
// ERROR TYPES
// ============================================================================

/**
 * IPC error response structure.
 * All IPC commands may return this on failure.
 */
export interface IpcError {
  /** Error code (categorized by type) */
  code: number;
  /** User-friendly error message in Spanish */
  message: string;
  /** Suggested action to resolve the error */
  suggestion?: string;
}

/**
 * Error code ranges for categorization.
 * 
 * @see Requirement 20.4 - Show specific message per error code
 */
export const ERROR_CODES = {
  /** Audio errors: 1000-1999 */
  AUDIO: {
    WASAPI_NOT_AVAILABLE: 1001,
    NO_DEVICES_AVAILABLE: 1002,
    DEVICE_DISCONNECTED: 1003,
    CAPTURE_FAILED: 1004,
    PLAYBACK_FAILED: 1005,
    RESAMPLING_FAILED: 1006,
  },
  /** Network errors: 2000-2999 */
  NETWORK: {
    CONNECTION_FAILED: 2001,
    TIMEOUT: 2002,
    GEMINI_DISCONNECTED: 2003,
    GEMINI_AUTH_FAILED: 2004,
    SYNC_FAILED: 2005,
  },
  /** Auth errors: 3000-3999 */
  AUTH: {
    INVALID_CREDENTIALS: 3001,
    SESSION_EXPIRED: 3002,
    TOKEN_REFRESH_FAILED: 3003,
    OAUTH_FAILED: 3004,
    REGISTRATION_FAILED: 3005,
    EMAIL_EXISTS: 3006,
  },
  /** BYOK errors: 4000-4999 */
  BYOK: {
    INVALID_FORMAT: 4001,
    KEYRING_ACCESS_FAILED: 4002,
    KEY_REJECTED: 4003,
  },
  /** Config errors: 5000-5999 */
  CONFIG: {
    SAVE_FAILED: 5001,
    LOAD_FAILED: 5002,
    MIGRATION_FAILED: 5003,
    INVALID_CONFIG: 5004,
  },
  /** Usage errors: 6000-6999 */
  USAGE: {
    LIMIT_REACHED: 6001,
    SYNC_FAILED: 6002,
  },
} as const;

// ============================================================================
// BACKEND EVENT TYPES (from events.rs)
// ============================================================================

/**
 * Device action for device change events.
 * Matches Rust DeviceAction enum.
 */
export type DeviceAction = 
  | 'connected'
  | 'disconnected'
  | { stateChanged: { newState: DeviceState } };

/**
 * Device state enumeration.
 * Matches Rust DeviceState enum.
 */
export type DeviceState = 'active' | 'disabled' | 'notPresent' | 'unplugged';

/**
 * Payload for device change events (from events.rs).
 */
export interface DeviceChangedPayload {
  /** Action that occurred */
  action: DeviceAction;
  /** Device that triggered the event */
  device: AudioDevice;
}

/**
 * Payload for device disconnection during capture (Requirement 2.7).
 */
export interface DeviceDisconnectedPayload {
  /** ID of the disconnected device */
  deviceId: string;
  /** Friendly name of the disconnected device */
  deviceName: string;
  /** Which channel was affected (system or user) */
  channel: ChannelType;
  /** User-friendly message in Spanish */
  message: string;
  /** Suggested recovery action */
  suggestion: string;
}

/**
 * Payload for channel state change events.
 */
export interface ChannelStateChangedPayload {
  /** Which channel changed */
  channel: ChannelType;
  /** New state of the channel */
  state: ChannelState;
}

/**
 * Payload for audio error events.
 */
export interface AudioErrorPayload {
  /** Error code (1xxx for audio errors) */
  code: number;
  /** User-friendly error message in Spanish */
  message: string;
  /** Suggested recovery action */
  suggestion: string;
  /** Which channel is affected (if applicable) */
  channel?: ChannelType;
}

/**
 * Payload for WASAPI not available error (Requirement 2.5).
 */
export interface WasapiNotAvailablePayload {
  /** Specific reason why WASAPI is not available */
  reason: string;
  /** User-friendly message in Spanish */
  message: string;
  /** Suggested recovery step */
  suggestion: string;
}

/**
 * Payload for no devices available error (Requirement 2.6).
 */
export interface NoDevicesAvailablePayload {
  /** User-friendly message in Spanish */
  message: string;
  /** Suggested recovery step */
  suggestion: string;
}

/**
 * Payload for token expiring soon event.
 * Emitted 10 minutes before the token expires.
 * 
 * @see Requirement 7.2 - Renew token 10 minutes before expiration
 */
export interface TokenExpiringPayload {
  /** Minutes until token expiration */
  minutesRemaining: number;
  /** User-friendly message in Spanish */
  message: string;
}

/**
 * Payload for Gemini error events.
 * Matches Rust GeminiErrorPayload in events.rs
 * 
 * @see Requirement 6.6 - Notify user if reconnection fails
 * @see Requirement 6.7 - Show error message on authentication failure
 */
export interface GeminiErrorPayload {
  /** Which channel had the error (system or user) */
  channel: ChannelType;
  /** Error description from the connection */
  error: string;
  /** Error code if available (e.g., "TIMEOUT", "AUTH_FAILED") */
  code: string | null;
  /** User-friendly error message in Spanish */
  message: string;
  /** Suggested recovery action in Spanish */
  suggestion: string;
}

/**
 * Payload for usage limit reached event (100% threshold).
 * Matches Rust UsageLimitPayload in events.rs
 * 
 * @see Requirement 10.7 - Block translation at 100%
 * @see Requirement 11.8 - Notify when usage limit is reached
 */
export interface UsageLimitPayload {
  /** Minutes used this month */
  used: number;
  /** Monthly limit in minutes */
  limit: number;
  /** Percentage of limit used (0-100) */
  percentage: number;
  /** User-friendly message in Spanish */
  message: string;
}

/**
 * Payload for update available events.
 * Matches Rust UpdateAvailablePayload in events.rs
 * 
 * @see Requirement 17.2 - Show notification with changelog
 */
export interface UpdateAvailablePayload {
  /** New version string (e.g., "1.2.0") */
  version: string;
  /** Changelog/release notes */
  changelog: string;
  /** Download URL (optional, may be handled by auto-updater) */
  downloadUrl: string | null;
  /** Whether this update is mandatory */
  mandatory: boolean;
}

// ============================================================================
// EVENT NAMES (from events.rs)
// ============================================================================

/**
 * All Tauri event names emitted by the backend.
 * These names match the constants in src-tauri/src/events.rs
 * 
 * @see Requirement 22.2 - Emit backend events to frontend
 */
export const EVENT_NAMES = {
  /** Audio metrics updated (emitted every 100ms during active capture) */
  AUDIO_METRICS: 'audio-metrics',
  /** Audio channel state changed */
  CHANNEL_STATE_CHANGED: 'channel-state',
  /** Audio device connected or disconnected */
  DEVICE_CHANGED: 'device-changed',
  /** Error occurred in audio subsystem */
  AUDIO_ERROR: 'audio-error',
  /** Device disconnected during capture (Requirement 2.7) */
  DEVICE_DISCONNECTED: 'device-disconnected',
  /** WASAPI not available (Requirement 2.5) */
  WASAPI_NOT_AVAILABLE: 'wasapi-not-available',
  /** No audio devices available (Requirement 2.6) */
  NO_DEVICES_AVAILABLE: 'no-devices-available',
  /** Token expiring soon (10 minutes before expiration) */
  TOKEN_EXPIRING: 'token-expiring',
  /** Error from Gemini Live connection */
  GEMINI_ERROR: 'gemini-error',
  /** Usage warning at 80% threshold (Requirement 10.6) */
  USAGE_WARNING: 'usage-warning',
  /** Usage limit reached at 100% (Requirements 10.7, 11.8) */
  USAGE_LIMIT_REACHED: 'usage-limit',
  /** Translation blocked due to usage limit (Requirement 10.7) */
  USAGE_BLOCKED: 'usage-blocked',
  /** Application update available */
  UPDATE_AVAILABLE: 'update-available',
} as const;

// ============================================================================
// UNION TYPE FOR ALL APP EVENTS
// ============================================================================

/**
 * Union type representing all possible application events.
 * Useful for type-safe event handling in components.
 * 
 * @example
 * function handleEvent(event: AppEvent) {
 *   switch (event.type) {
 *     case 'audio-metrics':
 *       updateMetrics(event.data);
 *       break;
 *     case 'channel-state':
 *       updateChannelState(event.data.channel, event.data.state);
 *       break;
 *     case 'usage-limit-reached':
 *       showUpgradeModal(event.data.upgradeOptions);
 *       break;
 *   }
 * }
 */
export type AppEvent = 
  // Audio events
  | { type: 'audio-metrics'; data: AudioMetrics }
  | { type: 'channel-state'; data: ChannelStateChangedPayload }
  | { type: 'device-changed'; data: DeviceChangedPayload }
  | { type: 'device-disconnected'; data: DeviceDisconnectedPayload }
  | { type: 'audio-error'; data: AudioErrorPayload }
  | { type: 'wasapi-not-available'; data: WasapiNotAvailablePayload }
  | { type: 'no-devices-available'; data: NoDevicesAvailablePayload }
  // Token events
  | { type: 'token-expiring'; data: TokenExpiringPayload }
  // Gemini events
  | { type: 'gemini-error'; data: GeminiErrorPayload }
  // Usage events (from events.rs - using UsageLimitPayload for backend events)
  | { type: 'usage-warning'; data: UsageWarningEvent }
  | { type: 'usage-limit'; data: UsageLimitPayload }
  | { type: 'usage-limit-reached'; data: UsageLimitReachedEvent }
  | { type: 'usage-blocked'; data: UsageBlockedEvent }
  // Update events
  | { type: 'update-available'; data: UpdateAvailablePayload }
  // Session events
  | { type: 'session:expired'; data: SessionExpirationEvent }
  | { type: 'session:expiring_soon'; data: SessionExpirationEvent }
  | { type: 'session:restored'; data: SessionRestoredEvent }
  | { type: 'session:cleared' }
  // Auth events
  | { type: 'auth:login_success'; data: UserInfo }
  | { type: 'auth:login_error'; data: string }
  | { type: 'auth:logout' }
  | { type: 'auth:session_changed'; data: UserInfo };

// ============================================================================
// GEMINI / TOKEN SERVICE TYPES
// ============================================================================

/**
 * Ephemeral token for Gemini Live connection.
 * 
 * @see Requirement 7.1 - Generate 1-hour ephemeral tokens
 * @see Requirement 7.3 - Store token only in memory
 */
export interface EphemeralToken {
  /** The token string for Gemini API authentication */
  token: string;
  /** Expiration time in ISO 8601 format (1 hour from generation) */
  expiresAt: string;
}

/**
 * Token Service API response for ephemeral token generation.
 * 
 * @see Requirement 7.5 - Reject if no active subscription
 */
export type EphemeralTokenResponse = 
  | {
      success: true;
      /** Gemini ephemeral token */
      token: string;
      /** Expiration time (1 hour from now) */
      expiresAt: string;
    }
  | {
      success: false;
      error: 'subscription_required' | 'rate_limited' | 'invalid_session';
    };

// ============================================================================
// GEMINI LIVE CLIENT TYPES
// ============================================================================

/**
 * Configuration for Gemini Live client.
 * 
 * @see Requirement 6.2 - Use gemini-3.5-live-translate-preview model
 */
export interface GeminiConfig {
  /** Source language (ISO 639-1) */
  sourceLang: string;
  /** Target language (ISO 639-1) */
  targetLang: string;
  /** Authentication token (ephemeral or BYOK API key) */
  token: string;
}

/**
 * Gemini Live WebSocket connection state.
 */
export type GeminiConnectionState = 
  | { type: 'disconnected' }
  | { type: 'connecting' }
  | { type: 'connected' }
  | { type: 'error'; message: string; retryCount: number };

// ============================================================================
// ONBOARDING / WIZARD TYPES
// ============================================================================

/**
 * Onboarding wizard step identifier.
 * 
 * @see Requirement 13.1 - Start wizard if no configuration saved
 */
export type OnboardingStep = 
  | 'welcome'
  | 'capture-device'   // System audio capture device selection
  | 'microphone'       // User microphone selection
  | 'output'           // Output device selection
  | 'virtual-driver'   // VB-Cable (Windows) or Virtual Audio Endpoint (macOS)
  | 'audio-test'       // Audio test for each device
  | 'complete';

/**
 * Onboarding wizard state.
 */
export interface OnboardingState {
  /** Current step */
  currentStep: OnboardingStep;
  /** Whether wizard has been completed */
  completed: boolean;
  /** Selected devices during wizard */
  selections: {
    captureDevice?: string;
    microphone?: string;
    outputDevice?: string;
  };
  /** Test results */
  testResults: {
    captureDeviceOk?: boolean;
    microphoneOk?: boolean;
    outputDeviceOk?: boolean;
  };
}

// ============================================================================
// SYSTEM TRAY TYPES
// ============================================================================

/**
 * System tray icon state.
 * 
 * @see Requirement 16.2 - Change icon color/shape for state
 */
export type TrayIconState = 'inactive' | 'translating' | 'error';

/**
 * System tray menu action.
 * 
 * @see Requirement 16.3 - Menu with show/pause/settings/close
 */
export type TrayMenuActionKebab = 
  | 'show-window'
  | 'toggle-translation'
  | 'open-settings'
  | 'quit';

// ============================================================================
// AUTO-UPDATER TYPES
// ============================================================================

/**
 * Update check result.
 * 
 * @see Requirement 17.1 - Check updates on startup and every 24h
 */
export interface UpdateCheckResult {
  /** Whether an update is available */
  updateAvailable: boolean;
  /** Update information if available */
  updateInfo: UpdateInfo | null;
  /** Current app version */
  currentVersion: string;
  /** Error message if check failed */
  error: string | null;
}

/**
 * Detailed information about an available update.
 * 
 * @see Requirement 17.2 - Show notification with changelog
 */
export interface UpdateInfo {
  /** New version string (e.g., "1.2.0") */
  version: string;
  /** Changelog/release notes (summarized) */
  changelog: string;
  /** Download URL for the update (if available) */
  downloadUrl: string | null;
  /** Whether the update is mandatory */
  mandatory: boolean;
  /** Size of the update in bytes (if known) */
  sizeBytes: number | null;
}

/**
 * Update download progress.
 */
export interface UpdateProgress {
  /** Bytes downloaded so far */
  downloaded: number;
  /** Total bytes to download */
  total: number;
  /** Download percentage (0-100) */
  percentage: number;
}

/**
 * Update state.
 * 
 * @see Requirement 17.3 - Download in background
 * @see Requirement 17.6 - Maintain current version if update fails
 */
export type UpdateState =
  | { type: 'idle' }
  | { type: 'checking' }
  | { type: 'available'; info: UpdateCheckResult }
  | { type: 'downloading'; progress: UpdateProgress }
  | { type: 'ready' }  // Ready to install
  | { type: 'error'; message: string };

// ============================================================================
// SUPPORTED LANGUAGES
// ============================================================================

/**
 * Supported languages for Gemini Live translation.
 * ISO 639-1 codes.
 * 
 * @see Requirement 15.3 - Support Gemini Live translate preview languages
 */
export const SUPPORTED_LANGUAGES = [
  { code: 'en', name: 'English', nativeName: 'English' },
  { code: 'es', name: 'Spanish', nativeName: 'Español' },
  { code: 'fr', name: 'French', nativeName: 'Français' },
  { code: 'de', name: 'German', nativeName: 'Deutsch' },
  { code: 'it', name: 'Italian', nativeName: 'Italiano' },
  { code: 'pt', name: 'Portuguese', nativeName: 'Português' },
  { code: 'ja', name: 'Japanese', nativeName: '日本語' },
  { code: 'ko', name: 'Korean', nativeName: '한국어' },
  { code: 'zh', name: 'Chinese', nativeName: '中文' },
  { code: 'ru', name: 'Russian', nativeName: 'Русский' },
  { code: 'ar', name: 'Arabic', nativeName: 'العربية' },
  { code: 'hi', name: 'Hindi', nativeName: 'हिन्दी' },
] as const;

export type LanguageCode = typeof SUPPORTED_LANGUAGES[number]['code'];

// ============================================================================
// BILLING / INVOICE TYPES
// ============================================================================

/**
 * Invoice information for billing history.
 * 
 * @see Requirement 10.5 - Show last 24 invoices as downloadable PDF
 */
export interface Invoice {
  /** Invoice ID */
  id: string;
  /** Invoice date (ISO 8601) */
  date: string;
  /** Amount in cents (USD) */
  amountCents: number;
  /** Invoice status */
  status: 'paid' | 'pending' | 'failed';
  /** PDF download URL */
  pdfUrl?: string;
  /** Whether PDF has been downloaded locally */
  downloaded: boolean;
  /** Local file path if downloaded */
  localPath?: string;
}

/**
 * Billing information summary.
 */
export interface BillingInfo {
  /** Current plan details */
  currentPlan: PlanDetails;
  /** Next billing date (ISO 8601) */
  nextBillingDate: string;
  /** Payment method on file */
  paymentMethod?: {
    type: 'card';
    last4: string;
    brand: string;
    expiryMonth: number;
    expiryYear: number;
  };
  /** Recent invoices (up to 24) */
  recentInvoices: Invoice[];
}

// ============================================================================
// PLAN CONSTANTS
// ============================================================================

/**
 * Plan details constants.
 * 
 * @see Requirement 10.1 - Three subscription plans
 */
export const PLAN_DETAILS: Record<SubscriptionPlan, PlanDetails> = {
  byok_free: {
    name: 'BYOK Free',
    price: 0,
    minutesLimit: 0,  // Unlimited
    features: [
      'Uso ilimitado',
      'Requiere API key propia de Gemini',
      'Costos facturados por Google',
      'Sin soporte prioritario',
    ],
  },
  starter: {
    name: 'Starter',
    price: 14.99,
    minutesLimit: 600,
    features: [
      '600 minutos/mes',
      'No requiere API key',
      'Soporte por email',
      'Actualizaciones automáticas',
    ],
  },
  pro: {
    name: 'Pro',
    price: 39.99,
    minutesLimit: 2000,
    features: [
      '2000 minutos/mes',
      'No requiere API key',
      'Soporte prioritario',
      'Actualizaciones automáticas',
      'Acceso anticipado a nuevas funciones',
    ],
  },
};



// ============================================================================
// AUDIO TEST TYPES
// ============================================================================

/**
 * Configuration for playing a test tone.
 * 
 * @see Requirement 13.8 - Play 3-second test tone for each device
 */
export interface TestToneConfig {
  /** Frequency of the test tone in Hz (default: 440 Hz = A4 note) */
  frequencyHz?: number;
  /** Duration of the test tone in milliseconds (default: 3000ms = 3 seconds) */
  durationMs?: number;
  /** Volume level from 0.0 to 1.0 (default: 0.5) */
  volume?: number;
}

/**
 * Result of an audio test playback.
 * 
 * @see Requirement 13.8 - Play 3-second test tone for each device
 * @see Requirement 13.9 - Allow selecting alternative device if test fails
 */
export interface AudioTestResult {
  /** Whether the test completed successfully (playback finished) */
  success: boolean;
  /** Device ID that was tested */
  deviceId: string;
  /** Device name for display */
  deviceName: string;
  /** Error message if test failed */
  error: string | null;
  /** Duration of playback in milliseconds */
  durationMs: number;
}

/**
 * State of a device audio test (for UI feedback).
 */
export type AudioTestStatus = 
  | { type: 'idle' }
  | { type: 'playing'; deviceId: string; deviceName: string }
  | { type: 'completed'; result: AudioTestResult }
  | { type: 'error'; message: string };

// ============================================================================
// SYSTEM TRAY TYPES
// ============================================================================

/**
 * System tray icon state.
 * 
 * The tray icon changes color based on application state:
 * - inactive: Gray - No translation in progress
 * - active: Green - Translation is running
 * - error: Red - An error has occurred
 * 
 * @see Requirement 16.1 - Show icon in System Tray (Windows) / Menu Bar (macOS)
 * @see Requirement 16.2 - Change icon color/shape to indicate state
 */
export type TrayState = 'inactive' | 'active' | 'error';

/**
 * System tray menu action.
 * 
 * @see Requirement 16.3 - Menu with: Show window, Pause/Resume, Settings, Close
 */
export type TrayMenuAction = 'showWindow' | 'togglePause' | 'openSettings' | 'quit';

/**
 * Payload for tray state change events.
 */
export interface TrayStateChangedPayload {
  /** Current tray state */
  state: TrayState;
  /** Human-readable description of the state */
  description: string;
}

/**
 * Payload for tray menu action events.
 */
export interface TrayMenuActionPayload {
  /** Menu action that was triggered */
  action: TrayMenuAction;
}

/**
 * Configuration for tray behavior.
 * 
 * @see Requirement 16.4 - Minimize to tray when closing window
 * @see Requirement 16.5 - Option to start minimized
 */
export interface TrayConfig {
  /** Whether to minimize to tray when closing window (instead of quitting) */
  minimizeToTray: boolean;
  /** Whether to start the application minimized to tray */
  startMinimized: boolean;
}

/**
 * Default tray configuration.
 */
export const DEFAULT_TRAY_CONFIG: TrayConfig = {
  minimizeToTray: true,
  startMinimized: false,
};

/**
 * Tray event names for Tauri event listeners.
 * 
 * @see Requirement 16.2 - State changes emit events
 * @see Requirement 16.3 - Menu actions emit events
 */
export const TRAY_EVENTS = {
  /** Emitted when tray state changes */
  STATE_CHANGED: 'tray-state-changed',
  /** Emitted when tray icon is clicked */
  ICON_CLICKED: 'tray-icon-clicked',
  /** Emitted when a tray menu action is triggered */
  MENU_ACTION: 'tray-menu-action',
} as const;

/**
 * Helper to convert TrayState to human-readable description.
 */
export function getTrayStateDescription(state: TrayState): string {
  switch (state) {
    case 'inactive':
      return 'Inactivo';
    case 'active':
      return 'Traduciendo';
    case 'error':
      return 'Error';
    default:
      return 'Desconocido';
  }
}

/**
 * Helper to get tray tooltip text based on state.
 */
export function getTrayTooltip(state: TrayState): string {
  return `Traductor Desktop - ${getTrayStateDescription(state)}`;
}
