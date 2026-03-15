use eframe::egui;
use jamhub_model::{ClipSource, TrackKind};

use crate::DawApp;

const TRACK_HEIGHT: f32 = 80.0;
const HEADER_WIDTH: f32 = 180.0;
const RULER_HEIGHT: f32 = 24.0;
const PIXELS_PER_SECOND_BASE: f32 = 100.0;

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    let pixels_per_second = PIXELS_PER_SECOND_BASE * app.zoom;
    let sample_rate = app.sample_rate() as f64;

    // Track headers (left panel)
    egui::SidePanel::left("track_headers")
        .exact_width(HEADER_WIDTH)
        .resizable(false)
        .show_inside(ui, |ui| {
            ui.allocate_space(egui::vec2(HEADER_WIDTH, RULER_HEIGHT));
            ui.separator();

            let mut track_actions: Vec<TrackAction> = Vec::new();

            for (i, track) in app.project.tracks.iter().enumerate() {
                ui.push_id(i, |ui| {
                    let color =
                        egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);
                    let is_selected = app.selected_track == Some(i);

                    let _header_response =
                        ui.allocate_ui(egui::vec2(HEADER_WIDTH, TRACK_HEIGHT), |ui| {
                            let header_rect = ui.max_rect();
                            // Invisible click area behind everything for track selection
                            let bg_response = ui.interact(
                                header_rect,
                                ui.id().with("track_bg").with(i),
                                egui::Sense::click(),
                            );
                            if bg_response.clicked() {
                                track_actions.push(TrackAction::Select(i));
                            }
                            if bg_response.double_clicked() {
                                track_actions.push(TrackAction::StartRename(i));
                            }
                            bg_response.context_menu(|ui| {
                                if ui.button("Rename Track").clicked() {
                                    track_actions.push(TrackAction::StartRename(i));
                                    ui.close_menu();
                                }
                                if ui.button("Duplicate Track").clicked() {
                                    track_actions.push(TrackAction::Duplicate(i));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button("Delete Track").clicked() {
                                    track_actions.push(TrackAction::Delete(i));
                                    ui.close_menu();
                                }
                            });
                            if is_selected {
                                ui.painter().rect_filled(
                                    header_rect,
                                    0.0,
                                    egui::Color32::from_rgb(45, 45, 55),
                                );
                            }
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.colored_label(color, "█");
                                    // Check if we're renaming this track
                                    if let Some((rename_idx, ref rename_buf)) = app.renaming_track {
                                        if rename_idx == i {
                                            let mut buf = rename_buf.clone();
                                            let r = ui.text_edit_singleline(&mut buf);
                                            if r.lost_focus() {
                                                track_actions.push(TrackAction::FinishRename(i, buf));
                                            } else {
                                                app.renaming_track = Some((i, buf));
                                            }
                                        } else {
                                            ui.strong(&track.name);
                                        }
                                    } else {
                                        ui.strong(&track.name);
                                    }
                                });

                                ui.horizontal(|ui| {
                                    if ui
                                        .selectable_label(track.muted, "M")
                                        .on_hover_text("Mute")
                                        .clicked()
                                    {
                                        track_actions.push(TrackAction::ToggleMute(i));
                                    }
                                    if ui
                                        .selectable_label(track.solo, "S")
                                        .on_hover_text("Solo")
                                        .clicked()
                                    {
                                        track_actions.push(TrackAction::ToggleSolo(i));
                                    }
                                    let armed = track.armed;
                                    if ui
                                        .selectable_label(
                                            armed,
                                            egui::RichText::new("R").color(if armed {
                                                egui::Color32::RED
                                            } else {
                                                ui.visuals().text_color()
                                            }),
                                        )
                                        .on_hover_text("Arm for recording")
                                        .clicked()
                                    {
                                        track_actions.push(TrackAction::ToggleArm(i));
                                    }
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Vol:");
                                    let mut vol = track.volume;
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut vol, 0.0..=1.5)
                                                .show_value(false),
                                        )
                                        .changed()
                                    {
                                        track_actions.push(TrackAction::SetVolume(i, vol));
                                    }
                                });
                            });
                        });
                    // click/double-click/context-menu handled inside allocate_ui above
                    ui.separator();
                });
            }

            // Apply track actions
            for action in track_actions {
                match action {
                    TrackAction::ToggleMute(i) => {
                        app.push_undo("Toggle mute");
                        app.project.tracks[i].muted = !app.project.tracks[i].muted;
                        app.sync_project();
                    }
                    TrackAction::ToggleSolo(i) => {
                        app.push_undo("Toggle solo");
                        app.project.tracks[i].solo = !app.project.tracks[i].solo;
                        app.sync_project();
                    }
                    TrackAction::ToggleArm(i) => {
                        app.project.tracks[i].armed = !app.project.tracks[i].armed;
                    }
                    TrackAction::SetVolume(i, v) => {
                        app.project.tracks[i].volume = v;
                        app.sync_project();
                    }
                    TrackAction::Select(i) => {
                        app.selected_track = Some(i);
                        app.selected_clip = None;
                    }
                    TrackAction::Delete(i) => {
                        if app.project.tracks.len() > 1 {
                            app.push_undo("Delete track");
                            app.project.tracks.remove(i);
                            app.selected_track =
                                Some(i.min(app.project.tracks.len() - 1));
                            app.selected_clip = None;
                            app.sync_project();
                        }
                    }
                    TrackAction::Duplicate(i) => {
                        app.push_undo("Duplicate track");
                        let mut t = app.project.tracks[i].clone();
                        t.id = uuid::Uuid::new_v4();
                        t.name = format!("{} (copy)", t.name);
                        app.project.tracks.insert(i + 1, t);
                        app.selected_track = Some(i + 1);
                        app.sync_project();
                    }
                    TrackAction::StartRename(i) => {
                        let name = app.project.tracks[i].name.clone();
                        app.renaming_track = Some((i, name));
                    }
                    TrackAction::FinishRename(i, name) => {
                        if !name.is_empty() {
                            app.push_undo("Rename track");
                            app.project.tracks[i].name = name;
                            app.sync_project();
                        }
                        app.renaming_track = None;
                    }
                }
            }

            ui.add_space(8.0);
            if ui.button("+ Add Track").clicked() {
                app.push_undo("Add track");
                let n = app.project.tracks.len() + 1;
                app.project
                    .add_track(&format!("Track {n}"), TrackKind::Audio);
                app.sync_project();
            }
        });

    // Timeline area
    egui::CentralPanel::default().show_inside(ui, |ui| {
        let available = ui.available_size();
        let rect = ui.max_rect();

        // Interactions
        let response = ui.interact(
            rect,
            ui.id().with("timeline_area"),
            egui::Sense::click_and_drag(),
        );

        // Middle mouse drag to scroll
        if response.dragged_by(egui::PointerButton::Middle) {
            app.scroll_x -= response.drag_delta().x;
            app.scroll_x = app.scroll_x.max(0.0);
        }

        // Right-click context menu
        response.context_menu(|ui| {
            if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                let tracks_y_start = rect.min.y + RULER_HEIGHT;
                // Find which clip was right-clicked
                let mut right_clicked_clip = None;
                for ti in 0..app.project.tracks.len() {
                    let y = tracks_y_start + ti as f32 * TRACK_HEIGHT;
                    for ci in 0..app.project.tracks[ti].clips.len() {
                        let clip = &app.project.tracks[ti].clips[ci];
                        let clip_x = rect.min.x
                            + (clip.start_sample as f64 / sample_rate) as f32 * pixels_per_second
                            - app.scroll_x;
                        let clip_w = (clip.duration_samples as f64 / sample_rate) as f32
                            * pixels_per_second;
                        let clip_rect = egui::Rect::from_min_size(
                            egui::pos2(clip_x, y + 2.0),
                            egui::vec2(clip_w, TRACK_HEIGHT - 4.0),
                        );
                        if clip_rect.contains(pos) {
                            right_clicked_clip = Some((ti, ci));
                        }
                    }
                }

                if let Some((ti, ci)) = right_clicked_clip {
                    let clip_name = app.project.tracks[ti].clips[ci].name.clone();
                    let is_muted = app.project.tracks[ti].clips[ci].muted;
                    ui.label(egui::RichText::new(&clip_name).strong());
                    if is_muted {
                        ui.label(egui::RichText::new("(muted take)").small().color(egui::Color32::GRAY));
                    }
                    ui.separator();

                    // Activate this take (mute all overlapping, unmute this)
                    let activate_label = if is_muted { "Activate Take" } else { "Mute Take" };
                    if ui.button(activate_label).clicked() {
                        app.push_undo("Toggle take");
                        if is_muted {
                            // Activating: mute all overlapping clips, unmute this one
                            let clip_start = app.project.tracks[ti].clips[ci].start_sample;
                            let clip_end = clip_start + app.project.tracks[ti].clips[ci].duration_samples;
                            for (j, c) in app.project.tracks[ti].clips.iter_mut().enumerate() {
                                let c_end = c.start_sample + c.duration_samples;
                                if j != ci && clip_start < c_end && clip_end > c.start_sample {
                                    c.muted = true;
                                }
                            }
                            app.project.tracks[ti].clips[ci].muted = false;
                        } else {
                            app.project.tracks[ti].clips[ci].muted = true;
                        }
                        app.sync_project();
                        ui.close_menu();
                    }
                    ui.separator();

                    if ui.button("Duplicate Clip").clicked() {
                        app.push_undo("Duplicate clip");
                        let mut new_clip = app.project.tracks[ti].clips[ci].clone();
                        new_clip.id = uuid::Uuid::new_v4();
                        new_clip.start_sample += new_clip.duration_samples;
                        new_clip.name = format!("{} (copy)", new_clip.name);
                        new_clip.muted = false;
                        app.project.tracks[ti].clips.push(new_clip);
                        app.sync_project();
                        ui.close_menu();
                    }
                    if ui.button("Delete Clip").clicked() {
                        app.push_undo("Delete clip");
                        app.project.tracks[ti].clips.remove(ci);
                        app.selected_clip = None;
                        app.sync_project();
                        ui.close_menu();
                    }
                } else {
                    // Right-clicked on empty area
                    if ui.button("Add Audio Track").clicked() {
                        app.push_undo("Add track");
                        let n = app.project.tracks.len() + 1;
                        app.project
                            .add_track(&format!("Track {n}"), TrackKind::Audio);
                        app.sync_project();
                        ui.close_menu();
                    }
                    if ui.button("Import Audio...").clicked() {
                        ui.close_menu();
                        app.open_import_dialog();
                    }
                    if ui.button("Paste at Playhead").clicked() {
                        // placeholder for future clipboard
                        ui.close_menu();
                    }
                }
            }
        });

        // Left click on empty area to set playhead and deselect clip
        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos {
                let tracks_y_start = rect.min.y + RULER_HEIGHT;

                // Check if clicked on a clip
                let mut clicked_clip = None;
                for (ti, track) in app.project.tracks.iter().enumerate() {
                    let y = tracks_y_start + ti as f32 * TRACK_HEIGHT;
                    for (ci, clip) in track.clips.iter().enumerate() {
                        let clip_x = rect.min.x
                            + (clip.start_sample as f64 / sample_rate) as f32 * pixels_per_second
                            - app.scroll_x;
                        let clip_w = (clip.duration_samples as f64 / sample_rate) as f32
                            * pixels_per_second;
                        let clip_rect = egui::Rect::from_min_size(
                            egui::pos2(clip_x, y + 2.0),
                            egui::vec2(clip_w, TRACK_HEIGHT - 4.0),
                        );
                        if clip_rect.contains(pos) {
                            clicked_clip = Some((ti, ci));
                        }
                    }
                }

                // Select the track under the cursor based on Y position
                if pos.y > tracks_y_start {
                    let track_idx = ((pos.y - tracks_y_start) / TRACK_HEIGHT) as usize;
                    if track_idx < app.project.tracks.len() {
                        app.selected_track = Some(track_idx);
                    }
                }

                if let Some((ti, ci)) = clicked_clip {
                    app.selected_clip = Some((ti, ci));
                    app.selected_track = Some(ti);
                } else {
                    // Set playhead
                    app.selected_clip = None;
                    let x_offset = pos.x - rect.min.x + app.scroll_x;
                    let seconds = x_offset as f64 / pixels_per_second as f64;
                    let sample_pos = (seconds * sample_rate) as u64;
                    let snapped = app.snap_to_beat(sample_pos);
                    app.send_command(jamhub_engine::EngineCommand::SetPosition(snapped));
                }
            }
        }

        // Clip dragging - find target first, then apply
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos {
                let tracks_y_start = rect.min.y + RULER_HEIGHT;
                let mut drag_target: Option<(usize, usize, u64)> = None;
                for ti in 0..app.project.tracks.len() {
                    let y = tracks_y_start + ti as f32 * TRACK_HEIGHT;
                    for ci in 0..app.project.tracks[ti].clips.len() {
                        let clip = &app.project.tracks[ti].clips[ci];
                        let clip_x = rect.min.x
                            + (clip.start_sample as f64 / sample_rate) as f32 * pixels_per_second
                            - app.scroll_x;
                        let clip_w = (clip.duration_samples as f64 / sample_rate) as f32
                            * pixels_per_second;
                        let clip_rect = egui::Rect::from_min_size(
                            egui::pos2(clip_x, y + 2.0),
                            egui::vec2(clip_w, TRACK_HEIGHT - 4.0),
                        );
                        if clip_rect.contains(pos) {
                            drag_target = Some((ti, ci, clip.start_sample));
                        }
                    }
                }
                if let Some((ti, ci, orig)) = drag_target {
                    app.push_undo("Move clip");
                    app.dragging_clip = Some(crate::ClipDragState {
                        track_idx: ti,
                        clip_idx: ci,
                        start_x: pos.x,
                        original_start_sample: orig,
                    });
                }
            }
        }

        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(ref drag) = app.dragging_clip {
                if let Some(pos) = response.interact_pointer_pos {
                    let dx = pos.x - drag.start_x;
                    let d_seconds = dx as f64 / pixels_per_second as f64;
                    let d_samples = (d_seconds * sample_rate) as i64;
                    let new_start =
                        (drag.original_start_sample as i64 + d_samples).max(0) as u64;
                    let snapped = app.snap_to_beat(new_start);

                    if drag.track_idx < app.project.tracks.len()
                        && drag.clip_idx < app.project.tracks[drag.track_idx].clips.len()
                    {
                        app.project.tracks[drag.track_idx].clips[drag.clip_idx]
                            .start_sample = snapped;
                    }
                }
            }
        }

        if response.drag_stopped() {
            if app.dragging_clip.is_some() {
                app.dragging_clip = None;
                app.sync_project();
            }
        }

        // Cmd+scroll to zoom, plain scroll to horizontal scroll
        ui.input(|i| {
            if i.modifiers.command {
                // Cmd + scroll wheel = zoom
                let scroll = i.smooth_scroll_delta.y;
                if scroll != 0.0 {
                    app.zoom = (app.zoom * (1.0 + scroll * 0.005)).clamp(0.1, 10.0);
                }
            } else {
                // Plain scroll = horizontal scroll
                let scroll_x = i.smooth_scroll_delta.x - i.smooth_scroll_delta.y;
                if scroll_x != 0.0 {
                    app.scroll_x = (app.scroll_x - scroll_x).max(0.0);
                }
            }
        });

        let painter = ui.painter();

        // Background
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(30, 30, 35));

        // Ruler
        let ruler_rect =
            egui::Rect::from_min_size(rect.min, egui::vec2(available.x, RULER_HEIGHT));
        painter.rect_filled(ruler_rect, 0.0, egui::Color32::from_rgb(40, 40, 48));

        // Beat/bar grid
        let bpm = app.project.tempo.bpm;
        let beats_per_bar = app.project.time_signature.numerator as f64;
        let seconds_per_beat = 60.0 / bpm;
        let pixels_per_beat = seconds_per_beat as f32 * pixels_per_second;

        let start_beat = (app.scroll_x / pixels_per_beat).floor() as i32;
        let visible_beats = (available.x / pixels_per_beat).ceil() as i32 + 2;

        for b in start_beat..(start_beat + visible_beats) {
            if b < 0 {
                continue;
            }
            let x = rect.min.x + b as f32 * pixels_per_beat - app.scroll_x;
            let is_bar = b as f64 % beats_per_bar == 0.0;

            let line_color = if is_bar {
                egui::Color32::from_rgb(80, 80, 90)
            } else {
                egui::Color32::from_rgb(50, 50, 58)
            };

            painter.line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                egui::Stroke::new(if is_bar { 1.0 } else { 0.5 }, line_color),
            );

            if is_bar {
                let bar = (b as f64 / beats_per_bar) as i32 + 1;
                let bar_time_sec = b as f64 * seconds_per_beat;
                let bar_min = (bar_time_sec / 60.0) as u32;
                let bar_sec = bar_time_sec % 60.0;
                let time_label = if bar_min > 0 {
                    format!("Bar {bar}  {bar_min}:{bar_sec:04.1}")
                } else {
                    format!("Bar {bar}  {bar_sec:.1}s")
                };
                painter.text(
                    egui::pos2(x + 4.0, rect.min.y + 4.0),
                    egui::Align2::LEFT_TOP,
                    time_label,
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(160, 160, 170),
                );
            }
        }

        // Track lanes and clips
        let tracks_y_start = rect.min.y + RULER_HEIGHT;
        for (i, track) in app.project.tracks.iter().enumerate() {
            let y = tracks_y_start + i as f32 * TRACK_HEIGHT;
            let lane_rect = egui::Rect::from_min_size(
                egui::pos2(rect.min.x, y),
                egui::vec2(available.x, TRACK_HEIGHT),
            );

            let is_selected = app.selected_track == Some(i);
            let bg = if is_selected {
                egui::Color32::from_rgb(40, 40, 50)
            } else if i % 2 == 0 {
                egui::Color32::from_rgb(35, 35, 40)
            } else {
                egui::Color32::from_rgb(30, 30, 35)
            };
            painter.rect_filled(lane_rect, 0.0, bg);

            painter.line_segment(
                [
                    egui::pos2(rect.min.x, y + TRACK_HEIGHT),
                    egui::pos2(rect.max.x, y + TRACK_HEIGHT),
                ],
                egui::Stroke::new(0.5, egui::Color32::from_rgb(50, 50, 58)),
            );

            // Muted overlay
            if track.muted {
                painter.rect_filled(
                    lane_rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(0, 0, 0, 80),
                );
            }

            let color =
                egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);

            for (ci, clip) in track.clips.iter().enumerate() {
                let clip_start_sec = clip.start_sample as f64 / sample_rate;
                let clip_dur_sec = clip.duration_samples as f64 / sample_rate;
                let clip_x =
                    rect.min.x + clip_start_sec as f32 * pixels_per_second - app.scroll_x;
                let clip_w = clip_dur_sec as f32 * pixels_per_second;

                if clip_x + clip_w < rect.min.x || clip_x > rect.max.x {
                    continue;
                }

                let clip_rect = egui::Rect::from_min_size(
                    egui::pos2(clip_x, y + 2.0),
                    egui::vec2(clip_w, TRACK_HEIGHT - 4.0),
                );

                let is_clip_selected = app.selected_clip == Some((i, ci));
                let is_clip_muted = clip.muted;

                // Clip background — muted clips are dimmed
                let draw_color = if is_clip_muted {
                    egui::Color32::from_rgb(80, 80, 80)
                } else {
                    color
                };
                let bg_alpha = if is_clip_muted {
                    0.15
                } else if is_clip_selected {
                    0.5
                } else {
                    0.3
                };
                painter.rect_filled(clip_rect, 4.0, draw_color.gamma_multiply(bg_alpha));

                // Waveform
                if let ClipSource::AudioBuffer { buffer_id } = &clip.source {
                    if let Some(peaks) = app.waveform_cache.get(buffer_id) {
                        let wave_color = if is_clip_muted {
                            egui::Color32::from_rgb(100, 100, 100)
                        } else {
                            color
                        };
                        draw_waveform(painter, &peaks, clip_rect, clip.duration_samples, wave_color);
                    }
                }

                // Border
                let border_width = if is_clip_selected { 2.0 } else { 1.0 };
                let border_color = if is_clip_selected {
                    egui::Color32::WHITE
                } else if is_clip_muted {
                    egui::Color32::from_rgb(80, 80, 80)
                } else {
                    color
                };
                painter.rect_stroke(
                    clip_rect,
                    4.0,
                    egui::Stroke::new(border_width, border_color),
                    egui::StrokeKind::Outside,
                );

                // Muted indicator
                if is_clip_muted {
                    painter.with_clip_rect(clip_rect).text(
                        egui::pos2(clip_rect.right() - 20.0, clip_rect.top() + 4.0),
                        egui::Align2::RIGHT_TOP,
                        "MUTED",
                        egui::FontId::proportional(8.0),
                        egui::Color32::from_rgb(150, 150, 150),
                    );
                }

                // Clip name
                let text_rect = clip_rect.shrink(3.0);
                painter.with_clip_rect(text_rect).text(
                    egui::pos2(clip_x + 4.0, y + 4.0),
                    egui::Align2::LEFT_TOP,
                    &clip.name,
                    egui::FontId::proportional(10.0),
                    egui::Color32::WHITE,
                );
            }
        }

        // Loop region
        if app.loop_enabled && app.loop_end > app.loop_start {
            let ls = app.loop_start as f64 / sample_rate;
            let le = app.loop_end as f64 / sample_rate;
            let lx1 = rect.min.x + ls as f32 * pixels_per_second - app.scroll_x;
            let lx2 = rect.min.x + le as f32 * pixels_per_second - app.scroll_x;
            let loop_rect = egui::Rect::from_min_max(
                egui::pos2(lx1.max(rect.min.x), rect.min.y),
                egui::pos2(lx2.min(rect.max.x), rect.max.y),
            );
            painter.rect_filled(
                loop_rect,
                0.0,
                egui::Color32::from_rgba_premultiplied(60, 100, 200, 25),
            );
            // Loop boundaries
            for lx in [lx1, lx2] {
                if lx >= rect.min.x && lx <= rect.max.x {
                    painter.line_segment(
                        [egui::pos2(lx, rect.min.y), egui::pos2(lx, rect.max.y)],
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 130, 220)),
                    );
                }
            }
        }

        // Playhead
        let pos = app.position_samples();
        let pos_sec = pos as f64 / sample_rate;
        let playhead_x = rect.min.x + pos_sec as f32 * pixels_per_second - app.scroll_x;

        if playhead_x >= rect.min.x && playhead_x <= rect.max.x {
            painter.line_segment(
                [
                    egui::pos2(playhead_x, rect.min.y),
                    egui::pos2(playhead_x, rect.max.y),
                ],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 80, 80)),
            );
            let tri_size = 6.0;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(playhead_x, ruler_rect.max.y),
                    egui::pos2(playhead_x - tri_size, ruler_rect.max.y - tri_size),
                    egui::pos2(playhead_x + tri_size, ruler_rect.max.y - tri_size),
                ],
                egui::Color32::from_rgb(255, 80, 80),
                egui::Stroke::NONE,
            ));
        }

        // Gentle auto-scroll: keep playhead visible during playback
        if app.transport_state() == jamhub_model::TransportState::Playing {
            let playhead_px = pos_sec as f32 * pixels_per_second;
            let view_left = app.scroll_x;
            let _view_right = app.scroll_x + available.x;

            // Only scroll if playhead goes past 80% of the visible area
            if playhead_px > view_left + available.x * 0.8 {
                // Smooth scroll: move view so playhead is at 20% from left
                let target = playhead_px - available.x * 0.2;
                app.scroll_x += (target - app.scroll_x) * 0.1; // lerp for smoothness
            } else if playhead_px < view_left {
                app.scroll_x = (playhead_px - available.x * 0.1).max(0.0);
            }
        }
    });
}

fn draw_waveform(
    painter: &egui::Painter,
    peaks: &jamhub_engine::waveform::WaveformPeaks,
    clip_rect: egui::Rect,
    total_samples: u64,
    color: egui::Color32,
) {
    let width = clip_rect.width();
    if width < 2.0 {
        return;
    }

    let samples_per_pixel = total_samples as f64 / width as f64;
    let peak_data = peaks.get_peaks_for_resolution(samples_per_pixel);
    let block_size = peaks.block_size_for_level(samples_per_pixel) as f64;

    let center_y = clip_rect.center().y;
    let half_height = clip_rect.height() * 0.4;

    let num_pixels = (width as usize).min(2000); // cap to avoid huge polygon
    let mut points_top: Vec<egui::Pos2> = Vec::with_capacity(num_pixels + 2);
    let mut points_bottom: Vec<egui::Pos2> = Vec::with_capacity(num_pixels + 2);

    for px in 0..num_pixels {
        let sample_start = px as f64 * samples_per_pixel;
        let sample_end = (px + 1) as f64 * samples_per_pixel;

        let peak_start = (sample_start / block_size) as usize;
        let peak_end = ((sample_end / block_size) as usize + 1).min(peak_data.len());

        if peak_start >= peak_data.len() {
            break;
        }

        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for pi in peak_start..peak_end {
            let (lo, hi) = peak_data[pi];
            if lo < min {
                min = lo;
            }
            if hi > max {
                max = hi;
            }
        }

        let x = clip_rect.min.x + px as f32;
        points_top.push(egui::pos2(x, center_y - max * half_height));
        points_bottom.push(egui::pos2(x, center_y - min * half_height));
    }

    if points_top.len() >= 2 {
        points_bottom.reverse();
        let mut polygon = points_top;
        polygon.extend(points_bottom);

        painter
            .with_clip_rect(clip_rect)
            .add(egui::Shape::convex_polygon(
                polygon,
                color.gamma_multiply(0.45),
                egui::Stroke::NONE,
            ));
    }
}

enum TrackAction {
    ToggleMute(usize),
    ToggleSolo(usize),
    ToggleArm(usize),
    SetVolume(usize, f32),
    Select(usize),
    Delete(usize),
    Duplicate(usize),
    StartRename(usize),
    FinishRename(usize, String),
}
