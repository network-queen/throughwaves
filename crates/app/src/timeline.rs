use eframe::egui;
use jamhub_model::{ClipSource, TrackKind};

use crate::DawApp;

const MIN_TRACK_HEIGHT: f32 = 40.0;
const BASE_LANE_HEIGHT: f32 = 60.0;
const TAKE_LANE_HEIGHT: f32 = 40.0;
const HEADER_WIDTH: f32 = 180.0;
const RULER_HEIGHT: f32 = 24.0;
const PIXELS_PER_SECOND_BASE: f32 = 100.0;
const RESIZE_HANDLE_PX: f32 = 8.0;

/// Compute the height of a track.
/// If user has dragged a custom height, use that.
/// Otherwise auto-compute from take lanes.
fn track_height(track: &jamhub_model::Track) -> f32 {
    if track.custom_height > 0.0 {
        return track.custom_height.max(MIN_TRACK_HEIGHT);
    }
    if !track.lanes_expanded {
        return BASE_LANE_HEIGHT;
    }
    let lanes = compute_take_lanes(track);
    let max_lane = lanes.iter().map(|&(_, l)| l).max().unwrap_or(0);
    if max_lane == 0 {
        BASE_LANE_HEIGHT
    } else {
        ((max_lane + 1) as f32 * TAKE_LANE_HEIGHT).max(BASE_LANE_HEIGHT)
    }
}

/// Compute the Y offset of each track (cumulative heights).
fn track_y_offsets(app: &DawApp) -> Vec<f32> {
    let mut offsets = Vec::with_capacity(app.project.tracks.len());
    let mut y = 0.0;
    for track in &app.project.tracks {
        offsets.push(y);
        y += track_height(track);
    }
    offsets
}

/// Assign each clip a take lane index. Overlapping clips get stacked.
/// Returns Vec<(clip_index, lane_index)>.
fn compute_take_lanes(track: &jamhub_model::Track) -> Vec<(usize, usize)> {
    let mut result: Vec<(usize, usize)> = Vec::new();
    // Track which lanes are occupied up to what sample
    let mut lane_ends: Vec<u64> = Vec::new();

    // Sort clips by start time for lane assignment
    let mut sorted_indices: Vec<usize> = (0..track.clips.len()).collect();
    sorted_indices.sort_by_key(|&i| track.clips[i].start_sample);

    for &ci in &sorted_indices {
        let clip = &track.clips[ci];
        let clip_end = clip.start_sample + clip.duration_samples;

        // Find first lane where this clip doesn't overlap
        let mut assigned_lane = None;
        for (lane_idx, end) in lane_ends.iter_mut().enumerate() {
            if clip.start_sample >= *end {
                *end = clip_end;
                assigned_lane = Some(lane_idx);
                break;
            }
        }

        let lane = if let Some(l) = assigned_lane {
            l
        } else {
            lane_ends.push(clip_end);
            lane_ends.len() - 1
        };

        result.push((ci, lane));
    }

    result
}

/// Find which track index a Y position falls on.
fn track_at_y(app: &DawApp, y: f32, tracks_y_start: f32) -> Option<usize> {
    let offsets = track_y_offsets(app);
    let rel_y = y - tracks_y_start;
    if rel_y < 0.0 {
        return None;
    }
    for (i, &offset) in offsets.iter().enumerate() {
        let h = track_height(&app.project.tracks[i]);
        if rel_y >= offset && rel_y < offset + h {
            return Some(i);
        }
    }
    None
}

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    let pixels_per_second = PIXELS_PER_SECOND_BASE * app.zoom;
    let sample_rate = app.sample_rate() as f64;

    // Pre-compute track Y offsets
    let track_offsets = track_y_offsets(app);

    // Track headers (left panel)
    egui::SidePanel::left("track_headers")
        .exact_width(HEADER_WIDTH)
        .resizable(false)
        .show_inside(ui, |ui| {
            ui.allocate_space(egui::vec2(HEADER_WIDTH, RULER_HEIGHT));
            ui.separator();

            let mut track_actions: Vec<TrackAction> = Vec::new();

            for (i, track) in app.project.tracks.iter().enumerate() {
                let h = track_height(track);
                let take_lanes = compute_take_lanes(track);
                let num_lanes = take_lanes.iter().map(|&(_, l)| l).max().unwrap_or(0) + 1;

                ui.push_id(i, |ui| {
                    let color = egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);
                    let is_selected = app.selected_track == Some(i);

                    // Allocate: header content + resize handle together
                    let total_h = h + RESIZE_HANDLE_PX;
                    ui.allocate_ui(egui::vec2(HEADER_WIDTH, total_h), |ui| {
                        let full_rect = ui.max_rect();

                        // Split into header area and resize handle area
                        let header_rect = egui::Rect::from_min_max(
                            full_rect.min,
                            egui::pos2(full_rect.max.x, full_rect.max.y - RESIZE_HANDLE_PX),
                        );
                        let handle_rect = egui::Rect::from_min_max(
                            egui::pos2(full_rect.min.x, full_rect.max.y - RESIZE_HANDLE_PX),
                            full_rect.max,
                        );

                        // Resize handle — MUST be registered first to get priority over bg
                        let handle_response = ui.interact(
                            handle_rect,
                            ui.id().with("resize").with(i),
                            egui::Sense::click_and_drag(),
                        );

                        // Header click area (excluding handle)
                        let bg_response = ui.interact(header_rect, ui.id().with("tbg").with(i), egui::Sense::click());
                        if bg_response.clicked() { track_actions.push(TrackAction::Select(i)); }
                        if bg_response.double_clicked() { track_actions.push(TrackAction::StartRename(i)); }
                        bg_response.context_menu(|ui| {
                            if ui.button("Rename").clicked() { track_actions.push(TrackAction::StartRename(i)); ui.close_menu(); }
                            if ui.button("Duplicate").clicked() { track_actions.push(TrackAction::Duplicate(i)); ui.close_menu(); }
                            if ui.button("Bounce (bake FX)").clicked() { /* handled in main */ ui.close_menu(); }
                            ui.separator();
                            if ui.button("Delete").clicked() { track_actions.push(TrackAction::Delete(i)); ui.close_menu(); }
                        });

                        // Background
                        let bg = if is_selected { egui::Color32::from_rgb(42, 42, 52) } else { egui::Color32::from_rgb(35, 35, 40) };
                        ui.painter().rect_filled(header_rect, 0.0, bg);

                        // Selected indicator — colored left border
                        if is_selected {
                            let bar = egui::Rect::from_min_size(header_rect.min, egui::vec2(3.0, header_rect.height()));
                            ui.painter().rect_filled(bar, 0.0, color);
                        }

                        ui.vertical(|ui| {
                            // Row 1: Track number + name + armed indicator
                            ui.horizontal(|ui| {
                                // Track number badge
                                ui.label(egui::RichText::new(format!("{}", i + 1)).small().color(egui::Color32::from_rgb(100, 100, 110)));
                                ui.colored_label(color, "■");

                                if let Some((rename_idx, ref rename_buf)) = app.renaming_track {
                                    if rename_idx == i {
                                        let mut buf = rename_buf.clone();
                                        let r = ui.text_edit_singleline(&mut buf);
                                        if r.lost_focus() { track_actions.push(TrackAction::FinishRename(i, buf)); }
                                        else { app.renaming_track = Some((i, buf)); }
                                    } else {
                                        ui.strong(&track.name);
                                    }
                                } else {
                                    ui.strong(&track.name);
                                }

                                if track.armed {
                                    ui.label(egui::RichText::new("REC").small().color(egui::Color32::RED));
                                }
                            });

                            // Row 2: M S R buttons + takes fold
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                let btn = egui::vec2(20.0, 16.0);

                                let m_bg = if track.muted { egui::Color32::from_rgb(180, 140, 20) } else { egui::Color32::from_rgb(50, 50, 55) };
                                if ui.add_sized(btn, egui::Button::new(egui::RichText::new("M").small().color(egui::Color32::WHITE)).fill(m_bg))
                                    .on_hover_text("Mute").clicked() { track_actions.push(TrackAction::ToggleMute(i)); }

                                let s_bg = if track.solo { egui::Color32::from_rgb(30, 130, 30) } else { egui::Color32::from_rgb(50, 50, 55) };
                                if ui.add_sized(btn, egui::Button::new(egui::RichText::new("S").small().color(egui::Color32::WHITE)).fill(s_bg))
                                    .on_hover_text("Solo").clicked() { track_actions.push(TrackAction::ToggleSolo(i)); }

                                let r_bg = if track.armed { egui::Color32::from_rgb(160, 30, 30) } else { egui::Color32::from_rgb(50, 50, 55) };
                                if ui.add_sized(btn, egui::Button::new(egui::RichText::new("R").small().color(egui::Color32::WHITE)).fill(r_bg))
                                    .on_hover_text("Arm for recording").clicked() { track_actions.push(TrackAction::ToggleArm(i)); }

                                // Takes fold/unfold — only when there are takes
                                if num_lanes > 1 {
                                    ui.add_space(4.0);
                                    let arrow = if track.lanes_expanded { "▼" } else { "▶" };
                                    let takes_text = format!("{arrow} {num_lanes}");
                                    if ui.add_sized(egui::vec2(30.0, 16.0),
                                        egui::Button::new(egui::RichText::new(takes_text).small().color(egui::Color32::from_rgb(200, 180, 100)))
                                            .fill(egui::Color32::from_rgb(55, 50, 40)))
                                        .on_hover_text(if track.lanes_expanded { "Collapse takes" } else { "Expand takes — click to see all recordings" })
                                        .clicked() {
                                        track_actions.push(TrackAction::ToggleLanes(i));
                                    }
                                }
                            });

                            // Row 3: Volume slider (compact)
                            ui.horizontal(|ui| {
                                let mut vol = track.volume;
                                if ui.add(egui::Slider::new(&mut vol, 0.0..=1.5).show_value(false)).changed() {
                                    track_actions.push(TrackAction::SetVolume(i, vol));
                                }
                                ui.label(egui::RichText::new(format!("{:.0}%", vol * 100.0)).small().color(egui::Color32::GRAY));
                            });
                        });

                        // Draw resize handle at bottom
                        let handle_color = if handle_response.hovered() || handle_response.dragged() {
                            egui::Color32::from_rgb(100, 180, 255)
                        } else {
                            egui::Color32::from_rgb(60, 60, 65)
                        };
                        ui.painter().rect_filled(handle_rect, 0.0, handle_color);

                        if handle_response.hovered() || handle_response.dragged() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                        }

                        if handle_response.dragged() {
                            let delta = handle_response.drag_delta().y;
                            let current = track_height(&app.project.tracks[i]);
                            let new_h = (current + delta).max(MIN_TRACK_HEIGHT).min(400.0);
                            track_actions.push(TrackAction::SetHeight(i, new_h));

                            if new_h > BASE_LANE_HEIGHT && !app.project.tracks[i].lanes_expanded {
                                track_actions.push(TrackAction::ToggleLanes(i));
                            }
                            if new_h <= MIN_TRACK_HEIGHT + 5.0 && app.project.tracks[i].lanes_expanded {
                                track_actions.push(TrackAction::ToggleLanes(i));
                            }
                        }

                        if handle_response.double_clicked() {
                            track_actions.push(TrackAction::SetHeight(i, 0.0));
                        }
                    });
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
                    TrackAction::ToggleLanes(i) => {
                        app.project.tracks[i].lanes_expanded =
                            !app.project.tracks[i].lanes_expanded;
                        // Reset custom height when toggling so auto-height takes over
                        app.project.tracks[i].custom_height = 0.0;
                    }
                    TrackAction::SetHeight(i, h) => {
                        app.project.tracks[i].custom_height = h;
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

        let tracks_y_start = rect.min.y + RULER_HEIGHT;

        // Pre-compute all clip rects (track_idx, clip_idx, lane, rect)
        // This avoids borrow issues in closures.
        let track_offsets = track_y_offsets(app);
        let clip_rects: Vec<(usize, usize, usize, egui::Rect)> = {
            let mut rects = Vec::new();
            for ti in 0..app.project.tracks.len() {
                let expanded = app.project.tracks[ti].lanes_expanded;
                let lanes = compute_take_lanes(&app.project.tracks[ti]);
                for (ci, lane) in lanes {
                    // When collapsed, skip muted clips and put active ones in lane 0
                    if !expanded && app.project.tracks[ti].clips[ci].muted {
                        continue;
                    }
                    let draw_lane = if expanded { lane } else { 0 };
                    let cr = make_clip_rect(
                        &app.project.tracks[ti].clips[ci],
                        draw_lane,
                        tracks_y_start + track_offsets[ti],
                        sample_rate,
                        pixels_per_second,
                        app.scroll_x,
                        rect.min.x,
                    );
                    rects.push((ti, ci, lane, cr));
                }
            }
            rects
        };

        // Right-click context menu
        let clip_rects_for_menu = clip_rects.clone();
        response.context_menu(|ui| {
            if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                let mut right_clicked_clip = None;
                for &(ti, ci, _, cr) in &clip_rects_for_menu {
                    if cr.contains(pos) {
                        right_clicked_clip = Some((ti, ci));
                    }
                }

                if let Some((ti, ci)) = right_clicked_clip {
                    let clip_name = app.project.tracks[ti].clips[ci].name.clone();
                    let is_muted = app.project.tracks[ti].clips[ci].muted;
                    ui.label(egui::RichText::new(&clip_name).strong());
                    if is_muted {
                        ui.label(
                            egui::RichText::new("(inactive take)")
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                    ui.separator();

                    let label = if is_muted {
                        "Activate This Take"
                    } else {
                        "Deactivate Take"
                    };
                    if ui.button(label).clicked() {
                        app.push_undo("Switch take");
                        if is_muted {
                            // Activate: mute overlapping, unmute this
                            let start = app.project.tracks[ti].clips[ci].start_sample;
                            let end = start
                                + app.project.tracks[ti].clips[ci].duration_samples;
                            for (j, c) in
                                app.project.tracks[ti].clips.iter_mut().enumerate()
                            {
                                let c_end = c.start_sample + c.duration_samples;
                                if j != ci && start < c_end && end > c.start_sample {
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
                }
            }
        });

        // Left click: select track, select/activate take, set playhead
        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos {
                // Select track under cursor
                if let Some(ti) = track_at_y(app, pos.y, tracks_y_start) {
                    app.selected_track = Some(ti);
                }

                // Check if clicked on a clip/take lane
                let mut clicked_clip = None;
                for &(ti, ci, _, cr) in &clip_rects {
                    if cr.contains(pos) {
                        clicked_clip = Some((ti, ci));
                    }
                }

                if let Some((ti, ci)) = clicked_clip {
                    let was_muted = app.project.tracks[ti].clips[ci].muted;
                    app.selected_clip = Some((ti, ci));
                    app.selected_track = Some(ti);

                    // Clicking a muted take activates it (Reaper behavior)
                    if was_muted {
                        app.push_undo("Activate take");
                        let start = app.project.tracks[ti].clips[ci].start_sample;
                        let end =
                            start + app.project.tracks[ti].clips[ci].duration_samples;
                        for (j, c) in
                            app.project.tracks[ti].clips.iter_mut().enumerate()
                        {
                            let c_end = c.start_sample + c.duration_samples;
                            if j != ci && start < c_end && end > c.start_sample {
                                c.muted = true;
                            }
                        }
                        app.project.tracks[ti].clips[ci].muted = false;
                        app.sync_project();
                    }
                } else {
                    // Click on empty area: set playhead
                    app.selected_clip = None;
                    let x_offset = pos.x - rect.min.x + app.scroll_x;
                    let seconds = x_offset as f64 / pixels_per_second as f64;
                    let sample_pos = (seconds * sample_rate) as u64;
                    let snapped = app.snap_to_beat(sample_pos);
                    app.send_command(jamhub_engine::EngineCommand::SetPosition(snapped));
                }
            }
        }

        // Clip dragging (horizontal + vertical for cross-track moves)
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos {
                let mut drag_target: Option<(usize, usize, u64)> = None;
                for &(ti, ci, _, cr) in &clip_rects {
                    if cr.contains(pos) {
                        drag_target =
                            Some((ti, ci, app.project.tracks[ti].clips[ci].start_sample));
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
                    // Horizontal movement (time)
                    let dx = pos.x - drag.start_x;
                    let d_seconds = dx as f64 / pixels_per_second as f64;
                    let d_samples = (d_seconds * sample_rate) as i64;
                    let new_start =
                        (drag.original_start_sample as i64 + d_samples).max(0) as u64;
                    let snapped = app.snap_to_beat(new_start);

                    if drag.track_idx < app.project.tracks.len()
                        && drag.clip_idx
                            < app.project.tracks[drag.track_idx].clips.len()
                    {
                        app.project.tracks[drag.track_idx].clips[drag.clip_idx]
                            .start_sample = snapped;

                        // Vertical movement: move clip to different track
                        if let Some(target_track) =
                            track_at_y(app, pos.y, tracks_y_start)
                        {
                            if target_track != drag.track_idx {
                                let clip = app.project.tracks[drag.track_idx]
                                    .clips
                                    .remove(drag.clip_idx);
                                app.project.tracks[target_track].clips.push(clip);
                                let new_ci =
                                    app.project.tracks[target_track].clips.len() - 1;
                                app.dragging_clip = Some(crate::ClipDragState {
                                    track_idx: target_track,
                                    clip_idx: new_ci,
                                    start_x: drag.start_x,
                                    original_start_sample: drag.original_start_sample,
                                });
                                app.selected_track = Some(target_track);
                            }
                        }
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

        // Scroll/zoom
        ui.input(|i| {
            if i.modifiers.command {
                let scroll = i.smooth_scroll_delta.y;
                if scroll != 0.0 {
                    app.zoom = (app.zoom * (1.0 + scroll * 0.005)).clamp(0.1, 10.0);
                }
            } else {
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

            painter.line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                egui::Stroke::new(
                    if is_bar { 1.0 } else { 0.5 },
                    if is_bar {
                        egui::Color32::from_rgb(80, 80, 90)
                    } else {
                        egui::Color32::from_rgb(50, 50, 58)
                    },
                ),
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

        // Track lanes with take sub-lanes
        for (i, track) in app.project.tracks.iter().enumerate() {
            let t_y = tracks_y_start + track_offsets[i];
            let t_h = track_height(track);
            let lane_rect = egui::Rect::from_min_size(
                egui::pos2(rect.min.x, t_y),
                egui::vec2(available.x, t_h),
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

            // Track separator
            painter.line_segment(
                [
                    egui::pos2(rect.min.x, t_y + t_h),
                    egui::pos2(rect.max.x, t_y + t_h),
                ],
                egui::Stroke::new(0.5, egui::Color32::from_rgb(50, 50, 58)),
            );

            // Take lane separators (only when expanded)
            let take_lanes = compute_take_lanes(track);
            let num_lanes = take_lanes.iter().map(|&(_, l)| l).max().unwrap_or(0) + 1;
            if num_lanes > 1 && track.lanes_expanded {
                for lane in 1..num_lanes {
                    let ly = t_y + lane as f32 * TAKE_LANE_HEIGHT;
                    painter.line_segment(
                        [egui::pos2(rect.min.x, ly), egui::pos2(rect.max.x, ly)],
                        egui::Stroke::new(
                            0.5,
                            egui::Color32::from_rgb(55, 50, 40),
                        ),
                    );
                }
            }

            // Muted track overlay
            if track.muted {
                painter.rect_filled(
                    lane_rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(0, 0, 0, 80),
                );
            }

            let color =
                egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);

            // Draw clips in their take lanes
            for &(ci, lane) in &take_lanes {
                let clip = &track.clips[ci];

                // When collapsed, only show active (non-muted) clips, all in lane 0
                let draw_lane = if track.lanes_expanded { lane } else { 0 };
                if !track.lanes_expanded && clip.muted {
                    continue; // hide inactive takes when collapsed
                }

                let cr = make_clip_rect(&track.clips[ci], draw_lane, tracks_y_start + track_offsets[i], sample_rate, pixels_per_second, app.scroll_x, rect.min.x);

                if cr.right() < rect.min.x || cr.left() > rect.max.x {
                    continue;
                }

                let is_clip_selected = app.selected_clip == Some((i, ci));
                let is_clip_muted = clip.muted;

                // Background
                let draw_color = if is_clip_muted {
                    egui::Color32::from_rgb(70, 70, 70)
                } else {
                    color
                };
                let bg_alpha = if is_clip_muted {
                    0.15
                } else if is_clip_selected {
                    0.5
                } else {
                    0.35
                };
                painter.rect_filled(cr, 3.0, draw_color.gamma_multiply(bg_alpha));

                // Waveform
                if let ClipSource::AudioBuffer { buffer_id } = &clip.source {
                    if let Some(peaks) = app.waveform_cache.get(buffer_id) {
                        let wc = if is_clip_muted {
                            egui::Color32::from_rgb(90, 90, 90)
                        } else {
                            color
                        };
                        draw_waveform(painter, &peaks, cr, clip.duration_samples, wc);
                    }
                }

                // Border
                let border_w = if is_clip_selected { 2.0 } else { 1.0 };
                let border_c = if is_clip_selected {
                    egui::Color32::WHITE
                } else if is_clip_muted {
                    egui::Color32::from_rgb(70, 70, 70)
                } else {
                    color
                };
                painter.rect_stroke(
                    cr,
                    3.0,
                    egui::Stroke::new(border_w, border_c),
                    egui::StrokeKind::Outside,
                );

                // Clip label
                let label = if is_clip_muted {
                    format!("{} (inactive)", clip.name)
                } else {
                    clip.name.clone()
                };
                let text_color = if is_clip_muted {
                    egui::Color32::from_rgb(130, 130, 130)
                } else {
                    egui::Color32::WHITE
                };
                painter.with_clip_rect(cr.shrink(2.0)).text(
                    egui::pos2(cr.left() + 4.0, cr.top() + 2.0),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::proportional(10.0),
                    text_color,
                );

                // Active indicator dot
                if !is_clip_muted && num_lanes > 1 {
                    painter.circle_filled(
                        egui::pos2(cr.right() - 8.0, cr.center().y),
                        3.0,
                        egui::Color32::from_rgb(80, 220, 80),
                    );
                }
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
        let playhead_x =
            rect.min.x + pos_sec as f32 * pixels_per_second - app.scroll_x;

        if playhead_x >= rect.min.x && playhead_x <= rect.max.x {
            painter.line_segment(
                [
                    egui::pos2(playhead_x, rect.min.y),
                    egui::pos2(playhead_x, rect.max.y),
                ],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 80, 80)),
            );
            let tri = 6.0;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(playhead_x, ruler_rect.max.y),
                    egui::pos2(playhead_x - tri, ruler_rect.max.y - tri),
                    egui::pos2(playhead_x + tri, ruler_rect.max.y - tri),
                ],
                egui::Color32::from_rgb(255, 80, 80),
                egui::Stroke::NONE,
            ));
        }

        // Auto-scroll
        if app.transport_state() == jamhub_model::TransportState::Playing {
            let playhead_px = pos_sec as f32 * pixels_per_second;
            let view_left = app.scroll_x;
            if playhead_px > view_left + available.x * 0.8 {
                let target = playhead_px - available.x * 0.2;
                app.scroll_x += (target - app.scroll_x) * 0.1;
            } else if playhead_px < view_left {
                app.scroll_x = (playhead_px - available.x * 0.1).max(0.0);
            }
        }
    });
}

fn make_clip_rect(
    clip: &jamhub_model::Clip,
    lane: usize,
    track_y: f32,
    sample_rate: f64,
    pixels_per_second: f32,
    scroll_x: f32,
    rect_min_x: f32,
) -> egui::Rect {
    let clip_x = rect_min_x
        + (clip.start_sample as f64 / sample_rate) as f32 * pixels_per_second
        - scroll_x;
    let clip_w = (clip.duration_samples as f64 / sample_rate) as f32 * pixels_per_second;
    let y = track_y + lane as f32 * TAKE_LANE_HEIGHT;
    let h = TAKE_LANE_HEIGHT - 2.0;
    egui::Rect::from_min_size(egui::pos2(clip_x, y + 1.0), egui::vec2(clip_w, h))
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

    let num_pixels = (width as usize).min(2000);
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
            if lo < min { min = lo; }
            if hi > max { max = hi; }
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
    ToggleLanes(usize),
    SetHeight(usize, f32),
}
