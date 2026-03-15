use std::sync::Arc;
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use parking_lot::Mutex;

/// Routes microphone input directly to speaker output for zero-latency monitoring.
pub struct InputMonitor {
    stream_in: Option<Stream>,
    stream_out: Option<Stream>,
    enabled: bool,
    pub volume: f32,
}

impl InputMonitor {
    pub fn new() -> Self {
        Self {
            stream_in: None,
            stream_out: None,
            enabled: false,
            volume: 0.7,
        }
    }

    pub fn toggle(&mut self) -> Result<bool, String> {
        if self.enabled {
            self.stop();
            Ok(false)
        } else {
            self.start()?;
            Ok(true)
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        let host = cpal::default_host();

        let in_device = host
            .default_input_device()
            .ok_or("No input device")?;
        let out_device = host
            .default_output_device()
            .ok_or("No output device")?;

        let in_config: cpal::StreamConfig = in_device
            .default_input_config()
            .map_err(|e| format!("Input config: {e}"))?
            .into();

        let out_config: cpal::StreamConfig = out_device
            .default_output_config()
            .map_err(|e| format!("Output config: {e}"))?
            .into();

        let in_channels = in_config.channels as usize;
        let out_channels = out_config.channels as usize;

        // Ring buffer shared between input and output
        let buffer = Arc::new(Mutex::new(std::collections::VecDeque::<f32>::with_capacity(8192)));
        let buf_in = buffer.clone();
        let buf_out = buffer.clone();
        let vol = self.volume;

        let stream_in = in_device
            .build_input_stream(
                &in_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut buf = buf_in.lock();
                    // Mix to mono
                    if in_channels > 1 {
                        for frame in data.chunks(in_channels) {
                            let mono: f32 = frame.iter().sum::<f32>() / in_channels as f32;
                            buf.push_back(mono * vol);
                        }
                    } else {
                        for &s in data {
                            buf.push_back(s * vol);
                        }
                    }
                    // Limit buffer size to prevent latency buildup
                    while buf.len() > 4096 {
                        buf.pop_front();
                    }
                },
                |e| eprintln!("Input monitor error: {e}"),
                None,
            )
            .map_err(|e| format!("Build input: {e}"))?;

        let stream_out = out_device
            .build_output_stream(
                &out_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut buf = buf_out.lock();
                    for frame in data.chunks_mut(out_channels) {
                        let sample = buf.pop_front().unwrap_or(0.0);
                        for ch in frame.iter_mut() {
                            *ch += sample; // add to existing output (mix)
                        }
                    }
                },
                |e| eprintln!("Output monitor error: {e}"),
                None,
            )
            .map_err(|e| format!("Build output: {e}"))?;

        stream_in.play().map_err(|e| format!("Play input: {e}"))?;
        stream_out.play().map_err(|e| format!("Play output: {e}"))?;

        self.stream_in = Some(stream_in);
        self.stream_out = Some(stream_out);
        self.enabled = true;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stream_in = None;
        self.stream_out = None;
        self.enabled = false;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}
