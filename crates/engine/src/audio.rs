use std::collections::VecDeque;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, StreamConfig};
use crossbeam_channel::Receiver;

pub struct AudioBackend {
    _host: Host,
    device: Device,
    config: StreamConfig,
    _stream: Option<Stream>,
}

impl AudioBackend {
    pub fn new() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No output device available")?;
        let supported = device
            .default_output_config()
            .map_err(|e| format!("Failed to get default output config: {e}"))?;

        // Audio backend initialized — device name, channels, sample rate logged at debug level
        let _ = device.name(); // used during development; silence lint

        let config: StreamConfig = supported.into();

        Ok(Self {
            _host: host,
            device,
            config,
            _stream: None,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate.0
    }

    pub fn channels(&self) -> u16 {
        self.config.channels
    }

    pub fn start(&mut self, audio_rx: Receiver<Vec<f32>>) -> Result<(), String> {
        // Use a VecDeque as a ring buffer — bounded to ~0.5s of audio
        // to prevent unbounded growth.
        let max_buffer = self.config.sample_rate.0 as usize * self.config.channels as usize / 2;
        let mut buffer: VecDeque<f32> = VecDeque::with_capacity(max_buffer);

        let stream = self
            .device
            .build_output_stream(
                &self.config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Only receive enough blocks to fill the hardware buffer.
                    // Don't drain the entire channel — that would let the engine
                    // race ahead and desync position from actual playback.
                    while buffer.len() < data.len() {
                        match audio_rx.try_recv() {
                            Ok(chunk) => {
                                for &s in &chunk {
                                    buffer.push_back(s);
                                }
                            }
                            Err(_) => break, // no more data available
                        }
                    }

                    // Copy to hardware output
                    for sample in data.iter_mut() {
                        *sample = buffer.pop_front().unwrap_or(0.0);
                    }
                },
                |err| {
                    eprintln!("Audio stream error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build output stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("Failed to play stream: {e}"))?;
        self._stream = Some(stream);
        Ok(())
    }
}
