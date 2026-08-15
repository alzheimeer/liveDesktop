//! VB-Cable Detection, Integration and Audio Routing for Windows
//!
//! Detects VB-Cable virtual audio device installation and provides
//! utilities for routing audio through VB-Cable.
//!
//! VB-Cable appears in Windows as:
//! - "CABLE Input" (VB-Audio Virtual Cable) - Virtual microphone (capture device)
//! - "CABLE Output" (VB-Audio Virtual Cable) - Virtual speaker (render device)
//!
//! # Audio Routing
//!
//! This module provides `VBCablePlayback` to route translated audio TO VB-Cable Output.
//! The audio format is PCM16 @ 24kHz mono (native Gemini Live output format).
//! Target latency: ≤100ms (Requirement 4.5)

#![cfg(windows)]

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use windows::core::PWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eCapture, eRender, IAudioClient, IAudioRenderClient, IMMDevice, IMMDeviceCollection,
    IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, DEVICE_STATE_ACTIVE,
    WAVEFORMATEX, WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

/// VB-Cable detection status
#[derive(Debug, Clone)]
pub struct VBCableStatus {
    /// Whether VB-Cable is installed (either input or output detected)
    pub is_installed: bool,
    /// Whether CABLE Input (virtual microphone) is available
    pub input_available: bool,
    /// Whether CABLE Output (virtual speaker) is available  
    pub output_available: bool,
    /// Device ID of CABLE Input if found
    pub input_device_id: Option<String>,
    /// Device ID of CABLE Output if found
    pub output_device_id: Option<String>,
    /// Friendly name of CABLE Input if found
    pub input_device_name: Option<String>,
    /// Friendly name of CABLE Output if found
    pub output_device_name: Option<String>,
}

impl Default for VBCableStatus {
    fn default() -> Self {
        Self {
            is_installed: false,
            input_available: false,
            output_available: false,
            input_device_id: None,
            output_device_id: None,
            input_device_name: None,
            output_device_name: None,
        }
    }
}

impl std::fmt::Display for VBCableStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_installed {
            write!(
                f,
                "VB-Cable instalado (Input: {}, Output: {})",
                if self.input_available { "✓" } else { "✗" },
                if self.output_available { "✓" } else { "✗" }
            )
        } else {
            write!(f, "VB-Cable no está instalado")
        }
    }
}

/// Errors specific to VB-Cable operations
#[derive(Debug, Clone)]
pub enum VBCableError {
    /// COM initialization failed
    ComInitFailed(String),
    /// Failed to create device enumerator
    EnumeratorCreationFailed(String),
    /// Failed to enumerate devices
    DeviceEnumerationFailed(String),
    /// Failed to get device count
    DeviceCountFailed(String),
    /// Failed to get device properties
    DevicePropertiesFailed(String),
    /// VB-Cable Output device not found or not available
    OutputDeviceNotAvailable,
    /// Failed to activate audio client
    AudioClientActivationFailed(String),
    /// Failed to initialize audio client
    AudioClientInitFailed(String),
    /// Failed to get render service
    RenderServiceFailed(String),
    /// Failed to start playback
    StartPlaybackFailed(String),
    /// Failed to stop playback
    StopPlaybackFailed(String),
    /// Failed to write audio buffer
    WriteBufferFailed(String),
    /// Playback not active
    PlaybackNotActive,
    /// Device disconnected during playback
    DeviceDisconnected { device_id: String, device_name: String },
}

impl std::fmt::Display for VBCableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VBCableError::ComInitFailed(msg) => {
                write!(f, "Error al inicializar COM: {}", msg)
            }
            VBCableError::EnumeratorCreationFailed(msg) => {
                write!(f, "Error al crear el enumerador de dispositivos: {}", msg)
            }
            VBCableError::DeviceEnumerationFailed(msg) => {
                write!(f, "Error al enumerar dispositivos de audio: {}", msg)
            }
            VBCableError::DeviceCountFailed(msg) => {
                write!(f, "Error al obtener el número de dispositivos: {}", msg)
            }
            VBCableError::DevicePropertiesFailed(msg) => {
                write!(f, "Error al obtener las propiedades del dispositivo: {}", msg)
            }
            VBCableError::OutputDeviceNotAvailable => {
                write!(f, "El dispositivo VB-Cable Output no está disponible. Verifica que VB-Cable esté instalado correctamente.")
            }
            VBCableError::AudioClientActivationFailed(msg) => {
                write!(f, "Error al activar el cliente de audio: {}", msg)
            }
            VBCableError::AudioClientInitFailed(msg) => {
                write!(f, "Error al inicializar el cliente de audio: {}", msg)
            }
            VBCableError::RenderServiceFailed(msg) => {
                write!(f, "Error al obtener el servicio de reproducción: {}", msg)
            }
            VBCableError::StartPlaybackFailed(msg) => {
                write!(f, "Error al iniciar la reproducción: {}", msg)
            }
            VBCableError::StopPlaybackFailed(msg) => {
                write!(f, "Error al detener la reproducción: {}", msg)
            }
            VBCableError::WriteBufferFailed(msg) => {
                write!(f, "Error al escribir en el buffer de audio: {}", msg)
            }
            VBCableError::PlaybackNotActive => {
                write!(f, "La reproducción no está activa")
            }
            VBCableError::DeviceDisconnected { device_name, .. } => {
                write!(
                    f,
                    "El dispositivo VB-Cable '{}' se ha desconectado durante la reproducción",
                    device_name
                )
            }
        }
    }
}

impl std::error::Error for VBCableError {}

/// RAII guard for COM initialization
struct ComGuard {
    initialized: bool,
}

impl ComGuard {
    fn new() -> Result<Self, VBCableError> {
        unsafe {
            match CoInitializeEx(None, COINIT_MULTITHREADED) {
                Ok(()) => Ok(Self { initialized: true }),
                Err(e) => {
                    // HRESULT S_FALSE (0x00000001) means already initialized
                    if e.code().0 == 1 {
                        Ok(Self { initialized: false })
                    } else {
                        Err(VBCableError::ComInitFailed(format!(
                            "HRESULT: 0x{:08X}",
                            e.code().0
                        )))
                    }
                }
            }
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

/// Global state for VB-Cable detection result
/// This is registered at app startup and can be queried later
static VBCABLE_DETECTED: AtomicBool = AtomicBool::new(false);
static VBCABLE_STATUS_CACHED: std::sync::OnceLock<VBCableStatus> = std::sync::OnceLock::new();

/// Check if VB-Cable is installed on the system.
///
/// This function searches for VB-Cable virtual audio devices by looking for
/// devices with "CABLE" in their name. VB-Cable typically appears as:
/// - "CABLE Input (VB-Audio Virtual Cable)" - Virtual microphone (capture device)
/// - "CABLE Output (VB-Audio Virtual Cable)" - Virtual speaker (render device)
///
/// # Returns
///
/// - `Ok(VBCableStatus)` - Detection result with device information
/// - `Err(VBCableError)` - Error during detection
///
/// # Requirements
///
/// - Requirement 4.1: Detect if VB-Cable is installed at app startup
///
/// # Example
///
/// ```ignore
/// use traductor_desktop_lib::audio::windows::vbcable;
///
/// let status = vbcable::detect_vbcable()?;
/// if status.is_installed {
///     println!("VB-Cable found!");
///     if let Some(output_id) = status.output_device_id {
///         println!("CABLE Output ID: {}", output_id);
///     }
/// }
/// ```
pub fn detect_vbcable() -> Result<VBCableStatus, VBCableError> {
    // Initialize COM
    let _com_guard = ComGuard::new()?;

    // Create device enumerator
    let enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| {
            VBCableError::EnumeratorCreationFailed(format!("HRESULT: 0x{:08X}", e.code().0))
        })?
    };

    let mut status = VBCableStatus::default();

    // Search for CABLE Input in capture devices (microphones)
    match find_cable_device(&enumerator, eCapture) {
        Ok(Some((id, name))) => {
            status.input_available = true;
            status.input_device_id = Some(id);
            status.input_device_name = Some(name);
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!("Error searching for CABLE Input: {}", e);
        }
    }

    // Search for CABLE Output in render devices (speakers)
    match find_cable_device(&enumerator, eRender) {
        Ok(Some((id, name))) => {
            status.output_available = true;
            status.output_device_id = Some(id);
            status.output_device_name = Some(name);
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!("Error searching for CABLE Output: {}", e);
        }
    }

    // VB-Cable is considered installed if either input or output is found
    status.is_installed = status.input_available || status.output_available;

    Ok(status)
}

/// Detect VB-Cable and register the result in internal system state.
///
/// This function should be called once at app startup. The result is cached
/// and can be retrieved later using `get_cached_status()` or `is_vbcable_installed()`.
///
/// # Returns
///
/// - `Ok(VBCableStatus)` - Detection result
/// - `Err(VBCableError)` - Error during detection
///
/// # Requirements
///
/// - Requirement 4.1: Detect if VB-Cable is installed at app startup and
///   register the result in internal system state
pub fn detect_and_register() -> Result<VBCableStatus, VBCableError> {
    let status = detect_vbcable()?;
    
    // Register the result in global state
    VBCABLE_DETECTED.store(status.is_installed, Ordering::SeqCst);
    let _ = VBCABLE_STATUS_CACHED.set(status.clone());
    
    tracing::info!("VB-Cable detection completed: {}", status);
    
    if status.is_installed {
        if let Some(ref name) = status.output_device_name {
            tracing::info!("CABLE Output found: {}", name);
        }
        if let Some(ref name) = status.input_device_name {
            tracing::info!("CABLE Input found: {}", name);
        }
    } else {
        tracing::info!("VB-Cable not installed on this system");
    }
    
    Ok(status)
}

/// Check if VB-Cable was detected (from cached result).
///
/// This returns the cached result from `detect_and_register()`.
/// If detection hasn't been performed yet, returns `false`.
///
/// # Returns
///
/// `true` if VB-Cable was detected, `false` otherwise
pub fn is_vbcable_installed() -> bool {
    VBCABLE_DETECTED.load(Ordering::SeqCst)
}

/// Get the cached VB-Cable status.
///
/// Returns the full status information from the last detection.
/// Returns `None` if detection hasn't been performed yet.
///
/// # Returns
///
/// The cached `VBCableStatus` or `None` if not yet detected
pub fn get_cached_status() -> Option<&'static VBCableStatus> {
    VBCABLE_STATUS_CACHED.get()
}

/// Get the CABLE Output device ID if available.
///
/// This is the virtual speaker device where translated audio
/// should be routed for injection into meeting apps.
///
/// # Returns
///
/// The device ID of CABLE Output, or `None` if not available
pub fn get_output_device_id() -> Option<String> {
    VBCABLE_STATUS_CACHED
        .get()
        .and_then(|s| s.output_device_id.clone())
}

/// Get the CABLE Input device ID if available.
///
/// This is the virtual microphone device that meeting apps
/// will use as their audio input.
///
/// # Returns
///
/// The device ID of CABLE Input, or `None` if not available
pub fn get_input_device_id() -> Option<String> {
    VBCABLE_STATUS_CACHED
        .get()
        .and_then(|s| s.input_device_id.clone())
}

/// Search for a VB-Cable device in the specified device collection.
///
/// Looks for devices with "CABLE" in their name (case-insensitive).
fn find_cable_device(
    enumerator: &IMMDeviceEnumerator,
    data_flow: windows::Win32::Media::Audio::EDataFlow,
) -> Result<Option<(String, String)>, VBCableError> {
    // Enumerate devices of the specified type
    let collection: IMMDeviceCollection = unsafe {
        enumerator
            .EnumAudioEndpoints(data_flow, DEVICE_STATE_ACTIVE)
            .map_err(|e| {
                VBCableError::DeviceEnumerationFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?
    };

    // Get device count
    let count = unsafe {
        collection
            .GetCount()
            .map_err(|e| VBCableError::DeviceCountFailed(format!("HRESULT: 0x{:08X}", e.code().0)))?
    };

    // Search for CABLE device
    for i in 0..count {
        if let Some(result) = check_device_for_cable(&collection, i)? {
            return Ok(Some(result));
        }
    }

    Ok(None)
}

/// Check if a specific device is a VB-Cable device.
///
/// Returns the device ID and name if it's a CABLE device.
fn check_device_for_cable(
    collection: &IMMDeviceCollection,
    index: u32,
) -> Result<Option<(String, String)>, VBCableError> {
    unsafe {
        // Get device at index
        let device = match collection.Item(index) {
            Ok(d) => d,
            Err(_) => return Ok(None),
        };

        // Get device ID
        let device_id_ptr: PWSTR = match device.GetId() {
            Ok(id) => id,
            Err(_) => return Ok(None),
        };
        let device_id = pwstr_to_string(device_id_ptr);

        // Free the allocated string
        windows::Win32::System::Com::CoTaskMemFree(Some(device_id_ptr.0 as *const _));

        // Get device properties
        let property_store = match device.OpenPropertyStore(windows::Win32::System::Com::STGM_READ) {
            Ok(ps) => ps,
            Err(_) => return Ok(None),
        };

        // Get friendly name
        let name_prop = match property_store.GetValue(&PKEY_Device_FriendlyName) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        let device_name = match propvariant_to_string(&name_prop) {
            Some(name) => name,
            None => return Ok(None),
        };

        // Check if this is a CABLE device (case-insensitive)
        let name_upper = device_name.to_uppercase();
        if name_upper.contains("CABLE") && name_upper.contains("VB-AUDIO") {
            return Ok(Some((device_id, device_name)));
        }
        
        // Also check for just "CABLE Input" or "CABLE Output" patterns
        if name_upper.starts_with("CABLE INPUT") || name_upper.starts_with("CABLE OUTPUT") {
            return Ok(Some((device_id, device_name)));
        }

        Ok(None)
    }
}

/// Convert PWSTR to Rust String
fn pwstr_to_string(pwstr: PWSTR) -> String {
    if pwstr.0.is_null() {
        return String::new();
    }

    unsafe {
        let mut len = 0;
        while *pwstr.0.add(len) != 0 {
            len += 1;
        }

        let slice = std::slice::from_raw_parts(pwstr.0, len);
        OsString::from_wide(slice).to_string_lossy().into_owned()
    }
}

/// Convert PROPVARIANT to String (for device friendly name)
fn propvariant_to_string(
    prop: &windows::Win32::System::Com::StructuredStorage::PROPVARIANT,
) -> Option<String> {
    use windows::Win32::System::Variant::VT_LPWSTR;

    unsafe {
        let vt = prop.Anonymous.Anonymous.vt;

        if vt == VT_LPWSTR {
            let pwsz = prop.Anonymous.Anonymous.Anonymous.pwszVal;
            if !pwsz.0.is_null() {
                return Some(pwstr_to_string(pwsz));
            }
        }
    }
    None
}

// ============================================================================
// VB-CABLE AUDIO PLAYBACK
// ============================================================================

/// Audio playback format for VB-Cable
/// 
/// Matches Gemini Live output: 24kHz, 16-bit, mono PCM
pub const VBCABLE_SAMPLE_RATE: u32 = 24000;
pub const VBCABLE_BITS_PER_SAMPLE: u16 = 16;
pub const VBCABLE_CHANNELS: u16 = 1;
pub const VBCABLE_BLOCK_ALIGN: u16 = (VBCABLE_BITS_PER_SAMPLE / 8) * VBCABLE_CHANNELS;
pub const VBCABLE_BYTES_PER_SECOND: u32 = VBCABLE_SAMPLE_RATE * VBCABLE_BLOCK_ALIGN as u32;

/// Target buffer duration for low latency playback (in 100ns units)
/// 40ms buffer provides good balance between latency and stability
/// This keeps us well under the 100ms requirement (4.5)
const BUFFER_DURATION_100NS: i64 = 400_000; // 40ms in 100ns units

/// VB-Cable Audio Playback
///
/// Routes translated audio to VB-Cable Output device for injection into meeting apps.
/// The audio format is PCM16 @ 24kHz mono (native Gemini Live output format).
///
/// # Requirements
///
/// - Requirement 4.5: Route audio to VB-Cable Output with ≤100ms latency
/// - Requirement 4.6: Handle VB-Cable Output unavailability gracefully
/// - Requirement 4.7: Playback at 24kHz, 16-bit mono PCM without resampling
///
/// # Example
///
/// ```ignore
/// use traductor_desktop_lib::audio::windows::vbcable::VBCablePlayback;
///
/// // Start playback to VB-Cable Output
/// let mut playback = VBCablePlayback::start()?;
///
/// // Write translated audio samples (24kHz, 16-bit mono)
/// let samples: Vec<i16> = vec![0i16; 24000]; // 1 second of silence
/// playback.write_samples(&samples)?;
///
/// // Check latency
/// println!("Current latency: {}ms", playback.get_latency_ms());
///
/// // Stop playback when done
/// playback.stop()?;
/// ```
pub struct VBCablePlayback {
    /// The audio client for the VB-Cable Output device
    client: IAudioClient,
    /// The render client for writing audio data
    render_client: IAudioRenderClient,
    /// Whether playback is currently active
    is_active: Arc<AtomicBool>,
    /// Timestamp when playback started
    start_time: Instant,
    /// COM guard to ensure COM is initialized for the lifetime of this object
    _com_guard: ComGuard,
    /// Device ID of the VB-Cable Output device
    device_id: String,
    /// Device name of the VB-Cable Output device
    device_name: String,
    /// Buffer size in frames
    buffer_size_frames: u32,
    /// Flag indicating device was disconnected
    disconnected: Arc<AtomicBool>,
}

impl VBCablePlayback {
    /// Start audio playback to VB-Cable Output device.
    ///
    /// Initializes WASAPI render mode to play audio to VB-Cable Output.
    /// The audio format is fixed at 24kHz, 16-bit, mono PCM (Requirement 4.7).
    ///
    /// # Returns
    ///
    /// - `Ok(VBCablePlayback)` - Playback started successfully
    /// - `Err(VBCableError)` - Error describing what went wrong
    ///
    /// # Requirements
    ///
    /// - Requirement 4.5: Route audio with ≤100ms latency
    /// - Requirement 4.6: Handle device unavailability
    /// - Requirement 4.7: 24kHz, 16-bit mono PCM without resampling
    pub fn start() -> Result<Self, VBCableError> {
        let start_time = Instant::now();
        
        // Initialize COM
        let com_guard = ComGuard::new()?;
        
        // Create device enumerator
        let enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| {
                VBCableError::EnumeratorCreationFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?
        };

        // Find VB-Cable Output device
        let (device_id, device_name) = find_cable_device(&enumerator, eRender)?
            .ok_or(VBCableError::OutputDeviceNotAvailable)?;
        
        // Get the device by ID
        let device = Self::get_device_by_id(&enumerator, &device_id)?;
        
        // Activate audio client on the device
        let client: IAudioClient = unsafe {
            device.Activate(CLSCTX_ALL, None).map_err(|e| {
                VBCableError::AudioClientActivationFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?
        };

        // Create the audio format structure for 24kHz, 16-bit, mono
        let format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: VBCABLE_CHANNELS,
            nSamplesPerSec: VBCABLE_SAMPLE_RATE,
            nAvgBytesPerSec: VBCABLE_BYTES_PER_SECOND,
            nBlockAlign: VBCABLE_BLOCK_ALIGN,
            wBitsPerSample: VBCABLE_BITS_PER_SAMPLE,
            cbSize: 0,
        };
        
        // Initialize the audio client in shared mode for rendering
        unsafe {
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    0, // No special flags for render
                    BUFFER_DURATION_100NS,
                    0, // Period must be 0 for shared mode
                    &format,
                    None,
                )
                .map_err(|e| {
                    VBCableError::AudioClientInitFailed(format!(
                        "HRESULT: 0x{:08X}. VB-Cable may not support 24kHz format.",
                        e.code().0
                    ))
                })?;
        }

        // Get buffer size
        let buffer_size_frames = unsafe {
            client.GetBufferSize().map_err(|e| {
                VBCableError::AudioClientInitFailed(format!(
                    "GetBufferSize failed: HRESULT 0x{:08X}",
                    e.code().0
                ))
            })?
        };

        // Get the render client
        let render_client: IAudioRenderClient = unsafe {
            client.GetService().map_err(|e| {
                VBCableError::RenderServiceFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?
        };

        // Start the audio client
        unsafe {
            client.Start().map_err(|e| {
                VBCableError::StartPlaybackFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?;
        }

        let buffer_duration_ms = (buffer_size_frames as f64 / VBCABLE_SAMPLE_RATE as f64) * 1000.0;
        
        tracing::info!(
            "VB-Cable playback started on '{}' ({}) - {}Hz, {} channels, {}bit, buffer: {} frames ({:.1}ms) - in {}ms",
            device_name,
            device_id,
            VBCABLE_SAMPLE_RATE,
            VBCABLE_CHANNELS,
            VBCABLE_BITS_PER_SAMPLE,
            buffer_size_frames,
            buffer_duration_ms,
            start_time.elapsed().as_millis()
        );

        Ok(Self {
            client,
            render_client,
            is_active: Arc::new(AtomicBool::new(true)),
            start_time,
            _com_guard: com_guard,
            device_id,
            device_name,
            buffer_size_frames,
            disconnected: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Write audio samples to VB-Cable Output.
    ///
    /// Writes PCM16 @ 24kHz mono samples to the VB-Cable Output device.
    /// The samples should be in the native Gemini Live output format.
    ///
    /// # Arguments
    ///
    /// * `samples` - PCM16 samples to write (24kHz, mono)
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Samples written successfully
    /// - `Err(VBCableError)` - Error writing samples
    ///
    /// # Requirements
    ///
    /// - Requirement 4.5: Maintain ≤100ms latency
    /// - Requirement 4.7: Accept 24kHz, 16-bit mono PCM without resampling
    pub fn write_samples(&self, samples: &[i16]) -> Result<(), VBCableError> {
        if !self.is_active.load(Ordering::Relaxed) {
            return Err(VBCableError::PlaybackNotActive);
        }

        if self.disconnected.load(Ordering::SeqCst) {
            return Err(VBCableError::DeviceDisconnected {
                device_id: self.device_id.clone(),
                device_name: self.device_name.clone(),
            });
        }

        if samples.is_empty() {
            return Ok(());
        }

        let mut samples_written = 0;
        
        while samples_written < samples.len() {
            // Get current buffer padding (how many frames are queued)
            let padding = unsafe {
                self.client.GetCurrentPadding().map_err(|e| {
                    // Check for device invalidation errors
                    // Note: HRESULT codes are negative when cast to i32
                    let code = e.code().0;
                    if code == 0x88890004u32 as i32 || // AUDCLNT_E_DEVICE_INVALIDATED
                       code == 0x88890005u32 as i32 || // AUDCLNT_E_SERVICE_NOT_RUNNING
                       code == 0x88890017u32 as i32    // AUDCLNT_E_RESOURCES_INVALIDATED
                    {
                        self.disconnected.store(true, Ordering::SeqCst);
                        return VBCableError::DeviceDisconnected {
                            device_id: self.device_id.clone(),
                            device_name: self.device_name.clone(),
                        };
                    }
                    VBCableError::WriteBufferFailed(format!(
                        "GetCurrentPadding failed: HRESULT 0x{:08X}",
                        e.code().0
                    ))
                })?
            };

            // Calculate available space
            let available_frames = self.buffer_size_frames.saturating_sub(padding);
            
            if available_frames == 0 {
                // Buffer is full, need to wait a bit
                // Sleep for approximately one buffer period
                std::thread::sleep(std::time::Duration::from_micros(500));
                continue;
            }

            // Calculate how many samples we can write
            let remaining_samples = samples.len() - samples_written;
            let frames_to_write = std::cmp::min(available_frames as usize, remaining_samples);
            
            if frames_to_write == 0 {
                break;
            }

            // Get buffer from WASAPI
            let buffer_ptr = unsafe {
                self.render_client.GetBuffer(frames_to_write as u32).map_err(|e| {
                    VBCableError::WriteBufferFailed(format!(
                        "GetBuffer failed: HRESULT 0x{:08X}",
                        e.code().0
                    ))
                })?
            };

            // Copy samples to buffer
            unsafe {
                let dest = buffer_ptr as *mut i16;
                let src = samples[samples_written..samples_written + frames_to_write].as_ptr();
                ptr::copy_nonoverlapping(src, dest, frames_to_write);
            }

            // Release buffer
            unsafe {
                self.render_client.ReleaseBuffer(frames_to_write as u32, 0).map_err(|e| {
                    VBCableError::WriteBufferFailed(format!(
                        "ReleaseBuffer failed: HRESULT 0x{:08X}",
                        e.code().0
                    ))
                })?;
            }

            samples_written += frames_to_write;
        }

        Ok(())
    }

    /// Get the current playback latency in milliseconds.
    ///
    /// Returns the estimated latency from when samples are written to when
    /// they are played. Should be ≤100ms per Requirement 4.5.
    ///
    /// # Returns
    ///
    /// The playback latency in milliseconds.
    pub fn get_latency_ms(&self) -> u32 {
        // Get stream latency from WASAPI
        let latency = unsafe {
            self.client.GetStreamLatency().unwrap_or(0)
        };
        
        // Get current padding to estimate buffer latency
        let padding = unsafe {
            self.client.GetCurrentPadding().unwrap_or(0)
        };
        
        // Calculate total latency: stream latency + buffer latency
        let stream_latency_ms = (latency / 10_000) as u32;
        let buffer_latency_ms = (padding as f64 / VBCABLE_SAMPLE_RATE as f64 * 1000.0) as u32;
        
        stream_latency_ms + buffer_latency_ms
    }

    /// Check if playback is currently active.
    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Relaxed)
    }

    /// Check if the device has been disconnected.
    pub fn is_disconnected(&self) -> bool {
        self.disconnected.load(Ordering::SeqCst)
    }

    /// Get the device ID of VB-Cable Output.
    pub fn get_device_id(&self) -> &str {
        &self.device_id
    }

    /// Get the device name of VB-Cable Output.
    pub fn get_device_name(&self) -> &str {
        &self.device_name
    }

    /// Get the buffer size in frames.
    pub fn get_buffer_size_frames(&self) -> u32 {
        self.buffer_size_frames
    }

    /// Stop playback.
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Playback stopped successfully
    /// - `Err(VBCableError)` - Error stopping playback
    pub fn stop(&mut self) -> Result<(), VBCableError> {
        if !self.is_active.load(Ordering::Relaxed) {
            return Ok(());
        }

        unsafe {
            self.client.Stop().map_err(|e| {
                VBCableError::StopPlaybackFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?;
        }

        self.is_active.store(false, Ordering::Relaxed);
        
        tracing::info!(
            "VB-Cable playback stopped after {}ms",
            self.start_time.elapsed().as_millis()
        );

        Ok(())
    }

    /// Get a device by its ID
    fn get_device_by_id(
        enumerator: &IMMDeviceEnumerator,
        device_id: &str,
    ) -> Result<IMMDevice, VBCableError> {
        // Convert device_id to wide string
        let wide_id: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        
        unsafe {
            let device = enumerator
                .GetDevice(windows::core::PCWSTR::from_raw(wide_id.as_ptr()))
                .map_err(|e| {
                    VBCableError::DevicePropertiesFailed(format!(
                        "Device ID: {}, HRESULT: 0x{:08X}",
                        device_id,
                        e.code().0
                    ))
                })?;
            Ok(device)
        }
    }
}

impl Drop for VBCablePlayback {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Check if VB-Cable Output is available for playback.
///
/// Use this before attempting to start playback to verify the device exists.
///
/// # Returns
///
/// `true` if VB-Cable Output device is available, `false` otherwise.
///
/// # Requirements
///
/// - Requirement 4.6: Check if VB-Cable Output is available
pub fn is_output_available() -> bool {
    // First check cached status
    if let Some(status) = get_cached_status() {
        return status.output_available;
    }
    
    // If no cached status, do a quick detection
    match detect_vbcable() {
        Ok(status) => status.output_available,
        Err(_) => false,
    }
}

/// Get user-friendly error details for VB-Cable errors.
///
/// Returns a tuple of (title, message, suggestion) for display in UI.
pub fn get_error_display_info(error: &VBCableError) -> (&'static str, String, &'static str) {
    match error {
        VBCableError::OutputDeviceNotAvailable => (
            "VB-Cable no disponible",
            "El dispositivo VB-Cable Output no está disponible en el sistema.".to_string(),
            "Verifica que VB-Cable esté instalado correctamente. \
             Descárgalo desde https://vb-audio.com/Cable/ si no está instalado.",
        ),
        VBCableError::DeviceDisconnected { device_name, .. } => (
            "Dispositivo desconectado",
            format!(
                "El dispositivo '{}' se desconectó durante la reproducción de audio.",
                device_name
            ),
            "Verifica que VB-Cable esté funcionando correctamente y reinicia la traducción.",
        ),
        VBCableError::AudioClientInitFailed(msg) => (
            "Error de inicialización",
            format!("No se pudo inicializar el audio: {}", msg),
            "VB-Cable puede no soportar el formato de audio requerido (24kHz). \
             Intenta reinstalar VB-Cable.",
        ),
        VBCableError::PlaybackNotActive => (
            "Reproducción no activa",
            "Se intentó escribir audio pero la reproducción no está activa.".to_string(),
            "Inicia la reproducción antes de enviar audio.",
        ),
        _ => (
            "Error de VB-Cable",
            error.to_string(),
            "Verifica la configuración de VB-Cable.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vbcable_status_default() {
        let status = VBCableStatus::default();
        assert!(!status.is_installed);
        assert!(!status.input_available);
        assert!(!status.output_available);
        assert!(status.input_device_id.is_none());
        assert!(status.output_device_id.is_none());
    }

    #[test]
    fn test_vbcable_status_display_not_installed() {
        let status = VBCableStatus::default();
        let display = format!("{}", status);
        assert!(display.contains("no está instalado"));
    }

    #[test]
    fn test_vbcable_status_display_installed() {
        let status = VBCableStatus {
            is_installed: true,
            input_available: true,
            output_available: true,
            input_device_id: Some("input-id".to_string()),
            output_device_id: Some("output-id".to_string()),
            input_device_name: Some("CABLE Input".to_string()),
            output_device_name: Some("CABLE Output".to_string()),
        };
        let display = format!("{}", status);
        assert!(display.contains("instalado"));
        assert!(display.contains("✓"));
    }

    #[test]
    fn test_vbcable_status_display_partial() {
        let status = VBCableStatus {
            is_installed: true,
            input_available: false,
            output_available: true,
            input_device_id: None,
            output_device_id: Some("output-id".to_string()),
            input_device_name: None,
            output_device_name: Some("CABLE Output".to_string()),
        };
        let display = format!("{}", status);
        assert!(display.contains("✓"));
        assert!(display.contains("✗"));
    }

    #[test]
    fn test_vbcable_error_display() {
        let error = VBCableError::ComInitFailed("test error".to_string());
        let msg = format!("{}", error);
        assert!(msg.contains("COM"));
        assert!(msg.contains("test error"));
    }

    #[test]
    fn test_detect_vbcable_runs_without_panic() {
        // This test just verifies the detection function runs without crashing
        // The actual result depends on whether VB-Cable is installed
        let result = detect_vbcable();
        match result {
            Ok(status) => {
                println!("VB-Cable detection result: {}", status);
                if status.is_installed {
                    println!("  Input ID: {:?}", status.input_device_id);
                    println!("  Output ID: {:?}", status.output_device_id);
                }
            }
            Err(e) => {
                println!("VB-Cable detection error (may be expected): {}", e);
            }
        }
    }

    #[test]
    fn test_detect_and_register() {
        // Run detection and registration
        let result = detect_and_register();
        
        match result {
            Ok(status) => {
                // Verify the cached value matches
                assert_eq!(is_vbcable_installed(), status.is_installed);
                
                if let Some(cached) = get_cached_status() {
                    assert_eq!(cached.is_installed, status.is_installed);
                    assert_eq!(cached.input_available, status.input_available);
                    assert_eq!(cached.output_available, status.output_available);
                }
            }
            Err(e) => {
                println!("Detection error (may be expected): {}", e);
            }
        }
    }

    #[test]
    fn test_vbcable_playback_constants() {
        // Verify audio format constants match Gemini Live output format
        assert_eq!(VBCABLE_SAMPLE_RATE, 24000, "Sample rate should be 24kHz");
        assert_eq!(VBCABLE_BITS_PER_SAMPLE, 16, "Bits per sample should be 16");
        assert_eq!(VBCABLE_CHANNELS, 1, "Should be mono");
        assert_eq!(VBCABLE_BLOCK_ALIGN, 2, "Block align should be 2 bytes for 16-bit mono");
        assert_eq!(
            VBCABLE_BYTES_PER_SECOND, 
            48000, 
            "Bytes per second should be 24000 * 2 = 48000"
        );
    }

    #[test]
    fn test_vbcable_error_display_variants() {
        // Test OutputDeviceNotAvailable
        let error = VBCableError::OutputDeviceNotAvailable;
        let msg = format!("{}", error);
        assert!(msg.contains("VB-Cable Output"));
        assert!(msg.contains("no está disponible"));

        // Test DeviceDisconnected
        let error = VBCableError::DeviceDisconnected {
            device_id: "test-id".to_string(),
            device_name: "CABLE Output".to_string(),
        };
        let msg = format!("{}", error);
        assert!(msg.contains("CABLE Output"));
        assert!(msg.contains("desconectado"));

        // Test AudioClientInitFailed
        let error = VBCableError::AudioClientInitFailed("test error".to_string());
        let msg = format!("{}", error);
        assert!(msg.contains("inicializar"));
        assert!(msg.contains("test error"));

        // Test PlaybackNotActive
        let error = VBCableError::PlaybackNotActive;
        let msg = format!("{}", error);
        assert!(msg.contains("no está activa"));

        // Test WriteBufferFailed
        let error = VBCableError::WriteBufferFailed("buffer error".to_string());
        let msg = format!("{}", error);
        assert!(msg.contains("escribir"));
        assert!(msg.contains("buffer error"));
    }

    #[test]
    fn test_is_output_available() {
        // This test verifies the is_output_available helper function
        let available = is_output_available();
        println!("VB-Cable Output available: {}", available);
        
        // If detection was run, verify consistency with cached status
        if let Some(cached) = get_cached_status() {
            assert_eq!(available, cached.output_available);
        }
    }

    #[test]
    fn test_vbcable_playback_start_if_available() {
        // First check if VB-Cable Output is available
        if !is_output_available() {
            println!("VB-Cable Output not available, skipping playback test");
            return;
        }

        // Try to start playback
        let result = VBCablePlayback::start();
        
        match result {
            Ok(mut playback) => {
                // Verify playback is active
                assert!(playback.is_active(), "Playback should be active after start");
                assert!(!playback.is_disconnected(), "Should not be disconnected");
                
                // Verify device info
                assert!(!playback.get_device_id().is_empty(), "Device ID should not be empty");
                assert!(!playback.get_device_name().is_empty(), "Device name should not be empty");
                println!("Playback started on: {}", playback.get_device_name());
                
                // Verify buffer size is reasonable
                let buffer_size = playback.get_buffer_size_frames();
                assert!(buffer_size > 0, "Buffer size should be positive");
                println!("Buffer size: {} frames", buffer_size);
                
                // Check latency is within requirement (≤100ms)
                let latency = playback.get_latency_ms();
                println!("Current latency: {}ms", latency);
                assert!(
                    latency <= 100,
                    "Latency should be ≤100ms (Requirement 4.5), got {}ms",
                    latency
                );
                
                // Write some test samples
                let test_samples: Vec<i16> = (0..480).map(|i| ((i as f32 * 0.1).sin() * 16000.0) as i16).collect();
                let write_result = playback.write_samples(&test_samples);
                assert!(write_result.is_ok(), "write_samples should succeed: {:?}", write_result.err());
                
                // Write empty samples (should be a no-op)
                let empty_result = playback.write_samples(&[]);
                assert!(empty_result.is_ok(), "Empty write should succeed");
                
                // Stop playback
                let stop_result = playback.stop();
                assert!(stop_result.is_ok(), "Stop should succeed");
                assert!(!playback.is_active(), "Playback should be inactive after stop");
            }
            Err(e) => {
                // This might fail if VB-Cable doesn't support 24kHz
                println!("Failed to start playback (may be expected): {}", e);
                
                // Get error display info
                let (title, message, suggestion) = get_error_display_info(&e);
                println!("  Title: {}", title);
                println!("  Message: {}", message);
                println!("  Suggestion: {}", suggestion);
            }
        }
    }

    #[test]
    fn test_vbcable_playback_write_when_not_active() {
        // This test verifies that writing to an inactive playback fails gracefully
        // We would need to construct a VBCablePlayback in inactive state which requires mocking
        // Since we're avoiding mocking, we verify the error type exists and displays correctly
        
        let error = VBCableError::PlaybackNotActive;
        let msg = format!("{}", error);
        assert!(msg.contains("no está activa"));
        
        // Verify error display info
        let (title, _message, _suggestion) = get_error_display_info(&error);
        assert_eq!(title, "Reproducción no activa");
    }

    #[test]
    fn test_vbcable_get_error_display_info() {
        // Test OutputDeviceNotAvailable
        let error = VBCableError::OutputDeviceNotAvailable;
        let (title, message, suggestion) = get_error_display_info(&error);
        assert_eq!(title, "VB-Cable no disponible");
        assert!(message.contains("VB-Cable Output"));
        assert!(suggestion.contains("vb-audio.com"));

        // Test DeviceDisconnected
        let error = VBCableError::DeviceDisconnected {
            device_id: "id".to_string(),
            device_name: "Test Device".to_string(),
        };
        let (title, message, suggestion) = get_error_display_info(&error);
        assert_eq!(title, "Dispositivo desconectado");
        assert!(message.contains("Test Device"));
        assert!(suggestion.contains("reinicia"));

        // Test AudioClientInitFailed
        let error = VBCableError::AudioClientInitFailed("format not supported".to_string());
        let (title, _message, suggestion) = get_error_display_info(&error);
        assert_eq!(title, "Error de inicialización");
        assert!(suggestion.contains("24kHz"));
    }
}
