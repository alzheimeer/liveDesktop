/**
 * Tauri IPC command invocations
 * 
 * This module provides TypeScript wrappers for all Tauri IPC commands.
 * It replaces fetch()/WebSocket from the web app with Tauri invoke().
 * 
 * @module ipc/commands
 * @see Requirement 12.2 - Replace fetch/WebSocket with Tauri IPC
 */

import { invoke } from '@tauri-apps/api/core';
import type { 
  AudioDevice, 
  ChannelConfig, 
  EngineState,
  AppConfig,
  UsageStats,
  DailyUsage,
  ChannelType,
  VBCableStatus,
  VirtualAudioStatus,
  ValidationResult,
  LoginResponse,
  SessionInfo,
  UpgradeOption,
  SubscriptionPlan,
  TestToneConfig,
  AudioTestResult,
} from './types';

// ============================================================================
// AUDIO COMMANDS
// ============================================================================

/**
 * Enumerate all available audio devices on the system.
 * 
 * Returns input (microphones), output (speakers), and loopback devices.
 * 
 * @returns Array of audio devices with id, name, type, and default status
 * @throws Error if device enumeration fails
 * 
 * @see Requirement 2.2 - Enumerate devices within 2 seconds
 * @see Requirement 14.1 - Enumerate dynamically all audio devices
 * 
 * @example
 * const devices = await enumerateAudioDevices();
 * const microphones = devices.filter(d => d.deviceType === 'input');
 */
export async function enumerateAudioDevices(): Promise<AudioDevice[]> {
  return invoke('enumerate_audio_devices');
}

/**
 * Start the system audio channel (meeting → user).
 * 
 * Captures audio from the system (Teams/Zoom/Meet) and translates it
 * to the user's target language.
 * 
 * @param config - Channel configuration with source/target languages and devices
 * @param token - Optional Gemini ephemeral token or BYOK API key
 * @throws Error if channel start fails (e.g., device unavailable, auth failure)
 * 
 * @see Requirement 2.1 - Capture system audio using WASAPI Loopback
 * @see Requirement 6.1 - Connect to Gemini Live
 * 
 * @example
 * await startSystemChannel({
 *   sourceLang: 'en',
 *   targetLang: 'es',
 *   inputDevice: 'loopback-device-id',
 *   outputDevice: 'speakers-id'
 * }, ephemeralToken);
 */
export async function startSystemChannel(config: ChannelConfig, token?: string): Promise<void> {
  return invoke('start_system_channel', { config, token: token ?? '' });
}

/**
 * Start the user audio channel (user → meeting).
 * 
 * Captures audio from the user's microphone and translates it
 * to the meeting's language, injecting it via VB-Cable or virtual audio.
 * 
 * @param config - Channel configuration with source/target languages and devices
 * @param token - Optional Gemini ephemeral token or BYOK API key
 * @throws Error if channel start fails
 * 
 * @see Requirement 4.5 - Route translated audio to VB-Cable Output
 * @see Requirement 6.5 - Support two simultaneous Gemini sessions
 * 
 * @example
 * await startUserChannel({
 *   sourceLang: 'es',
 *   targetLang: 'en',
 *   inputDevice: 'microphone-id',
 *   outputDevice: 'vbcable-output-id'
 * }, ephemeralToken);
 */
export async function startUserChannel(config: ChannelConfig, token?: string): Promise<void> {
  return invoke('start_user_channel', { config, token: token ?? '' });
}

/**
 * Stop a specific audio channel.
 * 
 * Stops capture/playback and closes the Gemini connection for the channel.
 * 
 * @param channel - 'system' or 'user' channel to stop
 * @throws Error if stopping fails
 * 
 * @example
 * await stopChannel('system'); // Stop system translation
 */
export async function stopChannel(channel: ChannelType): Promise<void> {
  return invoke('stop_channel', { channel });
}

/**
 * Change the audio device for an active channel without interrupting translation.
 * 
 * Hot-swaps the device while maintaining the Gemini session.
 * 
 * @param channel - 'system' or 'user' channel to modify
 * @param deviceId - New device ID to use
 * @throws Error if device change fails or device is unavailable
 * 
 * @see Requirement 14.4 - Apply device change without restarting session
 * 
 * @example
 * await changeAudioDevice('user', 'new-microphone-id');
 */
export async function changeAudioDevice(channel: ChannelType, deviceId: string): Promise<void> {
  return invoke('change_audio_device', { channel, deviceId });
}

/**
 * Get the current state of the audio engine.
 * 
 * Returns the state of both channels and current metrics.
 * 
 * @returns EngineState with system/user channel states and metrics
 * 
 * @example
 * const state = await getAudioState();
 * if (state.systemChannel.type === 'active') {
 *   console.log(`Latency: ${state.metrics?.latencyMs}ms`);
 * }
 */
export async function getAudioState(): Promise<EngineState> {
  return invoke('get_audio_state');
}

/**
 * Get VB-Cable installation status (Windows only).
 * 
 * Returns the cached VB-Cable detection result that was performed at app startup.
 * On non-Windows platforms, returns a status indicating VB-Cable is not applicable.
 * 
 * @returns VB-Cable status information
 * @throws Error with Spanish message if detection fails
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
export async function getVbCableStatus(): Promise<VBCableStatus> {
  return invoke('get_vbcable_status');
}

/**
 * Get virtual audio status (macOS only).
 * 
 * Checks the status of virtual audio capabilities on macOS:
 * - macOS 14+ (Sonoma): Native Virtual Audio Endpoint available
 * - macOS <14: Checks if BlackHole is installed, provides instructions if not
 * 
 * On non-macOS platforms, returns a status indicating virtual audio is not applicable.
 * 
 * @returns Virtual audio status information including:
 *   - statusType: Type of virtual audio (native, blackhole, requires_blackhole, not_available)
 *   - isAvailable: Whether virtual audio is ready to use
 *   - macosVersion: Current macOS version
 *   - installationInstructions: Instructions for installing BlackHole if needed
 * @throws Error with Spanish message if detection fails
 * 
 * @see Requirement 5.1 - Create Virtual_Audio_Endpoint using AudioDriverKit/CoreAudio
 * @see Requirement 5.7 - Show BlackHole installation instructions if Virtual_Audio_Endpoint fails
 * @see Requirement 13.7 - Verify Virtual_Audio_Endpoint on macOS, configure automatically
 * 
 * @example
 * const status = await getVirtualAudioStatus();
 * if (status.isAvailable) {
 *   if (status.isNative) {
 *     console.log('Using native macOS 14+ virtual audio');
 *   } else {
 *     console.log(`Using BlackHole: ${status.blackholeDeviceName}`);
 *   }
 * } else if (status.statusType === 'requires_blackhole') {
 *   showBlackHoleInstallPrompt(status.installationInstructions);
 * }
 */
export async function getVirtualAudioStatus(): Promise<VirtualAudioStatus> {
  return invoke('get_virtual_audio_status');
}

// ============================================================================
// AUTH COMMANDS
// ============================================================================

/**
 * Login with Google OAuth.
 * 
 * Initiates OAuth flow with Google. Opens the default browser for user authentication.
 * After successful authentication, the callback will be handled by the deep link handler.
 * 
 * @returns LoginResponse with success status. User info will be null until OAuth completes.
 * @throws Error if OAuth initialization fails
 * 
 * @see Requirement 9.2 - Complete OAuth flow with Google
 * 
 * @example
 * const response = await loginWithGoogle();
 * if (response.success) {
 *   console.log('OAuth flow initiated');
 * }
 */
export async function loginWithGoogle(): Promise<LoginResponse> {
  return invoke('login_with_google');
}

/**
 * Login with email and password.
 * 
 * @param email - User email (RFC 5322 format)
 * @param password - User password (minimum 8 characters)
 * @returns LoginResponse with success status and user info
 * @throws Error if login request fails
 * 
 * @see Requirement 9.6 - Return session token with 7-day expiration
 * @see Requirement 9.7 - Reject with generic error message
 * 
 * @example
 * const response = await loginWithEmail('user@example.com', 'password123');
 * if (response.success && response.user) {
 *   console.log(`Welcome, ${response.user.name}!`);
 * }
 */
export async function loginWithEmail(email: string, password: string): Promise<LoginResponse> {
  return invoke('login_with_email', { email, password });
}

/**
 * Register a new account with email and password.
 * 
 * @param email - User email (RFC 5322 format)
 * @param password - User password (minimum 8 characters)
 * @param name - Optional user display name (defaults to "Usuario")
 * @returns LoginResponse with success status and user info
 * @throws Error if registration request fails
 * 
 * @see Requirement 9.4 - Create account with valid email and password
 * @see Requirement 9.5 - Reject if email already exists
 * 
 * @example
 * const response = await registerWithEmail('new@example.com', 'password123', 'John');
 * if (!response.success) {
 *   console.error(response.error);
 * }
 */
export async function registerWithEmail(email: string, password: string, name?: string): Promise<LoginResponse> {
  return invoke('register_with_email', { email, password, name });
}

/**
 * Logout current user.
 * 
 * Clears session from both in-memory and SQLite storage.
 * Emits 'auth:logout' and 'session:cleared' events.
 * 
 * @throws Error if logout fails
 * 
 * @example
 * await logout();
 * // Navigate to login screen
 */
export async function logout(): Promise<void> {
  return invoke('logout');
}

/**
 * Get current session information.
 * 
 * @returns SessionInfo with authentication status and user info if authenticated
 * 
 * @example
 * const session = await getSession();
 * if (session.is_authenticated) {
 *   console.log(`Logged in as: ${session.user?.email}`);
 * }
 */
export async function getSession(): Promise<SessionInfo> {
  return invoke('get_session');
}

/**
 * Restore session from SQLite to in-memory storage.
 * 
 * Should be called during app initialization.
 * If session is expired, returns unauthenticated state and clears the stored session.
 * Emits 'session:restored' event if successful.
 * 
 * @returns SessionInfo with authentication status and user info if restored
 * 
 * @see Requirement 9.8 - Store tokens in encrypted SQLite
 * 
 * @example
 * // In app initialization
 * const session = await restoreSession();
 * if (session.is_authenticated) {
 *   startSessionExpirationChecker();
 * }
 */
export async function restoreSession(): Promise<SessionInfo> {
  return invoke('restore_session');
}

/**
 * Check if user is authenticated with a valid (non-expired) session.
 * 
 * @returns true if authenticated, false otherwise
 * 
 * @example
 * if (await isAuthenticated()) {
 *   showMainUI();
 * } else {
 *   showLoginScreen();
 * }
 */
export async function isAuthenticated(): Promise<boolean> {
  return invoke('is_authenticated');
}

/**
 * Start background task that checks session expiration.
 * 
 * Emits Tauri events when:
 * - Session is about to expire (24 hours before): 'session:expiring_soon'
 * - Session has expired: 'session:expired'
 * 
 * @param checkIntervalSecs - How often to check (default: 60 seconds)
 * 
 * @see Requirement 9.10 - Request re-authentication when token expires
 * 
 * @example
 * await startSessionExpirationChecker(30); // Check every 30 seconds
 */
export async function startSessionExpirationChecker(checkIntervalSecs?: number): Promise<void> {
  return invoke('start_session_expiration_checker', { checkIntervalSecs });
}

/**
 * Stop the session expiration checker background task.
 * 
 * @example
 * await stopSessionExpirationChecker();
 */
export async function stopSessionExpirationChecker(): Promise<void> {
  return invoke('stop_session_expiration_checker');
}

// ============================================================================
// BYOK COMMANDS (Bring Your Own Key)
// ============================================================================

/**
 * Store a BYOK API key in the OS keyring.
 * 
 * The key is stored securely in Windows Credential Manager or macOS Keychain.
 * 
 * @param apiKey - Gemini API key to store
 * @throws Error if keyring access fails
 * 
 * @see Requirement 8.3 - Store API key in OS Keyring
 * 
 * @example
 * await setByokKey('AIzaSy...');
 */
export async function setByokKey(apiKey: string): Promise<void> {
  return invoke('set_byok_key', { apiKey });
}

/**
 * Check if a BYOK API key exists in the keyring.
 * 
 * @returns true if a key is stored, false otherwise
 * 
 * @example
 * if (await getByokKeyExists()) {
 *   console.log('BYOK mode available');
 * }
 */
export async function getByokKeyExists(): Promise<boolean> {
  return invoke('get_byok_key_exists');
}

/**
 * Delete the BYOK API key from the keyring.
 * 
 * @throws Error if keyring access fails
 * 
 * @see Requirement 8.7 - Allow user to modify or delete API key
 * 
 * @example
 * await deleteByokKey();
 * console.log('BYOK key removed');
 */
export async function deleteByokKey(): Promise<void> {
  return invoke('delete_byok_key');
}

/**
 * Validate BYOK API key format only.
 * 
 * Checks if the key has valid format (1-256 alphanumeric chars including - and _).
 * Does NOT verify if the key actually works with Gemini API.
 * Use validateByokKeyFull for complete validation.
 * 
 * @param apiKey - The API key to validate
 * @returns true if format is valid, false otherwise
 * 
 * @see Requirement 8.2 - Validate key format before storage
 * 
 * @example
 * if (await validateByokKey('AIzaSy...')) {
 *   // Format is valid, proceed with full validation
 * }
 */
export async function validateByokKey(apiKey: string): Promise<boolean> {
  return invoke('validate_byok_key', { apiKey });
}

/**
 * Validate BYOK API key completely (format + Gemini API test).
 * 
 * This function:
 * 1. Validates the API key format (1-256 alphanumeric chars including - and _)
 * 2. Tests the key against Gemini's API to verify it works
 * 
 * @param apiKey - The API key to validate
 * @returns ValidationResult with:
 *   - valid: boolean - Whether the key is valid
 *   - error_message: string | null - Error description if invalid
 *   - suggestion: string | null - How to fix the issue if invalid
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
export async function validateByokKeyFull(apiKey: string): Promise<ValidationResult> {
  return invoke('validate_byok_key_full', { apiKey });
}

// ============================================================================
// CONFIG COMMANDS
// ============================================================================

/**
 * Get the current application configuration.
 * 
 * @returns AppConfig with languages, devices, and preferences
 * 
 * @see Requirement 25.2 - Load configuration on startup
 * 
 * @example
 * const config = await getConfig();
 * console.log(`Theme: ${config.preferences.theme}`);
 */
export async function getConfig(): Promise<AppConfig> {
  return invoke('get_config');
}

/**
 * Save the application configuration.
 * 
 * Persists configuration to encrypted SQLite storage.
 * 
 * @param config - Configuration to save
 * @throws Error if save fails
 * 
 * @see Requirement 25.1 - Save configuration
 * 
 * @example
 * await saveConfig({
 *   ...currentConfig,
 *   preferences: { ...currentConfig.preferences, theme: 'light' }
 * });
 */
export async function saveConfig(config: AppConfig): Promise<void> {
  return invoke('save_config', { config });
}

/**
 * Export configuration to a JSON file.
 * 
 * Saves configuration to the specified file path.
 * 
 * @param path - File path to export to
 * @throws Error if export fails
 * 
 * @example
 * await exportConfig('C:/Users/user/Documents/traductor-config.json');
 */
export async function exportConfig(path: string): Promise<void> {
  return invoke('export_config', { path });
}

/**
 * Import configuration from a JSON file.
 * 
 * Loads and validates configuration from the specified file.
 * 
 * @param path - File path to import from
 * @returns Imported AppConfig
 * @throws Error if import fails or file is invalid
 * 
 * @example
 * const config = await importConfig('C:/Users/user/Documents/traductor-config.json');
 * await saveConfig(config);
 */
export async function importConfig(path: string): Promise<AppConfig> {
  return invoke('import_config', { path });
}

/**
 * Export configuration as a JSON string.
 * 
 * Useful for sharing configuration without file system access.
 * 
 * @returns JSON string representation of the configuration
 * 
 * @example
 * const configJson = await exportConfigString();
 * navigator.clipboard.writeText(configJson);
 */
export async function exportConfigString(): Promise<string> {
  return invoke('export_config_string');
}

/**
 * Import configuration from a JSON string.
 * 
 * Parses and validates configuration from a JSON string.
 * 
 * @param configJson - JSON string to import
 * @returns Parsed AppConfig
 * @throws Error if JSON is invalid or configuration is malformed
 * 
 * @example
 * const json = await navigator.clipboard.readText();
 * const config = await importConfigString(json);
 * await saveConfig(config);
 */
export async function importConfigString(configJson: string): Promise<AppConfig> {
  return invoke('import_config_string', { configJson });
}

/**
 * Check if a configuration file exists.
 * 
 * Useful for determining whether to start the onboarding wizard.
 * 
 * @returns true if configuration exists, false otherwise
 * 
 * @see Requirement 13.1 - Start wizard if no configuration saved
 * 
 * @example
 * if (!(await configExists())) {
 *   showOnboardingWizard();
 * }
 */
export async function configExists(): Promise<boolean> {
  return invoke('config_exists');
}

/**
 * Reset configuration to default values.
 * 
 * Clears all user settings and returns to factory defaults.
 * 
 * @throws Error if reset fails
 * 
 * @example
 * await resetConfig();
 * showOnboardingWizard();
 */
export async function resetConfig(): Promise<void> {
  return invoke('reset_config');
}

// ============================================================================
// USAGE COMMANDS
// ============================================================================

/**
 * Get current month's usage statistics.
 * 
 * Returns minutes used, limit, and percentage for the current billing period.
 * 
 * @returns UsageStats with usage data and plan info
 * 
 * @see Requirement 11.4 - Show dashboard with usage vs limit
 * 
 * @example
 * const stats = await getUsageStats();
 * console.log(`Used: ${stats.currentMonth.used}/${stats.currentMonth.limit} min`);
 */
export async function getUsageStats(): Promise<UsageStats> {
  return invoke('get_usage_stats');
}

/**
 * Get usage history for the past N days.
 * 
 * Returns daily breakdown of system and user channel usage.
 * 
 * @param days - Number of days to retrieve (e.g., 30)
 * @returns Array of DailyUsage records
 * 
 * @see Requirement 11.4 - Show graph of daily usage for last 30 days
 * 
 * @example
 * const history = await getUsageHistory(30);
 * history.forEach(day => {
 *   console.log(`${day.date}: ${day.totalMinutes} min`);
 * });
 */
export async function getUsageHistory(days: number): Promise<DailyUsage[]> {
  return invoke('get_usage_history', { days });
}

/**
 * Check if translation can start based on current usage.
 * 
 * Returns false if user has reached their monthly limit and translation is blocked.
 * BYOK users always return true (no limits).
 * 
 * @returns true if translation is allowed, false if blocked
 * 
 * @see Requirement 10.7 - Block translation at 100%
 * 
 * @example
 * if (!(await canStartTranslation())) {
 *   showUpgradeModal();
 *   return;
 * }
 * await startSystemChannel(config, token);
 */
export async function canStartTranslation(): Promise<boolean> {
  return invoke('can_start_translation');
}

/**
 * Get available upgrade options for the current plan.
 * 
 * Returns a list of plans with higher limits than the current plan.
 * Useful for showing upgrade modal when user reaches their limit.
 * 
 * @returns Array of upgrade options
 * 
 * @see Requirement 10.7 - Show upgrade options when limit reached
 * 
 * @example
 * const options = await getUpgradeOptions();
 * const recommended = options.find(opt => opt.isRecommended);
 */
export async function getUpgradeOptions(): Promise<UpgradeOption[]> {
  return invoke('get_upgrade_options');
}

/**
 * Check usage limits and emit notifications if thresholds are crossed.
 * 
 * Should be called after each translation session ends.
 * Emits 'usage-warning' at 80% and 'usage-limit-reached' at 100%.
 * 
 * @see Requirement 10.6 - Notify at 80% usage
 * @see Requirement 10.7 - Block translation at 100%
 * @see Requirement 11.8 - Show notification at 100%
 * 
 * @example
 * await stopChannel('system');
 * await checkUsageLimits(); // Will emit events if thresholds crossed
 */
export async function checkUsageLimits(): Promise<void> {
  return invoke('check_usage_limits');
}

/**
 * Emit a blocked notification when translation is attempted but limit reached.
 * 
 * Emits the 'usage-blocked' event with upgrade options.
 * 
 * @see Requirement 10.7 - Block translation when limit reached
 * 
 * @example
 * if (!(await canStartTranslation())) {
 *   await emitUsageBlocked();
 *   return;
 * }
 */
export async function emitUsageBlocked(): Promise<void> {
  return invoke('emit_usage_blocked');
}

/**
 * Set the current user's subscription plan.
 * 
 * Should be called when user logs in or changes plan.
 * 
 * @param plan - Plan identifier: 'byok_free', 'starter', or 'pro'
 * 
 * @example
 * await setUserPlan('starter');
 */
export async function setUserPlan(plan: SubscriptionPlan): Promise<void> {
  return invoke('set_user_plan', { plan });
}

/**
 * Reset usage notification flags (for testing or admin purposes).
 * 
 * Resets the "warning sent" and "limit sent" flags so notifications
 * can be sent again in the current month.
 * 
 * @example
 * // For testing usage notifications
 * await resetUsageNotifications();
 */
export async function resetUsageNotifications(): Promise<void> {
  return invoke('reset_usage_notifications');
}


// ============================================================================
// AUDIO TEST COMMANDS
// ============================================================================

/**
 * Play a test tone on a specific audio device.
 * 
 * Plays a test tone (default: 440Hz for 3 seconds at 50% volume) on the specified
 * audio output device. Used during onboarding to verify that the user can hear
 * audio from each device.
 * 
 * @param deviceId - The ID of the audio device to test
 * @param deviceName - The friendly name of the device (for logging and result)
 * @param config - Optional test tone configuration (frequency, duration, volume)
 * @returns AudioTestResult indicating success or failure with details
 * @throws Error if something went wrong with playback
 * 
 * @see Requirement 13.8 - Play 3-second test tone for each device
 * @see Requirement 13.9 - Allow selecting alternative device if test fails
 * 
 * @example
 * const result = await playAudioTest('device-123', 'Auriculares', { durationMs: 3000 });
 * if (result.success) {
 *   console.log('User confirmed they heard the tone');
 * }
 */
export async function playAudioTest(
  deviceId: string,
  deviceName: string,
  config?: TestToneConfig
): Promise<AudioTestResult> {
  return invoke('play_audio_test', { deviceId, deviceName, config });
}

/**
 * Stop the currently playing audio test.
 * 
 * Cancels any audio test that is currently in progress.
 * The test will be marked as failed with a "cancelled" message.
 * 
 * @throws Error if no test is currently playing
 * 
 * @see Requirement 13.9 - Allow selecting alternative device if test fails
 * 
 * @example
 * await stopAudioTest();
 */
export async function stopAudioTest(): Promise<void> {
  return invoke('stop_audio_test');
}

/**
 * Check if an audio test is currently playing.
 * 
 * @returns true if a test is currently playing, false otherwise
 * 
 * @example
 * if (await isAudioTestPlaying()) {
 *   await stopAudioTest();
 * }
 */
export async function isAudioTestPlaying(): Promise<boolean> {
  return invoke('is_audio_test_playing');
}

// ============================================================================
// SYSTEM TRAY COMMANDS
// ============================================================================

import type { TrayState } from './types';

/**
 * Get the current system tray state.
 * 
 * Returns the current state of the tray icon:
 * - 'inactive': No translation in progress (gray icon)
 * - 'active': Translation is running (green icon)
 * - 'error': An error has occurred (red icon)
 * 
 * @returns Current TrayState
 * 
 * @see Requirement 16.2 - Tray icon color indicates state
 * 
 * @example
 * const state = await getTrayState();
 * if (state === 'error') {
 *   showErrorNotification();
 * }
 */
export async function getTrayState(): Promise<TrayState> {
  return invoke('get_tray_state');
}

/**
 * Set the system tray state and update the icon.
 * 
 * Changes the tray icon color based on the application state:
 * - 'inactive': Gray icon
 * - 'active': Green icon
 * - 'error': Red icon
 * 
 * @param state - The new tray state to set
 * @throws Error if tray icon update fails
 * 
 * @see Requirement 16.2 - Change icon color/shape to indicate state
 * 
 * @example
 * // When translation starts
 * await setTrayState('active');
 * 
 * // When an error occurs
 * await setTrayState('error');
 * 
 * // When translation stops
 * await setTrayState('inactive');
 */
export async function setTrayState(state: TrayState): Promise<void> {
  return invoke('set_tray_state', { state });
}

/**
 * Check if translation is paused via the tray.
 * 
 * @returns true if translation is paused, false otherwise
 * 
 * @example
 * if (await isTrayPaused()) {
 *   console.log('Translation is paused');
 * }
 */
export async function isTrayPaused(): Promise<boolean> {
  return invoke('is_tray_paused');
}

/**
 * Toggle the pause state via the tray.
 * 
 * Toggles between paused and resumed state for translation.
 * 
 * @returns The new pause state (true = paused, false = resumed)
 * 
 * @see Requirement 16.3 - Menu with Pause/Resume option
 * 
 * @example
 * const isPaused = await toggleTrayPause();
 * console.log(isPaused ? 'Paused' : 'Resumed');
 */
export async function toggleTrayPause(): Promise<boolean> {
  return invoke('toggle_tray_pause');
}

import type { TrayConfig } from './types';

/**
 * Get the current tray configuration.
 * 
 * Returns the configuration for tray behavior including:
 * - minimizeToTray: Whether to minimize to tray when closing window
 * - startMinimized: Whether to start the application minimized to tray
 * 
 * @returns Current TrayConfig
 * 
 * @see Requirement 16.4 - Minimize to tray when closing window
 * @see Requirement 16.5 - Option to start minimized
 * 
 * @example
 * const config = await getTrayConfig();
 * if (config.minimizeToTray) {
 *   console.log('App will minimize to tray on close');
 * }
 */
export async function getTrayConfig(): Promise<TrayConfig> {
  return invoke('get_tray_config');
}

/**
 * Set the tray configuration.
 * 
 * Updates the tray behavior settings:
 * - minimizeToTray: Whether to minimize to tray when closing window
 * - startMinimized: Whether to start the application minimized to tray
 * 
 * @param config - The new tray configuration
 * 
 * @see Requirement 16.4 - Minimize to tray when closing window
 * @see Requirement 16.5 - Option to start minimized
 * 
 * @example
 * // Enable minimize to tray and start minimized
 * await setTrayConfig({
 *   minimizeToTray: true,
 *   startMinimized: true
 * });
 */
export async function setTrayConfig(config: TrayConfig): Promise<void> {
  return invoke('set_tray_config', { config });
}

/**
 * Show the main window from the system tray.
 * 
 * Shows the window, unminimizes it if minimized, and brings it to focus.
 * 
 * @see Requirement 16.3 - Show window action from tray menu
 * 
 * @example
 * // When user clicks "Show Window" in tray menu
 * await showWindowFromTray();
 */
export async function showWindowFromTray(): Promise<void> {
  return invoke('show_window_from_tray');
}

/**
 * Hide the main window to the system tray.
 * 
 * Hides the window without closing the application.
 * The app continues running in the background.
 * 
 * @see Requirement 16.4 - Minimize to tray
 * 
 * @example
 * // Hide window programmatically
 * await hideWindowToTray();
 */
export async function hideWindowToTray(): Promise<void> {
  return invoke('hide_window_to_tray');
}

// ============================================================================
// UPDATER COMMANDS
// ============================================================================

import type { UpdateCheckResult, UpdateInfo } from './types';

/**
 * Manually check for application updates.
 * 
 * Checks the update server for a newer version. If an update is available,
 * returns the update information including version and changelog.
 * 
 * This command can be called by the user to manually trigger an update check.
 * The app also checks automatically on startup and every 24 hours.
 * 
 * @returns UpdateCheckResult with update availability and info
 * 
 * @see Requirement 17.1 - Check for updates on startup and every 24 hours
 * @see Requirement 17.2 - Show notification with changelog when update available
 * 
 * @example
 * const result = await checkForUpdates();
 * if (result.updateAvailable && result.updateInfo) {
 *   showUpdateNotification(result.updateInfo);
 * }
 */
export async function checkForUpdates(): Promise<UpdateCheckResult> {
  return invoke('check_for_updates_command');
}

/**
 * Get the last known update information.
 * 
 * Returns the update info from the last check if an update was found.
 * Returns null if no update is available or no check has been performed.
 * 
 * @returns UpdateInfo or null if no update available
 * 
 * @example
 * const info = await getUpdateInfo();
 * if (info) {
 *   console.log(`Update available: ${info.version}`);
 * }
 */
export async function getUpdateInfo(): Promise<UpdateInfo | null> {
  return invoke('get_update_info');
}

/**
 * Check if an update is currently being downloaded.
 * 
 * @returns true if download in progress, false otherwise
 * 
 * @see Requirement 17.3 - Download updates in background
 * 
 * @example
 * if (await isUpdateDownloading()) {
 *   showDownloadProgress();
 * }
 */
export async function isUpdateDownloading(): Promise<boolean> {
  return invoke('is_update_downloading');
}

/**
 * Get the current application version.
 * 
 * @returns Current version string (e.g., "1.0.0")
 * 
 * @example
 * const version = await getCurrentVersion();
 * console.log(`Running version: ${version}`);
 */
export async function getCurrentVersion(): Promise<string> {
  return invoke('get_current_version');
}

/**
 * Start the periodic update checker.
 * 
 * Initializes the auto-updater background task that:
 * 1. Checks for updates immediately on startup
 * 2. Checks for updates every 24 hours
 * 3. Emits 'update-available' event when an update is found
 * 
 * This is typically called during app initialization.
 * 
 * @see Requirement 17.1 - Check for updates on startup and every 24 hours
 * 
 * @example
 * // In app initialization
 * await startUpdateChecker();
 */
export async function startUpdateChecker(): Promise<void> {
  return invoke('start_update_checker');
}

/**
 * Stop the periodic update checker.
 * 
 * Stops the background task that checks for updates periodically.
 * This is typically called when the app is shutting down.
 * 
 * @example
 * // Before app shutdown
 * await stopUpdateChecker();
 */
export async function stopUpdateChecker(): Promise<void> {
  return invoke('stop_update_checker');
}

