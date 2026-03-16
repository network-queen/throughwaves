use std::collections::HashMap;

use eframe::egui;
use jamhub_model::{ClipSource, Scene, SessionClip, TrackKind};
use uuid::Uuid;

use crate::DawApp;

/// Width of the scene launch column on the left.
const SCENE_LAUNCH_WIDTH: f32 = 80.0;
/// Width of each track column.
const TRACK_COL_WIDTH: f32 = 120.0;
/// Height of each clip slot row.
const SLOT_HEIGHT: f32 = 48.0;
/// Height of the track header row.
const HEADER_HEIGHT: f32 = 60.0;
/// Height of the stop-all row at the bottom of columns.
const STOP_ROW_HEIGHT: f32 = 36.0;

/// Playback state for a session clip slot.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClipPlayState {
    /// Clip is stopped / idle.
    Stopped,
    /// Clip is playing (looping).
    Playing,
    /// Clip is queued to start at the next bar boundary.
    Queued,
}

/// Per-track session playback state.
pub struct TrackSessionState {
    /// Index of the currently playing scene slot (if any).
    pub playing_scene: Option<usize>,
    /// Index of the queued scene slot (if any) — will start at next bar.
    pub queued_scene: Option<usize>,
    /// Sample position where the current clip started playing.
    pub play_start_sample: u64,
}

/// Global session view state held in DawApp.
pub struct SessionViewState {
    /// Per-track playback state, keyed by track index.
    pub track_states: HashMap<usize, TrackSessionState>,
    /// Animation time for pulsing/blinking effects.
    pub anim_time: f64,
}

impl Default for SessionViewState {
    fn default() -> Self {
        Self {
            track_states: HashMap::new(),
            anim_time: 0.0,
        }
    }
}

/// Main session view rendering function.
pub fn show(app: &mut DawApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    // Update animation time
    app.session_view_state.anim_time += ctx.input(|i| i.predicted_dt as f64);

    // Ensure scenes exist (at least 8 default scenes)
    ensure_scenes(app);

    let accent = egui::Color32::from_rgb(235, 180, 60);
    let playing_color = egui::Color32::from_rgb(80, 220, 120);
    let queued_color = egui::Color32::from_rgb(255, 180, 60);
    let bg_dark = egui::Color32::from_rgb(24, 24, 28);
    let bg_slot = egui::Color32::from_rgb(32, 33, 40);
    let bg_slot_hover = egui::Color32::from_rgb(42, 44, 54);
    let text_dim = egui::Color32::from_rgb(140, 138, 134);

    // Process queued clips: check if we've crossed a bar boundary
    process_quantized_launch(app);

    let num_scenes = app.project.scenes.len();
    let num_tracks = app.project.tracks.len();

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let total_width = SCENE_LAUNCH_WIDTH + TRACK_COL_WIDTH * num_tracks as f32 + 20.0;
            let total_height = HEADER_HEIGHT + SLOT_HEIGHT * num_scenes as f32 + STOP_ROW_HEIGHT + 20.0;
            ui.set_min_size(egui::vec2(total_width, total_height));

            let origin = ui.cursor().min;

            // ---- Track headers ----
            for ti in 0..num_tracks {
                let x = origin.x + SCENE_LAUNCH_WIDTH + ti as f32 * TRACK_COL_WIDTH;
                let header_rect = egui::Rect::from_min_size(
                    egui::pos2(x, origin.y),
                    egui::vec2(TRACK_COL_WIDTH - 2.0, HEADER_HEIGHT - 2.0),
                );
                let track_color = {
                    let c = app.project.tracks[ti].color;
                    egui::Color32::from_rgb(c[0], c[1], c[2])
                };

                ui.painter().rect_filled(header_rect, 4.0, bg_dark);
                // Color strip at top
                let strip = egui::Rect::from_min_size(
                    header_rect.min,
                    egui::vec2(header_rect.width(), 3.0),
                );
                ui.painter().rect_filled(strip, 2.0, track_color);

                // Track name
                let name = app.project.tracks[ti].name.clone();
                let kind_label = match app.project.tracks[ti].kind {
                    TrackKind::Audio => "AUD",
                    TrackKind::Midi => "MID",
                    TrackKind::Bus => "BUS",
                };
                ui.painter().text(
                    egui::pos2(header_rect.center().x, header_rect.min.y + 16.0),
                    egui::Align2::CENTER_CENTER,
                    &name,
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
                ui.painter().text(
                    egui::pos2(header_rect.center().x, header_rect.min.y + 32.0),
                    egui::Align2::CENTER_CENTER,
                    kind_label,
                    egui::FontId::proportional(10.0),
                    text_dim,
                );

                // Stop button for this track (in header)
                let stop_rect = egui::Rect::from_min_size(
                    egui::pos2(header_rect.center().x - 14.0, header_rect.min.y + 42.0),
                    egui::vec2(28.0, 14.0),
                );
                let stop_resp = ui.allocate_rect(stop_rect, egui::Sense::click());
                let stop_fill = if stop_resp.hovered() {
                    egui::Color32::from_rgb(180, 60, 60)
                } else {
                    egui::Color32::from_rgb(120, 50, 50)
                };
                ui.painter().rect_filled(stop_rect, 3.0, stop_fill);
                ui.painter().text(
                    stop_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "STOP",
                    egui::FontId::proportional(8.0),
                    egui::Color32::WHITE,
                );
                if stop_resp.clicked() {
                    app.session_view_state.track_states.remove(&ti);
                    app.set_status(&format!("Stopped track: {}", name));
                }
            }

            // ---- Scene rows ----
            for si in 0..num_scenes {
                let y = origin.y + HEADER_HEIGHT + si as f32 * SLOT_HEIGHT;

                // Scene launch button (left column)
                let scene_rect = egui::Rect::from_min_size(
                    egui::pos2(origin.x, y),
                    egui::vec2(SCENE_LAUNCH_WIDTH - 2.0, SLOT_HEIGHT - 2.0),
                );
                let scene_resp = ui.allocate_rect(scene_rect, egui::Sense::click());
                let scene_fill = if scene_resp.hovered() {
                    accent.gamma_multiply(0.35)
                } else {
                    bg_dark
                };
                ui.painter().rect_filled(scene_rect, 4.0, scene_fill);

                // Scene name
                let scene_name = app.project.scenes[si].name.clone();
                ui.painter().text(
                    egui::pos2(scene_rect.min.x + 6.0, scene_rect.center().y - 6.0),
                    egui::Align2::LEFT_CENTER,
                    &scene_name,
                    egui::FontId::proportional(11.0),
                    egui::Color32::WHITE,
                );
                // Play triangle icon
                let tri_center = egui::pos2(scene_rect.max.x - 16.0, scene_rect.center().y);
                let tri_size = 6.0;
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(tri_center.x - tri_size * 0.5, tri_center.y - tri_size),
                        egui::pos2(tri_center.x + tri_size, tri_center.y),
                        egui::pos2(tri_center.x - tri_size * 0.5, tri_center.y + tri_size),
                    ],
                    accent,
                    egui::Stroke::NONE,
                ));

                if scene_resp.clicked() {
                    // Launch all clips in this scene
                    launch_scene(app, si);
                }

                // ---- Clip slots for each track ----
                for ti in 0..num_tracks {
                    let x = origin.x + SCENE_LAUNCH_WIDTH + ti as f32 * TRACK_COL_WIDTH;
                    let slot_rect = egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(TRACK_COL_WIDTH - 2.0, SLOT_HEIGHT - 2.0),
                    );

                    let play_state = get_clip_play_state(app, ti, si);
                    let has_clip = get_session_clip(app, ti, si).is_some();

                    let slot_resp = ui.allocate_rect(slot_rect, egui::Sense::click());

                    // Background
                    let fill = if slot_resp.hovered() {
                        bg_slot_hover
                    } else {
                        bg_slot
                    };
                    ui.painter().rect_filled(slot_rect, 4.0, fill);

                    if has_clip {
                        let clip_name = get_session_clip(app, ti, si)
                            .map(|c| c.name.clone())
                            .unwrap_or_default();
                        let clip_color = get_session_clip(app, ti, si)
                            .and_then(|c| c.color)
                            .map(|c| egui::Color32::from_rgb(c[0], c[1], c[2]))
                            .unwrap_or_else(|| {
                                let tc = app.project.tracks[ti].color;
                                egui::Color32::from_rgb(tc[0], tc[1], tc[2])
                            });

                        // Filled clip cell
                        let inner = slot_rect.shrink(2.0);
                        let clip_fill = match play_state {
                            ClipPlayState::Playing => {
                                // Pulsing glow effect
                                let pulse = (app.session_view_state.anim_time * 3.0).sin() as f32 * 0.15 + 0.85;
                                clip_color.gamma_multiply(pulse)
                            }
                            ClipPlayState::Queued => {
                                // Blinking effect
                                let blink = (app.session_view_state.anim_time * 6.0).sin() > 0.0;
                                if blink { queued_color } else { clip_color.gamma_multiply(0.5) }
                            }
                            ClipPlayState::Stopped => clip_color.gamma_multiply(0.45),
                        };
                        ui.painter().rect_filled(inner, 3.0, clip_fill);

                        // Clip name text
                        ui.painter().text(
                            egui::pos2(inner.min.x + 4.0, inner.center().y),
                            egui::Align2::LEFT_CENTER,
                            &clip_name,
                            egui::FontId::proportional(11.0),
                            egui::Color32::WHITE,
                        );

                        // Play state indicator dot
                        match play_state {
                            ClipPlayState::Playing => {
                                ui.painter().circle_filled(
                                    egui::pos2(inner.max.x - 8.0, inner.min.y + 8.0),
                                    4.0,
                                    playing_color,
                                );
                            }
                            ClipPlayState::Queued => {
                                ui.painter().circle_filled(
                                    egui::pos2(inner.max.x - 8.0, inner.min.y + 8.0),
                                    4.0,
                                    queued_color,
                                );
                            }
                            ClipPlayState::Stopped => {}
                        }

                        if slot_resp.clicked() {
                            // Toggle: if playing/queued, stop; otherwise launch
                            if play_state == ClipPlayState::Playing || play_state == ClipPlayState::Queued {
                                app.session_view_state.track_states.remove(&ti);
                                app.set_status(&format!("Stopped clip: {}", clip_name));
                            } else {
                                launch_clip(app, ti, si);
                            }
                        }
                    } else {
                        // Empty slot — show "+" button
                        let plus_color = if slot_resp.hovered() {
                            accent.gamma_multiply(0.7)
                        } else {
                            text_dim.gamma_multiply(0.5)
                        };
                        ui.painter().text(
                            slot_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "+",
                            egui::FontId::proportional(18.0),
                            plus_color,
                        );

                        if slot_resp.clicked() {
                            add_empty_clip(app, ti, si);
                        }
                    }

                    // Slot border
                    let border_color = match play_state {
                        ClipPlayState::Playing => playing_color.gamma_multiply(0.6),
                        ClipPlayState::Queued => queued_color.gamma_multiply(0.6),
                        _ => egui::Color32::from_rgb(50, 52, 60),
                    };
                    ui.painter().rect_stroke(slot_rect, 4.0, egui::Stroke::new(1.0, border_color), egui::StrokeKind::Inside);
                }
            }

            // ---- Stop-all row ----
            let stop_y = origin.y + HEADER_HEIGHT + num_scenes as f32 * SLOT_HEIGHT;
            // Global stop button in the scene column
            let global_stop_rect = egui::Rect::from_min_size(
                egui::pos2(origin.x, stop_y),
                egui::vec2(SCENE_LAUNCH_WIDTH - 2.0, STOP_ROW_HEIGHT - 2.0),
            );
            let global_stop_resp = ui.allocate_rect(global_stop_rect, egui::Sense::click());
            let gstop_fill = if global_stop_resp.hovered() {
                egui::Color32::from_rgb(180, 50, 50)
            } else {
                egui::Color32::from_rgb(100, 40, 40)
            };
            ui.painter().rect_filled(global_stop_rect, 4.0, gstop_fill);
            ui.painter().text(
                global_stop_rect.center(),
                egui::Align2::CENTER_CENTER,
                "STOP ALL",
                egui::FontId::proportional(11.0),
                egui::Color32::WHITE,
            );
            if global_stop_resp.clicked() {
                app.session_view_state.track_states.clear();
                app.set_status("All clips stopped");
            }

            // Per-track stop buttons in the stop row
            for ti in 0..num_tracks {
                let x = origin.x + SCENE_LAUNCH_WIDTH + ti as f32 * TRACK_COL_WIDTH;
                let stop_rect = egui::Rect::from_min_size(
                    egui::pos2(x, stop_y),
                    egui::vec2(TRACK_COL_WIDTH - 2.0, STOP_ROW_HEIGHT - 2.0),
                );
                let is_playing = app.session_view_state.track_states.contains_key(&ti);
                let resp = ui.allocate_rect(stop_rect, egui::Sense::click());
                let fill = if resp.hovered() {
                    egui::Color32::from_rgb(140, 50, 50)
                } else if is_playing {
                    egui::Color32::from_rgb(90, 35, 35)
                } else {
                    bg_dark
                };
                ui.painter().rect_filled(stop_rect, 4.0, fill);
                // Stop square icon
                let sq = egui::Rect::from_center_size(stop_rect.center(), egui::vec2(10.0, 10.0));
                let sq_color = if is_playing {
                    egui::Color32::from_rgb(220, 80, 80)
                } else {
                    text_dim.gamma_multiply(0.4)
                };
                ui.painter().rect_filled(sq, 2.0, sq_color);

                if resp.clicked() && is_playing {
                    app.session_view_state.track_states.remove(&ti);
                    let name = app.project.tracks[ti].name.clone();
                    app.set_status(&format!("Stopped track: {}", name));
                }
            }

            // ---- Add Scene button ----
            let add_y = stop_y + STOP_ROW_HEIGHT + 4.0;
            let add_rect = egui::Rect::from_min_size(
                egui::pos2(origin.x, add_y),
                egui::vec2(SCENE_LAUNCH_WIDTH - 2.0, 28.0),
            );
            let add_resp = ui.allocate_rect(add_rect, egui::Sense::click());
            let add_fill = if add_resp.hovered() {
                accent.gamma_multiply(0.25)
            } else {
                bg_dark
            };
            ui.painter().rect_filled(add_rect, 4.0, add_fill);
            ui.painter().text(
                add_rect.center(),
                egui::Align2::CENTER_CENTER,
                "+ Add Scene",
                egui::FontId::proportional(11.0),
                accent.gamma_multiply(0.8),
            );
            if add_resp.clicked() {
                let num = app.project.scenes.len() + 1;
                app.project.scenes.push(Scene {
                    id: Uuid::new_v4(),
                    name: format!("Scene {}", num),
                });
                // Extend all tracks' session_clips to match
                for track in &mut app.project.tracks {
                    track.session_clips.push(None);
                }
                app.set_status(&format!("Added Scene {}", num));
            }
        });

    // Request repaint for animations while any clip is playing/queued
    if !app.session_view_state.track_states.is_empty() {
        ctx.request_repaint();
    }
}

/// Ensure the project has at least some default scenes.
fn ensure_scenes(app: &mut DawApp) {
    if app.project.scenes.is_empty() {
        let default_count = 8;
        for i in 1..=default_count {
            app.project.scenes.push(Scene {
                id: Uuid::new_v4(),
                name: format!("Scene {}", i),
            });
        }
    }

    // Ensure each track has enough session_clips slots
    let num_scenes = app.project.scenes.len();
    for track in &mut app.project.tracks {
        while track.session_clips.len() < num_scenes {
            track.session_clips.push(None);
        }
    }
}

/// Get a reference to a session clip at (track_idx, scene_idx), if present.
fn get_session_clip(app: &DawApp, track_idx: usize, scene_idx: usize) -> Option<&SessionClip> {
    app.project
        .tracks
        .get(track_idx)
        .and_then(|t| t.session_clips.get(scene_idx))
        .and_then(|slot| slot.as_ref())
}

/// Get the play state of a clip at (track_idx, scene_idx).
fn get_clip_play_state(app: &DawApp, track_idx: usize, scene_idx: usize) -> ClipPlayState {
    if let Some(state) = app.session_view_state.track_states.get(&track_idx) {
        if state.playing_scene == Some(scene_idx) {
            return ClipPlayState::Playing;
        }
        if state.queued_scene == Some(scene_idx) {
            return ClipPlayState::Queued;
        }
    }
    ClipPlayState::Stopped
}

/// Launch a single clip: queue it for the next bar boundary.
fn launch_clip(app: &mut DawApp, track_idx: usize, scene_idx: usize) {
    let clip_name = get_session_clip(app, track_idx, scene_idx)
        .map(|c| c.name.clone())
        .unwrap_or_default();

    let state = app
        .session_view_state
        .track_states
        .entry(track_idx)
        .or_insert(TrackSessionState {
            playing_scene: None,
            queued_scene: None,
            play_start_sample: 0,
        });

    state.queued_scene = Some(scene_idx);

    app.set_status(&format!("Queued: {}", clip_name));
}

/// Launch all clips in a scene across all tracks.
fn launch_scene(app: &mut DawApp, scene_idx: usize) {
    let scene_name = app.project.scenes[scene_idx].name.clone();
    let num_tracks = app.project.tracks.len();

    for ti in 0..num_tracks {
        if get_session_clip(app, ti, scene_idx).is_some() {
            let state = app
                .session_view_state
                .track_states
                .entry(ti)
                .or_insert(TrackSessionState {
                    playing_scene: None,
                    queued_scene: None,
                    play_start_sample: 0,
                });
            state.queued_scene = Some(scene_idx);
        }
    }

    app.set_status(&format!("Launched: {}", scene_name));
}

/// Process quantized launch: move queued clips to playing state at bar boundaries.
fn process_quantized_launch(app: &mut DawApp) {
    let sr = app.sample_rate() as f64;
    let spb = app.project.tempo.samples_per_beat(sr);
    let beats_per_bar = app.project.time_signature.numerator as f64;
    let samples_per_bar = (spb * beats_per_bar) as u64;

    if samples_per_bar == 0 {
        return;
    }

    let pos = app.position_samples();

    // Check if we're at (or just past) a bar boundary
    let bar_window = (sr * 0.02) as u64; // ~20ms window
    let current_bar_pos = (pos / samples_per_bar) * samples_per_bar;
    let at_bar = pos.saturating_sub(current_bar_pos) < bar_window;

    if !at_bar && pos > bar_window {
        return;
    }

    // Collect track indices that need updating
    let track_indices: Vec<usize> = app
        .session_view_state
        .track_states
        .keys()
        .copied()
        .collect();

    for ti in track_indices {
        if let Some(state) = app.session_view_state.track_states.get_mut(&ti) {
            if let Some(queued) = state.queued_scene {
                state.playing_scene = Some(queued);
                state.queued_scene = None;
                state.play_start_sample = pos;
            }
        }
    }
}

/// Add an empty placeholder session clip to a slot.
fn add_empty_clip(app: &mut DawApp, track_idx: usize, scene_idx: usize) {
    let sr = app.sample_rate() as f64;
    let spb = app.project.tempo.samples_per_beat(sr);
    let beats_per_bar = app.project.time_signature.numerator as f64;
    let one_bar_samples = (spb * beats_per_bar) as u64;

    let track_name = app.project.tracks[track_idx].name.clone();
    let scene_name = app.project.scenes[scene_idx].name.clone();
    let clip_name = format!("{} - {}", track_name, scene_name);

    let track_color = app.project.tracks[track_idx].color;

    let clip = SessionClip {
        clip_id: Uuid::new_v4(),
        name: clip_name.clone(),
        color: Some(track_color),
        source: ClipSource::AudioBuffer {
            buffer_id: Uuid::new_v4(),
        },
        duration_samples: one_bar_samples * 4, // 4 bars default
    };

    if let Some(track) = app.project.tracks.get_mut(track_idx) {
        while track.session_clips.len() <= scene_idx {
            track.session_clips.push(None);
        }
        track.session_clips[scene_idx] = Some(clip);
    }

    app.set_status(&format!("Created clip: {}", clip_name));
}
