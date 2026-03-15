use std::collections::HashMap;
use std::path::Path;

use jamhub_model::{ClipBufferId, Project};

use crate::mixer::Mixer;

/// Export the entire project as a stereo WAV file (offline render).
pub fn export_wav(
    path: &Path,
    project: &Project,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
    channels: u16,
) -> Result<(), String> {
    let mut mixer = Mixer::new(sample_rate, channels);
    let block_size: usize = 1024;

    // Find the end of the last clip
    let end_sample = project
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter())
        .map(|c| c.start_sample + c.duration_samples)
        .max()
        .unwrap_or(0);

    if end_sample == 0 {
        return Err("Nothing to export — no clips in project".into());
    }

    // Add 1 second of tail for effects
    let total_samples = end_sample + sample_rate as u64;

    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("Failed to create WAV: {e}"))?;

    let mut position: u64 = 0;
    while position < total_samples {
        let block = mixer.render_block(project, position, block_size, audio_buffers);
        for &sample in &block {
            writer
                .write_sample(sample)
                .map_err(|e| format!("Write error: {e}"))?;
        }
        position += block_size as u64;
    }

    writer
        .finalize()
        .map_err(|e| format!("Failed to finalize WAV: {e}"))?;

    Ok(())
}
