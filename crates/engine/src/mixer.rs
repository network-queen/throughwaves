use std::collections::HashMap;

use uuid::Uuid;

use jamhub_model::{ClipBufferId, Project, TrackKind};

use crate::effects::EffectProcessor;

pub struct Mixer {
    sample_rate: u32,
    channels: u16,
    processors: HashMap<Uuid, EffectProcessor>,
}

impl Mixer {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            processors: HashMap::new(),
        }
    }

    pub fn render_block(
        &mut self,
        project: &Project,
        position_samples: u64,
        block_size: usize,
        audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
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

            // Render clips into mono buffer
            let mut track_mono = vec![0.0f32; block_size];
            let mut has_audio = false;

            for clip in &track.clips {
                if clip.muted {
                    continue;
                }

                let clip_end = clip.start_sample + clip.duration_samples;
                let block_end = position_samples + block_size as u64;

                if position_samples >= clip_end || block_end <= clip.start_sample {
                    continue;
                }

                if let jamhub_model::ClipSource::AudioBuffer { buffer_id } = &clip.source {
                    if let Some(buf) = audio_buffers.get(buffer_id) {
                        for i in 0..block_size {
                            let global_sample = position_samples + i as u64;
                            if global_sample < clip.start_sample || global_sample >= clip_end {
                                continue;
                            }
                            let clip_offset = (global_sample - clip.start_sample) as usize;
                            if clip_offset < buf.len() {
                                track_mono[i] += buf[clip_offset];
                                has_audio = true;
                            }
                        }
                    }
                }
            }

            if !has_audio {
                continue;
            }

            // Apply effects chain
            if !track.effects.is_empty() {
                let processor = self
                    .processors
                    .entry(track.id)
                    .or_insert_with(|| EffectProcessor::new(self.sample_rate));

                for effect in &track.effects {
                    processor.process(&mut track_mono, effect, self.sample_rate);
                }
            }

            // Read automation at current position
            let auto_volume = get_automation_value(
                &track.automation,
                jamhub_model::AutomationParam::Volume,
                position_samples,
                track.volume,
            );
            let auto_pan = get_automation_value(
                &track.automation,
                jamhub_model::AutomationParam::Pan,
                position_samples,
                track.pan,
            );

            // Apply volume and pan (with automation), mix into output
            let channels = self.channels as usize;
            let (left_gain, right_gain) = pan_law(auto_pan);

            for i in 0..block_size {
                let sample = track_mono[i] * auto_volume;
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

        for s in output.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }

        output
    }
}

/// Get interpolated automation value at a given sample position.
fn get_automation_value(
    automation: &[jamhub_model::AutomationLane],
    param: jamhub_model::AutomationParam,
    sample: u64,
    default: f32,
) -> f32 {
    let lane = automation.iter().find(|l| l.parameter == param);
    let lane = match lane {
        Some(l) if !l.points.is_empty() => l,
        _ => return default,
    };

    let points = &lane.points;

    // Before first point
    if sample <= points[0].sample {
        return points[0].value;
    }
    // After last point
    if sample >= points[points.len() - 1].sample {
        return points[points.len() - 1].value;
    }

    // Find surrounding points and interpolate
    for i in 0..points.len() - 1 {
        if sample >= points[i].sample && sample < points[i + 1].sample {
            let t = (sample - points[i].sample) as f32
                / (points[i + 1].sample - points[i].sample) as f32;
            return points[i].value + t * (points[i + 1].value - points[i].value);
        }
    }

    default
}

fn pan_law(pan: f32) -> (f32, f32) {
    let angle = (pan + 1.0) * 0.25 * std::f32::consts::PI;
    (angle.cos(), angle.sin())
}
