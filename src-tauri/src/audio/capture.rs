//! Audio capture abstraction
//!
//! Platform-agnostic interface for audio capture with support for
//! status monitoring and device disconnection detection.
//!
//! # Requirements
//!
//! - Requirement 2.1: Capture audio using platform APIs
//! - Requirement 2.4: Maintain capture latency <50ms
//! - Requirement 2.7: Detect device disconnection during capture

use crate::error::AudioError;

/// Status of a capture operation
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureStatus {
    /// Capture is operating normally
    Ok,
    /// Device was disconnected during capture
    DeviceDisconnected {
        /// ID of the disconnected device
        device_id: String,
        /// Friendly name of the disconnected device
        device_name: String,
    },
    /// Buffer overrun occurred, some frames were dropped
    BufferOverrun {
        /// Number of frames that were dropped
        dropped_frames: u32,
    },
    /// Device state changed but capture may continue
    DeviceStateChanged {
        /// New state description
        state: String,
    },
}

/// Result of reading audio buffer with status information
#[derive(Debug)]
pub struct CaptureResult {
    /// PCM samples captured (may be empty if device disconnected or no data)
    pub samples: Vec<i16>,
    /// Status of the capture operation
    pub status: CaptureStatus,
}

impl CaptureResult {
    /// Create a successful capture result
    pub fn ok(samples: Vec<i16>) -> Self {
        Self {
            samples,
            status: CaptureStatus::Ok,
        }
    }

    /// Create a result indicating device disconnection
    pub fn disconnected(device_id: String, device_name: String) -> Self {
        Self {
            samples: Vec::new(),
            status: CaptureStatus::DeviceDisconnected {
                device_id,
                device_name,
            },
        }
    }

    /// Check if capture is still healthy
    pub fn is_ok(&self) -> bool {
        matches!(self.status, CaptureStatus::Ok)
    }

    /// Check if device was disconnected
    pub fn is_disconnected(&self) -> bool {
        matches!(self.status, CaptureStatus::DeviceDisconnected { .. })
    }
}

/// Platform-agnostic audio capture trait
///
/// Implementations of this trait provide audio capture functionality
/// for specific platforms (WASAPI on Windows, ScreenCaptureKit on macOS).
pub trait AudioCapture {
    /// Start audio capture on the specified device
    ///
    /// # Arguments
    ///
    /// * `device_id` - ID of the device to capture from
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Device not found
    /// - Device already in use
    /// - Platform audio API not available
    fn start(&mut self, device_id: &str) -> Result<(), AudioError>;

    /// Stop audio capture
    fn stop(&mut self) -> Result<(), AudioError>;

    /// Read captured audio from the buffer
    ///
    /// Returns PCM samples captured since the last read.
    /// Returns an empty vector if no data is available.
    fn read_buffer(&mut self) -> Result<Vec<i16>, AudioError>;

    /// Read captured audio with status information (Requirement 2.7)
    ///
    /// This method is similar to `read_buffer()` but additionally returns
    /// status information about the capture, including device disconnection
    /// detection.
    ///
    /// # Returns
    ///
    /// A `CaptureResult` containing:
    /// - `samples`: PCM samples (may be empty)
    /// - `status`: Capture status (Ok, DeviceDisconnected, BufferOverrun, etc.)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = capture.read_buffer_with_status()?;
    /// match result.status {
    ///     CaptureStatus::Ok => {
    ///         // Process samples normally
    ///         process_audio(&result.samples);
    ///     }
    ///     CaptureStatus::DeviceDisconnected { device_id, device_name } => {
    ///         // Notify user and pause channel
    ///         emit_device_disconnected(&device_id, &device_name);
    ///     }
    ///     CaptureStatus::BufferOverrun { dropped_frames } => {
    ///         // Log warning but continue
    ///         log::warn!("Dropped {} frames", dropped_frames);
    ///     }
    ///     _ => {}
    /// }
    /// ```
    fn read_buffer_with_status(&mut self) -> Result<CaptureResult, AudioError>;

    /// Get the current capture latency in milliseconds
    ///
    /// Should be <50ms per Requirement 2.4.
    fn get_latency_ms(&self) -> u32;

    /// Check if capture is currently active
    fn is_active(&self) -> bool;

    /// Check if the capture device has been disconnected
    fn is_disconnected(&self) -> bool;

    /// Get the ID of the device being captured
    fn device_id(&self) -> &str;

    /// Get the friendly name of the device being captured
    fn device_name(&self) -> &str;
}

/// Null implementation for testing and fallback
pub struct NullCapture {
    device_id: String,
    device_name: String,
    active: bool,
}

impl NullCapture {
    pub fn new() -> Self {
        Self {
            device_id: "null".to_string(),
            device_name: "Null Device".to_string(),
            active: false,
        }
    }
}

impl Default for NullCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioCapture for NullCapture {
    fn start(&mut self, device_id: &str) -> Result<(), AudioError> {
        self.device_id = device_id.to_string();
        self.active = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AudioError> {
        self.active = false;
        Ok(())
    }

    fn read_buffer(&mut self) -> Result<Vec<i16>, AudioError> {
        if !self.active {
            return Err(AudioError::CaptureNotActive);
        }
        Ok(Vec::new())
    }

    fn read_buffer_with_status(&mut self) -> Result<CaptureResult, AudioError> {
        if !self.active {
            return Err(AudioError::CaptureNotActive);
        }
        Ok(CaptureResult::ok(Vec::new()))
    }

    fn get_latency_ms(&self) -> u32 {
        0
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn is_disconnected(&self) -> bool {
        false
    }

    fn device_id(&self) -> &str {
        &self.device_id
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_result_ok() {
        let result = CaptureResult::ok(vec![1, 2, 3]);
        assert!(result.is_ok());
        assert!(!result.is_disconnected());
        assert_eq!(result.samples, vec![1, 2, 3]);
    }

    #[test]
    fn test_capture_result_disconnected() {
        let result = CaptureResult::disconnected(
            "device-123".to_string(),
            "Test Device".to_string(),
        );
        assert!(!result.is_ok());
        assert!(result.is_disconnected());
        assert!(result.samples.is_empty());
    }

    #[test]
    fn test_null_capture() {
        let mut capture = NullCapture::new();
        
        // Not active initially
        assert!(!capture.is_active());
        
        // Start capture
        capture.start("test-device").unwrap();
        assert!(capture.is_active());
        assert_eq!(capture.device_id(), "test-device");
        
        // Read buffer
        let samples = capture.read_buffer().unwrap();
        assert!(samples.is_empty());
        
        // Read with status
        let result = capture.read_buffer_with_status().unwrap();
        assert!(result.is_ok());
        
        // Stop capture
        capture.stop().unwrap();
        assert!(!capture.is_active());
    }

    #[test]
    fn test_capture_status_equality() {
        let status1 = CaptureStatus::Ok;
        let status2 = CaptureStatus::Ok;
        assert_eq!(status1, status2);

        let status3 = CaptureStatus::DeviceDisconnected {
            device_id: "id".to_string(),
            device_name: "name".to_string(),
        };
        let status4 = CaptureStatus::DeviceDisconnected {
            device_id: "id".to_string(),
            device_name: "name".to_string(),
        };
        assert_eq!(status3, status4);
    }
}
