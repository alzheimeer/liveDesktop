//! Audio control commands
//!
//! Handles audio device enumeration, channel start/stop, device changes,
//! and audio test tone playback for onboarding.
//! Uses Tauri's state management to maintain a singleton AudioEngine instance.

use std::sync::Arc;
use tauri::{command, State};
use tokio::sync::RwLock;
use crate::audio::engine::{
    AudioDevice, AudioEngine, ChannelConfig, ChannelType, EngineState,
};
use crate::audio::test_tone::{AudioTestResult, AudioTestState, TestToneConfig};

/// Wrapper for AudioEngine state that can be shared across Tauri commands
pub struct AudioEngineState(pub Arc<RwLock<AudioEngine>>);

impl AudioEngineState {
    /// Create a new AudioEngineState with optional metrics sender
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(AudioEngine::new(None))))
    }
}

impl Default for AudioEngineState {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper for AudioTestState that can be shared across Tauri commands
pub struct AudioTestStateWrapper(pub Arc<AudioTestState>);

impl AudioTestStateWrapper {
    pub fn new() -> Self {
        Self(Arc::new(AudioTestState::new()))
    }
}

impl Default for AudioTestStateWrapper {
    fn default() -> Self {
        Self::new()
    }
}

/// VB-Cable status information for frontend
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VBCableStatusInfo {
    /// Whether VB-Cable is installed (either input or output detected)
    pub is_installed: bool,
    /// Whether CABLE Input (virtual microphone) is available
    pub input_available: bool,
    /// Whether CABLE Output (virtual speaker) is available  
    pub output_available: bool,
    /// Device ID of CABLE Output if found (for routing translated audio)
    pub output_device_id: Option<String>,
    /// Friendly name of CABLE Output if found
    pub output_device_name: Option<String>,
}

/// Enumerate all available audio devices
///
/// Returns a list of available audio devices for capture and playback.
/// On Windows, uses WASAPI to enumerate output devices for loopback capture.
/// The operation completes within 2 seconds (Requirement 2.2).
///
/// # Errors
///
/// Returns a user-friendly error message in Spanish if:
/// - WASAPI is not available (Requirement 2.5)
/// - No audio output devices are available (Requirement 2.6)
#[command]
pub async fn enumerate_audio_devices(
    engine_state: State<'_, AudioEngineState>,
) -> Result<Vec<AudioDevice>, String> {
    let engine = engine_state.0.read().await;
    engine.enumerate_devices().await
}

/// Start system channel (meeting -> user)
///
/// Starts capture of meeting audio, sends to Gemini for translation,
/// and plays the translated audio to the user's output device.
///
/// # Arguments
/// * `config` - Channel configuration with languages and devices
/// * `token` - Gemini API token (ephemeral or BYOK)
///
/// # Requirements
/// - Requirement 6.5: Support simultaneous Gemini sessions
#[command]
pub async fn start_system_channel(
    engine_state: State<'_, AudioEngineState>,
    config: ChannelConfig,
    token: String,
) -> Result<(), String> {
    let engine = engine_state.0.read().await;
    engine.start_system_channel(config, &token).await
}

/// Start user channel (user -> meeting)
///
/// Starts capture of user's microphone, sends to Gemini for translation,
/// and routes the translated audio to VB-Cable for meeting injection.
///
/// # Arguments
/// * `config` - Channel configuration with languages and devices
/// * `token` - Gemini API token (ephemeral or BYOK)
///
/// # Requirements
/// - Requirement 6.5: Support simultaneous Gemini sessions
/// - Requirement 4.5: Route audio to VB-Cable Output
#[command]
pub async fn start_user_channel(
    engine_state: State<'_, AudioEngineState>,
    config: ChannelConfig,
    token: String,
) -> Result<(), String> {
    let engine = engine_state.0.read().await;
    engine.start_user_channel(config, &token).await
}

/// Stop a specific channel
///
/// Stops capture, closes Gemini connection, and releases resources
/// for the specified channel.
///
/// # Arguments
/// * `channel` - "system" or "user"
#[command]
pub async fn stop_channel(
    engine_state: State<'_, AudioEngineState>,
    channel: String,
) -> Result<(), String> {
    let channel_type = match channel.to_lowercase().as_str() {
        "system" => ChannelType::System,
        "user" => ChannelType::User,
        _ => return Err(format!("Canal inválido: '{}'. Use 'system' o 'user'.", channel)),
    };
    
    let engine = engine_state.0.read().await;
    engine.stop_channel(channel_type).await
}

/// Change audio device for a channel without restarting the session
///
/// Performs hot-swap of audio devices while maintaining the Gemini connection.
///
/// # Arguments
/// * `channel` - "system" or "user"
/// * `device_id` - New device ID to use
/// * `is_input` - True for input device, false for output device
///
/// # Requirements
/// - Requirement 14.4: Apply device changes without restarting session
#[command]
pub async fn change_audio_device(
    engine_state: State<'_, AudioEngineState>,
    channel: String,
    device_id: String,
    is_input: Option<bool>,
) -> Result<(), String> {
    let channel_type = match channel.to_lowercase().as_str() {
        "system" => ChannelType::System,
        "user" => ChannelType::User,
        _ => return Err(format!("Canal inválido: '{}'. Use 'system' o 'user'.", channel)),
    };
    
    let engine = engine_state.0.read().await;
    engine.change_device(channel_type, &device_id, is_input.unwrap_or(true)).await
}

/// Get current audio engine state
///
/// Returns the current state of both audio channels and any pause reasons.
#[command]
pub async fn get_audio_state(
    engine_state: State<'_, AudioEngineState>,
) -> Result<EngineState, String> {
    let engine = engine_state.0.read().await;
    Ok(engine.get_state().await)
}

/// Get VB-Cable installation status (Windows only)
///
/// Returns the cached VB-Cable detection result that was performed at app startup.
/// On non-Windows platforms, returns a status indicating VB-Cable is not applicable.
///
/// # Requirements
///
/// - Requirement 4.1: Detect if VB-Cable is installed at app startup
///   and register the result in internal system state
///
/// # Returns
///
/// - `Ok(VBCableStatusInfo)` - The VB-Cable status information
/// - `Err(String)` - Error message if detection hasn't been performed
#[command]
pub fn get_vbcable_status() -> Result<VBCableStatusInfo, String> {
    #[cfg(target_os = "windows")]
    {
        use crate::audio::windows::vbcable;
        
        // Check if detection has been performed
        if let Some(status) = vbcable::get_cached_status() {
            Ok(VBCableStatusInfo {
                is_installed: status.is_installed,
                input_available: status.input_available,
                output_available: status.output_available,
                output_device_id: status.output_device_id.clone(),
                output_device_name: status.output_device_name.clone(),
            })
        } else {
            // Detection hasn't been performed yet, try to detect now
            match vbcable::detect_and_register() {
                Ok(status) => Ok(VBCableStatusInfo {
                    is_installed: status.is_installed,
                    input_available: status.input_available,
                    output_available: status.output_available,
                    output_device_id: status.output_device_id.clone(),
                    output_device_name: status.output_device_name.clone(),
                }),
                Err(e) => Err(format!(
                    "Error al detectar VB-Cable: {}. Verifica que Windows Audio Service esté ejecutándose.",
                    e
                )),
            }
        }
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        // VB-Cable is Windows-only, return N/A status for other platforms
        Ok(VBCableStatusInfo {
            is_installed: false,
            input_available: false,
            output_available: false,
            output_device_id: None,
            output_device_name: None,
        })
    }
}

/// Virtual audio status information for frontend (macOS)
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualAudioStatusInfo {
    /// Type of virtual audio available
    /// - "native" - macOS 14+ native virtual audio endpoint
    /// - "blackhole" - BlackHole driver installed (fallback for macOS <14)
    /// - "not_available" - No virtual audio available
    /// - "requires_blackhole" - macOS <14 and BlackHole not installed
    pub status_type: String,
    /// Whether virtual audio is available and ready
    pub is_available: bool,
    /// macOS version string (e.g., "14.0.0")
    pub macos_version: Option<String>,
    /// Whether this is native macOS 14+ virtual audio
    pub is_native: bool,
    /// BlackHole device ID if using fallback
    pub blackhole_device_id: Option<String>,
    /// BlackHole device name if using fallback
    pub blackhole_device_name: Option<String>,
    /// Installation instructions if BlackHole is required but not installed
    pub installation_instructions: Option<VirtualAudioInstructions>,
}

/// Installation instructions for virtual audio drivers
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualAudioInstructions {
    /// Title of the instructions
    pub title: String,
    /// Description of what the driver does
    pub description: String,
    /// Step-by-step installation instructions
    pub steps: Vec<String>,
    /// Download URL for the driver
    pub download_url: String,
    /// Homebrew command for installation (optional)
    pub homebrew_command: Option<String>,
}

/// Get virtual audio status (macOS only)
///
/// Checks the status of virtual audio capabilities on macOS:
/// - macOS 14+ (Sonoma): Native Virtual Audio Endpoint available
/// - macOS <14: Checks if BlackHole is installed, provides instructions if not
///
/// # Requirements
///
/// - Requirement 5.1: Create Virtual_Audio_Endpoint using AudioDriverKit/CoreAudio (macOS 14+)
/// - Requirement 5.7: Show BlackHole installation instructions if Virtual_Audio_Endpoint fails
/// - Requirement 13.7: Verify Virtual_Audio_Endpoint on macOS, configure automatically or show instructions
///
/// # Returns
///
/// - `Ok(VirtualAudioStatusInfo)` - The virtual audio status information
/// - `Err(String)` - Error message if detection fails
#[command]
pub fn get_virtual_audio_status() -> Result<VirtualAudioStatusInfo, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::audio::macos::virtual_audio::{VirtualAudioEndpoint, VirtualAudioStatus, BlackHoleInstructions};
        
        let status = VirtualAudioEndpoint::check_status();
        
        match status {
            VirtualAudioStatus::NativeAvailable { macos_version } => {
                Ok(VirtualAudioStatusInfo {
                    status_type: "native".to_string(),
                    is_available: true,
                    macos_version: Some(macos_version),
                    is_native: true,
                    blackhole_device_id: None,
                    blackhole_device_name: None,
                    installation_instructions: None,
                })
            }
            VirtualAudioStatus::BlackHoleAvailable { device_id, device_name } => {
                Ok(VirtualAudioStatusInfo {
                    status_type: "blackhole".to_string(),
                    is_available: true,
                    macos_version: None,
                    is_native: false,
                    blackhole_device_id: Some(device_id),
                    blackhole_device_name: Some(device_name),
                    installation_instructions: None,
                })
            }
            VirtualAudioStatus::RequiresBlackHole { macos_version, instructions } => {
                Ok(VirtualAudioStatusInfo {
                    status_type: "requires_blackhole".to_string(),
                    is_available: false,
                    macos_version: Some(macos_version),
                    is_native: false,
                    blackhole_device_id: None,
                    blackhole_device_name: None,
                    installation_instructions: Some(convert_instructions(instructions)),
                })
            }
            VirtualAudioStatus::NotAvailable { reason, instructions } => {
                Ok(VirtualAudioStatusInfo {
                    status_type: "not_available".to_string(),
                    is_available: false,
                    macos_version: None,
                    is_native: false,
                    blackhole_device_id: None,
                    blackhole_device_name: None,
                    installation_instructions: instructions.map(convert_instructions),
                })
            }
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        // Virtual Audio Endpoint is macOS-only, return N/A status for other platforms
        Ok(VirtualAudioStatusInfo {
            status_type: "not_applicable".to_string(),
            is_available: false,
            macos_version: None,
            is_native: false,
            blackhole_device_id: None,
            blackhole_device_name: None,
            installation_instructions: None,
        })
    }
}

#[cfg(target_os = "macos")]
fn convert_instructions(instructions: crate::audio::macos::virtual_audio::BlackHoleInstructions) -> VirtualAudioInstructions {
    VirtualAudioInstructions {
        title: instructions.title,
        description: instructions.description,
        steps: instructions.steps,
        download_url: instructions.download_url,
        homebrew_command: instructions.homebrew_command,
    }
}

// ============================================================================
// AUDIO TEST COMMANDS
// ============================================================================

/// Test tone configuration from frontend
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestToneConfigInput {
    /// Frequency of the test tone in Hz (default: 440 Hz)
    #[serde(default = "default_frequency")]
    pub frequency_hz: f32,
    /// Duration of the test tone in milliseconds (default: 3000ms)
    #[serde(default = "default_duration")]
    pub duration_ms: u32,
    /// Volume level from 0.0 to 1.0 (default: 0.5)
    #[serde(default = "default_volume")]
    pub volume: f32,
}

fn default_frequency() -> f32 { 440.0 }
fn default_duration() -> u32 { 3000 }
fn default_volume() -> f32 { 0.5 }

impl Default for TestToneConfigInput {
    fn default() -> Self {
        Self {
            frequency_hz: default_frequency(),
            duration_ms: default_duration(),
            volume: default_volume(),
        }
    }
}

impl From<TestToneConfigInput> for TestToneConfig {
    fn from(input: TestToneConfigInput) -> Self {
        Self {
            frequency_hz: input.frequency_hz,
            duration_ms: input.duration_ms,
            sample_rate: 24000, // Fixed at 24kHz for consistency
            volume: input.volume.clamp(0.0, 1.0),
        }
    }
}

/// Play a test tone on a specific audio device
///
/// Plays a 3-second test tone (default) on the specified audio output device.
/// Used during onboarding to verify that the user can hear audio from each device.
///
/// # Arguments
/// * `device_id` - The ID of the audio device to test
/// * `device_name` - The friendly name of the device (for logging and result)
/// * `config` - Optional test tone configuration (frequency, duration, volume)
///
/// # Requirements
///
/// - Requirement 13.8: Play 3-second test tone for each device
/// - Requirement 13.9: Allow selecting alternative device if test fails
///
/// # Returns
///
/// - `Ok(AudioTestResult)` - Result of the test (success or failure with details)
/// - `Err(String)` - Error message if something went wrong
#[command]
pub async fn play_audio_test(
    test_state: State<'_, AudioTestStateWrapper>,
    device_id: String,
    device_name: String,
    config: Option<TestToneConfigInput>,
) -> Result<AudioTestResult, String> {
    let tone_config: TestToneConfig = config.unwrap_or_default().into();
    let state = test_state.0.clone();
    
    tracing::info!(
        "Starting audio test on '{}' ({}), duration: {}ms",
        device_name,
        device_id,
        tone_config.duration_ms
    );
    
    #[cfg(target_os = "windows")]
    {
        use crate::audio::test_tone::windows_test;
        Ok(windows_test::play_test_tone(&device_id, &device_name, tone_config, state).await)
    }
    
    #[cfg(target_os = "macos")]
    {
        use crate::audio::test_tone::macos_test;
        Ok(macos_test::play_test_tone(&device_id, &device_name, tone_config, state).await)
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err("Prueba de audio no disponible en esta plataforma".to_string())
    }
}

/// Stop the currently playing audio test
///
/// Cancels any audio test that is currently in progress.
/// The test will be marked as failed with a "cancelled" message.
///
/// # Requirements
///
/// - Requirement 13.9: Allow selecting alternative device if test fails
#[command]
pub fn stop_audio_test(
    test_state: State<'_, AudioTestStateWrapper>,
) -> Result<(), String> {
    let state = test_state.0.clone();
    
    if state.is_playing() {
        tracing::info!("Stopping audio test by user request");
        state.stop();
        Ok(())
    } else {
        Err("No hay ninguna prueba de audio en progreso".to_string())
    }
}

/// Check if an audio test is currently playing
///
/// # Returns
///
/// - `true` if a test is currently playing
/// - `false` otherwise
#[command]
pub fn is_audio_test_playing(
    test_state: State<'_, AudioTestStateWrapper>,
) -> bool {
    test_state.0.is_playing()
}
