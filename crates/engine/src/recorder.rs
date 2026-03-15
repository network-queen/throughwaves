use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use parking_lot::Mutex;

pub struct RecordingResult {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

pub struct Recorder {
    stream: Option<Stream>,
    recording_buffer: Arc<Mutex<Vec<f32>>>,
    is_recording: bool,
    input_sample_rate: u32,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            recording_buffer: Arc::new(Mutex::new(Vec::new())),
            is_recording: false,
            input_sample_rate: 48000,
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;

        let supported = device
            .default_input_config()
            .map_err(|e| format!("Failed to get input config: {e}"))?;

        self.input_sample_rate = supported.sample_rate().0;

        println!(
            "Recording input: {:?}, {} channels, {}Hz, {:?}",
            device.name().unwrap_or_default(),
            supported.channels(),
            supported.sample_rate().0,
            supported.sample_format(),
        );

        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels as usize;

        let buffer = self.recording_buffer.clone();
        buffer.lock().clear();

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut buf = buffer.lock();
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
        println!("Recording started");
        Ok(())
    }

    pub fn stop(&mut self) -> RecordingResult {
        self.stream = None;
        self.is_recording = false;
        let mut buf = self.recording_buffer.lock();
        let samples = std::mem::take(&mut *buf);
        println!(
            "Recording stopped: {} samples ({:.2}s at {}Hz)",
            samples.len(),
            samples.len() as f64 / self.input_sample_rate as f64,
            self.input_sample_rate,
        );
        RecordingResult {
            samples,
            sample_rate: self.input_sample_rate,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording
    }
}

/// Simple linear resampling from one rate to another.
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (samples.len() as f64 / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos.floor() as usize;
        let frac = src_pos - idx as f64;

        let s = if idx + 1 < samples.len() {
            samples[idx] * (1.0 - frac as f32) + samples[idx + 1] * frac as f32
        } else if idx < samples.len() {
            samples[idx]
        } else {
            0.0
        };
        output.push(s);
    }

    output
}
