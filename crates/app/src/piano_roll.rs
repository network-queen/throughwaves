use std::collections::HashSet;

use eframe::egui;
use jamhub_model::{Clip, ClipSource, MidiCC, MidiNote, TrackKind};
use uuid::Uuid;

use crate::DawApp;

const KEY_WIDTH: f32 = 40.0;
const TICKS_PER_BEAT: u64 = 480;
const VELOCITY_LANE_HEIGHT: f32 = 80.0;
const CC_LANE_HEIGHT: f32 = 80.0;
const NOTE_RESIZE_HANDLE_WIDTH: f32 = 6.0;
const DEFAULT_DRUM_VELOCITY: u8 = 100;

// ── Scale definitions ──────────────────────────────────────────────

/// Musical scales as semitone intervals from the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Chromatic,
    Major,
    Minor,
    Pentatonic,
    Blues,
    Dorian,
    Mixolydian,
}

impl Scale {
    pub const ALL: &'static [Scale] = &[
        Scale::Chromatic,
        Scale::Major,
        Scale::Minor,
        Scale::Pentatonic,
        Scale::Blues,
        Scale::Dorian,
        Scale::Mixolydian,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Scale::Chromatic => "Chromatic",
            Scale::Major => "Major",
            Scale::Minor => "Minor",
            Scale::Pentatonic => "Pentatonic",
            Scale::Blues => "Blues",
            Scale::Dorian => "Dorian",
            Scale::Mixolydian => "Mixolydian",
        }
    }

    /// Returns the semitone intervals that belong to this scale (relative to root, mod 12).
    pub fn intervals(&self) -> &'static [u8] {
        match self {
            Scale::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Pentatonic => &[0, 2, 4, 7, 9],
            Scale::Blues => &[0, 3, 5, 6, 7, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
        }
    }

    /// Check if a MIDI pitch belongs to this scale with the given root note.
    pub fn contains_pitch(&self, root: u8, pitch: u8) -> bool {
        let degree = (pitch + 12 - (root % 12)) % 12;
        self.intervals().contains(&degree)
    }

    /// Snap a pitch to the nearest scale degree.
    pub fn snap_pitch(&self, root: u8, pitch: u8) -> u8 {
        if *self == Scale::Chromatic {
            return pitch;
        }
        let intervals = self.intervals();
        let root_mod = root % 12;
        let mut best = pitch;
        let mut best_dist = 128i16;
        // Search nearby pitches
        for offset in -6i16..=6 {
            let candidate = (pitch as i16 + offset).clamp(0, 127) as u8;
            let degree = (candidate + 12 - root_mod) % 12;
            if intervals.contains(&degree) {
                let dist = offset.abs();
                if dist < best_dist {
                    best_dist = dist;
                    best = candidate;
                }
            }
        }
        best
    }
}

const NOTE_NAMES: &[&str] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[allow(dead_code)]
fn note_name_with_octave(pitch: u8) -> String {
    let name = NOTE_NAMES[(pitch % 12) as usize];
    let octave = (pitch / 12) as i32 - 1;
    format!("{name}{octave}")
}

fn root_note_name(root: u8) -> &'static str {
    NOTE_NAMES[(root % 12) as usize]
}

// ── Drum map ───────────────────────────────────────────────────────

/// Standard General MIDI drum map entries.
struct DrumSound {
    pitch: u8,
    name: &'static str,
}

const DRUM_MAP: &[DrumSound] = &[
    DrumSound { pitch: 36, name: "Kick" },
    DrumSound { pitch: 37, name: "Side Stick" },
    DrumSound { pitch: 38, name: "Snare" },
    DrumSound { pitch: 39, name: "Clap" },
    DrumSound { pitch: 40, name: "E. Snare" },
    DrumSound { pitch: 41, name: "Low Tom" },
    DrumSound { pitch: 42, name: "Closed HH" },
    DrumSound { pitch: 43, name: "Mid Tom" },
    DrumSound { pitch: 44, name: "Pedal HH" },
    DrumSound { pitch: 45, name: "High Tom" },
    DrumSound { pitch: 46, name: "Open HH" },
    DrumSound { pitch: 47, name: "Lo-Mid Tom" },
    DrumSound { pitch: 48, name: "Hi-Mid Tom" },
    DrumSound { pitch: 49, name: "Crash 1" },
    DrumSound { pitch: 51, name: "Ride" },
    DrumSound { pitch: 52, name: "China" },
];

// ── CC presets ──────────────────────────────────────────────────────

struct CCPreset {
    number: u8,
    name: &'static str,
}

const CC_PRESETS: &[CCPreset] = &[
    CCPreset { number: 1, name: "Mod Wheel" },
    CCPreset { number: 7, name: "Volume" },
    CCPreset { number: 10, name: "Pan" },
    CCPreset { number: 11, name: "Expression" },
    CCPreset { number: 64, name: "Sustain" },
    CCPreset { number: 71, name: "Resonance" },
    CCPreset { number: 74, name: "Cutoff" },
];

// ── Interaction modes ──────────────────────────────────────────────

/// Interaction modes for the piano roll grid.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DragMode {
    /// Creating a new note by click-and-drag (note_index, start_tick, pitch).
    Creating(usize),
    /// Moving selected notes (delta_tick, delta_pitch from original position).
    Moving { anchor_idx: usize, start_tick: u64, start_pitch: u8 },
    /// Resizing the right edge of a note.
    Resizing(usize),
    /// Editing velocity in the velocity lane.
    VelocityEdit,
    /// Drawing CC values in the CC lane.
    CCEdit,
}

/// Persistent state for the piano roll editor.
pub struct PianoRollState {
    pub selected_notes: HashSet<usize>,
    drag_mode: Option<DragMode>,
    pub show_velocity_lane: bool,
    /// Quantize grid size in ticks (default = quarter note).
    pub quantize_ticks: u64,
    /// Swing amount 0.0-1.0 (0% = straight, 0.5 = dotted, 0.66 = triplet).
    pub swing_amount: f32,
    /// Humanize amount 0.0-1.0 controlling randomization range.
    pub humanize_amount: f32,
    /// Scale snapping
    pub scale: Scale,
    pub root_note: u8,
    /// Drum programming mode
    pub drum_mode: bool,
    /// CC lane
    pub show_cc_lane: bool,
    pub cc_number: u8,
    /// Simple LCG random state for humanize.
    rng_state: u64,
}

impl PianoRollState {
    /// Simple LCG pseudo-random number generator. Returns a value in [0, 1).
    fn next_rand(&mut self) -> f64 {
        // LCG parameters from Numerical Recipes
        self.rng_state = self.rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.rng_state >> 33) as f64) / (1u64 << 31) as f64
    }

    /// Returns a random value in [-1.0, 1.0).
    fn next_rand_bipolar(&mut self) -> f64 {
        self.next_rand() * 2.0 - 1.0
    }
}

impl Default for PianoRollState {
    fn default() -> Self {
        Self {
            selected_notes: HashSet::new(),
            drag_mode: None,
            show_velocity_lane: false,
            quantize_ticks: TICKS_PER_BEAT,
            swing_amount: 0.0,
            humanize_amount: 0.5,
            scale: Scale::Chromatic,
            root_note: 0, // C
            drum_mode: false,
            show_cc_lane: false,
            cc_number: 1, // Mod Wheel
            rng_state: 12345,
        }
    }
}

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_piano_roll {
        return;
    }

    let track_idx = match app.selected_track {
        Some(i) if i < app.project.tracks.len() => i,
        _ => return,
    };

    if app.project.tracks[track_idx].kind != TrackKind::Midi {
        let mut open = true;
        egui::Window::new("Piano Roll").constrain(false)
            .open(&mut open)
            .default_size([400.0, 150.0])
            .show(ctx, |ui| {
                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new("No MIDI track selected")
                        .size(16.0)
                        .color(egui::Color32::from_rgb(200, 180, 100)),
                );
                ui.add_space(8.0);
                ui.label("Select a MIDI track or create one from Track > Add MIDI Track");
            });
        if !open {
            app.show_piano_roll = false;
        }
        return;
    }

    let mut open = true;
    egui::Window::new("Piano Roll").constrain(false)
        .open(&mut open)
        .default_size([1000.0, 700.0])
        .min_size([600.0, 400.0])
        .resizable(true)
        .show(ctx, |ui| {
            let track_name = app.project.tracks[track_idx].name.clone();
            ui.heading(format!("Piano Roll: {track_name}"));

            // ── Toolbar row 1: Quantize, Scale, Mode ──────────────
            ui.horizontal(|ui| {
                // Quantize button
                if ui.button("Quantize").on_hover_text("Snap selected notes to grid (with swing)").clicked() {
                    quantize_selected(app, track_idx);
                }

                // Quantize grid selector
                let qt = app.piano_roll_state.quantize_ticks;
                egui::ComboBox::from_id_salt("pr_quant")
                    .selected_text(quantize_label(qt))
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for &(label, ticks) in &[
                            ("1 Bar", TICKS_PER_BEAT * 4),
                            ("1/2", TICKS_PER_BEAT * 2),
                            ("1/4", TICKS_PER_BEAT),
                            ("1/8", TICKS_PER_BEAT / 2),
                            ("1/16", TICKS_PER_BEAT / 4),
                            ("1/32", TICKS_PER_BEAT / 8),
                        ] {
                            if ui.selectable_label(qt == ticks, label).clicked() {
                                app.piano_roll_state.quantize_ticks = ticks;
                            }
                        }
                    });

                // Swing slider
                ui.label("Swing:");
                let swing_pct = (app.piano_roll_state.swing_amount * 100.0).round() as i32;
                let swing_text = match swing_pct {
                    0 => "Straight".to_string(),
                    50 => "Dotted".to_string(),
                    66 => "Triplet".to_string(),
                    v => format!("{v}%"),
                };
                ui.add(
                    egui::Slider::new(&mut app.piano_roll_state.swing_amount, 0.0..=1.0)
                        .text(swing_text)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                        .max_decimals(2),
                );

                ui.separator();

                // Scale selector
                let current_scale = app.piano_roll_state.scale;
                egui::ComboBox::from_id_salt("pr_scale")
                    .selected_text(current_scale.name())
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for &s in Scale::ALL {
                            if ui.selectable_label(current_scale == s, s.name()).clicked() {
                                app.piano_roll_state.scale = s;
                            }
                        }
                    });

                // Root note selector
                let root = app.piano_roll_state.root_note;
                egui::ComboBox::from_id_salt("pr_root")
                    .selected_text(root_note_name(root))
                    .width(50.0)
                    .show_ui(ui, |ui| {
                        for n in 0u8..12 {
                            if ui.selectable_label(root % 12 == n, NOTE_NAMES[n as usize]).clicked() {
                                app.piano_roll_state.root_note = n;
                            }
                        }
                    });

                ui.separator();

                // Drum mode toggle
                if ui
                    .selectable_label(app.piano_roll_state.drum_mode, "Drum")
                    .on_hover_text("Toggle drum step sequencer mode")
                    .clicked()
                {
                    app.piano_roll_state.drum_mode = !app.piano_roll_state.drum_mode;
                }

                // Velocity lane toggle
                if ui
                    .selectable_label(app.piano_roll_state.show_velocity_lane, "Velocity")
                    .clicked()
                {
                    app.piano_roll_state.show_velocity_lane =
                        !app.piano_roll_state.show_velocity_lane;
                }

                // CC lane toggle
                if ui
                    .selectable_label(app.piano_roll_state.show_cc_lane, "CC")
                    .on_hover_text("Show MIDI CC editing lane")
                    .clicked()
                {
                    app.piano_roll_state.show_cc_lane = !app.piano_roll_state.show_cc_lane;
                }

                // CC number selector (shown when CC lane is visible)
                if app.piano_roll_state.show_cc_lane {
                    let cc_num = app.piano_roll_state.cc_number;
                    let cc_label = CC_PRESETS
                        .iter()
                        .find(|p| p.number == cc_num)
                        .map(|p| format!("CC{}: {}", p.number, p.name))
                        .unwrap_or_else(|| format!("CC{cc_num}"));
                    egui::ComboBox::from_id_salt("pr_cc_num")
                        .selected_text(cc_label)
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for preset in CC_PRESETS {
                                let label = format!("CC{}: {}", preset.number, preset.name);
                                if ui.selectable_label(cc_num == preset.number, label).clicked() {
                                    app.piano_roll_state.cc_number = preset.number;
                                }
                            }
                        });
                }
            });

            // ── Toolbar row 2: Edit operations ────────────────────────
            ui.horizontal(|ui| {
                // Humanize button
                if ui.button("Humanize").on_hover_text("Randomize timing & velocity of selected notes").clicked() {
                    humanize_selected(app, track_idx);
                }

                // Humanize amount slider
                ui.add(
                    egui::Slider::new(&mut app.piano_roll_state.humanize_amount, 0.0..=1.0)
                        .text("Amount")
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                        .max_decimals(2),
                );

                ui.separator();

                // Legato button
                if ui.button("Legato").on_hover_text("Extend selected notes to reach the next note (no gaps)").clicked() {
                    legato_selected(app, track_idx);
                }

                // Staccato button
                if ui.button("Staccato").on_hover_text("Shorten selected notes to 50% duration").clicked() {
                    staccato_selected(app, track_idx);
                }

                ui.separator();

                // Select All button
                if ui.button("Select All").clicked() {
                    select_all_notes(app, track_idx);
                }

                // Deselect
                if ui.button("Deselect").clicked() {
                    app.piano_roll_state.selected_notes.clear();
                }

                // Delete selected
                if ui.button("Delete").clicked() {
                    delete_selected(app, track_idx);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let count = app.piano_roll_state.selected_notes.len();
                    if count > 0 {
                        ui.label(
                            egui::RichText::new(format!("{count} selected"))
                                .small()
                                .color(egui::Color32::from_rgb(180, 180, 255)),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("Click to add, drag to move/resize")
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                });
            });

            ui.separator();

            // ── Keyboard shortcuts ──────────────────────────────────
            let has_focus = ui.memory(|m| m.focused().is_none());
            if has_focus {
                let cmd = ui.input(|i| i.modifiers.command);
                if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                    delete_selected(app, track_idx);
                }
                if cmd && ui.input(|i| i.key_pressed(egui::Key::A)) {
                    select_all_notes(app, track_idx);
                }
            }

            // ── Decide which view to show ──────────────────────────
            if app.piano_roll_state.drum_mode {
                show_drum_grid(app, ui, track_idx);
            } else {
                show_note_grid(app, ui, track_idx);
            }

            // ── CC Lane ────────────────────────────────────────────
            if app.piano_roll_state.show_cc_lane {
                show_cc_lane(app, ui, track_idx);
            }
        });

    if !open {
        app.show_piano_roll = false;
    }
}

// ── Standard piano roll note grid ──────────────────────────────────

fn show_note_grid(app: &mut DawApp, ui: &mut egui::Ui, track_idx: usize) {
    let available = ui.available_size();
    let velocity_h = if app.piano_roll_state.show_velocity_lane {
        VELOCITY_LANE_HEIGHT
    } else {
        0.0
    };
    let cc_h = if app.piano_roll_state.show_cc_lane {
        CC_LANE_HEIGHT + 8.0 // plus separator
    } else {
        0.0
    };
    let grid_height = (available.y - velocity_h - cc_h).max(60.0);

    // ── Grid constants ──────────────────────────────────────
    let visible_notes_start: u8 = 36; // C2
    let visible_notes_end: u8 = 96; // C7
    let note_range = (visible_notes_end - visible_notes_start) as f32;

    let grid_width = available.x;
    let note_h = (grid_height / note_range).max(4.0);
    let pixels_per_tick = (grid_width - KEY_WIDTH) / (TICKS_PER_BEAT as f32 * 16.0);
    let quant = app.piano_roll_state.quantize_ticks;
    let scale = app.piano_roll_state.scale;
    let root = app.piano_roll_state.root_note;

    // ── Note grid area ──────────────────────────────────────
    let (response, painter) = ui.allocate_painter(
        egui::vec2(grid_width, grid_height),
        egui::Sense::click_and_drag(),
    );
    let rect = response.rect;

    // Background
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(30, 30, 35));

    // Find/create MIDI clip
    let midi_clip_idx = app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }));

    // ── Draw piano keys ─────────────────────────────────────
    for note in visible_notes_start..visible_notes_end {
        let y = rect.max.y - (note - visible_notes_start) as f32 * note_h - note_h;
        let is_black = matches!(note % 12, 1 | 3 | 6 | 8 | 10);
        let in_scale = scale.contains_pitch(root, note);

        // Highlight scale notes on the keyboard
        let key_color = if !is_black && in_scale && scale != Scale::Chromatic {
            egui::Color32::from_rgb(65, 70, 80) // brighter for in-scale white keys
        } else if is_black && in_scale && scale != Scale::Chromatic {
            egui::Color32::from_rgb(50, 55, 65)
        } else if is_black {
            egui::Color32::from_rgb(40, 40, 45)
        } else {
            egui::Color32::from_rgb(55, 55, 60)
        };

        let key_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, y),
            egui::vec2(KEY_WIDTH, note_h),
        );
        painter.rect_filled(key_rect, 0.0, key_color);

        // Scale highlight stripe on the grid area
        if in_scale && scale != Scale::Chromatic {
            let stripe = egui::Rect::from_min_size(
                egui::pos2(rect.min.x + KEY_WIDTH, y),
                egui::vec2(grid_width - KEY_WIDTH, note_h),
            );
            painter.rect_filled(
                stripe,
                0.0,
                egui::Color32::from_rgba_premultiplied(80, 100, 140, 15),
            );
        }

        if note % 12 == 0 {
            let octave = (note / 12) as i32 - 1;
            painter.text(
                egui::pos2(rect.min.x + 2.0, y + 1.0),
                egui::Align2::LEFT_TOP,
                format!("C{octave}"),
                egui::FontId::proportional(9.0),
                egui::Color32::from_rgb(160, 160, 170),
            );
        }

        // Horizontal grid line
        painter.line_segment(
            [
                egui::pos2(KEY_WIDTH + rect.min.x, y),
                egui::pos2(rect.max.x, y),
            ],
            egui::Stroke::new(
                0.5,
                if note % 12 == 0 {
                    egui::Color32::from_rgb(60, 60, 70)
                } else {
                    egui::Color32::from_rgb(40, 40, 48)
                },
            ),
        );
    }

    // ── Beat grid lines ─────────────────────────────────────
    for beat in 0..17 {
        let x =
            rect.min.x + KEY_WIDTH + beat as f32 * TICKS_PER_BEAT as f32 * pixels_per_tick;
        let is_bar = beat % app.project.time_signature.numerator as i32 == 0;
        painter.line_segment(
            [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
            egui::Stroke::new(
                if is_bar { 1.0 } else { 0.5 },
                if is_bar {
                    egui::Color32::from_rgb(70, 70, 80)
                } else {
                    egui::Color32::from_rgb(45, 45, 55)
                },
            ),
        );
    }

    // ── Draw MIDI notes and collect hit-test data ───────────
    #[allow(dead_code)]
    struct NoteRect {
        idx: usize,
        rect: egui::Rect,
        pitch: u8,
        start_tick: u64,
        duration_ticks: u64,
        velocity: u8,
    }
    let mut note_rects: Vec<NoteRect> = Vec::new();

    if let Some(ci) = midi_clip_idx {
        if let ClipSource::Midi { ref notes, .. } =
            app.project.tracks[track_idx].clips[ci].source
        {
            for (i, note) in notes.iter().enumerate() {
                if note.pitch < visible_notes_start || note.pitch >= visible_notes_end {
                    continue;
                }
                let x = rect.min.x
                    + KEY_WIDTH
                    + note.start_tick as f32 * pixels_per_tick;
                let w = (note.duration_ticks as f32 * pixels_per_tick).max(3.0);
                let y = rect.max.y
                    - (note.pitch - visible_notes_start) as f32 * note_h
                    - note_h;

                let note_rect = egui::Rect::from_min_size(
                    egui::pos2(x, y + 1.0),
                    egui::vec2(w, note_h - 2.0),
                );

                let is_selected = app.piano_roll_state.selected_notes.contains(&i);

                let vel_alpha = note.velocity as f32 / 127.0;
                let color = if is_selected {
                    egui::Color32::from_rgb(
                        (140.0 + 115.0 * vel_alpha) as u8,
                        (220.0 * vel_alpha) as u8,
                        200,
                    )
                } else {
                    egui::Color32::from_rgb(
                        (100.0 + 155.0 * vel_alpha) as u8,
                        (180.0 * vel_alpha) as u8,
                        255,
                    )
                };
                painter.rect_filled(note_rect, 2.0, color);

                // Selection highlight border
                if is_selected {
                    painter.rect_stroke(
                        note_rect,
                        2.0,
                        egui::Stroke::new(1.5, egui::Color32::WHITE),
                        egui::StrokeKind::Outside,
                    );
                }

                // Resize handle indicator (right edge)
                if is_selected && w > NOTE_RESIZE_HANDLE_WIDTH * 2.0 {
                    let handle = egui::Rect::from_min_size(
                        egui::pos2(note_rect.max.x - NOTE_RESIZE_HANDLE_WIDTH, note_rect.min.y),
                        egui::vec2(NOTE_RESIZE_HANDLE_WIDTH, note_rect.height()),
                    );
                    painter.rect_filled(
                        handle,
                        0.0,
                        egui::Color32::from_rgba_premultiplied(255, 255, 255, 40),
                    );
                }

                note_rects.push(NoteRect {
                    idx: i,
                    rect: note_rect,
                    pitch: note.pitch,
                    start_tick: note.start_tick,
                    duration_ticks: note.duration_ticks,
                    velocity: note.velocity,
                });
            }
        }
    }

    // ── Playhead ──────────────────────────────────────────────
    {
        let transport = app.transport_state();
        let pos_samples = app.position_samples();
        let sr = app.sample_rate() as f64;
        let bpm = app.project.tempo.bpm;
        let ticks_per_second = bpm / 60.0 * TICKS_PER_BEAT as f64;
        let pos_ticks = (pos_samples as f64 / sr * ticks_per_second) as f32;
        let playhead_x = rect.min.x + KEY_WIDTH + pos_ticks * pixels_per_tick;

        if playhead_x >= rect.min.x + KEY_WIDTH && playhead_x <= rect.max.x {
            // Playhead line
            painter.line_segment(
                [egui::pos2(playhead_x, rect.min.y), egui::pos2(playhead_x, rect.max.y)],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(235, 180, 60)),
            );
            // Small triangle at top
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(playhead_x, rect.min.y),
                    egui::pos2(playhead_x - 4.0, rect.min.y + 6.0),
                    egui::pos2(playhead_x + 4.0, rect.min.y + 6.0),
                ],
                egui::Color32::from_rgb(235, 180, 60),
                egui::Stroke::NONE,
            ));
        }

        // Request repaint while playing so the playhead moves
        if transport == jamhub_model::TransportState::Playing {
            ui.ctx().request_repaint();
        }
    }

    // ── Interaction handling ─────────────────────────────────
    let pointer_pos = response.interact_pointer_pos.or_else(|| response.hover_pos());

    // Helper: convert screen position to (tick, pitch)
    let pos_to_grid = |pos: egui::Pos2| -> (u64, u8) {
        let grid_x = (pos.x - rect.min.x - KEY_WIDTH).max(0.0);
        let grid_y = (rect.max.y - pos.y).max(0.0);
        let tick = (grid_x / pixels_per_tick) as u64;
        let pitch = visible_notes_start + (grid_y / note_h) as u8;
        (tick, pitch.min(visible_notes_end - 1))
    };

    let snap_tick = |t: u64| -> u64 {
        if quant > 0 {
            ((t + quant / 2) / quant) * quant
        } else {
            t
        }
    };

    // --- Drag start ---
    if response.drag_started() {
        if let Some(pos) = pointer_pos {
            let grid_x = pos.x - rect.min.x - KEY_WIDTH;
            if grid_x > 0.0 {
                // Check if we hit an existing note's resize handle
                let mut hit_resize = None;
                let mut hit_note = None;
                for nr in note_rects.iter().rev() {
                    if nr.rect.contains(pos) {
                        let handle_left = nr.rect.max.x - NOTE_RESIZE_HANDLE_WIDTH;
                        if pos.x >= handle_left
                            && app.piano_roll_state.selected_notes.contains(&nr.idx)
                        {
                            hit_resize = Some(nr.idx);
                        } else {
                            hit_note = Some(nr.idx);
                        }
                        break;
                    }
                }

                if let Some(idx) = hit_resize {
                    app.piano_roll_state.drag_mode = Some(DragMode::Resizing(idx));
                } else if let Some(idx) = hit_note {
                    let cmd = ui.input(|i| i.modifiers.command);
                    if !cmd && !app.piano_roll_state.selected_notes.contains(&idx) {
                        app.piano_roll_state.selected_notes.clear();
                    }
                    app.piano_roll_state.selected_notes.insert(idx);

                    if let Some(nr) = note_rects.iter().find(|n| n.idx == idx) {
                        app.piano_roll_state.drag_mode = Some(DragMode::Moving {
                            anchor_idx: idx,
                            start_tick: nr.start_tick,
                            start_pitch: nr.pitch,
                        });
                        app.push_undo("Move MIDI notes");
                    }
                } else {
                    // Create new note
                    let (tick, pitch) = pos_to_grid(pos);
                    let snapped = snap_tick(tick);
                    let snapped_pitch = scale.snap_pitch(root, pitch);
                    if snapped_pitch >= visible_notes_start && snapped_pitch < visible_notes_end {
                        let clip_idx = find_or_create_midi_clip(app, track_idx);
                        if let Some(ci) = clip_idx {
                            app.push_undo("Add MIDI note");
                            let new_idx;
                            if let ClipSource::Midi { ref mut notes, .. } =
                                app.project.tracks[track_idx].clips[ci].source
                            {
                                new_idx = notes.len();
                                notes.push(MidiNote {
                                    pitch: snapped_pitch,
                                    velocity: 100,
                                    start_tick: snapped,
                                    duration_ticks: quant.max(1),
                                });
                            } else {
                                new_idx = 0;
                            }
                            app.piano_roll_state.selected_notes.clear();
                            app.piano_roll_state.selected_notes.insert(new_idx);
                            app.piano_roll_state.drag_mode =
                                Some(DragMode::Creating(new_idx));
                        }
                    }
                }
            }
        }
    }

    // --- Drag update ---
    if response.dragged() {
        if let Some(pos) = pointer_pos {
            match app.piano_roll_state.drag_mode {
                Some(DragMode::Creating(idx)) => {
                    if let Some(ci) = midi_clip_idx.or_else(|| {
                        app.project.tracks[track_idx]
                            .clips
                            .iter()
                            .position(|c| matches!(c.source, ClipSource::Midi { .. }))
                    }) {
                        if let ClipSource::Midi { ref mut notes, .. } =
                            app.project.tracks[track_idx].clips[ci].source
                        {
                            if idx < notes.len() {
                                let (current_tick, _) = pos_to_grid(pos);
                                let start = notes[idx].start_tick;
                                let end = snap_tick(current_tick.max(start + 1));
                                notes[idx].duration_ticks =
                                    (end - start).max(quant.max(1));
                            }
                        }
                    }
                }
                Some(DragMode::Moving { .. }) => {
                    if let Some(ci) = midi_clip_idx {
                        if let ClipSource::Midi { ref mut notes, .. } =
                            app.project.tracks[track_idx].clips[ci].source
                        {
                            let dx_ticks =
                                (response.drag_delta().x / pixels_per_tick) as i64;
                            let dy_pitch =
                                -(response.drag_delta().y / note_h) as i8;

                            let selected: Vec<usize> =
                                app.piano_roll_state.selected_notes.iter().copied().collect();
                            for &si in &selected {
                                if si < notes.len() {
                                    let new_tick = (notes[si].start_tick as i64 + dx_ticks)
                                        .max(0) as u64;
                                    let new_pitch = (notes[si].pitch as i16 + dy_pitch as i16)
                                        .clamp(0, 127)
                                        as u8;
                                    notes[si].start_tick = new_tick;
                                    notes[si].pitch = new_pitch;
                                }
                            }
                        }
                    }
                }
                Some(DragMode::Resizing(idx)) => {
                    if let Some(ci) = midi_clip_idx {
                        if let ClipSource::Midi { ref mut notes, .. } =
                            app.project.tracks[track_idx].clips[ci].source
                        {
                            if idx < notes.len() {
                                let (current_tick, _) = pos_to_grid(pos);
                                let start = notes[idx].start_tick;
                                let end = snap_tick(current_tick.max(start + 1));
                                notes[idx].duration_ticks =
                                    (end - start).max(quant.max(1));
                            }
                        }
                    }
                }
                Some(DragMode::VelocityEdit) | Some(DragMode::CCEdit) => {
                    // Handled in their own sections
                }
                None => {}
            }
        }
    }

    // --- Drag end ---
    if response.drag_stopped() {
        // Snap notes to scale after move
        if matches!(
            app.piano_roll_state.drag_mode,
            Some(DragMode::Moving { .. })
        ) {
            if let Some(ci) = midi_clip_idx {
                if let ClipSource::Midi { ref mut notes, .. } =
                    app.project.tracks[track_idx].clips[ci].source
                {
                    let selected: Vec<usize> =
                        app.piano_roll_state.selected_notes.iter().copied().collect();
                    for &si in &selected {
                        if si < notes.len() {
                            notes[si].start_tick = snap_tick(notes[si].start_tick);
                            // Snap pitch to scale
                            notes[si].pitch = scale.snap_pitch(root, notes[si].pitch);
                        }
                    }
                }
            }
        }

        // Update clip duration after any edit
        update_clip_duration(app, track_idx);
        app.sync_project();
        app.piano_roll_state.drag_mode = None;
    }

    // --- Click (no drag) to select/deselect ---
    if response.clicked() {
        if let Some(pos) = pointer_pos {
            let grid_x = pos.x - rect.min.x - KEY_WIDTH;
            if grid_x > 0.0 {
                let cmd = ui.input(|i| i.modifiers.command);
                let mut hit = false;
                for nr in note_rects.iter().rev() {
                    if nr.rect.contains(pos) {
                        if cmd {
                            if app.piano_roll_state.selected_notes.contains(&nr.idx) {
                                app.piano_roll_state.selected_notes.remove(&nr.idx);
                            } else {
                                app.piano_roll_state.selected_notes.insert(nr.idx);
                            }
                        } else {
                            app.piano_roll_state.selected_notes.clear();
                            app.piano_roll_state.selected_notes.insert(nr.idx);
                        }
                        hit = true;
                        break;
                    }
                }
                if !hit && !cmd {
                    // Clicked empty space — note was already created by drag_started
                }
            }
        }
    }

    // ── Velocity Lane ───────────────────────────────────────
    if app.piano_roll_state.show_velocity_lane {
        show_velocity_lane(app, ui, track_idx, grid_width, pixels_per_tick);
    }
}

// ── Velocity lane (shared by note grid and drum grid) ──────────────

fn show_velocity_lane(
    app: &mut DawApp,
    ui: &mut egui::Ui,
    track_idx: usize,
    grid_width: f32,
    pixels_per_tick: f32,
) {
    ui.separator();

    // ── Velocity preset buttons ───────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Vel:")
                .small()
                .color(egui::Color32::GRAY),
        );
        const VELOCITY_PRESETS: &[(&str, u8)] = &[
            ("pp", 32),
            ("p", 48),
            ("mp", 64),
            ("mf", 80),
            ("f", 96),
            ("ff", 112),
            ("fff", 127),
        ];
        for &(name, vel) in VELOCITY_PRESETS {
            if ui
                .small_button(name)
                .on_hover_text(format!("Set selected notes to velocity {vel}"))
                .clicked()
            {
                set_selected_velocity(app, track_idx, vel);
            }
        }
    });

    let (vel_response, vel_painter) = ui.allocate_painter(
        egui::vec2(grid_width, VELOCITY_LANE_HEIGHT),
        egui::Sense::click_and_drag(),
    );
    let vel_rect = vel_response.rect;

    // Background
    vel_painter.rect_filled(
        vel_rect,
        0.0,
        egui::Color32::from_rgb(25, 25, 30),
    );

    // Horizontal guide lines at 25%, 50%, 75%, 100%
    for frac in &[0.25f32, 0.5, 0.75, 1.0] {
        let y = vel_rect.max.y - vel_rect.height() * frac;
        vel_painter.line_segment(
            [
                egui::pos2(vel_rect.min.x + KEY_WIDTH, y),
                egui::pos2(vel_rect.max.x, y),
            ],
            egui::Stroke::new(0.5, egui::Color32::from_rgb(50, 50, 60)),
        );
    }

    let midi_clip_idx = app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }));

    // Draw velocity bars for each note
    if let Some(ci) = midi_clip_idx {
        if let ClipSource::Midi { ref notes, .. } =
            app.project.tracks[track_idx].clips[ci].source
        {
            for (i, note) in notes.iter().enumerate() {
                let x = vel_rect.min.x
                    + KEY_WIDTH
                    + note.start_tick as f32 * pixels_per_tick;
                let bar_w = (note.duration_ticks as f32 * pixels_per_tick)
                    .max(3.0)
                    .min(12.0);
                let bar_h =
                    (note.velocity as f32 / 127.0) * vel_rect.height();
                let bar_rect = egui::Rect::from_min_size(
                    egui::pos2(x, vel_rect.max.y - bar_h),
                    egui::vec2(bar_w, bar_h),
                );

                let is_sel =
                    app.piano_roll_state.selected_notes.contains(&i);
                let color = if is_sel {
                    egui::Color32::from_rgb(140, 220, 200)
                } else {
                    egui::Color32::from_rgb(100, 150, 255)
                };
                vel_painter.rect_filled(bar_rect, 1.0, color);
            }
        }
    }

    // Velocity editing interaction
    if vel_response.drag_started() {
        app.piano_roll_state.drag_mode = Some(DragMode::VelocityEdit);
        app.push_undo("Edit velocity");
    }

    if (vel_response.dragged() || vel_response.clicked())
        && matches!(
            app.piano_roll_state.drag_mode,
            Some(DragMode::VelocityEdit) | None
        )
    {
        if let Some(pos) = vel_response.interact_pointer_pos {
            if let Some(ci) = midi_clip_idx {
                if let ClipSource::Midi { ref mut notes, .. } =
                    app.project.tracks[track_idx].clips[ci].source
                {
                    let mut best: Option<(usize, f32)> = None;
                    for (i, n) in notes.iter().enumerate() {
                        let nx = vel_rect.min.x
                            + KEY_WIDTH
                            + n.start_tick as f32 * pixels_per_tick;
                        let dist = (pos.x - nx).abs();
                        if dist < 20.0 {
                            if best.map_or(true, |(_, d)| dist < d) {
                                best = Some((i, dist));
                            }
                        }
                    }
                    if let Some((idx, _)) = best {
                        let frac = 1.0
                            - ((pos.y - vel_rect.min.y) / vel_rect.height())
                                .clamp(0.0, 1.0);
                        notes[idx].velocity = (frac * 127.0) as u8;
                    }
                }
            }
        }
    }

    if vel_response.drag_stopped() {
        if matches!(app.piano_roll_state.drag_mode, Some(DragMode::VelocityEdit)) {
            app.piano_roll_state.drag_mode = None;
            app.sync_project();
        }
    }
}

// ── Drum step sequencer grid ───────────────────────────────────────

fn show_drum_grid(app: &mut DawApp, ui: &mut egui::Ui, track_idx: usize) {
    let available = ui.available_size();
    let velocity_h = if app.piano_roll_state.show_velocity_lane {
        VELOCITY_LANE_HEIGHT + 8.0
    } else {
        0.0
    };
    let cc_h = if app.piano_roll_state.show_cc_lane {
        CC_LANE_HEIGHT + 8.0
    } else {
        0.0
    };
    let grid_height = (available.y - velocity_h - cc_h).max(60.0);

    let num_rows = DRUM_MAP.len();
    let row_h = (grid_height / num_rows as f32).max(16.0).min(28.0);
    let actual_grid_h = row_h * num_rows as f32;

    let grid_width = available.x;
    let quant = app.piano_roll_state.quantize_ticks;
    // Number of steps visible (4 bars worth)
    let total_ticks = TICKS_PER_BEAT * 16;
    let num_steps = (total_ticks / quant.max(1)) as usize;
    let step_w = ((grid_width - KEY_WIDTH) / num_steps as f32).max(8.0);

    let (response, painter) = ui.allocate_painter(
        egui::vec2(grid_width, actual_grid_h),
        egui::Sense::click(),
    );
    let rect = response.rect;

    // Background
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(28, 28, 33));

    let midi_clip_idx = app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }));

    // Collect existing note positions for hit detection
    let mut note_set: HashSet<(u8, u64)> = HashSet::new();
    if let Some(ci) = midi_clip_idx {
        if let ClipSource::Midi { ref notes, .. } =
            app.project.tracks[track_idx].clips[ci].source
        {
            for note in notes.iter() {
                // Quantize to step for matching
                let step_tick = (note.start_tick / quant.max(1)) * quant.max(1);
                note_set.insert((note.pitch, step_tick));
            }
        }
    }

    // Draw drum rows
    for (row, drum) in DRUM_MAP.iter().enumerate() {
        let y = rect.min.y + row as f32 * row_h;

        // Alternating row background
        if row % 2 == 0 {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(rect.min.x, y),
                    egui::vec2(grid_width, row_h),
                ),
                0.0,
                egui::Color32::from_rgb(32, 32, 38),
            );
        }

        // Drum name label
        let label_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, y),
            egui::vec2(KEY_WIDTH, row_h),
        );
        painter.rect_filled(label_rect, 0.0, egui::Color32::from_rgb(45, 45, 52));
        painter.text(
            egui::pos2(rect.min.x + 3.0, y + row_h * 0.5),
            egui::Align2::LEFT_CENTER,
            drum.name,
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(180, 180, 190),
        );

        // Draw step cells
        for step in 0..num_steps {
            let x = rect.min.x + KEY_WIDTH + step as f32 * step_w;
            let tick = step as u64 * quant.max(1);
            let has_note = note_set.contains(&(drum.pitch, tick));

            let cell_rect = egui::Rect::from_min_size(
                egui::pos2(x + 1.0, y + 1.0),
                egui::vec2(step_w - 2.0, row_h - 2.0),
            );

            // Beat emphasis
            let is_beat = (step * quant.max(1) as usize) % (TICKS_PER_BEAT as usize) == 0;
            let is_bar = (step * quant.max(1) as usize) % (TICKS_PER_BEAT as usize * 4) == 0;

            let bg = if has_note {
                egui::Color32::from_rgb(120, 180, 255)
            } else if is_bar {
                egui::Color32::from_rgb(50, 50, 58)
            } else if is_beat {
                egui::Color32::from_rgb(42, 42, 50)
            } else {
                egui::Color32::from_rgb(36, 36, 42)
            };

            painter.rect_filled(cell_rect, 2.0, bg);

            // Grid line at beats
            if is_beat {
                painter.line_segment(
                    [egui::pos2(x, rect.min.y), egui::pos2(x, rect.min.y + actual_grid_h)],
                    egui::Stroke::new(
                        if is_bar { 1.0 } else { 0.5 },
                        egui::Color32::from_rgb(55, 55, 65),
                    ),
                );
            }
        }

        // Horizontal row divider
        painter.line_segment(
            [
                egui::pos2(rect.min.x, y + row_h),
                egui::pos2(rect.max.x, y + row_h),
            ],
            egui::Stroke::new(0.5, egui::Color32::from_rgb(45, 45, 55)),
        );
    }

    // ── Click to toggle notes ──────────────────────────────
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos {
            let grid_x = pos.x - rect.min.x - KEY_WIDTH;
            let grid_y = pos.y - rect.min.y;
            if grid_x > 0.0 && grid_y > 0.0 {
                let row = (grid_y / row_h) as usize;
                let step = (grid_x / step_w) as usize;
                if row < DRUM_MAP.len() && step < num_steps {
                    let pitch = DRUM_MAP[row].pitch;
                    let tick = step as u64 * quant.max(1);

                    let clip_idx = find_or_create_midi_clip(app, track_idx);
                    if let Some(ci) = clip_idx {
                        // Check existence first with an immutable borrow
                        let existing = if let ClipSource::Midi { ref notes, .. } =
                            app.project.tracks[track_idx].clips[ci].source
                        {
                            notes.iter().position(|n| {
                                n.pitch == pitch
                                    && (n.start_tick / quant.max(1)) * quant.max(1) == tick
                            })
                        } else {
                            None
                        };

                        if let Some(idx) = existing {
                            // Toggle off — remove
                            app.push_undo("Remove drum hit");
                            if let ClipSource::Midi { ref mut notes, .. } =
                                app.project.tracks[track_idx].clips[ci].source
                            {
                                notes.remove(idx);
                            }
                            app.piano_roll_state.selected_notes.clear();
                        } else {
                            // Toggle on — add
                            app.push_undo("Add drum hit");
                            if let ClipSource::Midi { ref mut notes, .. } =
                                app.project.tracks[track_idx].clips[ci].source
                            {
                                notes.push(MidiNote {
                                    pitch,
                                    velocity: DEFAULT_DRUM_VELOCITY,
                                    start_tick: tick,
                                    duration_ticks: quant.max(1),
                                });
                            }
                        }
                        update_clip_duration(app, track_idx);
                        app.sync_project();
                    }
                }
            }
        }
    }

    // Velocity lane for drum mode
    if app.piano_roll_state.show_velocity_lane {
        let pixels_per_tick = step_w / quant.max(1) as f32;
        show_velocity_lane(app, ui, track_idx, grid_width, pixels_per_tick);
    }
}

// ── CC Lane ────────────────────────────────────────────────────────

fn show_cc_lane(app: &mut DawApp, ui: &mut egui::Ui, track_idx: usize) {
    ui.separator();
    let grid_width = ui.available_width();
    let pixels_per_tick = (grid_width - KEY_WIDTH) / (TICKS_PER_BEAT as f32 * 16.0);
    let cc_number = app.piano_roll_state.cc_number;

    let (cc_response, cc_painter) = ui.allocate_painter(
        egui::vec2(grid_width, CC_LANE_HEIGHT),
        egui::Sense::click_and_drag(),
    );
    let cc_rect = cc_response.rect;

    // Background
    cc_painter.rect_filled(
        cc_rect,
        0.0,
        egui::Color32::from_rgb(25, 28, 32),
    );

    // Label
    let cc_label = CC_PRESETS
        .iter()
        .find(|p| p.number == cc_number)
        .map(|p| p.name)
        .unwrap_or("CC");
    cc_painter.text(
        egui::pos2(cc_rect.min.x + 3.0, cc_rect.min.y + 2.0),
        egui::Align2::LEFT_TOP,
        cc_label,
        egui::FontId::proportional(9.0),
        egui::Color32::from_rgb(140, 160, 140),
    );

    // Guide lines
    for frac in &[0.25f32, 0.5, 0.75, 1.0] {
        let y = cc_rect.max.y - cc_rect.height() * frac;
        cc_painter.line_segment(
            [
                egui::pos2(cc_rect.min.x + KEY_WIDTH, y),
                egui::pos2(cc_rect.max.x, y),
            ],
            egui::Stroke::new(0.5, egui::Color32::from_rgb(45, 55, 50)),
        );
    }

    let midi_clip_idx = app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }));

    // Draw existing CC points and connecting lines
    if let Some(ci) = midi_clip_idx {
        if let ClipSource::Midi { ref cc_events, .. } =
            app.project.tracks[track_idx].clips[ci].source
        {
            // Filter to current CC number and sort by tick
            let mut relevant: Vec<&MidiCC> = cc_events
                .iter()
                .filter(|cc| cc.cc_number == cc_number)
                .collect();
            relevant.sort_by_key(|cc| cc.tick);

            // Draw connecting lines between points
            for pair in relevant.windows(2) {
                let x1 = cc_rect.min.x + KEY_WIDTH + pair[0].tick as f32 * pixels_per_tick;
                let y1 = cc_rect.max.y - (pair[0].value as f32 / 127.0) * cc_rect.height();
                let x2 = cc_rect.min.x + KEY_WIDTH + pair[1].tick as f32 * pixels_per_tick;
                let y2 = cc_rect.max.y - (pair[1].value as f32 / 127.0) * cc_rect.height();
                cc_painter.line_segment(
                    [egui::pos2(x1, y1), egui::pos2(x2, y2)],
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 180, 120)),
                );
            }

            // Draw points
            for cc in &relevant {
                let x = cc_rect.min.x + KEY_WIDTH + cc.tick as f32 * pixels_per_tick;
                let y = cc_rect.max.y - (cc.value as f32 / 127.0) * cc_rect.height();
                cc_painter.circle_filled(
                    egui::pos2(x, y),
                    3.5,
                    egui::Color32::from_rgb(100, 220, 140),
                );
            }
        }
    }

    // CC editing interaction — click or drag to add/update CC points
    if cc_response.drag_started() {
        app.piano_roll_state.drag_mode = Some(DragMode::CCEdit);
        app.push_undo("Edit MIDI CC");
    }

    if (cc_response.dragged() || cc_response.clicked())
        && matches!(
            app.piano_roll_state.drag_mode,
            Some(DragMode::CCEdit) | None
        )
    {
        if let Some(pos) = cc_response.interact_pointer_pos {
            let grid_x = pos.x - cc_rect.min.x - KEY_WIDTH;
            if grid_x > 0.0 {
                let tick = (grid_x / pixels_per_tick) as u64;
                let quant = app.piano_roll_state.quantize_ticks;
                let snapped_tick = if quant > 0 {
                    ((tick + quant / 2) / quant) * quant
                } else {
                    tick
                };
                let frac = 1.0
                    - ((pos.y - cc_rect.min.y) / cc_rect.height()).clamp(0.0, 1.0);
                let value = (frac * 127.0) as u8;

                let clip_idx = find_or_create_midi_clip(app, track_idx);
                if let Some(ci) = clip_idx {
                    if let ClipSource::Midi { ref mut cc_events, .. } =
                        app.project.tracks[track_idx].clips[ci].source
                    {
                        // Find existing CC at this tick (within quantize window)
                        let existing = cc_events.iter().position(|cc| {
                            cc.cc_number == cc_number && cc.tick == snapped_tick
                        });

                        if let Some(idx) = existing {
                            cc_events[idx].value = value;
                        } else {
                            cc_events.push(MidiCC {
                                tick: snapped_tick,
                                cc_number,
                                value,
                            });
                        }
                    }
                }
            }
        }
    }

    if cc_response.drag_stopped() {
        if matches!(app.piano_roll_state.drag_mode, Some(DragMode::CCEdit)) {
            app.piano_roll_state.drag_mode = None;
            app.sync_project();
        }
    }
}

// ── Helper functions ────────────────────────────────────────────────

fn find_or_create_midi_clip(app: &mut DawApp, track_idx: usize) -> Option<usize> {
    let existing = app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }));

    if let Some(idx) = existing {
        return Some(idx);
    }

    let clip = Clip {
        id: Uuid::new_v4(),
        name: "MIDI".to_string(),
        start_sample: 0,
        duration_samples: 0,
        source: ClipSource::Midi {
            notes: Vec::new(),
            cc_events: Vec::new(),
        },
        muted: false,
        fade_in_samples: 0,
        fade_out_samples: 0,
        color: None,
        playback_rate: 1.0,
        preserve_pitch: false,
        loop_count: 1,
        gain_db: 0.0,
        take_index: 0,
        content_offset: 0,
        transpose_semitones: 0,
        reversed: false,
    };
    app.project.tracks[track_idx].clips.push(clip);
    Some(app.project.tracks[track_idx].clips.len() - 1)
}

fn update_clip_duration(app: &mut DawApp, track_idx: usize) {
    let ci = match app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }))
    {
        Some(i) => i,
        None => return,
    };

    let max_tick = if let ClipSource::Midi { ref notes, .. } =
        app.project.tracks[track_idx].clips[ci].source
    {
        notes
            .iter()
            .map(|n| n.start_tick + n.duration_ticks)
            .max()
            .unwrap_or(0)
    } else {
        0
    };

    let sr = app.sample_rate() as f64;
    let samples_per_tick = app.project.tempo.samples_per_beat(sr) / TICKS_PER_BEAT as f64;
    app.project.tracks[track_idx].clips[ci].duration_samples =
        (max_tick as f64 * samples_per_tick) as u64;
}

fn select_all_notes(app: &mut DawApp, track_idx: usize) {
    let ci = match app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }))
    {
        Some(i) => i,
        None => return,
    };

    if let ClipSource::Midi { ref notes, .. } = app.project.tracks[track_idx].clips[ci].source {
        app.piano_roll_state.selected_notes = (0..notes.len()).collect();
    }
}

fn delete_selected(app: &mut DawApp, track_idx: usize) {
    if app.piano_roll_state.selected_notes.is_empty() {
        return;
    }

    let ci = match app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }))
    {
        Some(i) => i,
        None => return,
    };

    app.push_undo("Delete MIDI notes");

    // Remove in reverse index order to keep indices valid
    let mut indices: Vec<usize> = app.piano_roll_state.selected_notes.iter().copied().collect();
    indices.sort_unstable_by(|a, b| b.cmp(a));

    if let ClipSource::Midi { ref mut notes, .. } = app.project.tracks[track_idx].clips[ci].source {
        for idx in indices {
            if idx < notes.len() {
                notes.remove(idx);
            }
        }
    }

    app.piano_roll_state.selected_notes.clear();
    update_clip_duration(app, track_idx);
    app.sync_project();
}

fn quantize_selected(app: &mut DawApp, track_idx: usize) {
    if app.piano_roll_state.selected_notes.is_empty() {
        app.set_status("Select notes first, then quantize");
        return;
    }

    let ci = match app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }))
    {
        Some(i) => i,
        None => return,
    };

    let quant = app.piano_roll_state.quantize_ticks;
    if quant == 0 {
        return;
    }

    let swing = app.piano_roll_state.swing_amount;
    app.push_undo("Quantize MIDI notes");

    let selected: Vec<usize> = app.piano_roll_state.selected_notes.iter().copied().collect();
    if let ClipSource::Midi { ref mut notes, .. } = app.project.tracks[track_idx].clips[ci].source {
        for &idx in &selected {
            if idx < notes.len() {
                let t = notes[idx].start_tick;
                // Snap to nearest grid position
                let grid_pos = ((t + quant / 2) / quant) * quant;
                // Determine which grid index this is (0-based)
                let grid_index = grid_pos / quant;
                // Apply swing: odd-numbered grid positions shift toward the next one
                let swung = if grid_index % 2 == 1 && swing > 0.0 {
                    grid_pos + (swing * quant as f32) as u64
                } else {
                    grid_pos
                };
                notes[idx].start_tick = swung;
            }
        }
    }

    update_clip_duration(app, track_idx);
    app.sync_project();
    let swing_pct = (swing * 100.0).round() as i32;
    let swing_info = if swing_pct > 0 {
        format!(" (swing {swing_pct}%)")
    } else {
        String::new()
    };
    app.set_status(&format!(
        "Quantized {} notes to {}{swing_info}",
        selected.len(),
        quantize_label(quant)
    ));
}

/// Humanize selected notes: randomize timing and velocity by a configurable amount.
fn humanize_selected(app: &mut DawApp, track_idx: usize) {
    if app.piano_roll_state.selected_notes.is_empty() {
        app.set_status("Select notes first, then humanize");
        return;
    }

    let ci = match app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }))
    {
        Some(i) => i,
        None => return,
    };

    let amount = app.piano_roll_state.humanize_amount;
    // Max timing randomization: 20 ticks at 100%
    let max_timing_ticks = (amount * 20.0) as i64;
    // Max velocity randomization: 15 at 100%
    let max_vel = (amount * 15.0) as i32;

    app.push_undo("Humanize MIDI notes");

    let selected: Vec<usize> = app.piano_roll_state.selected_notes.iter().copied().collect();
    if let ClipSource::Midi { ref mut notes, .. } = app.project.tracks[track_idx].clips[ci].source {
        for &idx in &selected {
            if idx < notes.len() {
                // Randomize timing
                if max_timing_ticks > 0 {
                    let rand_t = app.piano_roll_state.next_rand_bipolar();
                    let delta = (rand_t * max_timing_ticks as f64).round() as i64;
                    let new_tick = (notes[idx].start_tick as i64 + delta).max(0) as u64;
                    notes[idx].start_tick = new_tick;
                }
                // Randomize velocity
                if max_vel > 0 {
                    let rand_v = app.piano_roll_state.next_rand_bipolar();
                    let delta = (rand_v * max_vel as f64).round() as i32;
                    let new_vel = (notes[idx].velocity as i32 + delta).clamp(1, 127) as u8;
                    notes[idx].velocity = new_vel;
                }
            }
        }
    }

    update_clip_duration(app, track_idx);
    app.sync_project();
    app.set_status(&format!(
        "Humanized {} notes ({:.0}%)",
        selected.len(),
        amount * 100.0
    ));
}

/// Legato: extend each selected note to reach the next note at the same pitch (minus 1 tick).
fn legato_selected(app: &mut DawApp, track_idx: usize) {
    if app.piano_roll_state.selected_notes.is_empty() {
        app.set_status("Select notes first, then apply legato");
        return;
    }

    let ci = match app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }))
    {
        Some(i) => i,
        None => return,
    };

    app.push_undo("Legato MIDI notes");

    let selected: Vec<usize> = app.piano_roll_state.selected_notes.iter().copied().collect();
    if let ClipSource::Midi { ref mut notes, .. } = app.project.tracks[track_idx].clips[ci].source {
        // For each selected note, find the next note at the same pitch
        // and extend duration to reach it (minus 1 tick for separation).
        // We need a snapshot of start_ticks and pitches to avoid borrow issues.
        let snapshot: Vec<(u64, u8)> = notes.iter().map(|n| (n.start_tick, n.pitch)).collect();

        for &idx in &selected {
            if idx >= notes.len() {
                continue;
            }
            let pitch = snapshot[idx].1;
            let start = snapshot[idx].0;

            // Find the next note at the same pitch (by start_tick, excluding self)
            let mut next_start: Option<u64> = None;
            for (i, &(s, p)) in snapshot.iter().enumerate() {
                if i == idx {
                    continue;
                }
                if p == pitch && s > start {
                    next_start = Some(match next_start {
                        Some(curr) => curr.min(s),
                        None => s,
                    });
                }
            }

            if let Some(ns) = next_start {
                // Extend to reach next note minus 1 tick separation
                let new_dur = ns.saturating_sub(start).saturating_sub(1).max(1);
                notes[idx].duration_ticks = new_dur;
            }
        }
    }

    update_clip_duration(app, track_idx);
    app.sync_project();
    app.set_status(&format!("Applied legato to {} notes", selected.len()));
}

/// Staccato: shorten each selected note to 50% of its duration.
fn staccato_selected(app: &mut DawApp, track_idx: usize) {
    if app.piano_roll_state.selected_notes.is_empty() {
        app.set_status("Select notes first, then apply staccato");
        return;
    }

    let ci = match app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }))
    {
        Some(i) => i,
        None => return,
    };

    app.push_undo("Staccato MIDI notes");

    let selected: Vec<usize> = app.piano_roll_state.selected_notes.iter().copied().collect();
    if let ClipSource::Midi { ref mut notes, .. } = app.project.tracks[track_idx].clips[ci].source {
        for &idx in &selected {
            if idx < notes.len() {
                let half = notes[idx].duration_ticks / 2;
                notes[idx].duration_ticks = half.max(1); // At least 1 tick
            }
        }
    }

    update_clip_duration(app, track_idx);
    app.sync_project();
    app.set_status(&format!(
        "Applied staccato to {} notes (50%)",
        selected.len()
    ));
}

/// Set velocity of all selected notes to a preset value.
fn set_selected_velocity(app: &mut DawApp, track_idx: usize, velocity: u8) {
    if app.piano_roll_state.selected_notes.is_empty() {
        app.set_status("Select notes first, then set velocity");
        return;
    }

    let ci = match app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }))
    {
        Some(i) => i,
        None => return,
    };

    app.push_undo("Set velocity preset");

    let selected: Vec<usize> = app.piano_roll_state.selected_notes.iter().copied().collect();
    if let ClipSource::Midi { ref mut notes, .. } = app.project.tracks[track_idx].clips[ci].source {
        for &idx in &selected {
            if idx < notes.len() {
                notes[idx].velocity = velocity;
            }
        }
    }

    app.sync_project();
    app.set_status(&format!(
        "Set {} notes to velocity {velocity}",
        selected.len()
    ));
}

fn quantize_label(ticks: u64) -> &'static str {
    match ticks {
        t if t == TICKS_PER_BEAT * 4 => "1 Bar",
        t if t == TICKS_PER_BEAT * 2 => "1/2",
        t if t == TICKS_PER_BEAT => "1/4",
        t if t == TICKS_PER_BEAT / 2 => "1/8",
        t if t == TICKS_PER_BEAT / 4 => "1/16",
        t if t == TICKS_PER_BEAT / 8 => "1/32",
        _ => "Custom",
    }
}
