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

        println!(
            "Audio output: {:?}, {} channels, {}Hz",
            device.name().unwrap_or_default(),
            supported.channels(),
            supported.sample_rate().0,
        );

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
        // Pre-allocate a ring buffer large enough for smooth playback
        let mut buffer: Vec<f32> = Vec::with_capacity(65536);

        let stream = self
            .device
            .build_output_stream(
                &self.config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Drain all available blocks into our local buffer
                    while let Ok(chunk) = audio_rx.try_recv() {
                        buffer.extend_from_slice(&chunk);
                    }

                    let to_copy = data.len().min(buffer.len());
                    if to_copy > 0 {
                        data[..to_copy].copy_from_slice(&buffer[..to_copy]);
                        buffer.drain(..to_copy);
                    }
                    // Fill remaining with silence (buffer underrun)
                    for sample in data[to_copy..].iter_mut() {
                        *sample = 0.0;
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
