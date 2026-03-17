use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use rayon::prelude::*;
use uuid::Uuid;

use jamhub_model::{ClipBufferId, Project};

use crate::effects::EffectProcessor;
use crate::synth::Synth;
use crate::vst3_host::Vst3Plugin;

/// PDC (Plugin Delay Compensation) information shared with the UI.
#[derive(Clone, Default, Debug)]
pub struct PdcState {
    /// Per-track total latency in samples (from VST3 plugins in the chain).
    pub track_latency: HashMap<Uuid, u32>,
    /// Maximum latency across all tracks — used to compute per-track compensation.
    pub max_latency: u32,
    /// Sample rate for converting samples to milliseconds in the UI.
    pub sample_rate: u32,
}

/// Thread-safe handle to PDC information, readable from the UI thread.
#[derive(Clone)]
pub struct PdcInfo {
    inner: Arc<RwLock<PdcState>>,
}

impl PdcInfo {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(PdcState::default())),
        }
    }

    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, PdcState> {
        self.inner.read()
    }

    fn write(&self) -> parking_lot::RwLockWriteGuard<'_, PdcState> {
        self.inner.write()
    }
}

/// Simple ring-buffer delay line for PDC compensation.
struct DelayBuffer {
    buffer: Vec<f32>,
    write_pos: usize,
    delay: usize,
}

impl DelayBuffer {
    fn new(max_delay: usize) -> Self {
        Self {
            buffer: vec![0.0; max_delay.max(1)],
            write_pos: 0,
            delay: 0,
        }
    }

    /// Set the current delay in samples. If the delay changes, the buffer is
    /// resized (and cleared) to avoid stale data.
    fn set_delay(&mut self, delay: usize) {
        if delay != self.delay || delay > self.buffer.len() {
            let capacity = delay.max(1);
            if capacity != self.buffer.len() {
                self.buffer = vec![0.0; capacity];
                self.write_pos = 0;
            }
            self.delay = delay;
        }
    }

    /// Process an entire block in-place: delay it by `self.delay` samples.
    fn process(&mut self, samples: &mut [f32]) {
        if self.delay == 0 {
            return;
        }
        for s in samples.iter_mut() {
            let read_pos = (self.write_pos + self.buffer.len() - self.delay) % self.buffer.len();
            let delayed = self.buffer[read_pos];
            self.buffer[self.write_pos] = *s;
            *s = delayed;
            self.write_pos = (self.write_pos + 1) % self.buffer.len();
        }
    }
}

pub struct Mixer {
    sample_rate: u32,
    channels: u16,
    processors: HashMap<Uuid, EffectProcessor>,
    /// Live VST3 plugin instances, keyed by EffectSlot ID
    vst_instances: HashMap<Uuid, Vst3Plugin>,
    /// Live VST3 instrument plugin instances, keyed by track ID
    vsti_instances: HashMap<Uuid, Vst3Plugin>,
    /// Dedicated effect processor for the master bus effects chain.
    master_processor: EffectProcessor,
    /// Per-track delay buffers for PDC compensation, keyed by track ID.
    pdc_delays: HashMap<Uuid, DelayBuffer>,
    /// Shared PDC information for the UI.
    pub pdc_info: PdcInfo,
    /// Pre-allocated reusable buffer for master output (avoids per-block allocation).
    output_buf: Vec<f32>,
    /// Pre-allocated reusable mono buffer for master effects downmix.
    master_mono_buf: Vec<f32>,
    /// Built-in synthesizers for MIDI tracks, keyed by track ID.
    synths: HashMap<Uuid, Synth>,
    /// Pre-allocated reusable mono buffer for per-track rendering (avoids per-block allocation).
    track_mono_buf: Vec<f32>,
    /// Pre-allocated reusable buffer for send routing (avoids per-block HashMap allocation).
    send_bufs: HashMap<Uuid, Vec<f32>>,
    /// Pre-allocated reusable buffer for pre-effect audio (avoids per-block HashMap allocation).
    pre_effect_bufs: HashMap<Uuid, Vec<f32>>,
    /// Pre-allocated reusable buffer for output target routing.
    output_target_bufs: HashMap<Uuid, Vec<f32>>,
}

impl Mixer {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            processors: HashMap::new(),
            vst_instances: HashMap::new(),
            vsti_instances: HashMap::new(),
            master_processor: EffectProcessor::new(sample_rate),
            pdc_delays: HashMap::new(),
            pdc_info: PdcInfo::new(),
            output_buf: Vec::new(),
            master_mono_buf: Vec::new(),
            synths: HashMap::new(),
            track_mono_buf: Vec::new(),
            send_bufs: HashMap::new(),
            pre_effect_bufs: HashMap::new(),
            output_target_bufs: HashMap::new(),
        }
    }

    /// Silence all sounding notes on a track's synth (reset all voices).
    pub fn silence_track_synth(&mut self, track_id: &Uuid) {
        if let Some(synth) = self.synths.get_mut(track_id) {
            synth.reset();
        }
    }

    /// Return the set of VST3 effect slot IDs whose plugins have crashed.
    pub fn crashed_plugin_ids(&self) -> std::collections::HashSet<Uuid> {
        self.vst_instances.iter()
            .filter(|(_, vst)| vst.crashed)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Load a VST3 plugin instance for a given effect slot.
    pub fn load_vst3(&mut self, slot_id: Uuid, path: &PathBuf) {
        let plugin = Vst3Plugin::load(path, self.sample_rate as f64, 256);
        if plugin.loaded {
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
        }
    }

    /// Unload a VST3 plugin instance.
    pub fn unload_vst3(&mut self, slot_id: &Uuid) {
        self.vst_instances.remove(slot_id);
    }

    /// Load a VST3 instrument plugin for a MIDI track.
    pub fn load_vsti(&mut self, track_id: Uuid, path: &PathBuf) {
        let plugin = Vst3Plugin::load(path, self.sample_rate as f64, 256);
        if plugin.loaded {
            self.vsti_instances.insert(track_id, plugin);
        } else {
            eprintln!(
                "Mixer: VSTi load failed for track {track_id}: {}",
                plugin.error.as_deref().unwrap_or("unknown")
            );
        }
    }

    /// Unload a VST3 instrument plugin from a MIDI track.
    pub fn unload_vsti(&mut self, track_id: &Uuid) {
        self.vsti_instances.remove(track_id);
    }

    /// Attach a parameter change receiver to a loaded VSTi plugin.
    pub fn attach_vsti_param_rx(&mut self, track_id: &Uuid, rx: crate::vst3_host::ParamChangeRx) {
        if let Some(vst) = self.vsti_instances.get_mut(track_id) {
            vst.param_change_rx = Some(rx);
        }
    }

    /// Return the set of track IDs whose VSTi instruments have crashed.
    pub fn crashed_vsti_ids(&self) -> std::collections::HashSet<Uuid> {
        self.vsti_instances.iter()
            .filter(|(_, vst)| vst.crashed)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Compute the total VST3 plugin latency for a track's effect chain.
    fn track_vst3_latency(&self, track: &jamhub_model::Track) -> u32 {
        let mut total: u32 = 0;
        for slot in &track.effects {
            if !slot.enabled {
                continue;
            }
            if let jamhub_model::TrackEffect::Vst3Plugin { .. } = &slot.effect {
                if let Some(vst) = self.vst_instances.get(&slot.id) {
                    total = total.saturating_add(vst.latency_samples);
                }
            }
        }
        total
    }

    /// Recalculate PDC delay values for all tracks and update shared state.
    fn update_pdc(&mut self, project: &Project) {
        let mut track_latency: HashMap<Uuid, u32> = HashMap::new();
        let mut max_latency: u32 = 0;

        for track in &project.tracks {
            let lat = self.track_vst3_latency(track);
            if lat > 0 {
                track_latency.insert(track.id, lat);
            }
            if lat > max_latency {
                max_latency = lat;
            }
        }

        // Update per-track delay buffers
        for track in &project.tracks {
            let lat = track_latency.get(&track.id).copied().unwrap_or(0);
            let compensation = (max_latency - lat) as usize;

            let delay_buf = self.pdc_delays.entry(track.id).or_insert_with(|| {
                DelayBuffer::new(compensation.max(1))
            });
            delay_buf.set_delay(compensation);
        }

        // Write shared state for UI
        let mut pdc = self.pdc_info.write();
        pdc.track_latency = track_latency;
        pdc.max_latency = max_latency;
        pdc.sample_rate = self.sample_rate;
    }

    pub fn render_block(
        &mut self,
        project: &Project,
        position_samples: u64,
        block_size: usize,
        audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    ) -> Vec<f32> {
        let num_samples = block_size * self.channels as usize;
        // Reuse pre-allocated output buffer (only reallocates if block size grows)
        self.output_buf.resize(num_samples, 0.0);
        self.output_buf.fill(0.0);
        let mut output = std::mem::take(&mut self.output_buf);

        // Recalculate PDC compensation values
        self.update_pdc(project);

        let any_solo = project.tracks.iter().any(|t| t.solo);

        // Reuse pre-allocated send buffers — clear values but keep allocations
        for buf in self.send_bufs.values_mut() {
            buf.resize(block_size, 0.0);
            buf.fill(0.0);
        }
        let mut send_buffers = std::mem::take(&mut self.send_bufs);

        // Reuse pre-allocated pre-effect audio buffers
        // Clear all buffers but keep allocations. Tracks that produce audio
        // will overwrite their buffer; we mark which ones are active below.
        for buf in self.pre_effect_bufs.values_mut() {
            buf.clear();
        }
        let mut pre_effect_audio = std::mem::take(&mut self.pre_effect_bufs);

        let block_end = position_samples + block_size as u64;

        // Collect audio tracks eligible for parallel rendering
        let audio_tracks: Vec<&jamhub_model::Track> = project.tracks.iter().filter(|track| {
            if track.muted { return false; }
            if any_solo && !track.solo { return false; }
            if track.kind == jamhub_model::TrackKind::Midi { return false; }
            track.clips.iter().any(|clip| {
                if clip.muted { return false; }
                let clip_end = clip.start_sample + clip.visual_duration_samples();
                clip.start_sample < block_end && clip_end > position_samples
            })
        }).collect();

        // Parallel render: each audio track's clips are rendered independently
        // using rayon, then results are collected back. We call the free function
        // render_track_clips_impl directly to avoid capturing &self (Mixer is !Sync
        // due to raw pointers in VST3 plugin fields).
        let parallel_results: Vec<(Uuid, Vec<f32>)> = audio_tracks
            .par_iter()
            .filter_map(|track| {
                let track_mono = render_track_clips_impl(track, position_samples, block_size, audio_buffers);
                if track_mono.iter().any(|&s| s != 0.0) {
                    Some((track.id, track_mono))
                } else {
                    None
                }
            })
            .collect();

        for (id, buf) in parallel_results {
            pre_effect_audio.insert(id, buf);
        }

        // MIDI tracks require &mut self (synths/VSTi), so render sequentially
        for track in &project.tracks {
            if track.muted { continue; }
            if any_solo && !track.solo { continue; }
            if track.kind != jamhub_model::TrackKind::Midi { continue; }
            let has_overlapping_clip = track.clips.iter().any(|clip| {
                if clip.muted { return false; }
                let clip_end = clip.start_sample + clip.visual_duration_samples();
                clip.start_sample < block_end && clip_end > position_samples
            });
            if !has_overlapping_clip { continue; }

            let track_mono = self.render_midi_track(track, position_samples, block_size, &project.tempo);
            if track_mono.iter().any(|&s| s != 0.0) {
                pre_effect_audio.insert(track.id, track_mono);
            }
        }

        // ---- Single pass: process ALL tracks uniformly ----
        // Each track renders its own clips (if any), receives send/routed audio,
        // applies effects, and mixes to output. "Bus" tracks just happen to have
        // no clips, so they only get send audio — but the code path is the same.
        for buf in self.output_target_bufs.values_mut() {
            buf.resize(block_size, 0.0);
            buf.fill(0.0);
        }
        let mut output_target_buffers = std::mem::take(&mut self.output_target_bufs);

        for track in &project.tracks {
            if track.muted {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }

            // Start with clip audio (if any) — take ownership to avoid clone
            let mut track_mono = pre_effect_audio.remove(&track.id)
                .unwrap_or_else(|| {
                    // Reuse the shared track mono buffer when no pre-effect audio exists
                    let mut buf = std::mem::take(&mut self.track_mono_buf);
                    buf.resize(block_size, 0.0);
                    buf.fill(0.0);
                    buf
                });

            // Mix in audio from sends targeting this track
            if let Some(send_audio) = send_buffers.remove(&track.id) {
                for i in 0..block_size {
                    track_mono[i] += send_audio[i];
                }
            }

            // Mix in audio from tracks routed via output_target to this track
            if let Some(routed) = output_target_buffers.remove(&track.id) {
                for i in 0..block_size {
                    track_mono[i] += routed[i];
                }
            }

            // If there's no audio at all, skip further processing
            let has_audio = track_mono.iter().any(|&s| s != 0.0);
            if !has_audio {
                continue;
            }

            // Mono summing: when enabled, average L+R channels before processing.
            // In our current mono pipeline this is a no-op, but the flag is respected
            // for future stereo expansion.

            // Phase invert: multiply all samples by -1.0
            if track.phase_inverted {
                for s in track_mono.iter_mut() {
                    *s = -*s;
                }
            }

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

            // Apply PDC delay compensation (aligns tracks with lower latency)
            if let Some(delay_buf) = self.pdc_delays.get_mut(&track.id) {
                delay_buf.process(&mut track_mono);
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
                    .or_insert_with(|| {
                        let mut v = Vec::with_capacity(block_size);
                        v.resize(block_size, 0.0);
                        v
                    });
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
                // Route to another track instead of master
                let target_buf = output_target_buffers
                    .entry(target_id)
                    .or_insert_with(|| {
                        let mut v = Vec::with_capacity(block_size);
                        v.resize(block_size, 0.0);
                        v
                    });
                for i in 0..block_size {
                    target_buf[i] += track_mono[i] * auto_volume;
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

        // Stash pre-allocated buffers back for reuse next block
        self.send_bufs = send_buffers;
        self.pre_effect_bufs = pre_effect_audio;
        self.output_target_bufs = output_target_buffers;

        // Return the buffer, stashing the allocation back for reuse next block.
        // The clone is an unavoidable memcpy — the caller (audio channel) consumes the Vec
        // while we retain the allocation for the next render_block call.
        let result = output.clone();
        self.output_buf = output;
        result
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

        // Downmix to mono for effect processing (reuse pre-allocated buffer)
        self.master_mono_buf.resize(frames, 0.0);
        let mut mono = std::mem::take(&mut self.master_mono_buf);
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
        // Stash the buffer back for reuse
        self.master_mono_buf = mono;
    }

    /// Collect MIDI note-on and note-off events for the current block from a track's clips.
    /// Returns (notes_on, notes_off) where:
    /// - notes_on: Vec<(sample_offset_in_block, pitch, velocity)>
    /// - notes_off: Vec<(sample_offset_in_block, pitch)>
    fn collect_midi_events(
        track: &jamhub_model::Track,
        position_samples: u64,
        block_size: usize,
        tempo: &jamhub_model::Tempo,
        sample_rate: u32,
    ) -> (Vec<(i32, u8, u8)>, Vec<(i32, u8)>) {
        let mut notes_on: Vec<(i32, u8, u8)> = Vec::new();
        let mut notes_off: Vec<(i32, u8)> = Vec::new();
        let block_end = position_samples + block_size as u64;

        let ticks_per_beat = 480.0_f64;
        let samples_per_beat = tempo.samples_per_beat(sample_rate as f64);
        let samples_per_tick = samples_per_beat / ticks_per_beat;

        for clip in &track.clips {
            if clip.muted { continue; }
            let clip_visual_end = clip.start_sample + clip.visual_duration_samples();
            if position_samples >= clip_visual_end || block_end <= clip.start_sample { continue; }

            if let jamhub_model::ClipSource::Midi { ref notes, .. } = clip.source {
                for note in notes {
                    let note_on_abs = clip.start_sample + (note.start_tick as f64 * samples_per_tick) as u64;
                    let note_off_abs = clip.start_sample + ((note.start_tick + note.duration_ticks) as f64 * samples_per_tick) as u64;

                    if note_on_abs >= position_samples && note_on_abs < block_end {
                        let offset = (note_on_abs - position_samples) as i32;
                        notes_on.push((offset, note.pitch, note.velocity));
                    }
                    if note_off_abs >= position_samples && note_off_abs < block_end {
                        let offset = (note_off_abs - position_samples) as i32;
                        notes_off.push((offset, note.pitch));
                    }
                }
            }
        }

        (notes_on, notes_off)
    }

    /// Render MIDI clips for a track through the built-in synthesizer or a VSTi plugin.
    fn render_midi_track(
        &mut self,
        track: &jamhub_model::Track,
        position_samples: u64,
        block_size: usize,
        tempo: &jamhub_model::Tempo,
    ) -> Vec<f32> {
        // Check if this track has a VSTi instrument assigned
        let use_vsti = track.instrument_plugin.is_some()
            && self.vsti_instances.contains_key(&track.id);

        if use_vsti {
            return self.render_midi_track_vsti(track, position_samples, block_size, tempo);
        }

        let synth = self.synths.entry(track.id).or_insert_with(Synth::new);

        // Update synth parameters from track settings
        synth.update_params(
            &track.synth_wave,
            track.synth_attack,
            track.synth_decay,
            track.synth_sustain,
            track.synth_release,
            track.synth_cutoff,
        );

        let mut track_mono = vec![0.0f32; block_size];
        let block_end = position_samples + block_size as u64;

        for clip in &track.clips {
            if clip.muted {
                continue;
            }
            let clip_visual_end = clip.start_sample + clip.visual_duration_samples();
            if position_samples >= clip_visual_end || block_end <= clip.start_sample {
                continue;
            }

            if let jamhub_model::ClipSource::Midi { ref notes, .. } = clip.source {
                let rendered = synth.render_block(
                    notes,
                    clip.start_sample,
                    position_samples,
                    block_size,
                    self.sample_rate,
                    tempo,
                );
                // Mix this clip's audio into the track buffer
                for i in 0..block_size {
                    let global_sample = position_samples + i as u64;
                    // Only add audio within the clip's visual boundaries
                    if global_sample >= clip.start_sample && global_sample < clip_visual_end {
                        let mut gain = 1.0f32;
                        // Per-clip gain
                        if clip.gain_db.abs() > 0.001 {
                            gain *= 10.0_f32.powf(clip.gain_db / 20.0);
                        }
                        // Per-clip fade in
                        if clip.fade_in_samples > 0 {
                            let pos_in_clip = global_sample - clip.start_sample;
                            if pos_in_clip < clip.fade_in_samples {
                                let t = pos_in_clip as f32 / clip.fade_in_samples as f32;
                                gain *= clip.fade_in_curve.apply(t);
                            }
                        }
                        // Per-clip fade out
                        if clip.fade_out_samples > 0 {
                            let pos_from_end = clip_visual_end - global_sample;
                            if pos_from_end <= clip.fade_out_samples {
                                let t = pos_from_end as f32 / clip.fade_out_samples as f32;
                                gain *= clip.fade_out_curve.apply(t);
                            }
                        }
                        track_mono[i] += rendered[i] * gain;
                    }
                }
            }
        }

        track_mono
    }

    /// Render MIDI clips for a track through a VSTi instrument plugin.
    fn render_midi_track_vsti(
        &mut self,
        track: &jamhub_model::Track,
        position_samples: u64,
        block_size: usize,
        tempo: &jamhub_model::Tempo,
    ) -> Vec<f32> {
        let mut track_mono = vec![0.0f32; block_size];

        // Collect MIDI events for this block across all clips
        let (notes_on, notes_off) = Self::collect_midi_events(
            track, position_samples, block_size, tempo, self.sample_rate,
        );

        // Process through the VSTi plugin
        if let Some(vsti) = self.vsti_instances.get_mut(&track.id) {
            vsti.apply_pending_param_changes();
            vsti.process_with_midi(&notes_on, &notes_off, &mut track_mono);
        }

        track_mono
    }
}

/// Render the raw clip audio for a track (pre-effects) into a mono buffer.
/// Free function so it can be called from rayon parallel iterators without
/// requiring Mixer: Sync.
fn render_track_clips_impl(
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
            // Speed and pitch are independent:
            // - playback_rate controls speed (duration changes)
            // - transpose_semitones controls pitch only (duration unchanged)
            let speed_rate = clip.playback_rate.max(0.01);
            let transpose_factor = if clip.transpose_semitones != 0 {
                2.0_f32.powf(clip.transpose_semitones as f32 / 12.0)
            } else {
                1.0
            };
            let needs_pitch_shift = clip.transpose_semitones != 0;
            let needs_speed_preserve = clip.preserve_pitch && (speed_rate - 1.0).abs() > 0.001;

            // OLA rate: controls how source samples map to output samples within OLA windows.
            // This determines the PITCH change. Duration is handled by visual_duration.
            //
            // - transpose only: ola_rate = transpose_factor (pitch shift, duration unchanged)
            // - speed + preserve_pitch: ola_rate = 1.0 (no pitch change, duration from visual_duration)
            // - both: ola_rate = transpose_factor (pitch shift + duration from visual_duration)
            // - speed without preserve: no OLA, simple resample at speed_rate (pitch changes with speed)
            let ola_rate = if needs_pitch_shift || needs_speed_preserve {
                transpose_factor // OLA handles pitch; visual_duration handles speed
            } else {
                1.0 // unused since we won't use OLA
            };

            // For non-OLA path: simple resampling rate
            let simple_rate = speed_rate;

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
                    // Use OLA when pitch and speed need to be independent
                    let use_ola = needs_pitch_shift || needs_speed_preserve;
                    if use_ola {
                        render_clip_ola_impl(
                            clip, buf, ola_rate, speed_rate, visual_duration, clip_visual_end,
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
                            let mut source_pos = visual_offset * simple_rate as f64 + clip.content_offset as f64;
                            if clip.loop_count > 1 && buf_len > 0 {
                                source_pos = source_pos % buf_len as f64;
                            }

                            // Non-destructive reverse: read from the end of the buffer backwards
                            if clip.reversed && buf_len > 0 {
                                source_pos = (buf_len as f64 - 1.0 - source_pos).max(0.0);
                            }

                            let source_idx = source_pos.floor() as usize;
                            let frac = source_pos - source_pos.floor();

                            if source_idx >= buf.len() {
                                continue;
                            }

                            // Linear interpolation between adjacent samples
                            // When reversed, interpolate towards the lower index
                            let (s0, s1) = if clip.reversed {
                                let a = buf[source_idx];
                                let b = if source_idx > 0 { buf[source_idx - 1] } else { a };
                                (a, b)
                            } else {
                                let a = buf[source_idx];
                                let b = if source_idx + 1 < buf.len() { buf[source_idx + 1] } else { a };
                                (a, b)
                            };
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

/// Render a clip using Overlap-Add (OLA) for independent pitch/speed control.
///
/// `pitch_rate`: controls pitch shift (transpose_factor). 1.0 = no pitch change.
///   > 1.0 reads source faster → higher pitch. < 1.0 → lower pitch.
/// `speed_rate`: controls playback speed. Duration = source_len / speed_rate.
///   Speed is already accounted for by visual_duration/clip_visual_end.
///
/// The OLA algorithm:
/// 1. Maps each output sample to a source position based on speed_rate
/// 2. Within each OLA window, reads source at pitch_rate to change pitch
/// 3. Overlapping windows with Hann weighting create smooth output
#[allow(clippy::too_many_arguments)]
fn render_clip_ola_impl(
    clip: &jamhub_model::Clip,
        buf: &[f32],
        pitch_rate: f32,
        speed_rate: f32,
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
        let window_size: usize = 1024;
        let hop_output = (window_size as f32 * 0.5) as usize; // 50% overlap in output

        // hop_input controls how much source we advance per output window.
        // pitch_rate > 1.0: read more source per window → higher pitch
        // speed_rate is handled by visual_duration (output is shorter/longer)
        //
        // For the source mapping within visual_duration:
        // Each output sample at visual_offset maps to source at visual_offset * speed_rate
        // Then within OLA windows, pitch_rate shifts which source samples we actually read.
        let hop_input = (hop_output as f32 * pitch_rate).max(1.0) as usize;

        if hop_output == 0 || hop_input == 0 {
            return;
        }

        for i in 0..block_size {
            let global_sample = position_samples + i as u64;
            if global_sample < clip.start_sample || global_sample >= clip_visual_end {
                continue;
            }

            let visual_offset = (global_sample - clip.start_sample) as usize;

            // Map visual offset to source offset (accounting for speed)
            let source_offset_base = (visual_offset as f64 * speed_rate as f64) as usize;

            // Determine which OLA window this output sample falls in
            let window_idx = visual_offset / hop_output;

            let mut sample_val = 0.0f32;
            let mut weight_sum = 0.0f32;

            let start_win = if window_idx > 0 { window_idx - 1 } else { 0 };
            let end_win = window_idx + 1;

            for w in start_win..=end_win {
                let win_output_start = w * hop_output;
                if visual_offset < win_output_start || visual_offset >= win_output_start + window_size {
                    continue;
                }

                let pos_in_win = visual_offset - win_output_start;

                // Source position for this window:
                // Base position = source_offset of window start + offset within window scaled by pitch
                let win_source_base = (win_output_start as f64 * speed_rate as f64) as usize;
                let source_in_win = (pos_in_win as f32 * pitch_rate) as usize;
                let mut source_idx = win_source_base + source_in_win + clip.content_offset as usize;

                // For looped clips, wrap source index using modulo
                if clip.loop_count > 1 && !buf.is_empty() {
                    source_idx = source_idx % buf.len();
                }

                if source_idx >= buf.len() {
                    continue;
                }

                // Non-destructive reverse for OLA path
                if clip.reversed && !buf.is_empty() {
                    source_idx = buf.len() - 1 - source_idx.min(buf.len() - 1);
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
                    let t = pos_in_clip as f32 / clip.fade_in_samples as f32;
                    gain *= clip.fade_in_curve.apply(t);
                }
            }

            // Per-clip fade out
            if clip.fade_out_samples > 0 {
                let pos_from_end = clip_visual_end - global_sample;
                if pos_from_end <= clip.fade_out_samples {
                    let t = pos_from_end as f32 / clip.fade_out_samples as f32;
                    gain *= clip.fade_out_curve.apply(t);
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
