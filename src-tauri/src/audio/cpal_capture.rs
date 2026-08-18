use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use crate::audio::resampler::resample_to_gemini_input;
use std::thread;

pub fn start_mic_capture(device_name: String, tx: mpsc::Sender<Vec<i16>>) -> Result<oneshot::Sender<()>, String> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    
    thread::spawn(move || {
        let host = cpal::default_host();
        
        // Find device
        let device_opt = if device_name == "default" {
            host.default_input_device()
        } else {
            host.input_devices()
                .ok()
                .and_then(|mut devices| devices.find(|x| x.name().unwrap_or_default() == device_name))
        };

        let device = match device_opt {
            Some(d) => d,
            None => {
                tracing::error!("Input device '{}' not found", device_name);
                return;
            }
        };
        
        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Error getting default config: {}", e);
                return;
            }
        };
        
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        
        tracing::info!("Starting capture on {} ({}Hz, {}ch)", device.name().unwrap_or_default(), sample_rate, channels);
        
        let err_fn = |err| tracing::error!("An error occurred on the capture stream: {}", err);
        
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        // Convert to i16 mono
                        let mut i16_samples = Vec::with_capacity(data.len() / channels as usize);
                        for chunk in data.chunks(channels as usize) {
                            let sample = chunk[0];
                            let scaled = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32);
                            i16_samples.push(scaled as i16);
                        }
                        
                        let resampled = if sample_rate != 16000 {
                            resample_to_gemini_input(&i16_samples, sample_rate)
                        } else {
                            i16_samples
                        };
                        
                        let _ = tx.try_send(resampled);
                    },
                    err_fn,
                    None,
                )
            },
            cpal::SampleFormat::I16 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let mut mono = Vec::with_capacity(data.len() / channels as usize);
                        for chunk in data.chunks(channels as usize) {
                            mono.push(chunk[0]);
                        }
                        
                        let resampled = if sample_rate != 16000 {
                            resample_to_gemini_input(&mono, sample_rate)
                        } else {
                            mono
                        };
                        
                        let _ = tx.try_send(resampled);
                    },
                    err_fn,
                    None,
                )
            },
            _ => {
                tracing::error!("Unsupported sample format");
                return;
            }
        };
        
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Error building stream: {}", e);
                return;
            }
        };
        
        if let Err(e) = stream.play() {
            tracing::error!("Error playing stream: {}", e);
            return;
        }
        
        // Block thread until shutdown signal
        let _ = shutdown_rx.blocking_recv();
        tracing::info!("Stopping mic capture thread");
    });
    
    Ok(shutdown_tx)
}
