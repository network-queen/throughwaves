use jamhub_model::{Project, TrackKind};

pub struct Mixer {
    sample_rate: u32,
    channels: u16,
}

impl Mixer {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
        }
    }

    /// Render a block of interleaved audio samples from the project at the given position.
    pub fn render_block(
        &self,
        project: &Project,
        position_samples: u64,
        block_size: usize,
        audio_buffers: &std::collections::HashMap<jamhub_model::ClipBufferId, Vec<f32>>,
    ) -> Vec<f32> {
        let num_samples = block_size * self.channels as usize;
        let mut output = vec![0.0f32; num_samples];

        let any_solo = project.tracks.iter().any(|t| t.solo);

        for track in &project.tracks {
            if track.muted {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }
            if track.kind != TrackKind::Audio {
                continue;
            }

            for clip in &track.clips {
                let clip_end = clip.start_sample + clip.duration_samples;
                let block_end = position_samples + block_size as u64;

                // Check if clip overlaps this block
                if position_samples >= clip_end || block_end <= clip.start_sample {
                    continue;
                }

                if let jamhub_model::ClipSource::AudioBuffer { buffer_id } = &clip.source {
                    if let Some(buf) = audio_buffers.get(buffer_id) as Option<&Vec<f32>> {
                        let channels = self.channels as usize;
                        for i in 0..block_size {
                            let global_sample = position_samples + i as u64;
                            if global_sample < clip.start_sample || global_sample >= clip_end {
                                continue;
                            }
                            let clip_offset = (global_sample - clip.start_sample) as usize;
                            // Mono source buffer, mix into all channels
                            if clip_offset < buf.len() {
                                let sample = buf[clip_offset] * track.volume;
                                let (left_gain, right_gain) = pan_law(track.pan);
                                for ch in 0..channels {
                                    let gain = if ch == 0 {
                                        left_gain
                                    } else if ch == 1 {
                                        right_gain
                                    } else {
                                        1.0
                                    };
                                    output[i * channels + ch] += sample * gain;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Clamp output
        for s in output.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }

        output
    }
}

/// Constant-power pan law. pan: -1.0 (left) to 1.0 (right)
fn pan_law(pan: f32) -> (f32, f32) {
    let angle = (pan + 1.0) * 0.25 * std::f32::consts::PI;
    (angle.cos(), angle.sin())
}
