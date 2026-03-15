use std::collections::HashMap;
use std::path::Path;

use jamhub_model::{ClipBufferId, Project};

use crate::mixer::Mixer;

pub struct ExportOptions {
    pub normalize: bool,
    pub bit_depth: u16,   // 16 or 32
    pub channels: u16,    // 1 (mono) or 2 (stereo)
    pub tail_seconds: f32, // extra seconds for effects tail
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            normalize: false,
            bit_depth: 32,
            channels: 2,
            tail_seconds: 1.0,
        }
    }
}

/// Export the entire project as a WAV file (offline render).
pub fn export_wav(
    path: &Path,
    project: &Project,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
    channels: u16,
) -> Result<(), String> {
    export_wav_with_options(path, project, audio_buffers, sample_rate, &ExportOptions {
        channels,
        ..Default::default()
    })
}

pub fn export_wav_with_options(
    path: &Path,
    project: &Project,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
    options: &ExportOptions,
) -> Result<(), String> {
    let mut mixer = Mixer::new(sample_rate, options.channels);
    let block_size: usize = 1024;

    // Find the end of the last clip (only non-muted)
    let end_sample = project
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter())
        .filter(|c| !c.muted)
        .map(|c| c.start_sample + c.duration_samples)
        .max()
        .unwrap_or(0);

    if end_sample == 0 {
        return Err("Nothing to export — no active clips in project".into());
    }

    let total_samples = end_sample + (sample_rate as f32 * options.tail_seconds) as u64;

    // Render all audio
    let mut all_samples: Vec<f32> = Vec::new();
    let mut position: u64 = 0;
    while position < total_samples {
        let block = mixer.render_block(project, position, block_size, audio_buffers);
        all_samples.extend_from_slice(&block);
        position += block_size as u64;
    }

    // Normalize if requested
    if options.normalize {
        let peak = all_samples
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max);
        if peak > 0.001 {
            let gain = 0.99 / peak;
            for s in all_samples.iter_mut() {
                *s *= gain;
            }
        }
    }

    // Write WAV
    if options.bit_depth == 16 {
        let spec = hound::WavSpec {
            channels: options.channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|e| format!("Failed to create WAV: {e}"))?;
        for &s in &all_samples {
            let sample_i16 = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            writer.write_sample(sample_i16)
                .map_err(|e| format!("Write error: {e}"))?;
        }
        writer.finalize().map_err(|e| format!("Finalize: {e}"))?;
    } else {
        let spec = hound::WavSpec {
            channels: options.channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|e| format!("Failed to create WAV: {e}"))?;
        for &s in &all_samples {
            writer.write_sample(s)
                .map_err(|e| format!("Write error: {e}"))?;
        }
        writer.finalize().map_err(|e| format!("Finalize: {e}"))?;
    }

    Ok(())
}

/// Bounce a single track to a new audio buffer (freeze/render effects).
pub fn bounce_track(
    project: &Project,
    track_idx: usize,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
) -> Result<Vec<f32>, String> {
    if track_idx >= project.tracks.len() {
        return Err("Invalid track index".into());
    }

    // Create a temporary project with only this track
    let mut temp_project = project.clone();
    let track = temp_project.tracks[track_idx].clone();
    temp_project.tracks = vec![track];
    temp_project.tracks[0].muted = false;
    temp_project.tracks[0].solo = false;
    temp_project.tracks[0].volume = 1.0;
    temp_project.tracks[0].pan = 0.0;

    let end_sample = temp_project.tracks[0]
        .clips
        .iter()
        .filter(|c| !c.muted)
        .map(|c| c.start_sample + c.duration_samples)
        .max()
        .unwrap_or(0);

    if end_sample == 0 {
        return Err("Track has no active clips".into());
    }

    let total = end_sample + sample_rate as u64; // 1s tail
    let block_size: usize = 1024;
    let mut mixer = Mixer::new(sample_rate, 1); // render mono

    let mut output = Vec::new();
    let mut pos: u64 = 0;
    while pos < total {
        let block = mixer.render_block(&temp_project, pos, block_size, audio_buffers);
        output.extend_from_slice(&block);
        pos += block_size as u64;
    }

    // Trim to exact length
    output.truncate(total as usize);
    Ok(output)
}
