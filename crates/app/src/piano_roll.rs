use eframe::egui;
use jamhub_model::{Clip, ClipSource, MidiNote, TrackKind};
use uuid::Uuid;

use crate::DawApp;

const NOTE_HEIGHT: f32 = 8.0;
const KEY_WIDTH: f32 = 40.0;
const TOTAL_NOTES: u8 = 128;
const TICKS_PER_BEAT: u64 = 480;

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_piano_roll {
        return;
    }

    let track_idx = match app.selected_track {
        Some(i) if i < app.project.tracks.len() => i,
        _ => return,
    };

    if app.project.tracks[track_idx].kind != TrackKind::Midi {
        app.show_piano_roll = false;
        app.set_status("Select a MIDI track to open piano roll");
        return;
    }

    let mut open = true;
    egui::Window::new("Piano Roll")
        .open(&mut open)
        .default_size([800.0, 400.0])
        .show(ctx, |ui| {
            let track_name = app.project.tracks[track_idx].name.clone();
            ui.heading(format!("Piano Roll: {track_name}"));

            ui.horizontal(|ui| {
                if ui.button("Add Note (test)").clicked() {
                    // Find or create a MIDI clip on this track
                    let clip_idx = find_or_create_midi_clip(app, track_idx);
                    if let Some(ci) = clip_idx {
                        if let ClipSource::Midi { ref mut notes } =
                            app.project.tracks[track_idx].clips[ci].source
                        {
                            notes.push(MidiNote {
                                pitch: 60, // Middle C
                                velocity: 100,
                                start_tick: notes.len() as u64 * TICKS_PER_BEAT,
                                duration_ticks: TICKS_PER_BEAT,
                            });
                            app.sync_project();
                        }
                    }
                }
                ui.label(
                    egui::RichText::new("Click grid to add notes, drag to move")
                        .small()
                        .color(egui::Color32::GRAY),
                );
            });

            ui.separator();

            // Piano roll grid
            let available = ui.available_size();
            let (response, painter) =
                ui.allocate_painter(available, egui::Sense::click_and_drag());
            let rect = response.rect;

            // Background
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(30, 30, 35));

            // Find MIDI clip
            let midi_clip_idx = app.project.tracks[track_idx]
                .clips
                .iter()
                .position(|c| matches!(c.source, ClipSource::Midi { .. }));

            let visible_notes_start: u8 = 36; // C2
            let visible_notes_end: u8 = 96; // C7
            let note_range = (visible_notes_end - visible_notes_start) as f32;
            let note_h = (rect.height() / note_range).max(4.0);
            let bpm = app.project.tempo.bpm;
            let pixels_per_tick = (available.x - KEY_WIDTH) / (TICKS_PER_BEAT as f32 * 16.0); // Show 16 beats

            // Piano keys
            for note in visible_notes_start..visible_notes_end {
                let y = rect.max.y
                    - (note - visible_notes_start) as f32 * note_h
                    - note_h;
                let is_black = matches!(note % 12, 1 | 3 | 6 | 8 | 10);
                let key_color = if is_black {
                    egui::Color32::from_rgb(40, 40, 45)
                } else {
                    egui::Color32::from_rgb(55, 55, 60)
                };
                let key_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x, y),
                    egui::vec2(KEY_WIDTH, note_h),
                );
                painter.rect_filled(key_rect, 0.0, key_color);

                // Note name on C notes
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

                // Grid line
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

            // Beat grid lines
            for beat in 0..17 {
                let x = rect.min.x + KEY_WIDTH + beat as f32 * TICKS_PER_BEAT as f32 * pixels_per_tick;
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

            // Draw MIDI notes
            if let Some(ci) = midi_clip_idx {
                if let ClipSource::Midi { ref notes } = app.project.tracks[track_idx].clips[ci].source
                {
                    for note in notes {
                        if note.pitch < visible_notes_start || note.pitch >= visible_notes_end {
                            continue;
                        }
                        let x = rect.min.x
                            + KEY_WIDTH
                            + note.start_tick as f32 * pixels_per_tick;
                        let w = note.duration_ticks as f32 * pixels_per_tick;
                        let y = rect.max.y
                            - (note.pitch - visible_notes_start) as f32 * note_h
                            - note_h;

                        let note_rect = egui::Rect::from_min_size(
                            egui::pos2(x, y + 1.0),
                            egui::vec2(w, note_h - 2.0),
                        );

                        let vel_alpha = note.velocity as f32 / 127.0;
                        let color = egui::Color32::from_rgb(
                            (100.0 + 155.0 * vel_alpha) as u8,
                            (180.0 * vel_alpha) as u8,
                            255,
                        );
                        painter.rect_filled(note_rect, 2.0, color);
                    }
                }
            }

            // Click to add note
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos {
                    let grid_x = pos.x - rect.min.x - KEY_WIDTH;
                    let grid_y = rect.max.y - pos.y;

                    if grid_x > 0.0 && grid_y > 0.0 {
                        let tick = (grid_x / pixels_per_tick) as u64;
                        // Quantize to quarter note
                        let tick = (tick / TICKS_PER_BEAT) * TICKS_PER_BEAT;
                        let pitch = visible_notes_start + (grid_y / note_h) as u8;

                        if pitch < visible_notes_end {
                            let clip_idx = find_or_create_midi_clip(app, track_idx);
                            if let Some(ci) = clip_idx {
                                app.push_undo("Add MIDI note");
                                if let ClipSource::Midi { ref mut notes } =
                                    app.project.tracks[track_idx].clips[ci].source
                                {
                                    notes.push(MidiNote {
                                        pitch,
                                        velocity: 100,
                                        start_tick: tick,
                                        duration_ticks: TICKS_PER_BEAT,
                                    });
                                }
                                // Update clip duration
                                let max_tick = if let ClipSource::Midi { ref notes } =
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
                                let samples_per_tick = app.project.tempo.samples_per_beat(sr)
                                    / TICKS_PER_BEAT as f64;
                                app.project.tracks[track_idx].clips[ci].duration_samples =
                                    (max_tick as f64 * samples_per_tick) as u64;
                                app.sync_project();
                            }
                        }
                    }
                }
            }
        });

    if !open {
        app.show_piano_roll = false;
    }
}

fn find_or_create_midi_clip(app: &mut DawApp, track_idx: usize) -> Option<usize> {
    // Find existing MIDI clip
    let existing = app.project.tracks[track_idx]
        .clips
        .iter()
        .position(|c| matches!(c.source, ClipSource::Midi { .. }));

    if let Some(idx) = existing {
        return Some(idx);
    }

    // Create new MIDI clip
    let clip = Clip {
        id: Uuid::new_v4(),
        name: "MIDI".to_string(),
        start_sample: 0,
        duration_samples: 0,
        source: ClipSource::Midi { notes: Vec::new() },
        muted: false,
    };
    app.project.tracks[track_idx].clips.push(clip);
    Some(app.project.tracks[track_idx].clips.len() - 1)
}
