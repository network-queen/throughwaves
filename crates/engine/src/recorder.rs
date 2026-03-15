use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;

pub struct Recorder {
    stream: Option<Stream>,
    recording_buffer: Arc<Mutex<Vec<f32>>>,
    is_recording: bool,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            recording_buffer: Arc::new(Mutex::new(Vec::new())),
            is_recording: false,
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get input config: {e}"))?;
        let config: cpal::StreamConfig = config.into();

        let buffer = self.recording_buffer.clone();
        buffer.lock().clear();

        let channels = config.channels as usize;

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut buf = buffer.lock();
                    // Mix to mono
                    if channels > 1 {
                        for frame in data.chunks(channels) {
                            let mono: f32 = frame.iter().sum::<f32>() / channels as f32;
                            buf.push(mono);
                        }
                    } else {
                        buf.extend_from_slice(data);
                    }
                },
                |err| {
                    eprintln!("Recording error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start recording: {e}"))?;

        self.stream = Some(stream);
        self.is_recording = true;
        Ok(())
    }

    pub fn stop(&mut self) -> Vec<f32> {
        self.stream = None;
        self.is_recording = false;
        let mut buf = self.recording_buffer.lock();
        std::mem::take(&mut *buf)
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording
    }
}
