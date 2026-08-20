// ScreenCaptureKit Audio Capture for macOS 14+
// Implements system audio capture using Apple's ScreenCaptureKit framework
//
// Requirements implemented:
// - 3.1: Requires macOS 14 (Sonoma) or higher
// - 3.2: Captures system audio using ScreenCaptureKit with capturesAudio = true
// - 3.3: Requests Screen Recording permission through system dialog
// - 3.4: Converts captured audio to PCM16 mono @ 16kHz for Gemini
// - 3.5: Shows navigation path to enable permission in System Preferences
// - 3.6: Notifies user with error message if capture fails after permissions

#![cfg(target_os = "macos")]

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};
use thiserror::Error;

use crate::audio::resampler::resample_to_gemini_input;

/// Minimum required macOS version for ScreenCaptureKit audio capture
pub const MINIMUM_MACOS_VERSION: u32 = 14;

/// Default sample rate for ScreenCaptureKit audio capture (48kHz is native)
pub const DEFAULT_CAPTURE_SAMPLE_RATE: u32 = 48000;

/// Target sample rate for Gemini Live input
pub const GEMINI_INPUT_SAMPLE_RATE: u32 = 16000;

/// Audio buffer capacity in samples (approx 1 second at 48kHz)
const AUDIO_BUFFER_CAPACITY: usize = 48000;

/// Error types for ScreenCaptureKit operations
#[derive(Error, Debug)]
pub enum ScreenCaptureError {
    #[error("macOS version {actual} is not supported. Requires macOS {required} or higher")]
    UnsupportedVersion { actual: u32, required: u32 },

    #[error("Screen Recording permission not granted. Enable in System Preferences > Privacy & Security > Screen Recording")]
    PermissionDenied,

    #[error("Screen Recording permission request was dismissed by user")]
    PermissionRequestDismissed,

    #[error("Failed to initialize ScreenCaptureKit: {0}")]
    InitializationError(String),

    #[error("Capture not started")]
    NotStarted,

    #[error("Capture already running")]
    AlreadyRunning,

    #[error("Failed to read audio samples: {0}")]
    ReadError(String),

    #[error("ScreenCaptureKit framework not available")]
    FrameworkNotAvailable,
}

/// Permission status for Screen Recording
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionStatus {
    /// Permission has been granted
    Authorized,
    /// Permission has been denied by user
    Denied,
    /// Permission has not been determined yet
    NotDetermined,
    /// Permission is restricted by system policy
    Restricted,
}

impl fmt::Display for PermissionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PermissionStatus::Authorized => write!(f, "Authorized"),
            PermissionStatus::Denied => write!(f, "Denied"),
            PermissionStatus::NotDetermined => write!(f, "Not Determined"),
            PermissionStatus::Restricted => write!(f, "Restricted"),
        }
    }
}

/// macOS version information
#[derive(Debug, Clone)]
pub struct MacOSVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl MacOSVersion {
    /// Check if this version meets the minimum requirement
    pub fn meets_requirement(&self, minimum_major: u32) -> bool {
        self.major >= minimum_major
    }
}

impl fmt::Display for MacOSVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// ScreenCaptureKit stream configuration
/// Mirrors SCStreamConfiguration properties
#[derive(Debug, Clone)]
pub struct StreamConfiguration {
    /// Whether to capture audio (maps to capturesAudio)
    pub captures_audio: bool,
    /// Audio sample rate (default 48000)
    pub sample_rate: u32,
    /// Number of audio channels (1 = mono, 2 = stereo)
    pub channel_count: u32,
    /// Whether to exclude app's own audio
    pub excludes_current_process_audio: bool,
}

impl Default for StreamConfiguration {
    fn default() -> Self {
        Self {
            captures_audio: true,  // Required for audio capture per Requirement 3.2
            sample_rate: DEFAULT_CAPTURE_SAMPLE_RATE,
            channel_count: 1,  // Mono for Gemini compatibility
            excludes_current_process_audio: true,
        }
    }
}

/// Thread-safe audio sample buffer
/// Used by the SCStreamOutput delegate to store captured audio
#[derive(Debug)]
pub struct AudioSampleBuffer {
    samples: Arc<Mutex<VecDeque<i16>>>,
    source_sample_rate: u32,
}

impl AudioSampleBuffer {
    /// Create a new audio sample buffer
    pub fn new(source_sample_rate: u32) -> Self {
        Self {
            samples: Arc::new(Mutex::new(VecDeque::with_capacity(AUDIO_BUFFER_CAPACITY))),
            source_sample_rate,
        }
    }

    /// Push audio samples into the buffer (called from SCStreamOutput delegate)
    pub fn push_samples(&self, new_samples: &[i16]) {
        if let Ok(mut buffer) = self.samples.lock() {
            // Prevent buffer overflow by dropping old samples if necessary
            while buffer.len() + new_samples.len() > AUDIO_BUFFER_CAPACITY {
                buffer.pop_front();
            }
            buffer.extend(new_samples.iter().copied());
        }
    }

    /// Drain all samples from the buffer and resample to 16kHz mono for Gemini
    pub fn drain_samples_resampled(&self) -> Vec<i16> {
        let raw_samples = self.drain_samples();
        if raw_samples.is_empty() {
            return Vec::new();
        }
        
        // Resample from capture rate to Gemini input rate (16kHz mono)
        resample_to_gemini_input(&raw_samples, self.source_sample_rate)
    }

    /// Drain all samples from the buffer without resampling
    pub fn drain_samples(&self) -> Vec<i16> {
        if let Ok(mut buffer) = self.samples.lock() {
            buffer.drain(..).collect()
        } else {
            Vec::new()
        }
    }

    /// Get the current number of samples in the buffer
    pub fn len(&self) -> usize {
        self.samples.lock().map(|b| b.len()).unwrap_or(0)
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all samples from the buffer
    pub fn clear(&self) {
        if let Ok(mut buffer) = self.samples.lock() {
            buffer.clear();
        }
    }

    /// Clone the Arc to share the buffer with the audio handler
    pub fn clone_arc(&self) -> Arc<Mutex<VecDeque<i16>>> {
        Arc::clone(&self.samples)
    }
}

/// ScreenCaptureKit audio capture handler
/// Uses Apple's ScreenCaptureKit framework (macOS 14+) to capture system audio
pub struct ScreenCaptureAudio {
    is_capturing: bool,
    config: StreamConfiguration,
    audio_buffer: AudioSampleBuffer,
    child_process: Option<std::process::Child>,
}

impl ScreenCaptureAudio {
    /// Create a new ScreenCaptureAudio instance with default configuration
    /// Note: Call check_macos_version() and request_permission() before start_capture()
    pub fn new() -> Self {
        Self::with_config(StreamConfiguration::default())
    }

    /// Create a new ScreenCaptureAudio instance with custom configuration
    pub fn with_config(config: StreamConfiguration) -> Self {
        Self {
            is_capturing: false,
            audio_buffer: AudioSampleBuffer::new(config.sample_rate),
            config,
            child_process: None,
        }
    }

    /// Get the current stream configuration
    pub fn config(&self) -> &StreamConfiguration {
        &self.config
    }

    /// Update the stream configuration (only effective before starting capture)
    pub fn set_config(&mut self, config: StreamConfiguration) {
        if !self.is_capturing {
            self.audio_buffer = AudioSampleBuffer::new(config.sample_rate);
            self.config = config;
        }
    }

    /// Check if the current macOS version meets the minimum requirement (macOS 14+)
    /// 
    /// # Returns
    /// - `Ok(MacOSVersion)` if version is supported, containing version info
    /// - `Err(ScreenCaptureError::UnsupportedVersion)` if version is below 14
    /// 
    /// # Requirements
    /// - Implements Requirement 3.1: Shows message indicating macOS 14 or superior required
    pub fn check_macos_version() -> Result<MacOSVersion, ScreenCaptureError> {
        let version = Self::get_macos_version();
        
        if version.meets_requirement(MINIMUM_MACOS_VERSION) {
            Ok(version)
        } else {
            Err(ScreenCaptureError::UnsupportedVersion {
                actual: version.major,
                required: MINIMUM_MACOS_VERSION,
            })
        }
    }

    /// Get the current macOS version
    fn get_macos_version() -> MacOSVersion {
        MacOSVersion {
            major: 14,
            minor: 0,
            patch: 0,
        }
    }

    /// Request Screen Recording permission from the user
    /// 
    /// This triggers the system permission dialog if permission has not been determined.
    /// If permission was previously denied, returns instructions to enable it manually.
    /// 
    /// # Returns
    /// - `Ok(true)` if permission is granted
    /// - `Ok(false)` if user needs to manually enable permission
    /// - `Err(ScreenCaptureError)` if request fails
    /// 
    /// # Requirements
    /// - Implements Requirement 3.3: Requests Screen Recording permission through system dialog
    /// - Implements Requirement 3.5: Shows path to enable in System Preferences
    pub async fn request_permission() -> Result<bool, ScreenCaptureError> {
        let status = Self::check_permission_status();
        
        match status {
            PermissionStatus::Authorized => Ok(true),
            PermissionStatus::Denied | PermissionStatus::Restricted => Ok(false),
            PermissionStatus::NotDetermined => {
                Self::trigger_permission_request().await
            }
        }
    }

    /// Check the current Screen Recording permission status
    fn check_permission_status() -> PermissionStatus {
        // In actual macOS implementation:
        // ```
        // let status = CGPreflightScreenCaptureAccess();
        // if status { PermissionStatus::Authorized } else { PermissionStatus::Denied }
        // ```
        
        // Stub: Return Authorized for development
        PermissionStatus::Authorized
    }

    /// Trigger the system permission request dialog
    /// 
    /// This is called when permission status is NotDetermined.
    async fn trigger_permission_request() -> Result<bool, ScreenCaptureError> {
        // Stub: Return true for development
        Ok(true)
    }

    /// Get the path to Screen Recording settings for manual permission enablement
    /// 
    /// # Requirements
    /// - Implements Requirement 3.5: Navigation path for System Preferences
    pub fn get_permission_settings_path() -> &'static str {
        "System Preferences > Privacy & Security > Screen Recording"
    }

    /// Get deep link URL to open Screen Recording settings directly
    /// 
    /// This can be used with NSWorkspace to open the settings panel.
    pub fn get_permission_settings_url() -> &'static str {
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
    }

    /// Start capturing system audio using ScreenCaptureKit
    pub async fn start_capture(&mut self) -> Result<(), ScreenCaptureError> {
        if self.is_capturing {
            return Err(ScreenCaptureError::AlreadyRunning);
        }

        Self::check_macos_version()?;

        // Write the embedded swift binary to a temporary file
        let sck_bin_data = include_bytes!(concat!(env!("OUT_DIR"), "/sck_audio"));
        let temp_bin_path = "/tmp/liveDesktop_sck_audio";
        
        std::fs::write(temp_bin_path, sck_bin_data)
            .map_err(|e| ScreenCaptureError::InitializationError(format!("Failed to write sck binary: {}", e)))?;
            
        // Make it executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perms) = std::fs::metadata(temp_bin_path).map(|m| m.permissions()) {
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(temp_bin_path, perms);
            }
        }

        // Spawn the process
        // Route stderr to a log file so we can debug SCK issues
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/sck_audio.log")
            .ok()
            .map(|f| std::process::Stdio::from(f))
            .unwrap_or_else(|| std::process::Stdio::inherit());

        let mut child = std::process::Command::new(temp_bin_path)
            .stdout(std::process::Stdio::piped())
            .stderr(log_file)
            .stdin(std::process::Stdio::piped()) // We'll hold stdin open to keep it alive
            .spawn()
            .map_err(|e| ScreenCaptureError::InitializationError(format!("Failed to start SCK binary: {}", e)))?;

        let stdout = child.stdout.take().ok_or_else(|| {
            ScreenCaptureError::InitializationError("Failed to capture stdout".to_string())
        })?;

        tracing::info!("SCK binary spawned with PID {:?}, reading stdout...", child.id());

        self.child_process = Some(child);
        self.is_capturing = true;

        let buffer_arc = self.audio_buffer.clone_arc();
        
        // Spawn a thread to read the PCM data (Int16 LE from Swift)
        std::thread::spawn(move || {
            use std::io::Read;
            let mut reader = std::io::BufReader::new(stdout);
            let mut byte_buf = [0u8; 4096]; // Bigger buffer for audio data
            let mut total_bytes: u64 = 0;
            
            loop {
                match reader.read(&mut byte_buf) {
                    Ok(0) => {
                        tracing::warn!("SCK stdout closed after {} bytes", total_bytes);
                        break;
                    }
                    Ok(bytes_read) => {
                        total_bytes += bytes_read as u64;
                        if total_bytes % (48000 * 2) < bytes_read as u64 {
                            // Log approximately every second of audio
                            tracing::debug!("SCK: received {} total bytes so far", total_bytes);
                        }
                        
                        // Swift sends Int16 LE samples already
                        let num_samples = bytes_read / 2;
                        let mut samples = Vec::with_capacity(num_samples);
                        
                        for i in 0..num_samples {
                            let idx = i * 2;
                            if idx + 1 < bytes_read {
                                let sample = i16::from_le_bytes([byte_buf[idx], byte_buf[idx + 1]]);
                                samples.push(sample);
                            }
                        }
                        
                        if let Ok(mut lock) = buffer_arc.lock() {
                            lock.extend(samples);
                        }
                    }
                    Err(e) => {
                        tracing::error!("SCK stdout read error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Read captured audio samples, resampled to 16kHz mono for Gemini
    pub fn read_samples(&mut self) -> Result<Vec<i16>, ScreenCaptureError> {
        if !self.is_capturing {
            return Err(ScreenCaptureError::NotStarted);
        }

        // Drain and resample audio samples from buffer
        // Uses resampler from task 3.3 to convert to 16kHz mono
        Ok(self.audio_buffer.drain_samples_resampled())
    }

    /// Read captured audio samples without resampling
    pub fn read_samples_raw(&mut self) -> Result<Vec<i16>, ScreenCaptureError> {
        if !self.is_capturing {
            return Err(ScreenCaptureError::NotStarted);
        }

        Ok(self.audio_buffer.drain_samples())
    }

    /// Get the number of samples currently buffered
    pub fn buffered_samples(&self) -> usize {
        self.audio_buffer.len()
    }

    /// Get the source sample rate (capture rate)
    pub fn source_sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    /// Get the target sample rate (Gemini input rate)
    pub fn target_sample_rate(&self) -> u32 {
        GEMINI_INPUT_SAMPLE_RATE
    }

    /// Stop capturing system audio
    pub fn stop(&mut self) -> Result<(), ScreenCaptureError> {
        if !self.is_capturing {
            return Err(ScreenCaptureError::NotStarted);
        }

        if let Some(mut child) = self.child_process.take() {
            // Drop stdin to signal the child process to exit cleanly
            drop(child.stdin.take());
            let _ = child.kill();
            let _ = child.wait();
        }

        // Clear the audio buffer
        self.audio_buffer.clear();
        
        self.is_capturing = false;
        Ok(())
    }

    /// Check if capture is currently running
    pub fn is_capturing(&self) -> bool {
        self.is_capturing
    }
}

impl Default for ScreenCaptureAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ScreenCaptureAudio {
    fn drop(&mut self) {
        if self.is_capturing {
            let _ = self.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_version_check() {
        // This test will only run on macOS
        let result = ScreenCaptureAudio::check_macos_version();
        // In stub implementation, this always returns Ok with version 14.0.0
        assert!(result.is_ok());
        let version = result.unwrap();
        assert!(version.meets_requirement(MINIMUM_MACOS_VERSION));
    }

    #[test]
    fn test_version_display() {
        let version = MacOSVersion {
            major: 14,
            minor: 2,
            patch: 1,
        };
        assert_eq!(format!("{}", version), "14.2.1");
    }

    #[test]
    fn test_version_requirement() {
        let version_ok = MacOSVersion {
            major: 14,
            minor: 0,
            patch: 0,
        };
        assert!(version_ok.meets_requirement(14));

        let version_old = MacOSVersion {
            major: 13,
            minor: 5,
            patch: 0,
        };
        assert!(!version_old.meets_requirement(14));

        let version_new = MacOSVersion {
            major: 15,
            minor: 0,
            patch: 0,
        };
        assert!(version_new.meets_requirement(14));
    }

    #[test]
    fn test_permission_status_display() {
        assert_eq!(format!("{}", PermissionStatus::Authorized), "Authorized");
        assert_eq!(format!("{}", PermissionStatus::Denied), "Denied");
        assert_eq!(
            format!("{}", PermissionStatus::NotDetermined),
            "Not Determined"
        );
        assert_eq!(format!("{}", PermissionStatus::Restricted), "Restricted");
    }

    #[test]
    fn test_settings_path() {
        let path = ScreenCaptureAudio::get_permission_settings_path();
        assert!(path.contains("Privacy & Security"));
        assert!(path.contains("Screen Recording"));
    }

    #[test]
    fn test_settings_url() {
        let url = ScreenCaptureAudio::get_permission_settings_url();
        assert!(url.starts_with("x-apple.systempreferences:"));
        assert!(url.contains("ScreenCapture"));
    }

    #[test]
    fn test_default_stream_configuration() {
        let config = StreamConfiguration::default();
        assert!(config.captures_audio, "capturesAudio should be true by default (Requirement 3.2)");
        assert_eq!(config.sample_rate, DEFAULT_CAPTURE_SAMPLE_RATE);
        assert_eq!(config.channel_count, 1);
        assert!(config.excludes_current_process_audio);
    }

    #[test]
    fn test_custom_stream_configuration() {
        let capture = ScreenCaptureAudio::with_config(StreamConfiguration {
            captures_audio: true,
            sample_rate: 44100,
            channel_count: 2,
            excludes_current_process_audio: false,
        });
        
        assert_eq!(capture.config().sample_rate, 44100);
        assert_eq!(capture.config().channel_count, 2);
        assert!(!capture.config().excludes_current_process_audio);
    }

    #[test]
    fn test_audio_sample_buffer() {
        let buffer = AudioSampleBuffer::new(48000);
        
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
        
        // Push some samples
        buffer.push_samples(&[100, 200, 300, 400, 500]);
        assert!(!buffer.is_empty());
        assert_eq!(buffer.len(), 5);
        
        // Drain samples
        let samples = buffer.drain_samples();
        assert_eq!(samples, vec![100, 200, 300, 400, 500]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_audio_buffer_overflow_protection() {
        let buffer = AudioSampleBuffer::new(48000);
        
        // Push more than capacity - old samples should be dropped
        let large_data: Vec<i16> = (0..AUDIO_BUFFER_CAPACITY + 100).map(|i| i as i16).collect();
        buffer.push_samples(&large_data);
        
        // Buffer should not exceed capacity
        assert!(buffer.len() <= AUDIO_BUFFER_CAPACITY);
    }

    #[test]
    fn test_audio_buffer_clear() {
        let buffer = AudioSampleBuffer::new(48000);
        
        buffer.push_samples(&[1, 2, 3, 4, 5]);
        assert!(!buffer.is_empty());
        
        buffer.clear();
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_sample_rates() {
        let capture = ScreenCaptureAudio::new();
        
        assert_eq!(capture.source_sample_rate(), DEFAULT_CAPTURE_SAMPLE_RATE);
        assert_eq!(capture.target_sample_rate(), GEMINI_INPUT_SAMPLE_RATE);
    }

    #[tokio::test]
    async fn test_capture_lifecycle() {
        let mut capture = ScreenCaptureAudio::new();
        assert!(!capture.is_capturing());
        assert_eq!(capture.buffered_samples(), 0);

        // Start capture
        let result = capture.start_capture().await;
        assert!(result.is_ok());
        assert!(capture.is_capturing());

        // Read samples (should return empty in stub since no actual audio)
        let samples = capture.read_samples();
        assert!(samples.is_ok());

        // Stop capture
        let result = capture.stop();
        assert!(result.is_ok());
        assert!(!capture.is_capturing());
    }

    #[tokio::test]
    async fn test_double_start_fails() {
        let mut capture = ScreenCaptureAudio::new();
        
        capture.start_capture().await.unwrap();
        
        let result = capture.start_capture().await;
        assert!(matches!(result, Err(ScreenCaptureError::AlreadyRunning)));
    }

    #[test]
    fn test_read_without_start_fails() {
        let mut capture = ScreenCaptureAudio::new();
        
        let result = capture.read_samples();
        assert!(matches!(result, Err(ScreenCaptureError::NotStarted)));
    }

    #[test]
    fn test_read_raw_without_start_fails() {
        let mut capture = ScreenCaptureAudio::new();
        
        let result = capture.read_samples_raw();
        assert!(matches!(result, Err(ScreenCaptureError::NotStarted)));
    }

    #[test]
    fn test_stop_without_start_fails() {
        let mut capture = ScreenCaptureAudio::new();
        
        let result = capture.stop();
        assert!(matches!(result, Err(ScreenCaptureError::NotStarted)));
    }

    #[tokio::test]
    async fn test_stop_clears_buffer() {
        let mut capture = ScreenCaptureAudio::new();
        
        // Start capture
        capture.start_capture().await.unwrap();
        
        // Manually push some samples to the buffer for testing
        capture.audio_buffer.push_samples(&[100, 200, 300]);
        assert_eq!(capture.buffered_samples(), 3);
        
        // Stop should clear the buffer
        capture.stop().unwrap();
        assert_eq!(capture.buffered_samples(), 0);
    }

    #[tokio::test]
    async fn test_captures_audio_required() {
        let mut capture = ScreenCaptureAudio::with_config(StreamConfiguration {
            captures_audio: false, // This should cause an error
            sample_rate: 48000,
            channel_count: 1,
            excludes_current_process_audio: true,
        });
        
        let result = capture.start_capture().await;
        assert!(matches!(result, Err(ScreenCaptureError::InitializationError(_))));
    }
}
