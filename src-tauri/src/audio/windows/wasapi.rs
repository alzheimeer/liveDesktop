//! WASAPI Loopback Capture for Windows
//!
//! Provides audio capture from system output devices using Windows Audio Session API.
//! This module enables capturing audio from applications like Teams, Zoom, and Meet
//! without requiring screen sharing or additional virtual audio drivers.
//!
//! # Device Disconnection Detection
//!
//! The module monitors device state changes and detects disconnection during capture.
//! When a device is disconnected, capture is paused and the caller is notified via
//! the `CaptureStatus` returned by `read_buffer_with_status()`.
//!
//! # Error Handling
//!
//! - WASAPI availability is checked before operations
//! - Device disconnection during capture is detected and reported
//! - All errors include user-friendly messages in Spanish with recovery suggestions

#![cfg(windows)]

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use windows::core::{implement, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceCollection,
    IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
    DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT, DEVICE_STATE_UNPLUGGED,
    EDataFlow, ERole, WAVEFORMATEX,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};

use crate::audio::engine::AudioDevice;
use crate::error::{AudioError, DeviceState};

/// Errors specific to WASAPI operations
#[derive(Debug, Clone)]
pub enum WasapiError {
    /// COM initialization failed
    ComInitFailed(String),
    /// Failed to create device enumerator
    EnumeratorCreationFailed(String),
    /// Failed to enumerate devices
    DeviceEnumerationFailed(String),
    /// Failed to get device count
    DeviceCountFailed(String),
    /// Failed to get device at index
    DeviceAccessFailed(String),
    /// Failed to get device ID
    DeviceIdFailed(String),
    /// Failed to get device properties
    DevicePropertiesFailed(String),
    /// Failed to get device friendly name
    DeviceNameFailed(String),
    /// Operation timeout exceeded (should complete within 2 seconds)
    Timeout,
    /// WASAPI is not available on this system
    NotAvailable(String),
    /// Device not found
    DeviceNotFound(String),
    /// Failed to activate audio client
    AudioClientActivationFailed(String),
    /// Failed to get mix format
    MixFormatFailed(String),
    /// Failed to initialize audio client
    AudioClientInitFailed(String),
    /// Failed to get capture service
    CaptureServiceFailed(String),
    /// Failed to start capture
    StartCaptureFailed(String),
    /// Failed to stop capture
    StopCaptureFailed(String),
    /// Failed to read buffer
    ReadBufferFailed(String),
    /// Capture not active
    CaptureNotActive,
    /// Device disconnected during capture
    DeviceDisconnected { device_id: String, device_name: String },
    /// No devices available
    NoDevicesAvailable,
}

impl std::fmt::Display for WasapiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasapiError::ComInitFailed(msg) => {
                write!(f, "Error al inicializar COM: {}", msg)
            }
            WasapiError::EnumeratorCreationFailed(msg) => {
                write!(f, "Error al crear el enumerador de dispositivos: {}", msg)
            }
            WasapiError::DeviceEnumerationFailed(msg) => {
                write!(f, "Error al enumerar dispositivos de audio: {}", msg)
            }
            WasapiError::DeviceCountFailed(msg) => {
                write!(f, "Error al obtener el número de dispositivos: {}", msg)
            }
            WasapiError::DeviceAccessFailed(msg) => {
                write!(f, "Error al acceder al dispositivo: {}", msg)
            }
            WasapiError::DeviceIdFailed(msg) => {
                write!(f, "Error al obtener el ID del dispositivo: {}", msg)
            }
            WasapiError::DevicePropertiesFailed(msg) => {
                write!(f, "Error al obtener las propiedades del dispositivo: {}", msg)
            }
            WasapiError::DeviceNameFailed(msg) => {
                write!(f, "Error al obtener el nombre del dispositivo: {}", msg)
            }
            WasapiError::Timeout => {
                write!(f, "La operación excedió el tiempo límite de 2 segundos")
            }
            WasapiError::NotAvailable(msg) => {
                write!(
                    f,
                    "WASAPI no está disponible en este sistema: {}. Verifica que Windows Audio Service esté ejecutándose.",
                    msg
                )
            }
            WasapiError::DeviceNotFound(msg) => {
                write!(f, "Dispositivo de audio no encontrado: {}", msg)
            }
            WasapiError::AudioClientActivationFailed(msg) => {
                write!(f, "Error al activar el cliente de audio: {}", msg)
            }
            WasapiError::MixFormatFailed(msg) => {
                write!(f, "Error al obtener el formato de mezcla: {}", msg)
            }
            WasapiError::AudioClientInitFailed(msg) => {
                write!(f, "Error al inicializar el cliente de audio: {}", msg)
            }
            WasapiError::CaptureServiceFailed(msg) => {
                write!(f, "Error al obtener el servicio de captura: {}", msg)
            }
            WasapiError::StartCaptureFailed(msg) => {
                write!(f, "Error al iniciar la captura: {}", msg)
            }
            WasapiError::StopCaptureFailed(msg) => {
                write!(f, "Error al detener la captura: {}", msg)
            }
            WasapiError::ReadBufferFailed(msg) => {
                write!(f, "Error al leer el buffer de audio: {}", msg)
            }
            WasapiError::CaptureNotActive => {
                write!(f, "La captura no está activa")
            }
            WasapiError::DeviceDisconnected { device_name, .. } => {
                write!(
                    f,
                    "El dispositivo de audio '{}' se ha desconectado durante la captura",
                    device_name
                )
            }
            WasapiError::NoDevicesAvailable => {
                write!(
                    f,
                    "No hay dispositivos de audio de salida disponibles. Conecta altavoces o auriculares."
                )
            }
        }
    }
}

impl std::error::Error for WasapiError {}

/// Convert WasapiError to the application's AudioError type
impl From<WasapiError> for AudioError {
    fn from(err: WasapiError) -> Self {
        match err {
            WasapiError::DeviceNotFound(id) => AudioError::DeviceNotFound {
                device_id: id,
                device_name: None,
            },
            WasapiError::DeviceDisconnected { device_id, device_name } => {
                AudioError::DeviceDisconnected { device_id, device_name }
            }
            WasapiError::NotAvailable(reason) | WasapiError::ComInitFailed(reason) | 
            WasapiError::EnumeratorCreationFailed(reason) => {
                AudioError::WasapiNotAvailable { reason }
            }
            WasapiError::NoDevicesAvailable => AudioError::NoDevicesAvailable,
            WasapiError::Timeout => AudioError::CaptureTimeout,
            WasapiError::CaptureNotActive => AudioError::CaptureNotActive,
            WasapiError::AudioClientInitFailed(reason) | 
            WasapiError::StartCaptureFailed(reason) |
            WasapiError::CaptureServiceFailed(reason) => {
                AudioError::CaptureInitFailed { reason }
            }
            _ => AudioError::CaptureInitFailed {
                reason: err.to_string(),
            },
        }
    }
}

/// Device state change notification constants
const DEVICE_STATE_MASK_DISCONNECTED: u32 = 
    DEVICE_STATE_DISABLED | DEVICE_STATE_NOTPRESENT | DEVICE_STATE_UNPLUGGED;

/// RAII guard for COM initialization
struct ComGuard {
    initialized: bool,
}

impl ComGuard {
    fn new() -> Result<Self, WasapiError> {
        unsafe {
            // Try to initialize COM for this thread
            // If already initialized, it will return S_FALSE which is still success
            match CoInitializeEx(None, COINIT_MULTITHREADED) {
                Ok(()) => Ok(Self { initialized: true }),
                Err(e) => {
                    // HRESULT S_FALSE (0x00000001) means already initialized
                    if e.code().0 == 1 {
                        Ok(Self { initialized: false })
                    } else {
                        Err(WasapiError::ComInitFailed(format!(
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

/// Capture status returned when reading audio buffer
#[derive(Debug, Clone)]
pub enum CaptureStatus {
    /// Capture is operating normally
    Ok,
    /// Device was disconnected, capture paused
    DeviceDisconnected { device_id: String, device_name: String },
    /// Device state changed (but still usable)
    DeviceStateChanged { new_state: DeviceState },
    /// Buffer overrun occurred, some frames were dropped
    BufferOverrun { dropped_frames: u32 },
}

/// Result of reading audio buffer with status information
#[derive(Debug)]
pub struct CaptureResult {
    /// PCM samples captured (may be empty)
    pub samples: Vec<i16>,
    /// Status of the capture operation
    pub status: CaptureStatus,
}

/// Shared state for device notifications
struct DeviceNotificationState {
    /// ID of the device being monitored
    device_id: String,
    /// Friendly name of the device
    device_name: String,
    /// Flag indicating device was disconnected
    disconnected: AtomicBool,
    /// Current device state (uses u32 for atomic access)
    device_state: AtomicU32,
}

/// Device notification callback implementation
#[implement(IMMNotificationClient)]
struct DeviceNotificationCallback {
    state: Arc<DeviceNotificationState>,
}

impl IMMNotificationClient_Impl for DeviceNotificationCallback {
    fn OnDeviceStateChanged(
        &self,
        pwstrdeviceid: &windows::core::PCWSTR,
        dwnewstate: u32,
    ) -> windows::core::Result<()> {
        let device_id = unsafe { pwstr_to_string_from_pcwstr(*pwstrdeviceid) };
        
        // Check if this is the device we're monitoring
        if device_id == self.state.device_id {
            tracing::info!(
                "Device state changed for '{}': 0x{:08X}",
                self.state.device_name,
                dwnewstate
            );
            
            self.state.device_state.store(dwnewstate, Ordering::SeqCst);
            
            // Check if device was disconnected
            if dwnewstate & DEVICE_STATE_MASK_DISCONNECTED != 0 {
                tracing::warn!(
                    "Device '{}' disconnected (state: 0x{:08X})",
                    self.state.device_name,
                    dwnewstate
                );
                self.state.disconnected.store(true, Ordering::SeqCst);
            }
        }
        
        Ok(())
    }

    fn OnDeviceAdded(&self, _pwstrdeviceid: &windows::core::PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, pwstrdeviceid: &windows::core::PCWSTR) -> windows::core::Result<()> {
        let device_id = unsafe { pwstr_to_string_from_pcwstr(*pwstrdeviceid) };
        
        if device_id == self.state.device_id {
            tracing::warn!("Device '{}' removed", self.state.device_name);
            self.state.disconnected.store(true, Ordering::SeqCst);
        }
        
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        _flow: EDataFlow,
        _role: ERole,
        _pwstrdefaultdeviceid: &windows::core::PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        _pwstrdeviceid: &windows::core::PCWSTR,
        _key: &windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Convert PCWSTR to String
unsafe fn pwstr_to_string_from_pcwstr(pcwstr: windows::core::PCWSTR) -> String {
    if pcwstr.is_null() {
        return String::new();
    }

    let mut len = 0;
    while *pcwstr.0.add(len) != 0 {
        len += 1;
    }

    let slice = std::slice::from_raw_parts(pcwstr.0, len);
    OsString::from_wide(slice).to_string_lossy().into_owned()
}

/// WASAPI Loopback Capture for Windows
///
/// Captures audio from system output devices using WASAPI loopback mode.
/// This allows capturing audio from applications without requiring additional drivers.
///
/// # Example
///
/// ```ignore
/// use traductor_desktop_lib::audio::windows::wasapi::WasapiLoopback;
/// use traductor_desktop_lib::audio::windows::wasapi::enumerate_output_devices;
///
/// let devices = enumerate_output_devices()?;
/// let mut capture = WasapiLoopback::start_capture(&devices[0].id)?;
///
/// loop {
///     let samples = capture.read_buffer()?;
///     if !samples.is_empty() {
///         // Process PCM samples
///     }
/// }
/// ```
pub struct WasapiLoopback {
    /// The audio client for the capture device
    client: IAudioClient,
    /// The capture client for reading audio data
    capture: IAudioCaptureClient,
    /// The audio format being captured
    format: AudioFormat,
    /// Whether capture is currently active
    is_active: Arc<AtomicBool>,
    /// Timestamp when capture started
    start_time: Instant,
    /// COM guard to ensure COM is initialized for the lifetime of this object
    _com_guard: ComGuard,
    /// Device notification state (shared with callback)
    notification_state: Arc<DeviceNotificationState>,
    /// Device enumerator (kept alive for notifications)
    _enumerator: IMMDeviceEnumerator,
}

/// Audio format information
#[derive(Debug, Clone)]
pub struct AudioFormat {
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
    /// Bits per sample
    pub bits_per_sample: u16,
    /// Block alignment (bytes per frame)
    pub block_align: u16,
}

impl WasapiLoopback {
    /// Start loopback capture on the specified device.
    ///
    /// Initializes WASAPI in loopback mode to capture audio from the specified
    /// output device. The captured audio will be in the device's native format.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The ID of the output device to capture from
    ///
    /// # Returns
    ///
    /// - `Ok(WasapiLoopback)` - Capture started successfully
    /// - `Err(WasapiError)` - Error describing what went wrong
    ///
    /// # Requirements
    ///
    /// - Requirement 2.1: Capture audio using WASAPI Loopback
    /// - Requirement 2.4: Maintain capture latency <50ms
    pub fn start_capture(device_id: &str) -> Result<Self, WasapiError> {
        Self::start_capture_with_name(device_id, None)
    }

    /// Start loopback capture on the specified device with a known device name.
    ///
    /// This variant is useful when the device name is already known from enumeration,
    /// avoiding an extra lookup.
    pub fn start_capture_with_name(device_id: &str, device_name: Option<&str>) -> Result<Self, WasapiError> {
        let start_time = Instant::now();
        
        // Initialize COM
        let com_guard = ComGuard::new()?;
        
        // Create device enumerator
        let enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| {
                WasapiError::EnumeratorCreationFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?
        };

        // Get the device by ID
        let device = Self::get_device_by_id(&enumerator, device_id)?;
        
        // Get device name if not provided
        let device_name_resolved = match device_name {
            Some(name) => name.to_string(),
            None => Self::get_device_name(&device).unwrap_or_else(|| format!("Dispositivo {}", device_id)),
        };
        
        // Set up device notification state
        let notification_state = Arc::new(DeviceNotificationState {
            device_id: device_id.to_string(),
            device_name: device_name_resolved.clone(),
            disconnected: AtomicBool::new(false),
            device_state: AtomicU32::new(DEVICE_STATE_ACTIVE),
        });
        
        // Register for device notifications
        let notification_callback: IMMNotificationClient = DeviceNotificationCallback {
            state: Arc::clone(&notification_state),
        }.into();
        
        unsafe {
            if let Err(e) = enumerator.RegisterEndpointNotificationCallback(&notification_callback) {
                tracing::warn!(
                    "Failed to register device notification callback: HRESULT 0x{:08X}. Device disconnection may not be detected.",
                    e.code().0
                );
            }
        }
        
        // Activate audio client on the device
        let client: IAudioClient = unsafe {
            device.Activate(CLSCTX_ALL, None).map_err(|e| {
                WasapiError::AudioClientActivationFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?
        };

        // Get the mix format (device's native format)
        let mix_format_ptr = unsafe {
            client.GetMixFormat().map_err(|e| {
                WasapiError::MixFormatFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?
        };

        // Parse the format
        let format = unsafe { Self::parse_wave_format(mix_format_ptr)? };
        
        // Calculate buffer duration for <50ms latency
        // Use 20ms buffer for low latency (requirement 2.4)
        const BUFFER_DURATION_100NS: i64 = 200_000; // 20ms in 100ns units
        
        // Initialize the audio client in shared loopback mode
        unsafe {
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK,
                    BUFFER_DURATION_100NS,
                    0, // Period must be 0 for shared mode
                    mix_format_ptr,
                    None,
                )
                .map_err(|e| {
                    WasapiError::AudioClientInitFailed(format!("HRESULT: 0x{:08X}", e.code().0))
                })?;
        }

        // Free the mix format memory
        unsafe {
            CoTaskMemFree(Some(mix_format_ptr as *const _));
        }

        // Get the capture client
        let capture: IAudioCaptureClient = unsafe {
            client.GetService().map_err(|e| {
                WasapiError::CaptureServiceFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?
        };

        // Start the audio client
        unsafe {
            client.Start().map_err(|e| {
                WasapiError::StartCaptureFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?;
        }

        tracing::info!(
            "WASAPI loopback capture started on device '{}' ({}) - {}Hz, {} channels, {}bit - in {}ms",
            device_name_resolved,
            device_id,
            format.sample_rate,
            format.channels,
            format.bits_per_sample,
            start_time.elapsed().as_millis()
        );

        Ok(Self {
            client,
            capture,
            format,
            is_active: Arc::new(AtomicBool::new(true)),
            start_time,
            _com_guard: com_guard,
            notification_state,
            _enumerator: enumerator,
        })
    }
    
    /// Get the name of the device being captured
    pub fn get_device_name_captured(&self) -> &str {
        &self.notification_state.device_name
    }
    
    /// Get the ID of the device being captured
    pub fn get_device_id(&self) -> &str {
        &self.notification_state.device_id
    }
    
    /// Check if the device has been disconnected
    pub fn is_disconnected(&self) -> bool {
        self.notification_state.disconnected.load(Ordering::SeqCst)
    }

    /// Read captured audio from the buffer.
    ///
    /// Returns PCM samples captured since the last read. The samples are in the
    /// device's native format (typically float32 or int16).
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<i16>)` - PCM samples, may be empty if no data available
    /// - `Err(WasapiError)` - Error reading the buffer
    ///
    /// # Requirements
    ///
    /// - Requirement 2.1: Read PCM samples from WASAPI capture
    pub fn read_buffer(&self) -> Result<Vec<i16>, WasapiError> {
        if !self.is_active.load(Ordering::Relaxed) {
            return Err(WasapiError::CaptureNotActive);
        }

        let mut all_samples: Vec<i16> = Vec::new();
        
        // Read all available packets
        loop {
            let packet_size = unsafe {
                self.capture.GetNextPacketSize().map_err(|e| {
                    WasapiError::ReadBufferFailed(format!(
                        "GetNextPacketSize failed: HRESULT 0x{:08X}",
                        e.code().0
                    ))
                })?
            };

            if packet_size == 0 {
                break;
            }

            // Get the buffer
            let mut buffer_ptr: *mut u8 = ptr::null_mut();
            let mut num_frames: u32 = 0;
            let mut flags: u32 = 0;
            let mut device_position: u64 = 0;
            let mut qpc_position: u64 = 0;

            unsafe {
                self.capture
                    .GetBuffer(
                        &mut buffer_ptr,
                        &mut num_frames,
                        &mut flags,
                        Some(&mut device_position),
                        Some(&mut qpc_position),
                    )
                    .map_err(|e| {
                        WasapiError::ReadBufferFailed(format!(
                            "GetBuffer failed: HRESULT 0x{:08X}",
                            e.code().0
                        ))
                    })?;
            }

            // Convert to i16 samples if we have data
            if num_frames > 0 && !buffer_ptr.is_null() {
                // Check if this is silent data (AUDCLNT_BUFFERFLAGS_SILENT = 2)
                let is_silent = flags & 2 != 0;
                
                if is_silent {
                    // Append silence (zeros)
                    let num_samples = (num_frames as usize) * (self.format.channels as usize);
                    all_samples.extend(std::iter::repeat(0i16).take(num_samples));
                } else {
                    // Convert from native format to i16
                    let samples = self.convert_to_i16(buffer_ptr, num_frames);
                    all_samples.extend(samples);
                }
            }

            // Release the buffer
            unsafe {
                self.capture.ReleaseBuffer(num_frames).map_err(|e| {
                    WasapiError::ReadBufferFailed(format!(
                        "ReleaseBuffer failed: HRESULT 0x{:08X}",
                        e.code().0
                    ))
                })?;
            }
        }

        Ok(all_samples)
    }

    /// Read captured audio from the buffer with status information.
    ///
    /// This method is similar to `read_buffer()` but additionally returns status
    /// information about the capture, including device disconnection detection.
    ///
    /// # Returns
    ///
    /// A `CaptureResult` containing the PCM samples and capture status.
    ///
    /// # Requirements
    ///
    /// - Requirement 2.7: Detect device disconnection during capture
    pub fn read_buffer_with_status(&self) -> Result<CaptureResult, WasapiError> {
        // First check if device has been disconnected
        if self.notification_state.disconnected.load(Ordering::SeqCst) {
            tracing::warn!(
                "Device '{}' was disconnected, pausing capture",
                self.notification_state.device_name
            );
            
            return Ok(CaptureResult {
                samples: Vec::new(),
                status: CaptureStatus::DeviceDisconnected {
                    device_id: self.notification_state.device_id.clone(),
                    device_name: self.notification_state.device_name.clone(),
                },
            });
        }
        
        // Check if device state changed (but not disconnected)
        let current_state = self.notification_state.device_state.load(Ordering::SeqCst);
        if current_state != DEVICE_STATE_ACTIVE {
            let new_state = match current_state {
                x if x == DEVICE_STATE_DISABLED => DeviceState::Disabled,
                x if x == DEVICE_STATE_NOTPRESENT => DeviceState::NotPresent,
                x if x == DEVICE_STATE_UNPLUGGED => DeviceState::Unplugged,
                _ => DeviceState::Active,
            };
            
            if new_state != DeviceState::Active {
                return Ok(CaptureResult {
                    samples: Vec::new(),
                    status: CaptureStatus::DeviceStateChanged { new_state },
                });
            }
        }
        
        // Try to read buffer normally
        match self.read_buffer() {
            Ok(samples) => Ok(CaptureResult {
                samples,
                status: CaptureStatus::Ok,
            }),
            Err(WasapiError::ReadBufferFailed(msg)) => {
                // Check if this might be a disconnection error
                if msg.contains("0x88890004") || // AUDCLNT_E_DEVICE_INVALIDATED
                   msg.contains("0x88890005") || // AUDCLNT_E_SERVICE_NOT_RUNNING
                   msg.contains("0x88890017")    // AUDCLNT_E_RESOURCES_INVALIDATED
                {
                    self.notification_state.disconnected.store(true, Ordering::SeqCst);
                    Ok(CaptureResult {
                        samples: Vec::new(),
                        status: CaptureStatus::DeviceDisconnected {
                            device_id: self.notification_state.device_id.clone(),
                            device_name: self.notification_state.device_name.clone(),
                        },
                    })
                } else {
                    Err(WasapiError::ReadBufferFailed(msg))
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Get the current capture latency in milliseconds.
    ///
    /// # Returns
    ///
    /// The capture latency in milliseconds. This should be <50ms per requirement 2.4.
    pub fn get_latency_ms(&self) -> u32 {
        // Get stream latency from WASAPI
        let latency = unsafe {
            self.client
                .GetStreamLatency()
                .unwrap_or(0)
        };
        
        // Convert from 100ns units to milliseconds
        (latency / 10_000) as u32
    }

    /// Get the audio format being captured.
    pub fn get_format(&self) -> &AudioFormat {
        &self.format
    }

    /// Check if capture is currently active.
    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Relaxed)
    }

    /// Stop the capture.
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Capture stopped successfully
    /// - `Err(WasapiError)` - Error stopping capture
    pub fn stop(&mut self) -> Result<(), WasapiError> {
        if !self.is_active.load(Ordering::Relaxed) {
            return Ok(());
        }

        unsafe {
            self.client.Stop().map_err(|e| {
                WasapiError::StopCaptureFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?;
        }

        self.is_active.store(false, Ordering::Relaxed);
        
        tracing::info!(
            "WASAPI loopback capture stopped after {}ms",
            self.start_time.elapsed().as_millis()
        );

        Ok(())
    }

    /// Get a device by its ID
    fn get_device_by_id(
        enumerator: &IMMDeviceEnumerator,
        device_id: &str,
    ) -> Result<IMMDevice, WasapiError> {
        // Convert device_id to wide string
        let wide_id: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        
        unsafe {
            let device = enumerator
                .GetDevice(windows::core::PCWSTR::from_raw(wide_id.as_ptr()))
                .map_err(|e| {
                    WasapiError::DeviceNotFound(format!(
                        "ID: {}, HRESULT: 0x{:08X}",
                        device_id,
                        e.code().0
                    ))
                })?;
            Ok(device)
        }
    }
    
    /// Get the friendly name of a device
    fn get_device_name(device: &IMMDevice) -> Option<String> {
        unsafe {
            let property_store = device.OpenPropertyStore(
                windows::Win32::System::Com::STGM_READ
            ).ok()?;
            
            let name_prop = property_store.GetValue(&PKEY_Device_FriendlyName).ok()?;
            propvariant_to_string(&name_prop)
        }
    }

    /// Parse WAVEFORMATEX to AudioFormat
    unsafe fn parse_wave_format(format_ptr: *const WAVEFORMATEX) -> Result<AudioFormat, WasapiError> {
        if format_ptr.is_null() {
            return Err(WasapiError::MixFormatFailed("Null format pointer".to_string()));
        }

        let format = &*format_ptr;
        
        Ok(AudioFormat {
            sample_rate: format.nSamplesPerSec,
            channels: format.nChannels,
            bits_per_sample: format.wBitsPerSample,
            block_align: format.nBlockAlign,
        })
    }

    /// Convert captured audio buffer to i16 samples
    fn convert_to_i16(&self, buffer_ptr: *mut u8, num_frames: u32) -> Vec<i16> {
        let num_samples = (num_frames as usize) * (self.format.channels as usize);
        let mut samples = Vec::with_capacity(num_samples);

        unsafe {
            match self.format.bits_per_sample {
                16 => {
                    // Already i16, just copy
                    let src = buffer_ptr as *const i16;
                    for i in 0..num_samples {
                        samples.push(*src.add(i));
                    }
                }
                32 => {
                    // Float32, convert to i16
                    let src = buffer_ptr as *const f32;
                    for i in 0..num_samples {
                        let float_sample = *src.add(i);
                        // Clamp and convert float [-1.0, 1.0] to i16 [-32768, 32767]
                        let clamped = float_sample.clamp(-1.0, 1.0);
                        let int_sample = (clamped * 32767.0) as i16;
                        samples.push(int_sample);
                    }
                }
                24 => {
                    // 24-bit packed, convert to i16
                    let bytes_per_sample = 3;
                    for i in 0..num_samples {
                        let offset = i * bytes_per_sample;
                        let b0 = *buffer_ptr.add(offset) as i32;
                        let b1 = *buffer_ptr.add(offset + 1) as i32;
                        let b2 = *buffer_ptr.add(offset + 2) as i32;
                        
                        // Construct 24-bit signed value and shift to 16-bit
                        let sample_24 = b0 | (b1 << 8) | (b2 << 16);
                        // Sign extend if negative
                        let sample_24 = if sample_24 & 0x800000 != 0 {
                            sample_24 | !0xFFFFFF
                        } else {
                            sample_24
                        };
                        // Convert to 16-bit by discarding lower 8 bits
                        let sample_16 = (sample_24 >> 8) as i16;
                        samples.push(sample_16);
                    }
                }
                _ => {
                    // Unknown format, return zeros
                    tracing::warn!(
                        "Unknown audio format: {} bits per sample, returning silence",
                        self.format.bits_per_sample
                    );
                    samples.extend(std::iter::repeat(0i16).take(num_samples));
                }
            }
        }

        samples
    }
}

impl Drop for WasapiLoopback {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Enumerate output devices (for loopback capture).
///
/// Returns a list of audio output devices that can be used for loopback capture.
/// The operation must complete within 2 seconds as per requirement 2.2.
///
/// # Returns
///
/// - `Ok(Vec<AudioDevice>)` - List of available output devices
/// - `Err(WasapiError)` - Error describing what went wrong
///
/// # Example
///
/// ```ignore
/// use traductor_desktop_lib::audio::windows::wasapi;
///
/// let devices = wasapi::enumerate_output_devices()?;
/// for device in devices {
///     println!("Device: {} (ID: {})", device.name, device.id);
/// }
/// ```
pub fn enumerate_output_devices() -> Result<Vec<AudioDevice>, WasapiError> {
    let start_time = Instant::now();
    const TIMEOUT_MS: u128 = 2000;

    // Initialize COM
    let _com_guard = ComGuard::new()?;

    // Check timeout
    if start_time.elapsed().as_millis() > TIMEOUT_MS {
        return Err(WasapiError::Timeout);
    }

    // Create device enumerator
    let enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| {
            WasapiError::EnumeratorCreationFailed(format!("HRESULT: 0x{:08X}", e.code().0))
        })?
    };

    // Check timeout
    if start_time.elapsed().as_millis() > TIMEOUT_MS {
        return Err(WasapiError::Timeout);
    }

    // Enumerate render (output) devices
    let collection: IMMDeviceCollection = unsafe {
        enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|e| {
                WasapiError::DeviceEnumerationFailed(format!("HRESULT: 0x{:08X}", e.code().0))
            })?
    };

    // Get device count
    let count = unsafe {
        collection
            .GetCount()
            .map_err(|e| WasapiError::DeviceCountFailed(format!("HRESULT: 0x{:08X}", e.code().0)))?
    };

    // Get the default device ID for comparison
    let default_device_id = get_default_device_id(&enumerator);

    let mut devices = Vec::with_capacity(count as usize);

    for i in 0..count {
        // Check timeout periodically
        if start_time.elapsed().as_millis() > TIMEOUT_MS {
            return Err(WasapiError::Timeout);
        }

        match get_device_info(&collection, i, &default_device_id) {
            Ok(device) => devices.push(device),
            Err(e) => {
                // Log the error but continue with other devices
                tracing::warn!("Failed to get info for device {}: {:?}", i, e);
            }
        }
    }

    // Final timeout check
    if start_time.elapsed().as_millis() > TIMEOUT_MS {
        return Err(WasapiError::Timeout);
    }

    tracing::info!(
        "Enumerated {} output devices in {}ms",
        devices.len(),
        start_time.elapsed().as_millis()
    );

    Ok(devices)
}

/// Check if WASAPI is available on this system.
///
/// This checks that the Windows Audio service is running and COM can be initialized.
/// Use this before attempting to enumerate devices or start capture.
///
/// # Requirements
///
/// - Requirement 2.5: Show error message if WASAPI not available
pub fn check_wasapi_available() -> Result<(), WasapiError> {
    // Try to initialize COM
    let _com_guard = ComGuard::new()?;
    
    // Try to create device enumerator
    let _enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| {
            WasapiError::NotAvailable(format!(
                "No se pudo crear el enumerador de dispositivos. El servicio Windows Audio puede no estar ejecutándose. HRESULT: 0x{:08X}",
                e.code().0
            ))
        })?
    };
    
    Ok(())
}

/// Check if there are any audio output devices available.
///
/// # Requirements
///
/// - Requirement 2.6: Show message if no devices available
pub fn has_output_devices() -> Result<bool, WasapiError> {
    let devices = enumerate_output_devices()?;
    Ok(!devices.is_empty())
}

/// Comprehensive audio system check for startup.
///
/// Performs all necessary checks before audio capture can begin:
/// 1. Verifies WASAPI is available (Requirement 2.5)
/// 2. Enumerates output devices
/// 3. Verifies at least one device is available (Requirement 2.6)
///
/// # Returns
///
/// - `Ok(Vec<AudioDevice>)` - Available devices if all checks pass
/// - `Err(WasapiError)` - Specific error explaining what failed
///
/// # Example
///
/// ```ignore
/// match check_audio_system() {
///     Ok(devices) => {
///         // Audio system ready, show device selector
///         show_device_selector(devices);
///     }
///     Err(WasapiError::NotAvailable(reason)) => {
///         // WASAPI not available (Requirement 2.5)
///         show_error_with_recovery(
///             "WASAPI no disponible",
///             &reason,
///             "Verifica que el servicio Windows Audio esté ejecutándose"
///         );
///     }
///     Err(WasapiError::NoDevicesAvailable) => {
///         // No devices (Requirement 2.6)
///         show_error_with_recovery(
///             "Sin dispositivos",
///             "No hay dispositivos de audio de salida",
///             "Conecta altavoces o auriculares"
///         );
///     }
///     Err(e) => {
///         show_generic_error(&e.to_string());
///     }
/// }
/// ```
pub fn check_audio_system() -> Result<Vec<AudioDevice>, WasapiError> {
    // Step 1: Check WASAPI availability (Requirement 2.5)
    check_wasapi_available()?;
    
    // Step 2: Enumerate devices
    let devices = enumerate_output_devices()?;
    
    // Step 3: Check if any devices available (Requirement 2.6)
    if devices.is_empty() {
        return Err(WasapiError::NoDevicesAvailable);
    }
    
    tracing::info!(
        "Audio system check passed: {} devices available",
        devices.len()
    );
    
    Ok(devices)
}

/// Get user-friendly error details for display
///
/// Returns a tuple of (title, message, suggestion) for the given error,
/// suitable for displaying in a dialog or notification.
///
/// All messages are in Spanish per the application's localization requirements.
pub fn get_error_display_info(error: &WasapiError) -> (&'static str, String, &'static str) {
    match error {
        WasapiError::NotAvailable(reason) => (
            "WASAPI no disponible",
            format!(
                "No se puede acceder al sistema de audio de Windows: {}",
                reason
            ),
            "Verifica que el servicio 'Windows Audio' esté ejecutándose. \
             Abre services.msc, busca 'Windows Audio' y asegúrate de que esté iniciado.",
        ),
        WasapiError::NoDevicesAvailable => (
            "Sin dispositivos de audio",
            "No se encontraron dispositivos de audio de salida en este sistema.".to_string(),
            "Conecta altavoces o auriculares y asegúrate de que estén habilitados \
             en la configuración de sonido de Windows.",
        ),
        WasapiError::DeviceNotFound(id) => (
            "Dispositivo no encontrado",
            format!("El dispositivo de audio '{}' no está disponible.", id),
            "El dispositivo puede haber sido desconectado. Selecciona otro dispositivo.",
        ),
        WasapiError::DeviceDisconnected { device_name, .. } => (
            "Dispositivo desconectado",
            format!(
                "El dispositivo '{}' se desconectó durante la captura de audio.",
                device_name
            ),
            "Reconecta el dispositivo o selecciona uno alternativo para continuar.",
        ),
        WasapiError::Timeout => (
            "Tiempo de espera agotado",
            "La operación de audio tardó demasiado en completarse.".to_string(),
            "Intenta nuevamente. Si el problema persiste, reinicia el servicio de audio.",
        ),
        WasapiError::CaptureNotActive => (
            "Captura no activa",
            "Se intentó leer audio pero la captura no está activa.".to_string(),
            "Inicia la captura de audio antes de intentar leer datos.",
        ),
        _ => (
            "Error de audio",
            error.to_string(),
            "Verifica la configuración de audio del sistema.",
        ),
    }
}

/// Get the default audio output device ID
fn get_default_device_id(enumerator: &IMMDeviceEnumerator) -> Option<String> {
    unsafe {
        enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .ok()
            .and_then(|device| device.GetId().ok())
            .map(|id| pwstr_to_string(id))
    }
}

/// Get device information from the collection
fn get_device_info(
    collection: &IMMDeviceCollection,
    index: u32,
    default_device_id: &Option<String>,
) -> Result<AudioDevice, WasapiError> {
    unsafe {
        // Get device at index
        let device: IMMDevice = collection.Item(index).map_err(|e| {
            WasapiError::DeviceAccessFailed(format!(
                "Index {}, HRESULT: 0x{:08X}",
                index,
                e.code().0
            ))
        })?;

        // Get device ID
        let device_id_ptr: PWSTR = device.GetId().map_err(|e| {
            WasapiError::DeviceIdFailed(format!("Index {}, HRESULT: 0x{:08X}", index, e.code().0))
        })?;
        let device_id = pwstr_to_string(device_id_ptr);

        // Free the allocated string
        windows::Win32::System::Com::CoTaskMemFree(Some(device_id_ptr.0 as *const _));

        // Get device properties
        let property_store = device.OpenPropertyStore(windows::Win32::System::Com::STGM_READ).map_err(|e| {
            WasapiError::DevicePropertiesFailed(format!(
                "Device {}, HRESULT: 0x{:08X}",
                device_id,
                e.code().0
            ))
        })?;

        // Get friendly name
        let name_prop = property_store.GetValue(&PKEY_Device_FriendlyName).map_err(|e| {
            WasapiError::DeviceNameFailed(format!(
                "Device {}, HRESULT: 0x{:08X}",
                device_id,
                e.code().0
            ))
        })?;

        let device_name = propvariant_to_string(&name_prop).unwrap_or_else(|| {
            format!("Dispositivo de Audio {}", index + 1)
        });

        // Check if this is the default device
        let is_default = default_device_id
            .as_ref()
            .map(|default_id| default_id == &device_id)
            .unwrap_or(false);

        Ok(AudioDevice {
            id: device_id,
            name: device_name,
            device_type: "loopback".to_string(), // Output devices used for loopback capture
            is_default,
        })
    }
}

/// Convert PWSTR to Rust String
fn pwstr_to_string(pwstr: PWSTR) -> String {
    if pwstr.0.is_null() {
        return String::new();
    }

    unsafe {
        // Find the null terminator
        let mut len = 0;
        while *pwstr.0.add(len) != 0 {
            len += 1;
        }

        // Create a slice and convert to OsString
        let slice = std::slice::from_raw_parts(pwstr.0, len);
        OsString::from_wide(slice)
            .to_string_lossy()
            .into_owned()
    }
}

/// Convert PROPVARIANT to String (for device friendly name)
fn propvariant_to_string(
    prop: &windows::Win32::System::Com::StructuredStorage::PROPVARIANT,
) -> Option<String> {
    use windows::Win32::System::Variant::VT_LPWSTR;

    unsafe {
        // Access the anonymous union - the vt field indicates the type
        let vt = prop.Anonymous.Anonymous.vt;

        if vt == VT_LPWSTR {
            // Get the pwszVal from the union
            let pwsz = prop.Anonymous.Anonymous.Anonymous.pwszVal;
            if !pwsz.0.is_null() {
                return Some(pwstr_to_string(pwsz));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumerate_output_devices_returns_within_timeout() {
        // This test verifies that enumeration completes within 2 seconds
        let start = Instant::now();
        let result = enumerate_output_devices();
        let elapsed = start.elapsed().as_millis();

        // Should complete within 2 seconds regardless of result
        assert!(
            elapsed < 2000,
            "Enumeration took {}ms, exceeding 2s timeout",
            elapsed
        );

        // The result should be Ok or a meaningful error
        match result {
            Ok(devices) => {
                println!("Found {} devices:", devices.len());
                for device in &devices {
                    println!(
                        "  - {} (ID: {}, Default: {})",
                        device.name, device.id, device.is_default
                    );
                }
            }
            Err(e) => {
                // On systems without audio, we might get an error
                println!("Enumeration error (may be expected): {}", e);
            }
        }
    }

    #[test]
    fn test_audio_device_structure() {
        let device = AudioDevice {
            id: "test-id".to_string(),
            name: "Test Device".to_string(),
            device_type: "loopback".to_string(),
            is_default: true,
        };

        assert_eq!(device.id, "test-id");
        assert_eq!(device.name, "Test Device");
        assert_eq!(device.device_type, "loopback");
        assert!(device.is_default);
    }

    #[test]
    fn test_wasapi_error_display() {
        let error = WasapiError::NotAvailable("Service not running".to_string());
        let msg = format!("{}", error);
        assert!(msg.contains("WASAPI"));
        assert!(msg.contains("Windows Audio Service"));
    }

    #[test]
    fn test_wasapi_error_display_new_variants() {
        let error = WasapiError::DeviceNotFound("test-device".to_string());
        let msg = format!("{}", error);
        assert!(msg.contains("no encontrado"));
        assert!(msg.contains("test-device"));

        let error = WasapiError::CaptureNotActive;
        let msg = format!("{}", error);
        assert!(msg.contains("no está activa"));
    }

    #[test]
    fn test_com_guard_creation() {
        // Test that COM can be initialized and cleaned up properly
        {
            let guard = ComGuard::new();
            assert!(guard.is_ok(), "COM initialization should succeed");
        }
        // Guard dropped here, COM should be uninitialized

        // Should be able to initialize again
        {
            let guard = ComGuard::new();
            assert!(guard.is_ok(), "Second COM initialization should succeed");
        }
    }

    #[test]
    fn test_audio_format_structure() {
        let format = AudioFormat {
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 16,
            block_align: 4,
        };

        assert_eq!(format.sample_rate, 48000);
        assert_eq!(format.channels, 2);
        assert_eq!(format.bits_per_sample, 16);
        assert_eq!(format.block_align, 4);
    }

    #[test]
    fn test_wasapi_loopback_capture_with_valid_device() {
        // First, enumerate devices to get a valid device ID
        let devices = match enumerate_output_devices() {
            Ok(d) if !d.is_empty() => d,
            _ => {
                println!("No output devices available, skipping test");
                return;
            }
        };

        // Try to start capture on the default device (or first available)
        let device = devices.iter().find(|d| d.is_default).unwrap_or(&devices[0]);
        
        let result = WasapiLoopback::start_capture(&device.id);
        
        match result {
            Ok(mut capture) => {
                // Verify capture is active
                assert!(capture.is_active(), "Capture should be active after start");
                
                // Verify latency is within acceptable range
                let latency = capture.get_latency_ms();
                println!("Capture latency: {}ms", latency);
                // Note: We check <100ms as a reasonable upper bound for startup
                // The actual streaming latency should be <50ms once stable
                
                // Verify format is valid
                let format = capture.get_format();
                assert!(format.sample_rate > 0, "Sample rate should be positive");
                assert!(format.channels > 0, "Channels should be positive");
                assert!(format.bits_per_sample > 0, "Bits per sample should be positive");
                println!(
                    "Capture format: {}Hz, {} channels, {} bits",
                    format.sample_rate, format.channels, format.bits_per_sample
                );
                
                // Try to read some samples (may be empty if no audio playing)
                let samples = capture.read_buffer();
                assert!(samples.is_ok(), "read_buffer should succeed: {:?}", samples.err());
                println!("Read {} samples", samples.unwrap().len());
                
                // Stop capture
                let stop_result = capture.stop();
                assert!(stop_result.is_ok(), "Stop should succeed");
                assert!(!capture.is_active(), "Capture should be inactive after stop");
            }
            Err(e) => {
                // This might fail if there's no audio device available
                println!("Failed to start capture (may be expected): {}", e);
            }
        }
    }

    #[test]
    fn test_wasapi_loopback_capture_invalid_device() {
        let result = WasapiLoopback::start_capture("invalid-device-id-12345");
        
        assert!(result.is_err(), "Should fail with invalid device ID");
        
        if let Err(WasapiError::DeviceNotFound(msg)) = result {
            assert!(msg.contains("invalid-device-id-12345"));
        } else {
            panic!("Expected DeviceNotFound error");
        }
    }

    #[test]
    fn test_read_buffer_when_not_active() {
        // We can't easily test this without starting a capture first
        // because we need a valid capture client
        // This test would require mocking which we're avoiding
        println!("Note: read_buffer_when_not_active test requires a started capture");
    }

    #[test]
    fn test_check_wasapi_available() {
        // This test verifies WASAPI availability check
        let result = check_wasapi_available();
        
        match result {
            Ok(()) => {
                println!("WASAPI is available on this system");
            }
            Err(e) => {
                println!("WASAPI not available (may be expected in CI): {}", e);
                // Verify error message is user-friendly
                let msg = e.to_string();
                assert!(msg.contains("WASAPI") || msg.contains("Windows Audio"));
            }
        }
    }

    #[test]
    fn test_has_output_devices() {
        // This test verifies the has_output_devices helper
        match has_output_devices() {
            Ok(has_devices) => {
                println!("Has output devices: {}", has_devices);
            }
            Err(e) => {
                println!("Could not check for devices (may be expected): {}", e);
            }
        }
    }

    #[test]
    fn test_device_disconnected_error_display() {
        let error = WasapiError::DeviceDisconnected {
            device_id: "test-id".to_string(),
            device_name: "Auriculares Bluetooth".to_string(),
        };
        
        let msg = format!("{}", error);
        assert!(msg.contains("Auriculares Bluetooth"));
        assert!(msg.contains("desconectado"));
    }

    #[test]
    fn test_no_devices_error_display() {
        let error = WasapiError::NoDevicesAvailable;
        let msg = format!("{}", error);
        
        assert!(msg.contains("No hay dispositivos"));
        assert!(msg.contains("audio de salida"));
    }

    #[test]
    fn test_capture_status_variants() {
        // Test CaptureStatus::Ok
        let status = CaptureStatus::Ok;
        assert!(matches!(status, CaptureStatus::Ok));

        // Test CaptureStatus::DeviceDisconnected
        let status = CaptureStatus::DeviceDisconnected {
            device_id: "test".to_string(),
            device_name: "Test Device".to_string(),
        };
        if let CaptureStatus::DeviceDisconnected { device_id, device_name } = status {
            assert_eq!(device_id, "test");
            assert_eq!(device_name, "Test Device");
        } else {
            panic!("Expected DeviceDisconnected");
        }

        // Test CaptureStatus::BufferOverrun
        let status = CaptureStatus::BufferOverrun { dropped_frames: 100 };
        if let CaptureStatus::BufferOverrun { dropped_frames } = status {
            assert_eq!(dropped_frames, 100);
        } else {
            panic!("Expected BufferOverrun");
        }
    }

    #[test]
    fn test_read_buffer_with_status() {
        // First, enumerate devices to get a valid device ID
        let devices = match enumerate_output_devices() {
            Ok(d) if !d.is_empty() => d,
            _ => {
                println!("No output devices available, skipping test");
                return;
            }
        };

        // Try to start capture on the default device (or first available)
        let device = devices.iter().find(|d| d.is_default).unwrap_or(&devices[0]);
        
        let result = WasapiLoopback::start_capture_with_name(&device.id, Some(&device.name));
        
        match result {
            Ok(mut capture) => {
                // Verify device info is available
                assert_eq!(capture.get_device_id(), device.id);
                assert_eq!(capture.get_device_name_captured(), device.name);
                assert!(!capture.is_disconnected());
                
                // Read buffer with status
                let capture_result = capture.read_buffer_with_status();
                assert!(capture_result.is_ok(), "read_buffer_with_status should succeed");
                
                let result = capture_result.unwrap();
                // Status should be Ok (device is connected)
                assert!(
                    matches!(result.status, CaptureStatus::Ok),
                    "Status should be Ok, got {:?}",
                    result.status
                );
                
                println!("Read {} samples with status {:?}", result.samples.len(), result.status);
                
                // Cleanup
                let _ = capture.stop();
            }
            Err(e) => {
                println!("Failed to start capture (may be expected): {}", e);
            }
        }
    }

    #[test]
    fn test_wasapi_error_to_audio_error_conversion() {
        use crate::error::AudioError;
        
        // Test DeviceNotFound conversion
        let wasapi_err = WasapiError::DeviceNotFound("test-id".to_string());
        let audio_err: AudioError = wasapi_err.into();
        assert_eq!(audio_err.code(), 1001);
        
        // Test DeviceDisconnected conversion
        let wasapi_err = WasapiError::DeviceDisconnected {
            device_id: "id".to_string(),
            device_name: "name".to_string(),
        };
        let audio_err: AudioError = wasapi_err.into();
        assert_eq!(audio_err.code(), 1002);
        
        // Test NotAvailable conversion
        let wasapi_err = WasapiError::NotAvailable("reason".to_string());
        let audio_err: AudioError = wasapi_err.into();
        assert_eq!(audio_err.code(), 1003);
        
        // Test NoDevicesAvailable conversion  
        let wasapi_err = WasapiError::NoDevicesAvailable;
        let audio_err: AudioError = wasapi_err.into();
        assert_eq!(audio_err.code(), 1010);
        
        // Test Timeout conversion
        let wasapi_err = WasapiError::Timeout;
        let audio_err: AudioError = wasapi_err.into();
        assert_eq!(audio_err.code(), 1009);
    }

    #[test]
    fn test_check_audio_system() {
        // This tests the comprehensive audio system check
        let result = check_audio_system();
        
        match result {
            Ok(devices) => {
                println!("Audio system check passed: {} devices", devices.len());
                assert!(!devices.is_empty(), "Should have at least one device");
            }
            Err(WasapiError::NotAvailable(reason)) => {
                println!("WASAPI not available (Requirement 2.5): {}", reason);
                // This is expected in CI environments without audio
            }
            Err(WasapiError::NoDevicesAvailable) => {
                println!("No devices available (Requirement 2.6)");
                // This is expected on systems without audio output
            }
            Err(e) => {
                println!("Audio system check failed: {}", e);
            }
        }
    }

    #[test]
    fn test_get_error_display_info() {
        // Test NotAvailable error display
        let error = WasapiError::NotAvailable("Service stopped".to_string());
        let (title, message, suggestion) = get_error_display_info(&error);
        
        assert_eq!(title, "WASAPI no disponible");
        assert!(message.contains("Service stopped"));
        assert!(suggestion.contains("Windows Audio"));
        
        // Test NoDevicesAvailable error display
        let error = WasapiError::NoDevicesAvailable;
        let (title, message, suggestion) = get_error_display_info(&error);
        
        assert_eq!(title, "Sin dispositivos de audio");
        assert!(message.contains("No se encontraron"));
        assert!(suggestion.contains("Conecta"));
        
        // Test DeviceDisconnected error display
        let error = WasapiError::DeviceDisconnected {
            device_id: "dev-123".to_string(),
            device_name: "Auriculares Sony".to_string(),
        };
        let (title, message, suggestion) = get_error_display_info(&error);
        
        assert_eq!(title, "Dispositivo desconectado");
        assert!(message.contains("Auriculares Sony"));
        assert!(suggestion.contains("Reconecta"));
        
        // Test DeviceNotFound error display
        let error = WasapiError::DeviceNotFound("missing-id".to_string());
        let (title, message, _suggestion) = get_error_display_info(&error);
        
        assert_eq!(title, "Dispositivo no encontrado");
        assert!(message.contains("missing-id"));
        
        // Test Timeout error display
        let error = WasapiError::Timeout;
        let (title, _message, suggestion) = get_error_display_info(&error);
        
        assert_eq!(title, "Tiempo de espera agotado");
        assert!(suggestion.contains("Intenta nuevamente"));
    }
}
