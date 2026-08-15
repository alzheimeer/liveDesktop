// Windows-specific audio modules
// Provides WASAPI loopback capture and VB-Cable integration

pub mod wasapi;
pub mod vbcable;

pub use wasapi::{
    AudioFormat, CaptureResult, CaptureStatus, WasapiError, WasapiLoopback,
    check_audio_system, check_wasapi_available, enumerate_output_devices,
    get_error_display_info, has_output_devices,
};
pub use vbcable::{
    VBCableError, VBCablePlayback, VBCableStatus,
    detect_and_register, detect_vbcable, get_cached_status,
    get_input_device_id, get_output_device_id, is_output_available,
    is_vbcable_installed, get_error_display_info as get_vbcable_error_display_info,
    VBCABLE_BITS_PER_SAMPLE, VBCABLE_BLOCK_ALIGN, VBCABLE_BYTES_PER_SECOND,
    VBCABLE_CHANNELS, VBCABLE_SAMPLE_RATE,
};
