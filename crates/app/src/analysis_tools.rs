// ── Analysis & Production Tools ──────────────────────────────────────────────
//
// 1. Reference Track Comparison (A/B)
// 2. Correlation Meter (L/R phase correlation)
// 3. Loudness Matching (auto-compensate when bypassing effects)
// 4. Audio-to-MIDI Conversion (monophonic pitch detection)
// 5. Chord Detection (chroma-based analysis)
//

use std::path::PathBuf;

use eframe::egui;
use uuid::Uuid;

use jamhub_model::{Clip, ClipSource, MidiNote, TrackKind};

use crate::DawApp;
use jamhub_engine::EngineCommand;

// ═══════════════════════════════════════════════════════════════════════════════
// Data Types
// ═══════════════════════════════════════════════════════════════════════════════

/// A reference track imported for A/B comparison against the current mix.
pub struct ReferenceTrack {
    pub path: PathBuf,
    pub samples: Vec<f32>,       // mono samples
    pub sample_rate: u32,
    pub playing: bool,
    pub gain_db: f32,
    /// Cached waveform peaks for display (downsampled).
    pub waveform_peaks: Vec<f32>,
}

/// Detected chord at a position in an audio clip.
#[derive(Debug, Clone)]
pub struct DetectedChord {
    pub start_seconds: f32,
    pub end_seconds: f32,
    pub name: String,
}

/// State for loudness matching after effect bypass toggle.
pub struct LoudnessMatchState {
    /// RMS measurement captured before the toggle.
    pub rms_before_db: f32,
    /// Time when the toggle happened (to wait ~500ms before measuring "after").
    pub toggle_time: std::time::Instant,
    /// Whether we are waiting for the "after" measurement.
    pub waiting_for_after: bool,
    /// Track index where the effect was toggled.
    pub track_idx: usize,
    /// Slot index of the toggled effect.
    pub slot_idx: usize,
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Reference Track Comparison
// ═══════════════════════════════════════════════════════════════════════════════

impl ReferenceTrack {
    /// Load a reference track from a file path.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let audio_data = jamhub_engine::load_audio(path)?;
        // Mix to mono
        let channels = audio_data.channels as usize;
        let mono: Vec<f32> = if channels == 1 {
            audio_data.samples.clone()
        } else {
            audio_data
                .samples
                .chunks(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect()
        };

        // Build waveform peaks for display (downsample to ~2000 points)
        let target_peaks = 2000usize;
        let chunk_size = (mono.len() / target_peaks).max(1);
        let waveform_peaks: Vec<f32> = mono
            .chunks(chunk_size)
            .map(|chunk| chunk.iter().fold(0.0f32, |acc, &s| acc.max(s.abs())))
            .collect();

        Ok(Self {
            path: path.to_path_buf(),
            samples: mono,
            sample_rate: audio_data.sample_rate,
            playing: false,
            gain_db: 0.0,
            waveform_peaks,
        })
    }

    /// Compute RMS level in dB.
    fn rms_db(&self) -> f32 {
        if self.samples.is_empty() {
            return -96.0;
        }
        let sum_sq: f64 = self.samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum_sq / self.samples.len() as f64).sqrt();
        if rms < 1e-10 {
            -96.0
        } else {
            (20.0 * rms.log10()) as f32
        }
    }

    /// Get the linear gain multiplier from gain_db.
    pub fn gain_linear(&self) -> f32 {
        10.0f32.powf(self.gain_db / 20.0)
    }
}

/// Draw the Reference Track A/B comparison window.
fn show_reference_track(app: &mut DawApp, ctx: &egui::Context) {
    let mut open = app.show_analysis;
    egui::Window::new("Reference Track A/B")
        .id(egui::Id::new("analysis_reference_track"))
        .open(&mut open)
        .default_width(500.0)
        .default_height(300.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Import Reference...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Audio", &["wav", "mp3", "flac", "ogg", "aiff"])
                        .pick_file()
                    {
                        match ReferenceTrack::load(&path) {
                            Ok(rt) => {
                                app.set_status(&format!(
                                    "Loaded reference: {}",
                                    path.file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("unknown")
                                ));
                                app.reference_track = Some(rt);
                            }
                            Err(e) => {
                                app.set_status(&format!("Failed to load reference: {e}"));
                            }
                        }
                    }
                }

                if app.reference_track.is_some() {
                    if ui.button("Remove Reference").clicked() {
                        app.reference_track = None;
                        app.ab_mode = false;
                        app.send_command(EngineCommand::StopReference);
                    }
                }
            });

            ui.separator();

            if app.reference_track.is_none() {
                ui.label(
                    egui::RichText::new("No reference track loaded. Click \"Import Reference...\" to begin.")
                        .color(egui::Color32::from_rgb(120, 118, 112)),
                );
            } else {
                // Extract display info without holding a mutable borrow
                let filename = app.reference_track.as_ref()
                    .and_then(|rt| rt.path.file_name().and_then(|n| n.to_str()).map(|s| s.to_string()))
                    .unwrap_or_else(|| "unknown".to_string());
                let duration_secs = app.reference_track.as_ref()
                    .map(|rt| rt.samples.len() as f64 / rt.sample_rate as f64)
                    .unwrap_or(0.0);
                let rt_sample_rate = app.reference_track.as_ref().map(|rt| rt.sample_rate).unwrap_or(44100);
                let peaks: Vec<f32> = app.reference_track.as_ref()
                    .map(|rt| rt.waveform_peaks.clone())
                    .unwrap_or_default();
                let current_gain_db = app.reference_track.as_ref().map(|rt| rt.gain_db).unwrap_or(0.0);

                ui.label(
                    egui::RichText::new(format!(
                        "{filename}  |  {:.1}s  |  {}Hz",
                        duration_secs, rt_sample_rate
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 158, 150)),
                );

                ui.add_space(4.0);

                // Waveform display
                let desired_height = 60.0;
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), desired_height),
                    egui::Sense::hover(),
                );
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(16, 16, 20));

                if !peaks.is_empty() {
                    let n = peaks.len();
                    let w = rect.width();
                    let mid_y = rect.center().y;
                    let half_h = rect.height() / 2.0 - 2.0;
                    let color = if app.ab_mode {
                        egui::Color32::from_rgb(80, 200, 120)
                    } else {
                        egui::Color32::from_rgb(60, 120, 180)
                    };

                    for i in 0..n {
                        let x = rect.min.x + (i as f32 / n as f32) * w;
                        let peak = peaks[i].min(1.0);
                        let h = peak * half_h;
                        painter.line_segment(
                            [
                                egui::pos2(x, mid_y - h),
                                egui::pos2(x, mid_y + h),
                            ],
                            egui::Stroke::new(1.0, color),
                        );
                    }
                }

                ui.add_space(6.0);

                // A/B toggle — big prominent button
                let ab_text = if app.ab_mode { "B (Reference)" } else { "A (Project)" };
                let ab_color = if app.ab_mode {
                    egui::Color32::from_rgb(80, 200, 120)
                } else {
                    egui::Color32::from_rgb(100, 160, 240)
                };

                let btn = egui::Button::new(
                    egui::RichText::new(ab_text).size(16.0).strong().color(egui::Color32::WHITE),
                )
                .fill(ab_color.gamma_multiply(0.4))
                .min_size(egui::vec2(160.0, 36.0));

                if ui.add(btn).clicked() {
                    app.ab_mode = !app.ab_mode;
                    if app.ab_mode {
                        if let Some(ref rt) = app.reference_track {
                            let gain = rt.gain_linear();
                            let scaled: Vec<f32> = rt.samples.iter().map(|&s| s * gain).collect();
                            app.send_command(EngineCommand::PlayReference {
                                samples: scaled,
                                sample_rate: rt.sample_rate,
                            });
                        }
                    } else {
                        app.send_command(EngineCommand::StopReference);
                    }
                }

                ui.add_space(4.0);

                // Volume slider
                let mut gain = current_gain_db;
                let slider = egui::Slider::new(&mut gain, -24.0..=12.0)
                    .suffix(" dB")
                    .text("Ref Volume")
                    .fixed_decimals(1);
                if ui.add(slider).changed() {
                    if let Some(ref mut rt) = app.reference_track {
                        rt.gain_db = gain;
                    }
                    // If currently in A/B mode, update the engine
                    if app.ab_mode {
                        if let Some(ref rt) = app.reference_track {
                            let gain_lin = 10.0f32.powf(gain / 20.0);
                            let scaled: Vec<f32> =
                                rt.samples.iter().map(|&s| s * gain_lin).collect();
                            app.send_command(EngineCommand::PlayReference {
                                samples: scaled,
                                sample_rate: rt.sample_rate,
                            });
                        }
                    }
                }

                ui.add_space(4.0);

                // Auto Match Loudness
                if ui
                    .button("Auto Match Loudness")
                    .on_hover_text("Adjust reference gain to match project RMS")
                    .clicked()
                {
                    let project_db = app.engine.as_ref()
                        .map(|e| e.lufs.read().momentary as f32)
                        .unwrap_or(-60.0);
                    let ref_db = app.reference_track.as_ref()
                        .map(|rt| rt.rms_db())
                        .unwrap_or(-60.0);
                    let diff = (project_db - ref_db).clamp(-24.0, 24.0);
                    if let Some(ref mut rt) = app.reference_track {
                        rt.gain_db = diff;
                    }
                    app.set_status(&format!(
                        "Reference gain set to {:.1} dB to match project",
                        diff
                    ));
                }
            }
        });
    app.show_analysis = open;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Correlation Meter
// ═══════════════════════════════════════════════════════════════════════════════

/// Calculate stereo correlation from interleaved stereo samples.
/// Returns a value from -1.0 (out of phase) to +1.0 (mono/in phase).
#[allow(dead_code)]
pub fn calculate_correlation(interleaved: &[f32], channels: usize) -> f32 {
    if channels < 2 || interleaved.len() < channels * 2 {
        return 1.0; // mono is perfectly correlated
    }

    let mut sum_lr: f64 = 0.0;
    let mut sum_ll: f64 = 0.0;
    let mut sum_rr: f64 = 0.0;

    for frame in interleaved.chunks(channels) {
        if frame.len() < 2 {
            continue;
        }
        let l = frame[0] as f64;
        let r = frame[1] as f64;
        sum_lr += l * r;
        sum_ll += l * l;
        sum_rr += r * r;
    }

    let denom = (sum_ll * sum_rr).sqrt();
    if denom < 1e-12 {
        return 0.0; // silence — no meaningful correlation
    }
    (sum_lr / denom).clamp(-1.0, 1.0) as f32
}

/// Draw the correlation meter UI.
fn show_correlation_meter(app: &mut DawApp, ui: &mut egui::Ui) {
    // Read recent stereo samples from the spectrum buffer to calculate correlation
    let correlation = if let Some(ref engine) = app.engine {
        // Read the engine state's correlation value (calculated in engine loop)
        engine.state.read().correlation
    } else {
        0.0
    };

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Phase Correlation")
            .size(12.0)
            .strong()
            .color(egui::Color32::from_rgb(200, 198, 194)),
    );

    let meter_height = 18.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), meter_height), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 26));

    // The meter maps -1..+1 to the full width
    let normalized = (correlation + 1.0) / 2.0; // 0.0 = -1, 0.5 = 0, 1.0 = +1
    let center_x = rect.min.x + rect.width() * 0.5;
    let indicator_x = rect.min.x + rect.width() * normalized;

    // Color based on value
    let bar_color = if correlation > 0.3 {
        egui::Color32::from_rgb(60, 200, 120) // green — good
    } else if correlation > 0.0 {
        egui::Color32::from_rgb(200, 200, 60) // yellow — caution
    } else {
        egui::Color32::from_rgb(240, 60, 60) // red — out of phase
    };

    // Draw bar from center to indicator
    let bar_left = center_x.min(indicator_x);
    let bar_right = center_x.max(indicator_x);
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(bar_left, rect.min.y + 2.0),
        egui::pos2(bar_right, rect.max.y - 2.0),
    );
    painter.rect_filled(bar_rect, 2.0, bar_color);

    // Center line
    painter.line_segment(
        [
            egui::pos2(center_x, rect.min.y),
            egui::pos2(center_x, rect.max.y),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 100, 110)),
    );

    // Labels
    let label_color = egui::Color32::from_rgb(140, 138, 132);
    let small_font = egui::FontId::proportional(9.0);
    painter.text(
        egui::pos2(rect.min.x + 4.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "OUT -1",
        small_font.clone(),
        label_color,
    );
    painter.text(
        egui::pos2(center_x, rect.min.y - 1.0),
        egui::Align2::CENTER_BOTTOM,
        "0",
        small_font.clone(),
        label_color,
    );
    painter.text(
        egui::pos2(rect.max.x - 4.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "+1 MONO",
        small_font,
        label_color,
    );

    // Numeric value
    ui.label(
        egui::RichText::new(format!("{:.2}", correlation))
            .size(11.0)
            .color(bar_color),
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Loudness Matching
// ═══════════════════════════════════════════════════════════════════════════════

/// Draw the Loudness Matching controls.
fn show_loudness_matching(app: &mut DawApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new("Loudness Match")
            .size(12.0)
            .strong()
            .color(egui::Color32::from_rgb(200, 198, 194)),
    );

    ui.horizontal(|ui| {
        let label = if app.loudness_match_enabled {
            "Loudness Match ON"
        } else {
            "Loudness Match OFF"
        };
        let color = if app.loudness_match_enabled {
            egui::Color32::from_rgb(80, 200, 120)
        } else {
            egui::Color32::from_rgb(120, 118, 112)
        };

        let btn = egui::Button::new(egui::RichText::new(label).color(color))
            .fill(if app.loudness_match_enabled {
                egui::Color32::from_rgb(30, 60, 40)
            } else {
                egui::Color32::from_rgb(34, 35, 42)
            });

        if ui.add(btn).on_hover_text(
            "When enabled, bypassing an effect auto-compensates volume difference",
        ).clicked() {
            app.loudness_match_enabled = !app.loudness_match_enabled;
            if !app.loudness_match_enabled {
                app.loudness_compensation_db = 0.0;
            }
        }

        if app.loudness_compensation_db.abs() > 0.01 {
            let sign = if app.loudness_compensation_db > 0.0 { "+" } else { "" };
            ui.label(
                egui::RichText::new(format!("{sign}{:.1} dB", app.loudness_compensation_db))
                    .size(12.0)
                    .color(egui::Color32::from_rgb(200, 180, 60)),
            );
        }
    });

    // Process pending loudness match measurement
    if let Some(ref lm_state) = app.loudness_match_state {
        if lm_state.waiting_for_after && lm_state.toggle_time.elapsed().as_millis() > 500 {
            // Capture "after" RMS
            let after_db = if let Some(ref engine) = app.engine {
                engine.lufs.read().momentary as f32
            } else {
                -60.0
            };
            let before_db = lm_state.rms_before_db;
            let compensation = before_db - after_db;
            let track_idx = lm_state.track_idx;

            // Apply compensation to track volume
            if let Some(track) = app.project.tracks.get_mut(track_idx) {
                let comp_linear = 10.0f32.powf(compensation / 20.0);
                track.volume = (track.volume * comp_linear).clamp(0.0, 4.0);
            }
            app.loudness_compensation_db = compensation;
            app.loudness_match_state = None;
            app.sync_project();
        }
    }

    ui.label(
        egui::RichText::new("Tip: bypass an effect while this is ON to auto-compensate volume.")
            .size(9.5)
            .color(egui::Color32::from_rgb(100, 98, 94)),
    );
}

/// Called when an effect is bypassed/enabled and loudness matching is active.
/// Captures "before" RMS and schedules measurement of "after".
pub fn on_effect_bypass_toggled(app: &mut DawApp, track_idx: usize, slot_idx: usize) {
    if !app.loudness_match_enabled {
        return;
    }

    // Capture current RMS from LUFS momentary
    let rms_before_db = if let Some(ref engine) = app.engine {
        engine.lufs.read().momentary as f32
    } else {
        return;
    };

    app.loudness_match_state = Some(LoudnessMatchState {
        rms_before_db,
        toggle_time: std::time::Instant::now(),
        waiting_for_after: true,
        track_idx,
        slot_idx,
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Audio-to-MIDI Conversion (YIN-based monophonic pitch detection)
// ═══════════════════════════════════════════════════════════════════════════════

/// Convert an audio buffer to MIDI notes using simplified YIN pitch detection.
///
/// Only detects monophonic melodies (single notes, not chords).
/// Returns MIDI notes with timing in ticks relative to the clip start.
pub fn audio_to_midi(
    samples: &[f32],
    sample_rate: u32,
    tempo_bpm: f64,
) -> Vec<MidiNote> {
    if samples.is_empty() || sample_rate == 0 {
        return Vec::new();
    }

    let frame_size: usize = 2048;
    let hop_size: usize = 512;
    let min_note_duration_samples = (sample_rate as f64 * 0.05) as usize; // 50ms minimum
    let yin_threshold: f32 = 0.15;

    // Minimum/maximum detectable frequency (MIDI range ~C2 to C7)
    let min_freq = 65.0f32;  // ~C2
    let max_freq = 2100.0f32; // ~C7
    let min_period = (sample_rate as f32 / max_freq) as usize;
    let max_period = (sample_rate as f32 / min_freq) as usize;

    // Convert sample position to MIDI ticks (480 PPQN)
    let ppqn = 480.0;
    let samples_per_beat = sample_rate as f64 * 60.0 / tempo_bpm;
    let samples_per_tick = samples_per_beat / ppqn;

    struct PitchFrame {
        sample_pos: usize,
        midi_note: Option<u8>,
    }

    let mut frames: Vec<PitchFrame> = Vec::new();

    let mut pos = 0usize;
    while pos + frame_size <= samples.len() {
        let frame = &samples[pos..pos + frame_size];

        // Check if frame has enough energy (skip silence)
        let energy: f32 = frame.iter().map(|&s| s * s).sum::<f32>() / frame.len() as f32;
        if energy < 1e-6 {
            frames.push(PitchFrame {
                sample_pos: pos,
                midi_note: None,
            });
            pos += hop_size;
            continue;
        }

        // YIN difference function
        let tau_max = max_period.min(frame_size / 2);
        let tau_min = min_period.max(2);
        let mut diff = vec![0.0f32; tau_max];

        for tau in tau_min..tau_max {
            let mut sum = 0.0f32;
            for j in 0..(frame_size - tau) {
                let d = frame[j] - frame[j + tau];
                sum += d * d;
            }
            diff[tau] = sum;
        }

        // Cumulative mean normalized difference function
        let mut cmnd = vec![0.0f32; tau_max];
        if tau_max > 0 {
            cmnd[0] = 1.0;
        }
        let mut running_sum = 0.0f32;
        for tau in 1..tau_max {
            running_sum += diff[tau];
            cmnd[tau] = if running_sum > 1e-10 {
                diff[tau] * tau as f32 / running_sum
            } else {
                1.0
            };
        }

        // Find the first dip below threshold
        let mut detected_period: Option<usize> = None;
        let mut prev = 1.0f32;
        for tau in tau_min..tau_max {
            if cmnd[tau] < yin_threshold && cmnd[tau] < prev {
                // Found a dip — use parabolic interpolation for better accuracy
                let better_tau = if tau > 0 && tau + 1 < tau_max {
                    let s0 = cmnd[tau - 1];
                    let s1 = cmnd[tau];
                    let s2 = cmnd[tau + 1];
                    let denom = 2.0 * s1 - s2 - s0;
                    if denom.abs() > 1e-10 {
                        tau as f32 + (s0 - s2) / (2.0 * denom)
                    } else {
                        tau as f32
                    }
                } else {
                    tau as f32
                };

                let freq = sample_rate as f32 / better_tau;
                if freq >= min_freq && freq <= max_freq {
                    detected_period = Some(tau);
                    let midi_f = 69.0 + 12.0 * (freq / 440.0).log2();
                    let midi_note = midi_f.round() as i32;
                    if midi_note >= 0 && midi_note <= 127 {
                        frames.push(PitchFrame {
                            sample_pos: pos,
                            midi_note: Some(midi_note as u8),
                        });
                    } else {
                        frames.push(PitchFrame {
                            sample_pos: pos,
                            midi_note: None,
                        });
                    }
                }
                break;
            }
            prev = cmnd[tau];
        }

        if detected_period.is_none() {
            frames.push(PitchFrame {
                sample_pos: pos,
                midi_note: None,
            });
        }

        pos += hop_size;
    }

    // Group consecutive frames with the same MIDI note into notes
    let mut notes: Vec<MidiNote> = Vec::new();
    let mut current_note: Option<(u8, usize, usize)> = None; // (pitch, start_sample, end_sample)

    for frame in &frames {
        match (frame.midi_note, &mut current_note) {
            (Some(pitch), Some(ref mut note)) if pitch == note.0 => {
                // Same note continues
                note.2 = frame.sample_pos + hop_size;
            }
            (Some(pitch), _) => {
                // New note (or different pitch)
                if let Some((prev_pitch, start, end)) = current_note.take() {
                    let duration = end.saturating_sub(start);
                    if duration >= min_note_duration_samples {
                        let start_tick = (start as f64 / samples_per_tick) as u64;
                        let dur_ticks = (duration as f64 / samples_per_tick).max(1.0) as u64;
                        notes.push(MidiNote {
                            pitch: prev_pitch,
                            velocity: 100,
                            start_tick,
                            duration_ticks: dur_ticks,
                        });
                    }
                }
                current_note = Some((pitch, frame.sample_pos, frame.sample_pos + hop_size));
            }
            (None, Some(_)) => {
                // Note ended
                if let Some((prev_pitch, start, end)) = current_note.take() {
                    let duration = end.saturating_sub(start);
                    if duration >= min_note_duration_samples {
                        let start_tick = (start as f64 / samples_per_tick) as u64;
                        let dur_ticks = (duration as f64 / samples_per_tick).max(1.0) as u64;
                        notes.push(MidiNote {
                            pitch: prev_pitch,
                            velocity: 100,
                            start_tick,
                            duration_ticks: dur_ticks,
                        });
                    }
                }
            }
            (None, None) => {} // silence continues
        }
    }

    // Flush last note
    if let Some((pitch, start, end)) = current_note {
        let duration = end.saturating_sub(start);
        if duration >= min_note_duration_samples {
            let start_tick = (start as f64 / samples_per_tick) as u64;
            let dur_ticks = (duration as f64 / samples_per_tick).max(1.0) as u64;
            notes.push(MidiNote {
                pitch,
                velocity: 100,
                start_tick,
                duration_ticks: dur_ticks,
            });
        }
    }

    notes
}

/// Apply quantization to MIDI notes (snap to nearest grid).
fn quantize_notes(notes: &mut [MidiNote], grid_ticks: u64) {
    if grid_ticks == 0 {
        return;
    }
    for note in notes.iter_mut() {
        let remainder = note.start_tick % grid_ticks;
        if remainder > grid_ticks / 2 {
            note.start_tick += grid_ticks - remainder;
        } else {
            note.start_tick -= remainder;
        }
        // Snap duration to at least one grid unit
        if note.duration_ticks < grid_ticks {
            note.duration_ticks = grid_ticks;
        }
    }
}

/// Perform audio-to-MIDI conversion on a clip and create a new MIDI track.
pub fn convert_clip_to_midi(app: &mut DawApp, track_idx: usize, clip_idx: usize, quantize: bool) {
    let clip = match app.project.tracks.get(track_idx).and_then(|t| t.clips.get(clip_idx)) {
        Some(c) => c.clone(),
        None => {
            app.set_status("Cannot convert: clip not found");
            return;
        }
    };

    // Get the audio samples for this clip
    let samples = match &clip.source {
        ClipSource::AudioBuffer { buffer_id } => {
            match app.audio_buffers.get(buffer_id) {
                Some(buf) => buf.clone(),
                None => {
                    app.set_status("Cannot convert: audio buffer not found");
                    return;
                }
            }
        }
        ClipSource::AudioFile { path } => {
            match jamhub_engine::load_audio(std::path::Path::new(path)) {
                Ok(data) => data.samples,
                Err(e) => {
                    app.set_status(&format!("Cannot convert: {e}"));
                    return;
                }
            }
        }
        ClipSource::Midi { .. } => {
            app.set_status("Clip is already MIDI");
            return;
        }
    };

    let sr = app.sample_rate();
    let bpm = app.project.tempo.bpm;

    let mut notes = audio_to_midi(&samples, sr, bpm);

    if quantize && !notes.is_empty() {
        // Quantize to 16th notes (480 PPQN / 4 = 120 ticks per 16th)
        quantize_notes(&mut notes, 120);
    }

    if notes.is_empty() {
        app.set_status("No pitched content detected in this clip");
        return;
    }

    let note_count = notes.len();

    // Create a new MIDI track with the detected notes
    app.push_undo("Convert audio to MIDI");
    let track_name = format!(
        "{} (MIDI)",
        app.project.tracks[track_idx].clips[clip_idx].name
    );
    let new_track_id = app.project.add_track(&track_name, TrackKind::Midi);

    // Find the new track and add a clip
    if let Some(track) = app.project.tracks.iter_mut().find(|t| t.id == new_track_id) {
        track.clips.push(Clip {
            id: Uuid::new_v4(),
            name: format!("{} (converted)", clip.name),
            start_sample: clip.start_sample,
            duration_samples: clip.duration_samples,
            source: ClipSource::Midi {
                notes,
                cc_events: Vec::new(),
            },
            muted: false,
            fade_in_samples: 0,
            fade_out_samples: 0,
            color: clip.color,
            playback_rate: 1.0,
            preserve_pitch: false,
            loop_count: 1,
            gain_db: 0.0,
            take_index: 0,
            content_offset: 0,
            transpose_semitones: 0,
            reversed: false,
        });
    }

    app.sync_project();
    app.set_status(&format!(
        "Converted to MIDI: {note_count} notes detected"
    ));
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Chord Detection (chroma-based analysis)
// ═══════════════════════════════════════════════════════════════════════════════

/// Note names for display.
const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Major chord template (root, major 3rd, perfect 5th).
const MAJOR_TEMPLATE: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];

/// Minor chord template (root, minor 3rd, perfect 5th).
const MINOR_TEMPLATE: [f32; 12] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];

/// Dominant 7th chord template.
const DOM7_TEMPLATE: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0];

/// Minor 7th chord template.
const MIN7_TEMPLATE: [f32; 12] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0];

/// Diminished chord template.
const DIM_TEMPLATE: [f32; 12] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0];

/// Augmented chord template.
const AUG_TEMPLATE: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

/// Suspended 4th chord template.
const SUS4_TEMPLATE: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];

/// All chord templates with suffix labels.
const CHORD_TEMPLATES: [(&[f32; 12], &str); 7] = [
    (&MAJOR_TEMPLATE, ""),
    (&MINOR_TEMPLATE, "m"),
    (&DOM7_TEMPLATE, "7"),
    (&MIN7_TEMPLATE, "m7"),
    (&DIM_TEMPLATE, "dim"),
    (&AUG_TEMPLATE, "aug"),
    (&SUS4_TEMPLATE, "sus4"),
];

/// Rotate a chord template by `steps` semitones.
fn rotate_template(template: &[f32; 12], steps: usize) -> [f32; 12] {
    let mut rotated = [0.0f32; 12];
    for i in 0..12 {
        rotated[(i + steps) % 12] = template[i];
    }
    rotated
}

/// Cosine similarity between two chroma vectors.
fn cosine_similarity(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for i in 0..12 {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = (norm_a * norm_b).sqrt();
    if denom < 1e-10 {
        0.0
    } else {
        dot / denom
    }
}

/// Perform in-place radix-2 Cooley-Tukey FFT.
fn fft_in_place(real: &mut [f32], imag: &mut [f32]) {
    let n = real.len();
    debug_assert!(n.is_power_of_two(), "FFT size must be power of 2");
    debug_assert_eq!(real.len(), imag.len());

    // Bit-reversal permutation
    let mut j = 0usize;
    for i in 0..n {
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
        let mut m = n >> 1;
        while m >= 1 && j >= m {
            j -= m;
            m >>= 1;
        }
        j += m;
    }

    // Butterfly stages
    let mut step = 2;
    while step <= n {
        let half = step / 2;
        let angle_step = -std::f32::consts::TAU / step as f32;
        for k in (0..n).step_by(step) {
            for j_inner in 0..half {
                let angle = angle_step * j_inner as f32;
                let wr = angle.cos();
                let wi = angle.sin();
                let a = k + j_inner;
                let b = a + half;
                let tr = wr * real[b] - wi * imag[b];
                let ti = wr * imag[b] + wi * real[b];
                real[b] = real[a] - tr;
                imag[b] = imag[a] - ti;
                real[a] += tr;
                imag[a] += ti;
            }
        }
        step <<= 1;
    }
}

/// Build a Hann window of given size.
fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| {
            0.5 * (1.0 - (std::f32::consts::TAU * i as f32 / size as f32).cos())
        })
        .collect()
}

/// Detect chords from audio samples.
///
/// Returns a list of (start_seconds, end_seconds, chord_name).
pub fn detect_chords(
    samples: &[f32],
    sample_rate: u32,
) -> Vec<DetectedChord> {
    if samples.is_empty() || sample_rate == 0 {
        return Vec::new();
    }

    let fft_size: usize = 8192;
    let hop_size: usize = 4096;
    let window = hann_window(fft_size);

    let mut results: Vec<(f64, String)> = Vec::new(); // (time_seconds, chord_name)

    let mut pos = 0usize;
    while pos + fft_size <= samples.len() {
        let time_sec = pos as f64 / sample_rate as f64;

        // Apply window and prepare FFT buffers
        let mut real = vec![0.0f32; fft_size];
        let mut imag = vec![0.0f32; fft_size];
        for i in 0..fft_size {
            real[i] = samples[pos + i] * window[i];
        }

        fft_in_place(&mut real, &mut imag);

        // Compute magnitude spectrum (first half only)
        let half = fft_size / 2;
        let mut magnitudes = vec![0.0f32; half];
        for i in 0..half {
            magnitudes[i] = (real[i] * real[i] + imag[i] * imag[i]).sqrt();
        }

        // Map FFT bins to 12 chroma bins
        let mut chroma = [0.0f32; 12];
        let freq_resolution = sample_rate as f32 / fft_size as f32;

        for bin in 1..half {
            let freq = bin as f32 * freq_resolution;
            // Only consider musically relevant frequencies (C2 ~65Hz to C7 ~2100Hz)
            if freq < 60.0 || freq > 2200.0 {
                continue;
            }
            // Map frequency to pitch class (0 = C, 1 = C#, ..., 11 = B)
            let midi_note = 69.0 + 12.0 * (freq / 440.0).log2();
            let pitch_class = ((midi_note.round() as i32 % 12) + 12) % 12;
            chroma[pitch_class as usize] += magnitudes[bin];
        }

        // Normalize chroma vector
        let chroma_max = chroma.iter().cloned().fold(0.0f32, f32::max);
        if chroma_max > 1e-6 {
            for c in chroma.iter_mut() {
                *c /= chroma_max;
            }

            // Compare against all chord templates for all root notes
            let mut best_score = -1.0f32;
            let mut best_chord = String::new();

            for root in 0..12 {
                for &(template, suffix) in &CHORD_TEMPLATES {
                    let rotated = rotate_template(template, root);
                    let score = cosine_similarity(&chroma, &rotated);
                    if score > best_score {
                        best_score = score;
                        best_chord = format!("{}{}", NOTE_NAMES[root], suffix);
                    }
                }
            }

            // Only report if confidence is reasonable
            if best_score > 0.6 {
                results.push((time_sec, best_chord));
            } else {
                results.push((time_sec, String::new())); // unknown/ambiguous
            }
        } else {
            results.push((time_sec, String::new())); // silence
        }

        pos += hop_size;
    }

    // Group consecutive identical chords into segments
    let hop_duration = hop_size as f64 / sample_rate as f64;
    let mut chords: Vec<DetectedChord> = Vec::new();

    for (i, (time, name)) in results.iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        if let Some(last) = chords.last_mut() {
            if last.name == *name {
                // Extend the current chord segment
                last.end_seconds = (*time + hop_duration) as f32;
                continue;
            }
        }
        // Start a new chord segment
        let end = if i + 1 < results.len() {
            results[i + 1].0 as f32
        } else {
            (*time + hop_duration) as f32
        };
        chords.push(DetectedChord {
            start_seconds: *time as f32,
            end_seconds: end,
            name: name.clone(),
        });
    }

    chords
}

/// Perform chord detection on a clip and store results.
pub fn detect_clip_chords(app: &mut DawApp, track_idx: usize, clip_idx: usize) {
    let clip = match app.project.tracks.get(track_idx).and_then(|t| t.clips.get(clip_idx)) {
        Some(c) => c.clone(),
        None => {
            app.set_status("Cannot detect chords: clip not found");
            return;
        }
    };

    // Get the audio samples for this clip
    let samples = match &clip.source {
        ClipSource::AudioBuffer { buffer_id } => {
            match app.audio_buffers.get(buffer_id) {
                Some(buf) => buf.clone(),
                None => {
                    app.set_status("Cannot detect chords: audio buffer not found");
                    return;
                }
            }
        }
        ClipSource::AudioFile { path } => {
            match jamhub_engine::load_audio(std::path::Path::new(path)) {
                Ok(data) => data.samples,
                Err(e) => {
                    app.set_status(&format!("Cannot detect chords: {e}"));
                    return;
                }
            }
        }
        ClipSource::Midi { .. } => {
            app.set_status("Chord detection is for audio clips only");
            return;
        }
    };

    let sr = app.sample_rate();
    let chords = detect_chords(&samples, sr);

    if chords.is_empty() {
        app.set_status("No chords detected in this clip");
        return;
    }

    let chord_count = chords.len();

    // Store detected chords (keyed by clip ID for overlay rendering)
    app.detected_chords.insert(clip.id, chords);
    app.set_status(&format!(
        "Detected {} chord regions",
        chord_count
    ));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Drawing chord overlays on the timeline
// ═══════════════════════════════════════════════════════════════════════════════

/// Draw detected chord labels above a clip on the timeline.
pub fn draw_chord_overlay(
    painter: &egui::Painter,
    clip_rect: egui::Rect,
    _clip_id: Uuid,
    clip_duration_samples: u64,
    sample_rate: u32,
    chords: &[DetectedChord],
) {
    if chords.is_empty() || clip_duration_samples == 0 || sample_rate == 0 {
        return;
    }

    let clip_duration_secs = clip_duration_samples as f32 / sample_rate as f32;
    let chord_height = 16.0;
    let chord_rect = egui::Rect::from_min_size(
        egui::pos2(clip_rect.min.x, clip_rect.min.y - chord_height - 2.0),
        egui::vec2(clip_rect.width(), chord_height),
    );

    // Color palette for chord types
    let chord_colors: &[egui::Color32] = &[
        egui::Color32::from_rgb(80, 160, 220),  // major
        egui::Color32::from_rgb(180, 100, 200),  // minor
        egui::Color32::from_rgb(220, 160, 60),   // 7th
        egui::Color32::from_rgb(100, 200, 140),  // m7
        egui::Color32::from_rgb(220, 80, 80),    // dim
        egui::Color32::from_rgb(200, 120, 60),   // aug
        egui::Color32::from_rgb(120, 180, 200),  // sus4
    ];

    for chord in chords {
        let x_start = clip_rect.min.x
            + (chord.start_seconds / clip_duration_secs) * clip_rect.width();
        let x_end = clip_rect.min.x
            + (chord.end_seconds / clip_duration_secs).min(1.0) * clip_rect.width();

        if x_end <= x_start + 2.0 {
            continue;
        }

        // Choose color based on chord suffix
        let color_idx = if chord.name.contains("dim") {
            4
        } else if chord.name.contains("aug") {
            5
        } else if chord.name.contains("sus") {
            6
        } else if chord.name.contains("m7") {
            3
        } else if chord.name.contains('7') {
            2
        } else if chord.name.contains('m') && !chord.name.starts_with("maj") {
            1
        } else {
            0
        };
        let color = chord_colors[color_idx % chord_colors.len()];

        let block_rect = egui::Rect::from_min_max(
            egui::pos2(x_start, chord_rect.min.y),
            egui::pos2(x_end, chord_rect.max.y),
        );

        painter.rect_filled(block_rect, 3.0, color.gamma_multiply(0.35));
        painter.rect_stroke(block_rect, 3.0, egui::Stroke::new(0.5, color.gamma_multiply(0.6)), egui::StrokeKind::Inside);

        // Draw chord name label
        if block_rect.width() > 18.0 {
            painter.text(
                block_rect.center(),
                egui::Align2::CENTER_CENTER,
                &chord.name,
                egui::FontId::proportional(9.5),
                color,
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Main UI — Analysis Tools Window
// ═══════════════════════════════════════════════════════════════════════════════

/// Draw the combined analysis tools window.
pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_analysis {
        return;
    }

    show_reference_track(app, ctx);

    // Correlation meter and loudness matching go in a secondary panel
    let mut open = app.show_analysis;
    egui::Window::new("Analysis Tools")
        .id(egui::Id::new("analysis_tools_panel"))
        .open(&mut open)
        .default_width(350.0)
        .default_height(240.0)
        .show(ctx, |ui| {
            show_correlation_meter(app, ui);
            ui.separator();
            show_loudness_matching(app, ui);
        });
    app.show_analysis = open;
}
