use std::collections::HashMap;
use std::path::PathBuf;

use uuid::Uuid;

use jamhub_model::{ClipBufferId, Project, TrackKind};

use crate::effects::EffectProcessor;
use crate::vst3_host::Vst3Plugin;

pub struct Mixer {
    sample_rate: u32,
    channels: u16,
    processors: HashMap<Uuid, EffectProcessor>,
    /// Live VST3 plugin instances, keyed by EffectSlot ID
    vst_instances: HashMap<Uuid, Vst3Plugin>,
    /// Dedicated effect processor for the master bus effects chain.
    master_processor: EffectProcessor,
}

impl Mixer {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            processors: HashMap::new(),
            vst_instances: HashMap::new(),
            master_processor: EffectProcessor::new(sample_rate),
        }
    }

    /// Load a VST3 plugin instance for a given effect slot.
    pub fn load_vst3(&mut self, slot_id: Uuid, path: &PathBuf) {
        println!("Mixer: loading VST3 for slot {slot_id} from {}", path.display());
        let plugin = Vst3Plugin::load(path, self.sample_rate as f64, 256);
        if plugin.loaded {
            println!("Mixer: VST3 loaded for slot {slot_id}, processing={}", plugin.processing);
            self.vst_instances.insert(slot_id, plugin);
        } else {
            eprintln!(
                "Mixer: VST3 load failed for slot {slot_id}: {}",
                plugin.error.as_deref().unwrap_or("unknown")
            );
        }
    }

    /// Attach a parameter change receiver to a loaded VST3 plugin.
    pub fn attach_param_rx(&mut self, slot_id: &Uuid, rx: crate::vst3_host::ParamChangeRx) {
        if let Some(vst) = self.vst_instances.get_mut(slot_id) {
            vst.param_change_rx = Some(rx);
            println!("Mixer: attached param rx for slot {slot_id}");
        }
    }

    /// Unload a VST3 plugin instance.
    pub fn unload_vst3(&mut self, slot_id: &Uuid) {
        if self.vst_instances.remove(slot_id).is_some() {
            println!("Mixer: VST3 unloaded for slot {slot_id}");
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

        // Accumulator for send contributions to bus/aux tracks, keyed by target track ID.
        let mut send_buffers: HashMap<Uuid, Vec<f32>> = HashMap::new();

        // ---- Pre-pass: Render raw (pre-effect) audio for all Audio tracks ----
        // Stored for sidechain access so compressors on other tracks can use
        // this track's audio as their detection signal.
        let mut pre_effect_audio: HashMap<Uuid, Vec<f32>> = HashMap::new();

        for track in &project.tracks {
            if track.kind != TrackKind::Audio {
                continue;
            }
            if track.muted {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }

            let track_mono = self.render_track_clips(track, position_samples, block_size, audio_buffers);
            if track_mono.iter().any(|&s| s != 0.0) {
                pre_effect_audio.insert(track.id, track_mono);
            }
        }

        // ---- Pass 1: Apply effects, volume/pan, sends for Audio tracks ----
        // We also accumulate bus-routed audio into send_buffers via output_target.
        let mut bus_output_buffers: HashMap<Uuid, Vec<f32>> = HashMap::new();

        for track in &project.tracks {
            if track.muted || track.kind != TrackKind::Audio {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }

            let mut track_mono = match pre_effect_audio.get(&track.id) {
                Some(buf) => buf.clone(),
                None => continue,
            };

            // Frozen tracks: skip all effect processing (effects already baked into frozen audio)
            if track.frozen {
                // The frozen audio is already rendered into the clips via frozen_buffer_id,
                // so render_track_clips already returned the frozen audio. Skip effects.
            }
            // Apply effects chain (built-in + VST3) with sidechain support
            else if !track.effects.is_empty() {
                let sidechain_audio = track.sidechain_track_id
                    .and_then(|sc_id| pre_effect_audio.get(&sc_id));

                let processor = self
                    .processors
                    .entry(track.id)
                    .or_insert_with(|| EffectProcessor::new(self.sample_rate));

                for (slot_index, slot) in track.effects.iter().enumerate() {
                    if !slot.enabled {
                        continue;
                    }

                    match &slot.effect {
                        jamhub_model::TrackEffect::Vst3Plugin { .. } => {
                            if let Some(vst) = self.vst_instances.get_mut(&slot.id) {
                                vst.apply_pending_param_changes();
                                vst.process(&mut track_mono);
                            }
                        }
                        jamhub_model::TrackEffect::Compressor { threshold_db, ratio, attack_ms, release_ms }
                            if sidechain_audio.is_some() =>
                        {
                            // Sidechain compressor: use another track's audio for detection
                            let automated = apply_effect_automation(
                                &slot.effect,
                                slot_index,
                                &track.automation,
                                position_samples,
                            );
                            if let jamhub_model::TrackEffect::Compressor { threshold_db, ratio, attack_ms, release_ms } = &automated {
                                processor.process_compressor(
                                    &mut track_mono,
                                    sidechain_audio.map(|v| v.as_slice()),
                                    *threshold_db,
                                    *ratio,
                                    *attack_ms,
                                    *release_ms,
                                    self.sample_rate,
                                );
                            }
                        }
                        effect => {
                            let automated = apply_effect_automation(
                                effect,
                                slot_index,
                                &track.automation,
                                position_samples,
                            );
                            processor.process(&mut track_mono, &automated, self.sample_rate);
                        }
                    }
                }
            }

            // Read automation at current position
            let auto_volume = get_automation_value(
                &track.automation,
                &jamhub_model::AutomationParam::Volume,
                position_samples,
                track.volume,
            );
            let auto_pan = get_automation_value(
                &track.automation,
                &jamhub_model::AutomationParam::Pan,
                position_samples,
                track.pan,
            );

            // --- Send routing ---
            for send in &track.sends {
                let send_buf = send_buffers
                    .entry(send.target_track_id)
                    .or_insert_with(|| vec![0.0f32; block_size]);
                if send.pre_fader {
                    for i in 0..block_size {
                        send_buf[i] += track_mono[i] * send.level;
                    }
                } else {
                    for i in 0..block_size {
                        send_buf[i] += track_mono[i] * auto_volume * send.level;
                    }
                }
            }

            // Apply volume and pan, route to output_target or master
            let channels = self.channels as usize;
            let (left_gain, right_gain) = pan_law(auto_pan);

            if let Some(target_id) = track.output_target {
                // Route to a bus track instead of master
                let bus_buf = bus_output_buffers
                    .entry(target_id)
                    .or_insert_with(|| vec![0.0f32; block_size]);
                for i in 0..block_size {
                    bus_buf[i] += track_mono[i] * auto_volume;
                }
            } else {
                // Route to master output
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
        }

        // ---- Pass 2: Process Bus/Aux tracks ----
        // Bus tracks receive audio from sends AND from tracks routed via output_target.
        for track in &project.tracks {
            if track.kind != TrackKind::Bus {
                continue;
            }
            if track.muted {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }

            // Combine audio from sends and output_target routing
            let mut track_mono = send_buffers
                .remove(&track.id)
                .unwrap_or_else(|| vec![0.0f32; block_size]);

            if let Some(routed) = bus_output_buffers.remove(&track.id) {
                for i in 0..block_size {
                    track_mono[i] += routed[i];
                }
            }

            let has_audio = track_mono.iter().any(|&s| s != 0.0);
            if !has_audio {
                continue;
            }

            // Apply effects chain on bus track with sidechain support
            if !track.effects.is_empty() {
                let sidechain_audio = track.sidechain_track_id
                    .and_then(|sc_id| pre_effect_audio.get(&sc_id));

                let processor = self
                    .processors
                    .entry(track.id)
                    .or_insert_with(|| EffectProcessor::new(self.sample_rate));

                for (slot_index, slot) in track.effects.iter().enumerate() {
                    if !slot.enabled {
                        continue;
                    }

                    match &slot.effect {
                        jamhub_model::TrackEffect::Vst3Plugin { .. } => {
                            if let Some(vst) = self.vst_instances.get_mut(&slot.id) {
                                vst.apply_pending_param_changes();
                                vst.process(&mut track_mono);
                            }
                        }
                        jamhub_model::TrackEffect::Compressor { threshold_db, ratio, attack_ms, release_ms }
                            if sidechain_audio.is_some() =>
                        {
                            let automated = apply_effect_automation(
                                &slot.effect,
                                slot_index,
                                &track.automation,
                                position_samples,
                            );
                            if let jamhub_model::TrackEffect::Compressor { threshold_db, ratio, attack_ms, release_ms } = &automated {
                                processor.process_compressor(
                                    &mut track_mono,
                                    sidechain_audio.map(|v| v.as_slice()),
                                    *threshold_db,
                                    *ratio,
                                    *attack_ms,
                                    *release_ms,
                                    self.sample_rate,
                                );
                            }
                        }
                        effect => {
                            let automated = apply_effect_automation(
                                effect,
                                slot_index,
                                &track.automation,
                                position_samples,
                            );
                            processor.process(&mut track_mono, &automated, self.sample_rate);
                        }
                    }
                }
            }

            // Bus volume and pan
            let auto_volume = get_automation_value(
                &track.automation,
                &jamhub_model::AutomationParam::Volume,
                position_samples,
                track.volume,
            );
            let auto_pan = get_automation_value(
                &track.automation,
                &jamhub_model::AutomationParam::Pan,
                position_samples,
                track.pan,
            );

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

        output
    }

    /// Apply the master bus effect chain to interleaved output samples.
    /// This runs built-in effects and VST3 plugins from `project.master_effects`
    /// on a downmixed mono copy, then writes back to the interleaved buffer.
    pub fn apply_master_effects(
        &mut self,
        output: &mut [f32],
        project: &Project,
    ) {
        if project.master_effects.is_empty() {
            return;
        }
        let channels = self.channels as usize;
        let frames = output.len() / channels.max(1);

        // Downmix to mono for effect processing
        let mut mono = vec![0.0f32; frames];
        for i in 0..frames {
            let mut sum = 0.0f32;
            for ch in 0..channels {
                sum += output[i * channels + ch];
            }
            mono[i] = sum / channels as f32;
        }

        for slot in &project.master_effects {
            if !slot.enabled {
                continue;
            }
            match &slot.effect {
                jamhub_model::TrackEffect::Vst3Plugin { .. } => {
                    if let Some(vst) = self.vst_instances.get_mut(&slot.id) {
                        vst.apply_pending_param_changes();
                        vst.process(&mut mono);
                    }
                }
                effect => {
                    self.master_processor.process(&mut mono, effect, self.sample_rate);
                }
            }
        }

        // Write processed mono back to all channels
        for i in 0..frames {
            for ch in 0..channels {
                output[i * channels + ch] = mono[i];
            }
        }
    }

    /// Render the raw clip audio for a track (pre-effects) into a mono buffer.
    fn render_track_clips(
        &self,
        track: &jamhub_model::Track,
        position_samples: u64,
        block_size: usize,
        audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    ) -> Vec<f32> {
        let mut track_mono = vec![0.0f32; block_size];

        let mut active_clips: Vec<usize> = Vec::new();
        for (ci, clip) in track.clips.iter().enumerate() {
            if !clip.muted {
                active_clips.push(ci);
            }
        }
        active_clips.sort_by_key(|&ci| track.clips[ci].start_sample);

        for (aci, &ci) in active_clips.iter().enumerate() {
            let clip = &track.clips[ci];
            let rate = clip.playback_rate.max(0.01);
            // Visual duration accounts for playback rate
            let visual_duration = clip.visual_duration_samples();
            let clip_visual_end = clip.start_sample + visual_duration;
            let block_end = position_samples + block_size as u64;

            if position_samples >= clip_visual_end || block_end <= clip.start_sample {
                continue;
            }

            // Detect auto-crossfade: check if this clip overlaps with the next clip
            let mut crossfade_out_start: Option<u64> = None;
            let mut crossfade_out_len: u64 = 0;
            if aci + 1 < active_clips.len() {
                let next_clip = &track.clips[active_clips[aci + 1]];
                if !next_clip.muted && next_clip.start_sample < clip_visual_end {
                    crossfade_out_start = Some(next_clip.start_sample);
                    crossfade_out_len = clip_visual_end - next_clip.start_sample;
                }
            }
            // Detect auto-crossfade in: check if previous clip overlaps into this one
            let mut crossfade_in_start: Option<u64> = None;
            let mut crossfade_in_len: u64 = 0;
            if aci > 0 {
                let prev_clip = &track.clips[active_clips[aci - 1]];
                let prev_visual_end = prev_clip.start_sample + prev_clip.visual_duration_samples();
                if !prev_clip.muted && clip.start_sample < prev_visual_end {
                    crossfade_in_start = Some(clip.start_sample);
                    crossfade_in_len = prev_visual_end - clip.start_sample;
                }
            }

            if let jamhub_model::ClipSource::AudioBuffer { buffer_id } = &clip.source {
                if let Some(buf) = audio_buffers.get(buffer_id) {
                    // If preserve_pitch is enabled and rate != 1.0, use OLA time-stretching
                    if clip.preserve_pitch && (rate - 1.0).abs() > 0.001 {
                        self.render_clip_ola(
                            clip, buf, rate, visual_duration, clip_visual_end,
                            position_samples, block_size,
                            crossfade_out_start, crossfade_out_len,
                            crossfade_in_start, crossfade_in_len,
                            &mut track_mono,
                        );
                    } else {
                        // Standard linear interpolation resampling (with loop support)
                        let buf_len = buf.len();
                        for i in 0..block_size {
                            let global_sample = position_samples + i as u64;
                            if global_sample < clip.start_sample || global_sample >= clip_visual_end {
                                continue;
                            }
                            // Position within visual timeline relative to clip start
                            let visual_offset = (global_sample - clip.start_sample) as f64;
                            // Map to source buffer position using playback rate
                            // For looped clips, wrap the source position using modulo
                            let mut source_pos = visual_offset * rate as f64;
                            if clip.loop_count > 1 && buf_len > 0 {
                                source_pos = source_pos % buf_len as f64;
                            }
                            let source_idx = source_pos.floor() as usize;
                            let frac = source_pos - source_pos.floor();

                            if source_idx >= buf.len() {
                                continue;
                            }

                            // Linear interpolation between adjacent samples
                            let s0 = buf[source_idx];
                            let s1 = if source_idx + 1 < buf.len() { buf[source_idx + 1] } else { s0 };
                            let sample_val = s0 + (s1 - s0) * frac as f32;

                            let mut gain = 1.0f32;

                            // Per-clip fade in (in visual time)
                            if clip.fade_in_samples > 0 {
                                let pos_in_clip = global_sample - clip.start_sample;
                                if pos_in_clip < clip.fade_in_samples {
                                    gain *= pos_in_clip as f32 / clip.fade_in_samples as f32;
                                }
                            }

                            // Per-clip fade out (in visual time)
                            if clip.fade_out_samples > 0 {
                                let pos_from_end = clip_visual_end - global_sample;
                                if pos_from_end <= clip.fade_out_samples {
                                    gain *= pos_from_end as f32 / clip.fade_out_samples as f32;
                                }
                            }

                            // Auto-crossfade out
                            if let Some(xf_start) = crossfade_out_start {
                                if global_sample >= xf_start && crossfade_out_len > 0 {
                                    let xf_pos = global_sample - xf_start;
                                    let xf_gain = 1.0 - (xf_pos as f32 / crossfade_out_len as f32);
                                    gain *= xf_gain;
                                }
                            }

                            // Auto-crossfade in
                            if let Some(xf_start) = crossfade_in_start {
                                if global_sample >= xf_start && global_sample < xf_start + crossfade_in_len {
                                    let xf_pos = global_sample - xf_start;
                                    let xf_gain = xf_pos as f32 / crossfade_in_len as f32;
                                    gain *= xf_gain;
                                }
                            }

                            // Apply clip gain (dB) before track processing
                            if clip.gain_db.abs() > 0.001 {
                                gain *= 10.0_f32.powf(clip.gain_db / 20.0);
                            }

                            track_mono[i] += sample_val * gain;
                        }
                    }
                }
            }
        }

        track_mono
    }

    /// Render a clip using Overlap-Add (OLA) time-stretching to preserve pitch.
    /// Splits audio into overlapping windows, positions them at the output rate,
    /// and crossfades between them.
    #[allow(clippy::too_many_arguments)]
    fn render_clip_ola(
        &self,
        clip: &jamhub_model::Clip,
        buf: &[f32],
        rate: f32,
        _visual_duration: u64,
        clip_visual_end: u64,
        position_samples: u64,
        block_size: usize,
        crossfade_out_start: Option<u64>,
        crossfade_out_len: u64,
        crossfade_in_start: Option<u64>,
        crossfade_in_len: u64,
        output: &mut [f32],
    ) {
        // OLA parameters
        let window_size: usize = 1024;
        let hop_input = (window_size as f32 * 0.5) as usize; // 50% overlap in source
        let hop_output = (hop_input as f64 / rate as f64) as usize; // output hop scaled by rate

        if hop_output == 0 || hop_input == 0 {
            return;
        }

        for i in 0..block_size {
            let global_sample = position_samples + i as u64;
            if global_sample < clip.start_sample || global_sample >= clip_visual_end {
                continue;
            }

            let visual_offset = (global_sample - clip.start_sample) as usize;

            // Determine which OLA window(s) this output sample falls in
            // Output window n starts at n * hop_output, corresponds to source at n * hop_input
            let window_idx = visual_offset / hop_output;
            let _pos_in_window = visual_offset % hop_output;

            let mut sample_val = 0.0f32;
            let mut weight_sum = 0.0f32;

            // Check current and adjacent windows for overlap
            let start_win = if window_idx > 0 { window_idx - 1 } else { 0 };
            let end_win = window_idx + 1;

            for w in start_win..=end_win {
                let win_output_start = w * hop_output;
                if visual_offset < win_output_start || visual_offset >= win_output_start + window_size {
                    continue;
                }

                let pos_in_win = visual_offset - win_output_start;
                let source_start = w * hop_input;
                let mut source_idx = source_start + pos_in_win;

                // For looped clips, wrap source index using modulo
                if clip.loop_count > 1 && !buf.is_empty() {
                    source_idx = source_idx % buf.len();
                }

                if source_idx >= buf.len() {
                    continue;
                }

                // Hann window for smooth crossfade
                let t = pos_in_win as f32 / window_size as f32;
                let hann = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * t).cos());

                sample_val += buf[source_idx] * hann;
                weight_sum += hann;
            }

            if weight_sum > 0.001 {
                sample_val /= weight_sum;
            }

            let mut gain = 1.0f32;

            // Per-clip fade in
            if clip.fade_in_samples > 0 {
                let pos_in_clip = global_sample - clip.start_sample;
                if pos_in_clip < clip.fade_in_samples {
                    gain *= pos_in_clip as f32 / clip.fade_in_samples as f32;
                }
            }

            // Per-clip fade out
            if clip.fade_out_samples > 0 {
                let pos_from_end = clip_visual_end - global_sample;
                if pos_from_end <= clip.fade_out_samples {
                    gain *= pos_from_end as f32 / clip.fade_out_samples as f32;
                }
            }

            // Auto-crossfade out
            if let Some(xf_start) = crossfade_out_start {
                if global_sample >= xf_start && crossfade_out_len > 0 {
                    let xf_pos = global_sample - xf_start;
                    let xf_gain = 1.0 - (xf_pos as f32 / crossfade_out_len as f32);
                    gain *= xf_gain;
                }
            }

            // Auto-crossfade in
            if let Some(xf_start) = crossfade_in_start {
                if global_sample >= xf_start && global_sample < xf_start + crossfade_in_len {
                    let xf_pos = global_sample - xf_start;
                    let xf_gain = xf_pos as f32 / crossfade_in_len as f32;
                    gain *= xf_gain;
                }
            }

            // Apply clip gain (dB) before track processing
            if clip.gain_db.abs() > 0.001 {
                gain *= 10.0_f32.powf(clip.gain_db / 20.0);
            }

            output[i] += sample_val * gain;
        }
    }
}

/// Get interpolated automation value at a given sample position.
fn get_automation_value(
    automation: &[jamhub_model::AutomationLane],
    param: &jamhub_model::AutomationParam,
    sample: u64,
    default: f32,
) -> f32 {
    let lane = automation.iter().find(|l| &l.parameter == param);
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

/// Apply effect parameter automation: for each automatable param of this effect,
/// check if there's an automation lane and override the param value at the current position.
fn apply_effect_automation(
    effect: &jamhub_model::TrackEffect,
    slot_index: usize,
    automation: &[jamhub_model::AutomationLane],
    position_samples: u64,
) -> jamhub_model::TrackEffect {
    let mut result = effect.clone();
    for param_name in effect.automatable_params() {
        let auto_param = jamhub_model::AutomationParam::EffectParam {
            slot_index,
            param_name: param_name.to_string(),
        };
        if let Some(current) = effect.get_param(param_name) {
            let value = get_automation_value(automation, &auto_param, position_samples, current);
            if value != current {
                result = result.with_param(param_name, value);
            }
        }
    }
    result
}

fn pan_law(pan: f32) -> (f32, f32) {
    let angle = (pan + 1.0) * 0.25 * std::f32::consts::PI;
    (angle.cos(), angle.sin())
}
