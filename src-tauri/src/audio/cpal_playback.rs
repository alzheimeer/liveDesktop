use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::sync::oneshot;

pub fn start_playback(device_name: String, rx_buffer: Arc<Mutex<Vec<i16>>>) -> Result<(oneshot::Sender<()>, u32), String> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    
    let host = cpal::default_host();
    let device_opt = if device_name == "default" {
        host.default_output_device()
    } else {
        host.output_devices()
            .ok()
            .and_then(|mut devices| devices.find(|x| x.name().unwrap_or_default() == device_name))
    };

    let device = device_opt.ok_or_else(|| format!("Output device '{}' not found", device_name))?;
    
    let config = device.default_output_config()
        .map_err(|e| format!("Error getting default config: {}", e))?;
        
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    
    thread::spawn(move || {
        tracing::info!("Starting playback on {} ({}Hz, {}ch)", device.name().unwrap_or_default(), sample_rate, channels);
        
        let err_fn = |err| tracing::error!("An error occurred on the playback stream: {}", err);
        
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                device.build_output_stream(
                    &config.into(),
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let mut buf = rx_buffer.lock().unwrap();
                        let mut idx = 0;
                        for frame in data.chunks_mut(channels as usize) {
                            let sample = if idx < buf.len() {
                                let s = buf[idx] as f32 / i16::MAX as f32;
                                idx += 1;
                                s
                            } else {
                                0.0
                            };
                            for channel_sample in frame.iter_mut() {
                                *channel_sample = sample;
                            }
                        }
                        if idx > 0 {
                            buf.drain(0..idx);
                        }
                    },
                    err_fn,
                    None,
                )
            },
            cpal::SampleFormat::I16 => {
                device.build_output_stream(
                    &config.into(),
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        let mut buf = rx_buffer.lock().unwrap();
                        let mut idx = 0;
                        for frame in data.chunks_mut(channels as usize) {
                            let sample = if idx < buf.len() {
                                let s = buf[idx];
                                idx += 1;
                                s
                            } else {
                                0
                            };
                            for channel_sample in frame.iter_mut() {
                                *channel_sample = sample;
                            }
                        }
                        if idx > 0 {
                            buf.drain(0..idx);
                        }
                    },
                    err_fn,
                    None,
                )
            },
            _ => {
                tracing::error!("Unsupported sample format for playback");
                return;
            }
        };
        
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Error building playback stream: {}", e);
                return;
            }
        };
        
        if let Err(e) = stream.play() {
            tracing::error!("Error playing stream: {}", e);
            return;
        }
        
        // Block thread until shutdown signal
        let _ = shutdown_rx.blocking_recv();
        tracing::info!("Stopping playback thread");
    });
    
    Ok((shutdown_tx, sample_rate))
}
