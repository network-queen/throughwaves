use eframe::egui;
use jamhub_model::{ClipSource, TrackKind};
use uuid::Uuid;

use crate::DawApp;

const BASE_LANE_HEIGHT: f32 = 48.0;
const TAKE_LANE_HEIGHT: f32 = 48.0;
const HEADER_WIDTH: f32 = 200.0;
const RULER_HEIGHT: f32 = 34.0;
const PIXELS_PER_SECOND_BASE: f32 = 100.0;
const GROUP_HEADER_HEIGHT: f32 = 24.0;
const GROUP_INDENT: f32 = 12.0;
const MINIMAP_HEIGHT: f32 = 28.0;

/// Compute the height of a track, scaled by vertical zoom.
/// If user has dragged a custom height, use that.
/// Otherwise auto-compute from take lanes.
fn track_height(track: &jamhub_model::Track, vz: f32) -> f32 {
    if track.custom_height > 0.0 {
        return (track.custom_height * vz).max(40.0);
    }
    if !track.lanes_expanded {
        return BASE_LANE_HEIGHT * vz;
    }
    let lanes = compute_take_lanes(track);
    let max_lane = lanes.iter().map(|&(_, l)| l).max().unwrap_or(0);
    if max_lane == 0 {
        BASE_LANE_HEIGHT * vz
    } else {
        ((max_lane + 1) as f32 * TAKE_LANE_HEIGHT * vz).max(BASE_LANE_HEIGHT * vz)
    }
}

/// Calculate the optimal track height based on clip content.
/// For audio tracks: comfortable waveform display (80px).
/// For MIDI tracks: fit all note content with padding.
/// If multiple take lanes are visible, account for those.
fn auto_fit_track_height(track: &jamhub_model::Track) -> f32 {
    let lanes = compute_take_lanes(track);
    let max_lane = lanes.iter().map(|&(_, l)| l).max().unwrap_or(0);

    let base = match track.kind {
        jamhub_model::TrackKind::Audio | jamhub_model::TrackKind::Bus => 80.0,
        jamhub_model::TrackKind::Midi => {
            // Fit MIDI notes: find pitch range, map to height with padding
            let mut min_note = 127u8;
            let mut max_note = 0u8;
            for clip in &track.clips {
                if let jamhub_model::ClipSource::Midi { ref notes, .. } = clip.source {
                    for note in notes {
                        min_note = min_note.min(note.pitch);
                        max_note = max_note.max(note.pitch);
                    }
                }
            }
            if max_note >= min_note {
                let note_range = (max_note - min_note) as f32 + 1.0;
                // 2px per note + 20px padding, minimum 60px
                (note_range * 2.0 + 20.0).max(60.0).min(200.0)
            } else {
                80.0 // default if no MIDI content
            }
        }
    };

    if track.lanes_expanded && max_lane > 0 {
        // Multiple take lanes: scale by lane count
        ((max_lane + 1) as f32 * TAKE_LANE_HEIGHT).max(base)
    } else {
        base
    }
}

/// Determine the ordered list of group IDs that appear before each track.
/// Returns a list of (group_id, first_track_index) for each unique group.
fn group_order(app: &DawApp) -> Vec<(Uuid, usize)> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for (i, track) in app.project.tracks.iter().enumerate() {
        if let Some(gid) = track.group_id {
            if seen.insert(gid) {
                result.push((gid, i));
            }
        }
    }
    result
}

/// Check if a track is hidden because its group is collapsed.
fn is_track_collapsed(app: &DawApp, track_idx: usize) -> bool {
    if let Some(gid) = app.project.tracks[track_idx].group_id {
        app.collapsed_groups.contains(&gid)
    } else {
        false
    }
}

/// Compute the Y offset of each track (cumulative heights),
/// accounting for group headers and collapsed groups.
/// Returns (offsets_per_track, group_header_positions).
/// group_header_positions: Vec<(group_id, y_position)>
fn track_y_offsets_with_groups(app: &DawApp) -> (Vec<f32>, Vec<(Uuid, f32)>) {
    let vz = app.track_height_zoom;
    let _groups = group_order(app);
    let mut offsets = Vec::with_capacity(app.project.tracks.len());
    let mut group_headers: Vec<(Uuid, f32)> = Vec::new();
    let mut y = 0.0;
    let mut rendered_groups = std::collections::HashSet::new();

    for (i, track) in app.project.tracks.iter().enumerate() {
        // If this track belongs to a group and we haven't rendered the group header yet
        if let Some(gid) = track.group_id {
            if rendered_groups.insert(gid) {
                group_headers.push((gid, y));
                y += GROUP_HEADER_HEIGHT;
            }
        }

        offsets.push(y);
        if !is_track_collapsed(app, i) {
            y += track_height(track, vz);
        }
    }
    (offsets, group_headers)
}

/// Compute the Y offset of each track (cumulative heights).
fn track_y_offsets(app: &DawApp) -> Vec<f32> {
    let (offsets, _) = track_y_offsets_with_groups(app);
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
        let clip_end = clip.start_sample + clip.visual_duration_samples();

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
    let vz = app.track_height_zoom;
    for (i, &offset) in offsets.iter().enumerate() {
        let h = track_height(&app.project.tracks[i], vz);
        if rel_y >= offset && rel_y < offset + h {
            return Some(i);
        }
    }
    None
}

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    let pixels_per_second = PIXELS_PER_SECOND_BASE * app.zoom;
    let sample_rate = app.sample_rate() as f64;

    // Track headers (left panel)
    egui::SidePanel::left("track_headers")
        .exact_width(HEADER_WIDTH)
        .resizable(false)
        .show_inside(ui, |ui| {
            if app.show_automation {
                // Automation parameter selector dropdown
                ui.allocate_ui(egui::vec2(HEADER_WIDTH, RULER_HEIGHT), |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.label(
                            egui::RichText::new("Auto:")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(200, 170, 60)),
                        );
                        // Build list of available params: Volume, Pan, Mute + effect params
                        let mut all_params: Vec<jamhub_model::AutomationParam> = vec![
                            jamhub_model::AutomationParam::Volume,
                            jamhub_model::AutomationParam::Pan,
                            jamhub_model::AutomationParam::Mute,
                        ];
                        // Add effect params from the selected track (or first track with effects)
                        let effect_track = app
                            .selected_track
                            .and_then(|i| app.project.tracks.get(i))
                            .or_else(|| {
                                app.project.tracks.iter().find(|t| !t.effects.is_empty())
                            });
                        if let Some(track) = effect_track {
                            for (slot_index, slot) in track.effects.iter().enumerate() {
                                if slot.effect.is_vst() {
                                    continue;
                                }
                                for param_name in slot.effect.automatable_params() {
                                    all_params.push(jamhub_model::AutomationParam::EffectParam {
                                        slot_index,
                                        param_name: param_name.to_string(),
                                    });
                                }
                            }
                        }
                        let current_name = app.automation_param.name();
                        egui::ComboBox::from_id_salt("auto_param_sel")
                            .selected_text(
                                egui::RichText::new(&current_name)
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(220, 200, 120)),
                            )
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                for p in &all_params {
                                    let label = p.name();
                                    if ui
                                        .selectable_label(
                                            *p == app.automation_param,
                                            egui::RichText::new(&label).size(10.0),
                                        )
                                        .clicked()
                                    {
                                        app.automation_param = p.clone();
                                    }
                                }
                            });
                    });
                });
            } else {
                ui.allocate_space(egui::vec2(HEADER_WIDTH, RULER_HEIGHT));
            }

            // Draw separator line manually (no extra spacing)
            let sep_rect = ui.cursor();
            ui.painter().line_segment(
                [egui::pos2(sep_rect.min.x, sep_rect.min.y), egui::pos2(sep_rect.min.x + HEADER_WIDTH, sep_rect.min.y)],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 41, 48)),
            );

            // Zero all spacing so headers align exactly with timeline tracks
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.spacing_mut().item_spacing.x = 0.0;

            let mut track_actions: Vec<TrackAction> = Vec::new();

            // Pre-collect track levels to avoid borrow conflict with app inside the loop
            let track_levels: Vec<(f32, f32)> = app.project.tracks.iter().map(|t| {
                app.levels()
                    .map(|l| l.get_track_level(&t.id))
                    .unwrap_or((0.0, 0.0))
            }).collect();

            // Track which group headers we've already drawn
            let mut rendered_group_headers = std::collections::HashSet::new();

            for (i, track) in app.project.tracks.iter().enumerate() {
                // Draw group folder header if this is the first track of a group
                if let Some(gid) = track.group_id {
                    if rendered_group_headers.insert(gid) {
                        // Find the group metadata
                        let group_meta = app.project.groups.iter().find(|g| g.id == gid);
                        let group_name = group_meta.map(|g| g.name.as_str()).unwrap_or("Group");
                        let group_color = group_meta.map(|g| g.color).unwrap_or([120, 120, 180]);
                        let gc = egui::Color32::from_rgb(group_color[0], group_color[1], group_color[2]);
                        let is_collapsed = app.collapsed_groups.contains(&gid);

                        ui.push_id(format!("grp_{}", gid), |ui| {
                            ui.allocate_ui(egui::vec2(HEADER_WIDTH, GROUP_HEADER_HEIGHT), |ui| {
                                let grp_rect = ui.max_rect();
                                let grp_response = ui.interact(grp_rect, ui.id().with("grp_hdr"), egui::Sense::click());

                                // Background
                                ui.painter().rect_filled(grp_rect, 0.0, egui::Color32::from_rgb(30, 31, 40));
                                // Color bar on left
                                let bar_rect = egui::Rect::from_min_size(grp_rect.min, egui::vec2(3.0, grp_rect.height()));
                                ui.painter().rect_filled(bar_rect, 0.0, gc);

                                // Collapse arrow + name
                                let arrow = if is_collapsed { "\u{25B6}" } else { "\u{25BC}" };

                                // Count group children for mute/solo summary
                                let child_count = app.project.tracks.iter().filter(|t| t.group_id == Some(gid)).count();
                                let all_muted = app.project.tracks.iter().filter(|t| t.group_id == Some(gid)).all(|t| t.muted);
                                let any_solo = app.project.tracks.iter().filter(|t| t.group_id == Some(gid)).any(|t| t.solo);

                                ui.horizontal(|ui| {
                                    ui.add_space(5.0);
                                    ui.label(egui::RichText::new(arrow).size(10.0).color(gc));
                                    ui.label(egui::RichText::new(group_name).size(11.0).strong().color(egui::Color32::from_rgb(200, 200, 210)));
                                    ui.label(egui::RichText::new(format!("({})", child_count)).size(9.0).color(egui::Color32::from_rgb(110, 110, 120)));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let btn = egui::vec2(20.0, 18.0);
                                        // Solo button
                                        let s_bg = if any_solo { egui::Color32::from_rgb(50, 160, 60) } else { egui::Color32::from_rgb(36, 37, 44) };
                                        let s_tc = if any_solo { egui::Color32::WHITE } else { egui::Color32::from_rgb(145, 142, 138) };
                                        if ui.add_sized(btn, egui::Button::new(egui::RichText::new("S").size(9.0).color(s_tc)).fill(s_bg).corner_radius(9.0)).clicked() {
                                            track_actions.push(TrackAction::ToggleGroupSolo(gid));
                                        }
                                        // Mute button
                                        let m_bg = if all_muted { egui::Color32::from_rgb(200, 160, 30) } else { egui::Color32::from_rgb(36, 37, 44) };
                                        let m_tc = if all_muted { egui::Color32::WHITE } else { egui::Color32::from_rgb(145, 142, 138) };
                                        if ui.add_sized(btn, egui::Button::new(egui::RichText::new("M").size(9.0).color(m_tc)).fill(m_bg).corner_radius(9.0)).clicked() {
                                            track_actions.push(TrackAction::ToggleGroupMute(gid));
                                        }
                                    });
                                });

                                // Click to toggle collapse
                                if grp_response.clicked() {
                                    track_actions.push(TrackAction::ToggleGroup(gid));
                                }

                                // Right-click context menu for group
                                grp_response.context_menu(|ui| {
                                    if ui.button("Ungroup").clicked() {
                                        track_actions.push(TrackAction::DeleteGroup(gid));
                                        ui.close_menu();
                                    }
                                });
                            });
                        });
                    }
                }

                // Skip drawing this track if its group is collapsed
                if is_track_collapsed(app, i) {
                    continue;
                }

                let h = track_height(track, app.track_height_zoom);
                let take_lanes = compute_take_lanes(track);
                let num_lanes = take_lanes.iter().map(|&(_, l)| l).max().unwrap_or(0) + 1;
                let is_grouped = track.group_id.is_some();

                ui.push_id(i, |ui| {
                    let _color = egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);
                    // Saturate/vibrantize the track color for the left stripe
                    let vibrant_color = {
                        let r = track.color[0] as f32 / 255.0;
                        let g = track.color[1] as f32 / 255.0;
                        let b = track.color[2] as f32 / 255.0;
                        let max_c = r.max(g).max(b);
                        let boost = if max_c > 0.01 { 1.0 / max_c } else { 1.0 };
                        let boost = boost.min(1.5); // don't over-boost
                        egui::Color32::from_rgb(
                            (r * boost * 255.0).min(255.0) as u8,
                            (g * boost * 255.0).min(255.0) as u8,
                            (b * boost * 255.0).min(255.0) as u8,
                        )
                    };
                    let is_selected = app.selected_track == Some(i);

                    let (header_rect, _) = ui.allocate_exact_size(egui::vec2(HEADER_WIDTH, h), egui::Sense::hover());
                    ui.allocate_ui_at_rect(header_rect, |ui| {
                        let header_rect = header_rect;

                        // Click area for entire header — only handles selection & context menu
                        let bg_response = ui.interact(header_rect, ui.id().with("tbg").with(i), egui::Sense::click());
                        if bg_response.clicked() { track_actions.push(TrackAction::Select(i)); }

                        // Track header tooltip — detailed info on hover
                        bg_response.clone().on_hover_ui(|ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                            let kind_str = match track.kind {
                                TrackKind::Audio => "Audio",
                                TrackKind::Midi => "MIDI",
                                TrackKind::Bus => "Bus",
                            };
                            ui.label(egui::RichText::new(&track.name).strong().size(13.0));
                            ui.label(format!("Type: {}", kind_str));
                            ui.label(format!("Clips: {}", track.clips.len()));
                            ui.label(format!("Effects: {}", track.effects.len()));
                            // Total track duration
                            let total_dur_samples = track.clips.iter()
                                .map(|c| c.start_sample + c.visual_duration_samples())
                                .max()
                                .unwrap_or(0);
                            let dur_sec = total_dur_samples as f64 / sample_rate;
                            let dur_min = (dur_sec / 60.0).floor() as u32;
                            let dur_remainder = dur_sec - dur_min as f64 * 60.0;
                            ui.label(format!("Duration: {}:{:05.2}", dur_min, dur_remainder));
                            // Volume and pan
                            let vol_db = if track.volume > 0.0 { 20.0 * track.volume.log10() } else { -100.0 };
                            ui.label(format!("Volume: {:.1} dB", vol_db));
                            let pan_str = if track.pan.abs() < 0.01 {
                                "Center".to_string()
                            } else if track.pan < 0.0 {
                                format!("{:.0}% L", -track.pan * 100.0)
                            } else {
                                format!("{:.0}% R", track.pan * 100.0)
                            };
                            ui.label(format!("Pan: {}", pan_str));
                        });

                        bg_response.context_menu(|ui| {
                            if ui.button("Rename").clicked() { track_actions.push(TrackAction::StartRename(i)); ui.close_menu(); }
                            if ui.button("Duplicate").clicked() { track_actions.push(TrackAction::Duplicate(i)); ui.close_menu(); }
                            ui.separator();
                            if num_lanes > 1 {
                                let label = if track.lanes_expanded { "Collapse Takes" } else { "Expand Takes" };
                                if ui.button(label).clicked() { track_actions.push(TrackAction::ToggleLanes(i)); ui.close_menu(); }
                                let has_muted = track.clips.iter().any(|c| c.muted);
                                if has_muted {
                                    if ui.button("Flatten Comp").on_hover_text("Remove all inactive takes, keep only active clips").clicked() {
                                        track_actions.push(TrackAction::FlattenComp(i));
                                        ui.close_menu();
                                    }
                                }
                                ui.separator();
                            }
                            // Group/ungroup options
                            if is_grouped {
                                if ui.button("Remove from Group").clicked() { track_actions.push(TrackAction::RemoveFromGroup(i)); ui.close_menu(); }
                            } else {
                                if ui.button("Create Group").clicked() { track_actions.push(TrackAction::CreateGroupFromTrack(i)); ui.close_menu(); }
                            }
                            ui.separator();
                            // Freeze / Unfreeze
                            if track.frozen {
                                if ui.button("Unfreeze Track").clicked() { track_actions.push(TrackAction::Unfreeze(i)); ui.close_menu(); }
                            } else if track.kind == TrackKind::Audio && !track.clips.is_empty() {
                                if ui.button("Freeze Track").clicked() { track_actions.push(TrackAction::Freeze(i)); ui.close_menu(); }
                            }
                            ui.separator();
                            // Track reordering
                            if i > 0 {
                                if ui.button("Move Up [Alt+Up]").clicked() { track_actions.push(TrackAction::MoveUp(i)); ui.close_menu(); }
                            }
                            if i + 1 < app.project.tracks.len() {
                                if ui.button("Move Down [Alt+Down]").clicked() { track_actions.push(TrackAction::MoveDown(i)); ui.close_menu(); }
                            }
                            ui.separator();
                            // Track height presets
                            ui.menu_button("Track Height", |ui| {
                                let presets: &[(f32, &str)] = &[
                                    (40.0, "Small (40px)"),
                                    (80.0, "Medium (80px)"),
                                    (120.0, "Large (120px)"),
                                    (200.0, "Extra Large (200px)"),
                                ];
                                for &(height, label) in presets {
                                    if ui.button(label).clicked() {
                                        track_actions.push(TrackAction::SetHeight(i, height));
                                        ui.close_menu();
                                    }
                                }
                                ui.separator();
                                if ui.button("Auto").clicked() {
                                    track_actions.push(TrackAction::SetHeight(i, 0.0));
                                    ui.close_menu();
                                }
                            });
                            ui.separator();
                            if ui.button("Set Color...").clicked() { track_actions.push(TrackAction::OpenColorPalette(i)); ui.close_menu(); }
                            ui.separator();
                            if ui.button("Save as Template...").clicked() { track_actions.push(TrackAction::SaveAsTemplate(i)); ui.close_menu(); }
                            if ui.button("Add from Template...").clicked() { track_actions.push(TrackAction::AddFromTemplate); ui.close_menu(); }
                            ui.separator();
                            if ui.button("Delete").clicked() { track_actions.push(TrackAction::Delete(i)); ui.close_menu(); }
                        });

                        // Background — warm charcoal with blue/purple undertones, hover brightening
                        let bg_response_hovered = bg_response.hovered();
                        let bg = if track.frozen {
                            if is_selected {
                                egui::Color32::from_rgb(26, 36, 56)
                            } else if bg_response_hovered {
                                egui::Color32::from_rgb(28, 36, 52)
                            } else {
                                egui::Color32::from_rgb(20, 28, 42)
                            }
                        } else if track.armed {
                            // Red tint for armed tracks
                            if is_selected {
                                egui::Color32::from_rgb(44, 26, 30)
                            } else if bg_response_hovered {
                                egui::Color32::from_rgb(42, 28, 32)
                            } else {
                                egui::Color32::from_rgb(34, 20, 24)
                            }
                        } else if is_selected {
                            egui::Color32::from_rgb(34, 32, 48)
                        } else if bg_response_hovered {
                            // Hover: 1px lighter background
                            egui::Color32::from_rgb(32, 33, 44)
                        } else {
                            egui::Color32::from_rgb(24, 25, 32)
                        };
                        ui.painter().rect_filled(header_rect, 0.0, bg);
                        // Hover highlight: subtle top edge glow
                        if bg_response_hovered && !is_selected {
                            ui.painter().line_segment(
                                [header_rect.left_top(), header_rect.right_top()],
                                egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(255, 255, 255, 6)),
                            );
                        }

                        // Group color bar on left for grouped tracks
                        if is_grouped {
                            if let Some(gid) = track.group_id {
                                let group_color = app.project.groups.iter()
                                    .find(|g| g.id == gid)
                                    .map(|g| egui::Color32::from_rgb(g.color[0], g.color[1], g.color[2]))
                                    .unwrap_or(egui::Color32::from_rgb(120, 120, 180));
                                let gbar = egui::Rect::from_min_size(header_rect.min, egui::vec2(GROUP_INDENT - 4.0, header_rect.height()));
                                ui.painter().rect_filled(gbar, 0.0, group_color.gamma_multiply(0.15));
                                // Vertical accent line
                                let line_x = header_rect.min.x + 1.5;
                                ui.painter().line_segment(
                                    [egui::pos2(line_x, header_rect.min.y), egui::pos2(line_x, header_rect.max.y)],
                                    egui::Stroke::new(2.0, group_color.gamma_multiply(0.6)),
                                );
                            }
                        }

                        // 5px left accent stripe with rounded top/bottom — vibrant saturated color
                        // Selected track: gold stripe with left glow; otherwise: vibrant track color
                        let bar_offset_x = if is_grouped { GROUP_INDENT } else { 0.0 };
                        let bar_w = 5.0;
                        let bar_color = if is_selected {
                            egui::Color32::from_rgb(240, 192, 64) // gold for selected
                        } else {
                            vibrant_color
                        };
                        let bar_rect = egui::Rect::from_min_size(
                            egui::pos2(header_rect.min.x + bar_offset_x, header_rect.min.y + 2.0),
                            egui::vec2(bar_w, header_rect.height() - 4.0),
                        );
                        ui.painter().rect_filled(bar_rect, 3.0, bar_color);
                        // Selected track: subtle left glow
                        if is_selected {
                            let glow_rect = egui::Rect::from_min_size(
                                egui::pos2(header_rect.min.x + bar_offset_x, header_rect.min.y),
                                egui::vec2(12.0, header_rect.height()),
                            );
                            ui.painter().rect_filled(glow_rect, 0.0, egui::Color32::from_rgba_premultiplied(240, 192, 64, 12));
                        }

                        ui.add_space(if is_grouped { GROUP_INDENT + bar_w + 4.0 } else { bar_w + 4.0 });
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;

                            // Row 1: Track number + type badge + name
                            ui.horizontal(|ui| {
                                // Track number — small dim at top-left
                                let num_text = egui::RichText::new(format!("{}", i + 1))
                                    .size(9.0)
                                    .color(egui::Color32::from_rgb(80, 78, 74));
                                ui.label(num_text);

                                // Track type indicator — pill-shaped badge
                                let type_label = match track.kind {
                                    TrackKind::Audio => "AUD",
                                    TrackKind::Midi => "MIDI",
                                    TrackKind::Bus => "BUS",
                                };
                                let type_color = match track.kind {
                                    TrackKind::Audio => egui::Color32::from_rgb(80, 200, 190),
                                    TrackKind::Midi => egui::Color32::from_rgb(160, 128, 224),
                                    TrackKind::Bus => egui::Color32::from_rgb(240, 192, 64),
                                };
                                let type_bg = match track.kind {
                                    TrackKind::Audio => egui::Color32::from_rgb(30, 50, 48),
                                    TrackKind::Midi => egui::Color32::from_rgb(38, 30, 52),
                                    TrackKind::Bus => egui::Color32::from_rgb(44, 38, 24),
                                };
                                let badge_text = egui::RichText::new(type_label)
                                    .size(8.0)
                                    .color(type_color);
                                ui.add_sized(
                                    egui::vec2(30.0, 14.0),
                                    egui::Button::new(badge_text).fill(type_bg).corner_radius(7.0).sense(egui::Sense::hover()),
                                );

                                // Pulsing red circle when track is armed for recording
                                if track.armed {
                                    let bpm = app.project.tempo.bpm as f64;
                                    let beat_period = if bpm > 0.0 { 60.0 / bpm } else { 1.0 };
                                    let time = ui.input(|i| i.time);
                                    let phase = (time % beat_period) / beat_period;
                                    // Smooth pulse: use sine wave, range 0.4 to 1.0
                                    let pulse = 0.4 + 0.6 * (1.0 - (phase * std::f64::consts::TAU).cos()) as f32 / 2.0;
                                    let alpha = (pulse * 255.0) as u8;
                                    let circle_size = 6.0 + pulse * 2.0;
                                    let (circle_rect, _) = ui.allocate_exact_size(egui::vec2(circle_size + 2.0, 14.0), egui::Sense::hover());
                                    let center = circle_rect.center();
                                    ui.painter().circle_filled(center, circle_size / 2.0, egui::Color32::from_rgba_premultiplied(232, 60, 60, alpha));
                                    ui.ctx().request_repaint(); // keep animating
                                }

                                // Name area — double-clickable for rename
                                if let Some((rename_idx, ref rename_buf)) = app.renaming_track {
                                    if rename_idx == i {
                                        let mut buf = rename_buf.clone();
                                        let r = ui.text_edit_singleline(&mut buf);
                                        if r.lost_focus() { track_actions.push(TrackAction::FinishRename(i, buf)); }
                                        else { app.renaming_track = Some((i, buf)); }
                                    } else {
                                        // Clickable name label
                                        let name_resp = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&track.name)
                                                    .size(14.0)
                                                    .strong()
                                                    .color(egui::Color32::from_rgb(242, 240, 236))
                                            ).sense(egui::Sense::click())
                                        );
                                        // Underline on hover
                                        if name_resp.hovered() {
                                            let name_rect = name_resp.rect;
                                            ui.painter().line_segment(
                                                [egui::pos2(name_rect.min.x, name_rect.max.y), egui::pos2(name_rect.max.x, name_rect.max.y)],
                                                egui::Stroke::new(1.0, egui::Color32::from_rgb(240, 238, 232).gamma_multiply(0.4)),
                                            );
                                        }
                                        if name_resp.double_clicked() {
                                            track_actions.push(TrackAction::StartRename(i));
                                        }
                                    }
                                } else {
                                    // Clickable name label
                                    let name_resp = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&track.name)
                                                .size(13.0)
                                                .strong()
                                                .color(egui::Color32::from_rgb(240, 238, 232))
                                        ).sense(egui::Sense::click())
                                    );
                                    // Underline on hover
                                    if name_resp.hovered() {
                                        let name_rect = name_resp.rect;
                                        ui.painter().line_segment(
                                            [egui::pos2(name_rect.min.x, name_rect.max.y), egui::pos2(name_rect.max.x, name_rect.max.y)],
                                            egui::Stroke::new(1.0, egui::Color32::from_rgb(240, 238, 232).gamma_multiply(0.4)),
                                        );
                                    }
                                    if name_resp.double_clicked() {
                                        track_actions.push(TrackAction::StartRename(i));
                                    }
                                }

                                if track.frozen {
                                    ui.label(egui::RichText::new("\u{2744}").size(10.0).color(egui::Color32::from_rgb(100, 180, 255)))
                                        .on_hover_text("Frozen \u{2014} effects baked offline");
                                }
                                // Takes badge — shows count when track has overlapping clips
                                let max_takes = track.max_take_count();
                                if max_takes > 1 {
                                    let takes_bg = egui::Color32::from_rgb(52, 44, 24);
                                    let takes_text = egui::RichText::new(format!("{}", max_takes))
                                        .size(8.5)
                                        .color(egui::Color32::from_rgb(220, 190, 70));
                                    ui.add_sized(
                                        egui::vec2(18.0, 14.0),
                                        egui::Button::new(takes_text).fill(takes_bg).corner_radius(7.0).sense(egui::Sense::hover()),
                                    ).on_hover_text(format!("{} overlapping takes on this track", max_takes));
                                }
                            });

                            // Row 2: M/S/R/FX buttons — 24px circles with gradient fills
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let circ_size = egui::vec2(24.0, 24.0);
                                let btn_text_size = 10.0;

                                // Mute — warm amber gradient when active
                                let m_active = track.muted;
                                let m_bg = if m_active { egui::Color32::from_rgb(230, 175, 50) } else { egui::Color32::from_rgb(34, 35, 44) };
                                let m_tc = if m_active { egui::Color32::from_rgb(30, 22, 8) } else { egui::Color32::from_rgb(128, 126, 135) };
                                let m_resp = ui.add_sized(circ_size, egui::Button::new(
                                    egui::RichText::new("M").size(btn_text_size).strong().color(m_tc)
                                ).fill(m_bg).corner_radius(12.0));
                                if m_resp.hovered() && !m_active {
                                    ui.painter().circle_stroke(m_resp.rect.center(), 11.5, egui::Stroke::new(1.0, egui::Color32::from_rgb(230, 175, 50).gamma_multiply(0.45)));
                                }
                                if m_active {
                                    // Amber glow gradient overlay
                                    ui.painter().circle_filled(m_resp.rect.center(), 13.5, egui::Color32::from_rgba_premultiplied(230, 175, 50, 18));
                                }
                                if m_resp.on_hover_text("Mute \u{2014} silence this track").clicked() { track_actions.push(TrackAction::ToggleMute(i)); }

                                // Solo — emerald green gradient when active
                                let s_active = track.solo;
                                let s_bg = if s_active { egui::Color32::from_rgb(50, 190, 90) } else { egui::Color32::from_rgb(34, 35, 44) };
                                let s_tc = if s_active { egui::Color32::from_rgb(8, 28, 12) } else { egui::Color32::from_rgb(128, 126, 135) };
                                let s_resp = ui.add_sized(circ_size, egui::Button::new(
                                    egui::RichText::new("S").size(btn_text_size).strong().color(s_tc)
                                ).fill(s_bg).corner_radius(12.0));
                                if s_resp.hovered() && !s_active {
                                    ui.painter().circle_stroke(s_resp.rect.center(), 11.5, egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 190, 90).gamma_multiply(0.45)));
                                }
                                if s_active {
                                    ui.painter().circle_filled(s_resp.rect.center(), 13.5, egui::Color32::from_rgba_premultiplied(50, 190, 90, 18));
                                }
                                if s_resp.on_hover_text("Solo \u{2014} hear only this track\nCtrl+click for exclusive solo").clicked() {
                                    let modifiers = ui.input(|i| i.modifiers);
                                    if modifiers.ctrl {
                                        track_actions.push(TrackAction::ToggleSoloExclusive(i));
                                    } else {
                                        track_actions.push(TrackAction::ToggleSolo(i));
                                    }
                                }

                                // Record arm — ruby red gradient with glow
                                let r_active = track.armed;
                                let r_bg = if r_active { egui::Color32::from_rgb(210, 55, 65) } else { egui::Color32::from_rgb(34, 35, 44) };
                                let r_tc = if r_active { egui::Color32::WHITE } else { egui::Color32::from_rgb(128, 126, 135) };
                                let r_resp = ui.add_sized(circ_size, egui::Button::new(
                                    egui::RichText::new("R").size(btn_text_size).strong().color(r_tc)
                                ).fill(r_bg).corner_radius(12.0));
                                if r_resp.hovered() && !r_active {
                                    ui.painter().circle_stroke(r_resp.rect.center(), 11.5, egui::Stroke::new(1.0, egui::Color32::from_rgb(210, 55, 65).gamma_multiply(0.45)));
                                }
                                if r_active {
                                    // Ruby red glow behind armed button
                                    ui.painter().circle_filled(r_resp.rect.center(), 14.0, egui::Color32::from_rgba_premultiplied(210, 55, 65, 22));
                                    ui.painter().circle_filled(r_resp.rect.center(), 16.0, egui::Color32::from_rgba_premultiplied(210, 55, 65, 10));
                                }
                                if r_resp.on_hover_text("Arm for recording [R to record]").clicked() { track_actions.push(TrackAction::ToggleArm(i)); }

                                // FX — purple when active, with count badge
                                let fx_count = track.effects.len();
                                let fx_active = fx_count > 0;
                                let fx_bg = if fx_active { egui::Color32::from_rgb(160, 128, 224) } else { egui::Color32::from_rgb(38, 39, 46) };
                                let fx_tc = if fx_active { egui::Color32::WHITE } else { egui::Color32::from_rgb(140, 138, 132) };
                                let fx_label = if fx_count > 0 { format!("FX") } else { "FX".into() };
                                let fx_resp = ui.add_sized(egui::vec2(28.0, 22.0), egui::Button::new(
                                    egui::RichText::new(&fx_label).size(btn_text_size).strong().color(fx_tc)
                                ).fill(fx_bg).corner_radius(11.0));
                                if fx_resp.hovered() && !fx_active {
                                    ui.painter().circle_stroke(fx_resp.rect.center(), 10.5, egui::Stroke::new(1.0, egui::Color32::from_rgb(160, 128, 224).gamma_multiply(0.4)));
                                }
                                // FX count badge — small circle with number overlaid at top-right
                                if fx_count > 0 {
                                    let badge_center = egui::pos2(fx_resp.rect.right() - 2.0, fx_resp.rect.top() + 2.0);
                                    ui.painter().circle_filled(badge_center, 6.0, egui::Color32::from_rgb(100, 60, 160));
                                    ui.painter().text(
                                        badge_center, egui::Align2::CENTER_CENTER,
                                        format!("{fx_count}"),
                                        egui::FontId::proportional(7.5),
                                        egui::Color32::WHITE,
                                    );
                                }
                                if fx_resp.on_hover_text("Effects chain [Cmd+E]").clicked() {
                                    track_actions.push(TrackAction::Select(i));
                                    track_actions.push(TrackAction::OpenFx);
                                }

                                if num_lanes > 1 {
                                    ui.add_space(2.0);
                                    let arrow = if track.lanes_expanded { "\u{25BC}" } else { "\u{25B6}" };
                                    let lanes_active = track.lanes_expanded;
                                    let lanes_bg = if lanes_active { egui::Color32::from_rgb(180, 150, 50) } else { egui::Color32::from_rgb(38, 39, 46) };
                                    let lanes_tc = if lanes_active { egui::Color32::from_rgb(30, 25, 10) } else { egui::Color32::from_rgb(140, 138, 132) };
                                    if ui.add_sized(egui::vec2(30.0, 22.0), egui::Button::new(
                                        egui::RichText::new(format!("{arrow}{num_lanes}")).size(9.0).color(lanes_tc)
                                    ).fill(lanes_bg).corner_radius(11.0))
                                        .on_hover_text("Toggle take lanes [T]").clicked() {
                                        track_actions.push(TrackAction::ToggleLanes(i));
                                    }
                                }

                                // Volume & Pan rotary knobs — right-aligned, Reaper-style
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    let knob_r = 9.0;
                                    let knob_size = egui::vec2(knob_r * 2.0 + 2.0, knob_r * 2.0 + 2.0);

                                    // Pan knob
                                    let mut pan = track.pan;
                                    let pan_id = ui.id().with("pan_knob").with(i);
                                    let (pan_rect, pan_resp) = ui.allocate_exact_size(knob_size, egui::Sense::click_and_drag());
                                    if pan_resp.dragged() {
                                        pan = (pan - pan_resp.drag_delta().y * 0.008).clamp(-1.0, 1.0);
                                    }
                                    if pan_resp.double_clicked() { pan = 0.0; }
                                    if pan != track.pan { track_actions.push(TrackAction::SetPan(i, pan)); }
                                    // Draw pan knob
                                    draw_rotary_knob(ui.painter(), pan_rect.center(), knob_r,
                                        (pan + 1.0) / 2.0, // normalize -1..1 to 0..1
                                        egui::Color32::from_rgb(80, 180, 220),
                                        pan_resp.hovered(),
                                    );
                                    let pan_tip = if pan < -0.01 { format!("Pan: {:.0}% L", pan.abs() * 100.0) }
                                        else if pan > 0.01 { format!("Pan: {:.0}% R", pan * 100.0) }
                                        else { "Pan: Center".into() };
                                    pan_resp.on_hover_text(format!("{pan_tip}\nDrag up/down, double-click to center"));

                                    // Volume knob — works in dB space: -40 to +40, symmetric
                                    let vol = track.volume;
                                    let vol_db = if vol > 0.0001 { 20.0 * vol.log10() } else { -40.0 };
                                    let mut vol_db_edit = vol_db.clamp(-40.0, 40.0);
                                    let (vol_rect, vol_resp) = ui.allocate_exact_size(knob_size, egui::Sense::click_and_drag());
                                    if vol_resp.dragged() {
                                        vol_db_edit = (vol_db_edit - vol_resp.drag_delta().y * 0.4).clamp(-40.0, 40.0);
                                        let new_vol = 10.0_f32.powf(vol_db_edit / 20.0);
                                        track_actions.push(TrackAction::SetVolume(i, new_vol));
                                    }
                                    if vol_resp.double_clicked() {
                                        track_actions.push(TrackAction::SetVolume(i, 1.0)); // 0 dB
                                    }
                                    // Draw volume knob: map -40..+40 dB to 0..1
                                    let knob_norm = ((vol_db_edit + 40.0) / 80.0).clamp(0.0, 1.0);
                                    draw_rotary_knob(ui.painter(), vol_rect.center(), knob_r,
                                        knob_norm,
                                        egui::Color32::from_rgb(80, 210, 140),
                                        vol_resp.hovered(),
                                    );
                                    let vol_db_str = if vol > 0.0001 { format!("{:.1}", vol_db_edit) } else { "-\u{221E}".into() };
                                    // Effective dB = track volume dB + max clip gain dB
                                    let max_clip_gain = track.clips.iter()
                                        .map(|c| c.gain_db)
                                        .fold(0.0_f32, f32::max);
                                    let effective_db = vol_db_edit + max_clip_gain;
                                    let eff_str = if vol > 0.0001 {
                                        if max_clip_gain.abs() > 0.01 {
                                            format!("Track: {vol_db_str} dB + Clip: {max_clip_gain:+.1} dB = {effective_db:.1} dB")
                                        } else {
                                            format!("Volume: {vol_db_str} dB")
                                        }
                                    } else {
                                        "Volume: -\u{221E} dB".into()
                                    };
                                    vol_resp.on_hover_text(format!("{eff_str}\nRange: -40 to +40 dB\nDrag up/down, double-click for 0 dB"));
                                    // Small dB label below knob
                                    let label_str = if max_clip_gain.abs() > 0.01 && vol > 0.0001 {
                                        format!("{:.1}", effective_db)
                                    } else {
                                        vol_db_str
                                    };
                                    ui.painter().text(
                                        egui::pos2(vol_rect.center().x, vol_rect.max.y + 1.0),
                                        egui::Align2::CENTER_TOP,
                                        &label_str,
                                        egui::FontId::proportional(7.0),
                                        egui::Color32::from_rgb(100, 100, 110),
                                    );
                                });
                            });

                            // Tiny horizontal level meter (2px tall)
                            {
                                let (left, right) = track_levels[i];
                                let peak = left.max(right).clamp(0.0, 1.5);
                                let meter_width = HEADER_WIDTH - 16.0;
                                let (_, meter_rect) = ui.allocate_space(egui::vec2(meter_width, 2.0));
                                // Background
                                ui.painter().rect_filled(meter_rect, 1.0, egui::Color32::from_rgb(20, 20, 26));
                                // Filled portion with green-yellow-red gradient
                                if peak > 0.001 {
                                    let fill_frac = (peak / 1.5).min(1.0);
                                    let fill_w = meter_width * fill_frac;
                                    let fill_rect = egui::Rect::from_min_size(
                                        meter_rect.min,
                                        egui::vec2(fill_w, 2.0),
                                    );
                                    let color = if peak > 1.0 {
                                        egui::Color32::from_rgb(255, 60, 60) // red (clipping)
                                    } else if peak > 0.7 {
                                        egui::Color32::from_rgb(255, 200, 40) // yellow
                                    } else {
                                        egui::Color32::from_rgb(80, 200, 80) // green
                                    };
                                    ui.painter().rect_filled(fill_rect, 1.0, color);
                                }
                            }
                        });
                    });

                    // Thin separator line at bottom of track header (painted, not allocated)
                    ui.painter().line_segment(
                        [egui::pos2(header_rect.left(), header_rect.bottom() - 0.5),
                         egui::pos2(header_rect.right(), header_rect.bottom() - 0.5)],
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 43, 50)),
                    );
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
                    TrackAction::ToggleSoloExclusive(i) => {
                        app.push_undo("Exclusive solo");
                        let was_solo = app.project.tracks[i].solo;
                        // Un-solo all tracks first
                        for track in app.project.tracks.iter_mut() {
                            track.solo = false;
                        }
                        // If this track was already the only soloed one, leave all un-soloed
                        // Otherwise, solo just this track
                        if !was_solo {
                            app.project.tracks[i].solo = true;
                        }
                        app.sync_project();
                    }
                    TrackAction::ToggleArm(i) => {
                        app.push_undo("Toggle arm");
                        app.project.tracks[i].armed = !app.project.tracks[i].armed;
                        app.sync_project();
                    }
                    TrackAction::SetVolume(i, v) => {
                        app.push_undo("Change volume");
                        app.project.tracks[i].volume = v;
                        app.sync_project();
                    }
                    TrackAction::SetPan(i, v) => {
                        app.push_undo("Change pan");
                        app.project.tracks[i].pan = v;
                        app.sync_project();
                    }
                    TrackAction::Select(i) => {
                        app.selected_track = Some(i);
                        app.selected_clips.clear();
                    }
                    TrackAction::Delete(i) => {
                        if app.project.tracks.len() > 1 {
                            app.push_undo("Delete track");
                            app.project.tracks.remove(i);
                            app.selected_track =
                                Some(i.min(app.project.tracks.len() - 1));
                            app.selected_clips.clear();
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
                    TrackAction::OpenFx => {
                        app.show_effects = true;
                    }
                    TrackAction::ToggleLanes(i) => {
                        app.project.tracks[i].lanes_expanded =
                            !app.project.tracks[i].lanes_expanded;
                        app.project.tracks[i].custom_height = 0.0;
                    }
                    TrackAction::CreateGroupFromTrack(i) => {
                        app.push_undo("Create group");
                        let group_id = Uuid::new_v4();
                        let track_color = app.project.tracks[i].color;
                        app.project.groups.push(jamhub_model::TrackGroup {
                            id: group_id,
                            name: format!("Group {}", app.project.groups.len() + 1),
                            color: track_color,
                            collapsed: false,
                        });
                        app.project.tracks[i].group_id = Some(group_id);
                        // Also add the next track to the group if it exists
                        if i + 1 < app.project.tracks.len() && app.project.tracks[i + 1].group_id.is_none() {
                            app.project.tracks[i + 1].group_id = Some(group_id);
                        }
                        app.sync_project();
                    }
                    TrackAction::RemoveFromGroup(i) => {
                        app.push_undo("Remove from group");
                        let old_gid = app.project.tracks[i].group_id;
                        app.project.tracks[i].group_id = None;
                        // If no more tracks in this group, remove the group
                        if let Some(gid) = old_gid {
                            let remaining = app.project.tracks.iter().filter(|t| t.group_id == Some(gid)).count();
                            if remaining == 0 {
                                app.project.groups.retain(|g| g.id != gid);
                                app.collapsed_groups.remove(&gid);
                            }
                        }
                        app.sync_project();
                    }
                    TrackAction::ToggleGroup(gid) => {
                        if app.collapsed_groups.contains(&gid) {
                            app.collapsed_groups.remove(&gid);
                        } else {
                            app.collapsed_groups.insert(gid);
                        }
                    }
                    TrackAction::ToggleGroupMute(gid) => {
                        app.push_undo("Toggle group mute");
                        let all_muted = app.project.tracks.iter()
                            .filter(|t| t.group_id == Some(gid))
                            .all(|t| t.muted);
                        let new_state = !all_muted;
                        for track in app.project.tracks.iter_mut() {
                            if track.group_id == Some(gid) {
                                track.muted = new_state;
                            }
                        }
                        app.sync_project();
                    }
                    TrackAction::ToggleGroupSolo(gid) => {
                        app.push_undo("Toggle group solo");
                        let any_solo = app.project.tracks.iter()
                            .filter(|t| t.group_id == Some(gid))
                            .any(|t| t.solo);
                        let new_state = !any_solo;
                        for track in app.project.tracks.iter_mut() {
                            if track.group_id == Some(gid) {
                                track.solo = new_state;
                            }
                        }
                        app.sync_project();
                    }
                    TrackAction::DeleteGroup(gid) => {
                        app.push_undo("Ungroup");
                        for track in app.project.tracks.iter_mut() {
                            if track.group_id == Some(gid) {
                                track.group_id = None;
                            }
                        }
                        app.project.groups.retain(|g| g.id != gid);
                        app.collapsed_groups.remove(&gid);
                        app.sync_project();
                    }
                    TrackAction::MoveUp(i) => {
                        if i > 0 && i < app.project.tracks.len() {
                            app.push_undo("Move track up");
                            app.project.tracks.swap(i, i - 1);
                            app.selected_track = Some(i - 1);
                            app.selected_clips.clear();
                            app.sync_project();
                        }
                    }
                    TrackAction::MoveDown(i) => {
                        if i + 1 < app.project.tracks.len() {
                            app.push_undo("Move track down");
                            app.project.tracks.swap(i, i + 1);
                            app.selected_track = Some(i + 1);
                            app.selected_clips.clear();
                            app.sync_project();
                        }
                    }
                    TrackAction::Freeze(i) => {
                        app.selected_track = Some(i);
                        app.freeze_selected_track();
                    }
                    TrackAction::Unfreeze(i) => {
                        app.selected_track = Some(i);
                        app.unfreeze_selected_track();
                    }
                    TrackAction::FlattenComp(i) => {
                        app.flatten_comp(i);
                    }
                    TrackAction::SetHeight(i, height) => {
                        if i < app.project.tracks.len() {
                            app.project.tracks[i].custom_height = height;
                            let label = if height == 0.0 { "Auto".to_string() } else { format!("{}px", height as u32) };
                            app.set_status(&format!("Track height: {}", label));
                        }
                    }
                    TrackAction::SaveAsTemplate(i) => {
                        let name = app.project.tracks[i].name.clone();
                        app.template_name_input = Some(crate::templates::TemplateNameInput {
                            name,
                            track_idx: i,
                        });
                    }
                    TrackAction::AddFromTemplate => {
                        app.show_track_template_picker = true;
                    }
                    TrackAction::OpenColorPalette(i) => {
                        app.color_palette_track = Some(i);
                    }
                }
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let add_btn_size = egui::vec2(70.0, 24.0);
                if ui.add_sized(add_btn_size, egui::Button::new(
                    egui::RichText::new("+ Audio").size(11.0).color(egui::Color32::from_rgb(80, 200, 190))
                ).fill(egui::Color32::from_rgb(28, 42, 40)).corner_radius(12.0))
                    .on_hover_text("Add a new audio track").clicked() {
                    app.push_undo("Add track");
                    let n = app.project.tracks.len() + 1;
                    app.project
                        .add_track(&format!("Track {n}"), TrackKind::Audio);
                    app.selected_track = Some(app.project.tracks.len() - 1);
                    app.sync_project();
                }
                if ui.add_sized(add_btn_size, egui::Button::new(
                    egui::RichText::new("+ MIDI").size(11.0).color(egui::Color32::from_rgb(160, 128, 224))
                ).fill(egui::Color32::from_rgb(36, 28, 48)).corner_radius(12.0))
                    .on_hover_text("Add a new MIDI track").clicked() {
                    app.push_undo("Add track");
                    let n = app.project.tracks.len() + 1;
                    app.project
                        .add_track(&format!("MIDI {n}"), TrackKind::Midi);
                    app.selected_track = Some(app.project.tracks.len() - 1);
                    app.sync_project();
                }
            });

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
                // Skip collapsed group tracks
                if is_track_collapsed(app, ti) {
                    continue;
                }
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
                                + app.project.tracks[ti].clips[ci].visual_duration_samples();
                            for (j, c) in
                                app.project.tracks[ti].clips.iter_mut().enumerate()
                            {
                                let c_end = c.start_sample + c.visual_duration_samples();
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
                    // Flatten Comp option — visible when there are muted takes on this track
                    let has_muted_takes = app.project.tracks[ti].clips.iter().any(|c| c.muted);
                    if has_muted_takes {
                        if ui.button("Flatten Comp").on_hover_text("Remove all inactive takes, keep only active clips").clicked() {
                            app.flatten_comp(ti);
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button("Duplicate Clip").clicked() {
                        app.push_undo("Duplicate clip");
                        let mut new_clip = app.project.tracks[ti].clips[ci].clone();
                        new_clip.id = uuid::Uuid::new_v4();
                        new_clip.start_sample += new_clip.visual_duration_samples();
                        new_clip.name = format!("{} (copy)", new_clip.name);
                        new_clip.muted = false;
                        app.project.tracks[ti].clips.push(new_clip);
                        app.sync_project();
                        ui.close_menu();
                    }
                    let has_effects = !app.project.tracks[ti].effects.is_empty();
                    if ui.add_enabled(has_effects, egui::Button::new("Bounce in Place"))
                        .on_hover_text("Render clip through track effects into a new audio buffer")
                        .on_disabled_hover_text("No effects on this track")
                        .clicked()
                    {
                        app.bounce_clip_in_place(ti, ci);
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.menu_button("Process", |ui| {
                        if ui.button("Normalize").on_hover_text("Normalize peak to 0dB").clicked() {
                            app.selected_clips = std::collections::HashSet::from([(ti, ci)]);
                            app.normalize_clip();
                            ui.close_menu();
                        }
                        if ui.button("Reverse").on_hover_text("Reverse audio").clicked() {
                            app.selected_clips = std::collections::HashSet::from([(ti, ci)]);
                            app.reverse_clip();
                            ui.close_menu();
                        }
                        if ui.button("Fade In").on_hover_text("100ms smooth fade in").clicked() {
                            app.selected_clips = std::collections::HashSet::from([(ti, ci)]);
                            app.fade_in_clip();
                            ui.close_menu();
                        }
                        if ui.button("Fade Out").on_hover_text("100ms smooth fade out").clicked() {
                            app.selected_clips = std::collections::HashSet::from([(ti, ci)]);
                            app.fade_out_clip();
                            ui.close_menu();
                        }
                        ui.menu_button("Fade Curve", |ui| {
                            let clip = &app.project.tracks[ti].clips[ci];
                            let cur_in = clip.fade_in_curve;
                            let cur_out = clip.fade_out_curve;
                            ui.label("Fade In Curve:");
                            for curve in jamhub_model::FadeCurve::ALL {
                                let label = if curve == cur_in { format!("* {}", curve.name()) } else { curve.name().to_string() };
                                if ui.button(label).clicked() {
                                    app.project.tracks[ti].clips[ci].fade_in_curve = curve;
                                    ui.close_menu();
                                }
                            }
                            ui.separator();
                            ui.label("Fade Out Curve:");
                            for curve in jamhub_model::FadeCurve::ALL {
                                let label = if curve == cur_out { format!("* {}", curve.name()) } else { curve.name().to_string() };
                                if ui.button(label).clicked() {
                                    app.project.tracks[ti].clips[ci].fade_out_curve = curve;
                                    ui.close_menu();
                                }
                            }
                        });
                        if ui.button("Invert Phase").on_hover_text("Flip polarity (phase invert)").clicked() {
                            app.selected_clips = std::collections::HashSet::from([(ti, ci)]);
                            app.invert_clip();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Gain +3dB").on_hover_text("Boost level by 3dB").clicked() {
                            app.selected_clips = std::collections::HashSet::from([(ti, ci)]);
                            app.gain_up_clip();
                            ui.close_menu();
                        }
                        if ui.button("Gain -3dB").on_hover_text("Reduce level by 3dB").clicked() {
                            app.selected_clips = std::collections::HashSet::from([(ti, ci)]);
                            app.gain_down_clip();
                            ui.close_menu();
                        }
                        if ui.button("Silence").on_hover_text("Zero out all audio").clicked() {
                            app.selected_clips = std::collections::HashSet::from([(ti, ci)]);
                            app.silence_clip();
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    // Clip color picker
                    ui.menu_button("Set Color", |ui| {
                        let colors: &[([u8; 3], &str)] = &[
                            ([220, 80, 80], "Red"),
                            ([230, 150, 50], "Orange"),
                            ([220, 200, 60], "Yellow"),
                            ([80, 190, 80], "Green"),
                            ([60, 160, 220], "Blue"),
                            ([140, 100, 220], "Purple"),
                            ([200, 100, 180], "Pink"),
                            ([160, 160, 160], "Gray"),
                        ];
                        for &(color, name) in colors {
                            let c = egui::Color32::from_rgb(color[0], color[1], color[2]);
                            let resp = ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                                ui.painter().rect_filled(rect, 2.0, c);
                                ui.button(name)
                            });
                            if resp.inner.clicked() {
                                app.push_undo("Set clip color");
                                app.project.tracks[ti].clips[ci].color = Some(color);
                                app.sync_project();
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui.button("Reset to Track Color").clicked() {
                            app.push_undo("Reset clip color");
                            app.project.tracks[ti].clips[ci].color = None;
                            app.sync_project();
                            ui.close_menu();
                        }
                    });
                    // Speed submenu
                    ui.menu_button("Speed", |ui| {
                        let current_rate = app.project.tracks[ti].clips[ci].playback_rate;
                        let preserve = app.project.tracks[ti].clips[ci].preserve_pitch;
                        let speeds: &[(f32, &str)] = &[
                            (0.5, "Half Speed (0.5x)"),
                            (0.75, "0.75x"),
                            (1.0, "Normal (1x)"),
                            (1.5, "1.5x"),
                            (2.0, "Double Speed (2x)"),
                        ];
                        for &(rate, label) in speeds {
                            let is_current = (current_rate - rate).abs() < 0.01;
                            let text = if is_current {
                                format!("{} \u{2713}", label)
                            } else {
                                label.to_string()
                            };
                            if ui.button(&text).clicked() {
                                app.push_undo("Set clip speed");
                                app.project.tracks[ti].clips[ci].playback_rate = rate;
                                app.sync_project();
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui.button("Custom...").clicked() {
                            app.speed_input = Some(crate::SpeedInputState {
                                track_idx: ti,
                                clip_idx: ci,
                                input_buf: format!("{:.2}", current_rate),
                            });
                            ui.close_menu();
                        }
                        ui.separator();
                        let pitch_label = if preserve {
                            "Preserve Pitch \u{2713}"
                        } else {
                            "Preserve Pitch"
                        };
                        if ui.button(pitch_label).clicked() {
                            app.push_undo("Toggle preserve pitch");
                            app.project.tracks[ti].clips[ci].preserve_pitch = !preserve;
                            app.sync_project();
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    // Loop submenu
                    let current_loop = app.project.tracks[ti].clips[ci].loop_count.max(1);
                    ui.menu_button(format!("Loop ({}x)", current_loop), |ui| {
                        for &count in &[1u32, 2, 4, 8] {
                            let label = if count == 1 { "1x (no loop)".to_string() } else { format!("{}x", count) };
                            if ui.selectable_label(current_loop == count, label).clicked() {
                                app.push_undo("Set clip loop");
                                app.project.tracks[ti].clips[ci].loop_count = count;
                                app.sync_project();
                                ui.close_menu();
                            }
                        }
                    });
                    ui.separator();
                    // Consolidate (needs multiple clips selected on same track)
                    if app.selected_clips.len() >= 2 {
                        let all_same_track = app.selected_clips.iter().all(|&(t, _)| t == ti);
                        if all_same_track {
                            if ui.button("Consolidate [Cmd+J]").clicked() {
                                app.consolidate_selected_clips();
                                ui.close_menu();
                            }
                            ui.separator();
                        }
                    }
                    // Non-destructive reverse toggle
                    let is_reversed = app.project.tracks[ti].clips[ci].reversed;
                    let rev_label = if is_reversed {
                        "Reverse (non-destructive) \u{2713}"
                    } else {
                        "Reverse (non-destructive)"
                    };
                    if ui.button(rev_label).on_hover_text("Toggle non-destructive reverse playback").clicked() {
                        app.push_undo("Toggle clip reverse");
                        app.project.tracks[ti].clips[ci].reversed = !is_reversed;
                        app.sync_project();
                        ui.close_menu();
                    }
                    // Detect Tempo — basic onset detection for audio clips
                    let is_audio_clip = matches!(
                        app.project.tracks[ti].clips[ci].source,
                        ClipSource::AudioBuffer { .. } | ClipSource::AudioFile { .. }
                    );
                    if is_audio_clip {
                        if ui.button("Detect Tempo").on_hover_text("Detect BPM from audio and offer to set project tempo").clicked() {
                            let detected = app.detect_clip_tempo(ti, ci);
                            if let Some(bpm) = detected {
                                let rounded = (bpm * 10.0).round() / 10.0;
                                app.set_status(&format!("Detected tempo: {:.1} BPM — applied to project", rounded));
                                app.push_undo("Set tempo from audio");
                                app.project.tempo.bpm = rounded;
                                app.sync_project();
                            } else {
                                app.set_status("Could not detect tempo from this clip");
                            }
                            ui.close_menu();
                        }
                        if ui.button("Convert to MIDI").on_hover_text("Detect pitch and create a MIDI track (monophonic)").clicked() {
                            crate::analysis_tools::convert_clip_to_midi(app, ti, ci, true);
                            ui.close_menu();
                        }
                        if ui.button("Detect Chords").on_hover_text("Analyze audio and show chord progression overlay").clicked() {
                            crate::analysis_tools::detect_clip_chords(app, ti, ci);
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button("Properties...").on_hover_text("Open clip properties panel").clicked() {
                        app.editing_clip = Some((ti, ci));
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Delete Clip").clicked() {
                        app.push_undo("Delete clip");
                        app.project.tracks[ti].clips.remove(ci);
                        app.selected_clips.clear();
                        app.editing_clip = None;
                        app.sync_project();
                        ui.close_menu();
                    }
                } else {
                    if ui.button("Add Audio Track").clicked() {
                        app.push_undo("Add track");
                        let n = app.project.tracks.len() + 1;
                        app.project
                            .add_track(&format!("Track {n}"), TrackKind::Audio);
                        app.selected_track = Some(app.project.tracks.len() - 1);
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

        // Double-click anywhere: navigate playhead to that exact time point
        // Double-click on a clip: open/close clip properties panel
        // Double-click on a track separator: auto-fit track height
        if response.double_clicked() {
            if let Some(pos) = response.interact_pointer_pos {
                // Check if double-click is on a track separator — auto-fit height
                let mut separator_hit = false;
                {
                    let sep_zone = 6.0;
                    for (i, track) in app.project.tracks.iter().enumerate() {
                        if is_track_collapsed(app, i) {
                            continue;
                        }
                        let sep_y = tracks_y_start + track_offsets[i] + track_height(track, app.track_height_zoom);
                        if (pos.y - sep_y).abs() < sep_zone && pos.x >= rect.min.x && pos.x <= rect.max.x {
                            // Auto-fit: calculate optimal height based on content
                            let optimal = auto_fit_track_height(&app.project.tracks[i]);
                            app.project.tracks[i].custom_height = optimal;
                            separator_hit = true;
                            break;
                        }
                    }
                }
                if separator_hit {
                    // Skip normal double-click behavior
                } else {

                let mut clicked_on_clip = false;
                for &(ti, ci, _, cr) in &clip_rects {
                    if cr.contains(pos) {
                        clicked_on_clip = true;
                        app.selected_clips = std::collections::HashSet::from([(ti, ci)]);
                        app.selected_track = Some(ti);

                        // MIDI clip: open piano roll; Audio clip: open clip properties
                        let is_midi = matches!(
                            app.project.tracks[ti].clips[ci].source,
                            ClipSource::Midi { .. }
                        );
                        if is_midi || app.project.tracks[ti].kind == TrackKind::Midi {
                            app.show_piano_roll = true;
                        } else {
                            // Toggle clip properties panel
                            if app.editing_clip == Some((ti, ci)) {
                                app.editing_clip = None;
                            } else {
                                app.editing_clip = Some((ti, ci));
                            }
                        }
                        break;
                    }
                }

                if !clicked_on_clip {
                    let x_offset = pos.x - rect.min.x + app.scroll_x;
                    let seconds = x_offset as f64 / pixels_per_second as f64;
                    let sample_pos = (seconds * sample_rate) as u64;
                    let snapped = app.snap_position(sample_pos);
                    app.send_command(jamhub_engine::EngineCommand::SetPosition(snapped));

                    if let Some(ti) = track_at_y(app, pos.y, tracks_y_start) {
                        app.selected_track = Some(ti);
                    }
                }

                } // end else (not separator_hit)
            }
        }

        // Single click: select track, select/activate take, set playhead
        // Supports multi-select: Cmd+Click toggles, Shift+Click selects range
        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos {
                let modifiers = ui.input(|i| i.modifiers);

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

                    if modifiers.command {
                        // Cmd+Click: toggle this clip in multi-selection
                        if app.selected_clips.contains(&(ti, ci)) {
                            app.selected_clips.remove(&(ti, ci));
                        } else {
                            app.selected_clips.insert((ti, ci));
                        }
                    } else if modifiers.shift && !app.selected_clips.is_empty() {
                        // Shift+Click: select range of clips between first selected and clicked
                        // Find the "anchor" clip (first selected clip on same track, or any)
                        let anchor = app.selected_clips.iter()
                            .find(|&&(t, _)| t == ti)
                            .or_else(|| app.selected_clips.iter().next())
                            .copied();
                        if let Some((anchor_ti, anchor_ci)) = anchor {
                            if anchor_ti == ti {
                                // Same track: select all clips between anchor and clicked
                                let lo = anchor_ci.min(ci);
                                let hi = anchor_ci.max(ci);
                                for idx in lo..=hi {
                                    if idx < app.project.tracks[ti].clips.len() {
                                        app.selected_clips.insert((ti, idx));
                                    }
                                }
                            } else {
                                // Different tracks: just add this clip
                                app.selected_clips.insert((ti, ci));
                            }
                        }
                    } else {
                        // Plain click: select only this clip
                        app.selected_clips.clear();
                        app.selected_clips.insert((ti, ci));
                    }
                    app.selected_track = Some(ti);

                    // Clicking a muted take activates it (Reaper behavior)
                    if was_muted {
                        app.push_undo("Activate take");
                        let start = app.project.tracks[ti].clips[ci].start_sample;
                        let end =
                            start + app.project.tracks[ti].clips[ci].visual_duration_samples();
                        for (j, c) in
                            app.project.tracks[ti].clips.iter_mut().enumerate()
                        {
                            let c_end = c.start_sample + c.visual_duration_samples();
                            if j != ci && start < c_end && end > c.start_sample {
                                c.muted = true;
                            }
                        }
                        app.project.tracks[ti].clips[ci].muted = false;
                        app.sync_project();
                    }
                } else {
                    // Click on empty area: set playhead, deselect clips
                    app.selected_clips.clear();
                    let x_offset = pos.x - rect.min.x + app.scroll_x;
                    let seconds = x_offset as f64 / pixels_per_second as f64;
                    let sample_pos = (seconds * sample_rate) as u64;
                    let snapped = app.snap_position(sample_pos);
                    app.send_command(jamhub_engine::EngineCommand::SetPosition(snapped));
                }
            }
        }

        // Double-click on empty area: clear selection/loop
        if response.double_clicked() {
            if let Some(pos) = response.interact_pointer_pos {
                // Only if not on a clip
                let on_clip = clip_rects.iter().any(|&(_, _, _, cr)| cr.contains(pos));
                if !on_clip {
                    app.selection_start = None;
                    app.selection_end = None;
                    app.loop_enabled = false;
                    app.loop_start = 0;
                    app.loop_end = 0;
                    app.send_command(jamhub_engine::EngineCommand::SetLoop {
                        enabled: false,
                        start: 0,
                        end: 0,
                    });
                }
            }
        }

        // Clip dragging / edge trimming / fade handles / selection range
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos {
                let edge_zone = 8.0; // pixels from edge for trim detection
                let fade_handle_zone = 20.0; // pixels for fade handle hit area
                let mut action_taken = false;

                // Check for track separator drag (resize track height)
                {
                    let sep_zone = 4.0;
                    for (i, track) in app.project.tracks.iter().enumerate() {
                        if is_track_collapsed(app, i) {
                            continue;
                        }
                        let sep_y = tracks_y_start + track_offsets[i] + track_height(track, app.track_height_zoom);
                        if (pos.y - sep_y).abs() < sep_zone && pos.x >= rect.min.x && pos.x <= rect.max.x {
                            app.dragging_separator = Some(i);
                            action_taken = true;
                            break;
                        }
                    }
                }

                // Check for fade handle drag first (top corners, highest priority)
                if !action_taken {
                // Pre-scan to find target (avoids borrow conflict with app.push_undo)
                let mut fade_target: Option<(usize, usize, bool, u64)> = None; // (ti, ci, is_fade_in, orig_samples)
                for &(ti, ci, _, cr) in &clip_rects {
                    if !cr.contains(pos) {
                        continue;
                    }
                    if (pos.y - cr.top()) >= fade_handle_zone {
                        continue;
                    }
                    let clip = &app.project.tracks[ti].clips[ci];
                    if clip.muted {
                        continue;
                    }

                    let fade_in_px = (clip.fade_in_samples as f64 / sample_rate) as f32 * pixels_per_second;
                    let fi_handle_x = cr.left() + fade_in_px;
                    let fade_out_px = (clip.fade_out_samples as f64 / sample_rate) as f32 * pixels_per_second;
                    let fo_handle_x = cr.right() - fade_out_px;

                    if (pos.x - fi_handle_x).abs() < fade_handle_zone {
                        fade_target = Some((ti, ci, true, clip.fade_in_samples));
                        break;
                    } else if (pos.x - fo_handle_x).abs() < fade_handle_zone {
                        fade_target = Some((ti, ci, false, clip.fade_out_samples));
                        break;
                    }
                }
                if let Some((ti, ci, is_fade_in, orig)) = fade_target {
                    let label = if is_fade_in { "Adjust fade in" } else { "Adjust fade out" };
                    app.push_undo(label);
                    app.dragging_fade = Some(crate::FadeDragState {
                        track_idx: ti,
                        clip_idx: ci,
                        fade_edge: if is_fade_in { crate::FadeEdge::FadeIn } else { crate::FadeEdge::FadeOut },
                        original_fade_samples: orig,
                    });
                    action_taken = true;
                }
                }

                // Check for clip gain handle drag (click the circle at top-right of clip)
                if !action_taken {
                    for &(ti, ci, _, cr) in &clip_rects {
                        if !cr.contains(pos) {
                            continue;
                        }
                        let clip_muted = app.project.tracks[ti].clips[ci].muted;
                        let clip_gain = app.project.tracks[ti].clips[ci].gain_db;
                        if clip_muted {
                            continue;
                        }
                        let gain_range = 24.0_f32;
                        let handle_area_top = cr.top() + 6.0;
                        let handle_area_bot = cr.top() + cr.height().min(40.0);
                        let handle_area_center = (handle_area_top + handle_area_bot) * 0.5;
                        let handle_y = handle_area_center - (clip_gain / gain_range) * (handle_area_bot - handle_area_top) * 0.5;
                        let handle_y = handle_y.clamp(handle_area_top, handle_area_bot);
                        let handle_x = cr.right() - 22.0;
                        let dx = pos.x - handle_x;
                        let dy = pos.y - handle_y;
                        if (dx * dx + dy * dy).sqrt() < 10.0 {
                            app.push_undo("Adjust clip gain");
                            app.dragging_clip_gain = Some(crate::ClipGainDragState {
                                track_idx: ti,
                                clip_idx: ci,
                                start_y: pos.y,
                                original_gain_db: clip_gain,
                            });
                            action_taken = true;
                                break;
                            }
                        }
                }

                // Slip editing: Ctrl+drag on a clip to shift content within boundaries
                if !action_taken {
                    let mods = ui.input(|i| i.modifiers);
                    if mods.ctrl && !mods.command {
                        for &(ti, ci, _, cr) in &clip_rects {
                            if !cr.contains(pos) {
                                continue;
                            }
                            if app.project.tracks[ti].clips[ci].muted {
                                continue;
                            }
                            app.push_undo("Slip edit");
                            let orig_offset = app.project.tracks[ti].clips[ci].content_offset;
                            app.slip_editing = Some(crate::SlipEditState {
                                track_idx: ti,
                                clip_idx: ci,
                                start_x: pos.x,
                                original_content_offset: orig_offset,
                            });
                            action_taken = true;
                            break;
                        }
                    }
                }

                // Check for Alt+right-edge stretch or normal edge trim
                if !action_taken {
                let modifiers = ui.input(|i| i.modifiers);
                for &(ti, ci, _, cr) in &clip_rects {
                    if !cr.contains(pos) {
                        continue;
                    }

                    let left_edge = (pos.x - cr.left()).abs() < edge_zone;
                    let right_edge = (pos.x - cr.right()).abs() < edge_zone;

                    // Alt+drag right edge = stretch (change playback rate)
                    if right_edge && modifiers.alt {
                        let orig_dur = app.project.tracks[ti].clips[ci].duration_samples;
                        let orig_rate = app.project.tracks[ti].clips[ci].playback_rate;
                        app.push_undo("Stretch clip");
                        app.stretching_clip = Some(crate::ClipStretchState {
                            track_idx: ti,
                            clip_idx: ci,
                            original_duration: orig_dur,
                            original_rate: orig_rate,
                        });
                        action_taken = true;
                        break;
                    }

                    if left_edge || right_edge {
                        let orig_start = app.project.tracks[ti].clips[ci].start_sample;
                        let orig_dur = app.project.tracks[ti].clips[ci].duration_samples;
                        app.push_undo("Trim clip");
                        app.trimming_clip = Some(crate::ClipTrimState {
                            track_idx: ti,
                            clip_idx: ci,
                            edge: if left_edge {
                                crate::TrimEdge::Left
                            } else {
                                crate::TrimEdge::Right
                            },
                            original_start: orig_start,
                            original_duration: orig_dur,
                        });
                        action_taken = true;
                        break;
                    }
                }
                }

                // If not trimming, check for automation point adding
                if !action_taken && app.show_automation {
                    if let Some(ti) = track_at_y(app, pos.y, tracks_y_start) {
                        let x_offset = pos.x - rect.min.x + app.scroll_x;
                        let sample = (x_offset as f64 / pixels_per_second as f64 * sample_rate) as u64;
                        let sample = app.snap_position(sample);

                        // Calculate value from Y position within track
                        let offsets = track_y_offsets(app);
                        let track_top = tracks_y_start + offsets[ti];
                        let t_h = track_height(&app.project.tracks[ti], app.track_height_zoom);
                        let rel_y = (pos.y - track_top) / t_h;
                        let param = app.automation_param.clone();
                        let (min_val, max_val) = param.range();
                        let value = max_val - rel_y * (max_val - min_val);
                        let value = value.clamp(min_val, max_val);

                        // Add or update automation point
                        app.push_undo("Add automation point");
                        let sr = app.sample_rate();
                        let lane = app.project.tracks[ti]
                            .automation
                            .iter_mut()
                            .find(|l| l.parameter == param);

                        if let Some(lane) = lane {
                            let min_dist = sr as u64 / 10;
                            lane.points.retain(|p| {
                                (p.sample as i64 - sample as i64).unsigned_abs() > min_dist
                            });
                            lane.points.push(jamhub_model::AutomationPoint { sample, value, curve: 0.0 });
                            lane.points.sort_by_key(|p| p.sample);
                        } else {
                            app.project.tracks[ti].automation.push(
                                jamhub_model::AutomationLane {
                                    parameter: param,
                                    points: vec![jamhub_model::AutomationPoint { sample, value, curve: 0.0 }],
                                    visible: true,
                                },
                            );
                        }
                        app.sync_project();
                        action_taken = true;
                    }
                }

                // Swipe comping: when lanes are expanded and user drags on a muted take lane clip,
                // start swipe comp gesture instead of clip drag
                if !action_taken {
                    let mut swipe_target: Option<(usize, usize, usize)> = None; // (ti, ci, lane)
                    for &(ti, ci, lane, cr) in &clip_rects {
                        if cr.contains(pos)
                            && app.project.tracks[ti].lanes_expanded
                            && app.project.tracks[ti].clips[ci].muted
                        {
                            swipe_target = Some((ti, ci, lane));
                            break;
                        }
                    }
                    if let Some((ti, ci, lane)) = swipe_target {
                        app.push_undo("Swipe comp");
                        // Calculate sample position from X
                        let x_offset = pos.x - rect.min.x + app.scroll_x;
                        let sample = (x_offset as f64 / pixels_per_second as f64 * sample_rate) as u64;
                        app.swipe_comping = Some(crate::SwipeCompState {
                            track_idx: ti,
                            lane,
                            start_sample: sample,
                            current_sample: sample,
                        });
                        // Immediately activate the clicked take
                        let clip_start = app.project.tracks[ti].clips[ci].start_sample;
                        let clip_end = clip_start + app.project.tracks[ti].clips[ci].visual_duration_samples();
                        for (j, c) in app.project.tracks[ti].clips.iter_mut().enumerate() {
                            let c_end = c.start_sample + c.visual_duration_samples();
                            if clip_start < c_end && clip_end > c.start_sample {
                                c.muted = j != ci;
                            }
                        }
                        app.sync_project();
                        action_taken = true;
                    }
                }

                // If not trimming, automation, or swipe comping, try clip drag
                if !action_taken {
                    let mut drag_target: Option<(usize, usize, u64)> = None;
                    for &(ti, ci, _, cr) in &clip_rects {
                        if cr.contains(pos) {
                            drag_target = Some((
                                ti,
                                ci,
                                app.project.tracks[ti].clips[ci].start_sample,
                            ));
                        }
                    }
                    if let Some((ti, ci, _orig)) = drag_target {
                        // If dragging a selected clip with multiple selected, do multi-drag
                        if app.selected_clips.contains(&(ti, ci)) && app.selected_clips.len() > 1 {
                            app.push_undo("Move clips");
                            let originals: Vec<(usize, usize, u64)> = app.selected_clips.iter()
                                .filter_map(|&(t, c)| {
                                    if t < app.project.tracks.len() && c < app.project.tracks[t].clips.len() {
                                        Some((t, c, app.project.tracks[t].clips[c].start_sample))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            app.dragging_clips = Some(crate::MultiClipDragState {
                                start_x: pos.x,
                                originals,
                            });
                        } else {
                            // Single clip drag (also selects it)
                            app.push_undo("Move clip");
                            let orig = app.project.tracks[ti].clips[ci].start_sample;
                            app.dragging_clip = Some(crate::ClipDragState {
                                track_idx: ti,
                                clip_idx: ci,
                                start_x: pos.x,
                                original_start_sample: orig,
                            });
                            if !app.selected_clips.contains(&(ti, ci)) {
                                app.selected_clips.clear();
                                app.selected_clips.insert((ti, ci));
                            }
                        }
                        action_taken = true;
                    }
                }

                // If nothing else, start rubber-band or time selection
                if !action_taken {
                    let modifiers = ui.input(|i| i.modifiers);
                    if modifiers.alt {
                        // Alt+drag: rubber-band (marquee) clip selection
                        app.rubber_band_origin = Some(pos);
                        app.rubber_band_active = true;
                    } else {
                        // Check if dragging near a selection edge (Reaper-style resize)
                        // MUST match the visual hover zone exactly (both 10px)
                        let drag_zone = 10.0;
                        let mut started_edge_drag = false;
                        if let (Some(sel_s), Some(sel_e)) = (app.selection_start, app.selection_end) {
                            let s1 = sel_s.min(sel_e);
                            let s2 = sel_s.max(sel_e);
                            let sx1 = rect.min.x + (s1 as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                            let sx2 = rect.min.x + (s2 as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                            let dist_left = (pos.x - sx1).abs();
                            let dist_right = (pos.x - sx2).abs();
                            if dist_left < drag_zone && dist_left <= dist_right && pos.y >= rect.min.y && pos.y <= rect.max.y {
                                app.dragging_selection_edge = 1;
                                started_edge_drag = true;
                            } else if dist_right < drag_zone && pos.y >= rect.min.y && pos.y <= rect.max.y {
                                app.dragging_selection_edge = 2;
                                started_edge_drag = true;
                            }
                        }
                        if !started_edge_drag {
                            // Normal drag on empty area: start new time selection
                            let x_offset = pos.x - rect.min.x + app.scroll_x;
                            let sample =
                                (x_offset as f64 / pixels_per_second as f64 * sample_rate) as u64;
                            app.selection_start = Some(app.snap_position(sample));
                            app.selection_end = app.selection_start;
                            app.selecting = true;
                        }
                    }
                }
            }
        }

        if response.dragged_by(egui::PointerButton::Primary) {
            // Handle fade handle dragging
            if let Some(ref fade_drag) = app.dragging_fade {
                if let Some(pos) = response.interact_pointer_pos {
                    let ti = fade_drag.track_idx;
                    let ci = fade_drag.clip_idx;
                    let fade_edge = &fade_drag.fade_edge;
                    if ti < app.project.tracks.len()
                        && ci < app.project.tracks[ti].clips.len()
                    {
                        let clip = &app.project.tracks[ti].clips[ci];
                        let clip_x_start = rect.min.x
                            + (clip.start_sample as f64 / sample_rate) as f32 * pixels_per_second
                            - app.scroll_x;
                        let visual_dur = clip.visual_duration_samples();
                        let clip_x_end = clip_x_start
                            + (visual_dur as f64 / sample_rate) as f32 * pixels_per_second;

                        match fade_edge {
                            crate::FadeEdge::FadeIn => {
                                // Fade in: handle position is relative to clip start
                                let dx = (pos.x - clip_x_start).max(0.0).min(clip_x_end - clip_x_start);
                                let fade_seconds = dx as f64 / pixels_per_second as f64;
                                let fade_samples = (fade_seconds * sample_rate) as u64;
                                let fade_samples = fade_samples.min(visual_dur);
                                app.project.tracks[ti].clips[ci].fade_in_samples = fade_samples;
                            }
                            crate::FadeEdge::FadeOut => {
                                // Fade out: handle position is relative to clip end
                                let dx = (clip_x_end - pos.x).max(0.0).min(clip_x_end - clip_x_start);
                                let fade_seconds = dx as f64 / pixels_per_second as f64;
                                let fade_samples = (fade_seconds * sample_rate) as u64;
                                let fade_samples = fade_samples.min(visual_dur);
                                app.project.tracks[ti].clips[ci].fade_out_samples = fade_samples;
                            }
                        }
                    }
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
            // Handle clip gain drag
            else if let Some(ref gain_drag) = app.dragging_clip_gain {
                if let Some(pos) = response.interact_pointer_pos {
                    let ti = gain_drag.track_idx;
                    let ci = gain_drag.clip_idx;
                    let start_y = gain_drag.start_y;
                    let orig_gain = gain_drag.original_gain_db;
                    if ti < app.project.tracks.len()
                        && ci < app.project.tracks[ti].clips.len()
                    {
                        // Dragging up = increase gain, down = decrease
                        let dy = start_y - pos.y; // positive = up
                        let db_per_pixel = 0.5; // 0.5 dB per pixel of drag
                        let new_gain = (orig_gain + dy * db_per_pixel).clamp(-24.0, 24.0);
                        app.project.tracks[ti].clips[ci].gain_db = new_gain;
                    }
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                }
            }
            // Handle swipe comping drag — activate takes as mouse moves across take lanes
            else if let Some(ref swipe) = app.swipe_comping {
                if let Some(pos) = response.interact_pointer_pos {
                    let ti = swipe.track_idx;
                    let lane = swipe.lane;
                    // Calculate current sample position from X
                    let x_offset = pos.x - rect.min.x + app.scroll_x;
                    let current_sample = (x_offset as f64 / pixels_per_second as f64 * sample_rate) as u64;

                    // Find the lane assignments for this track
                    if ti < app.project.tracks.len() {
                        let take_lanes = compute_take_lanes(&app.project.tracks[ti]);
                        // Find the clip in the swiped lane that covers the current position
                        for &(ci, cl) in &take_lanes {
                            if cl != lane {
                                continue;
                            }
                            let clip = &app.project.tracks[ti].clips[ci];
                            let clip_start = clip.start_sample;
                            let clip_end = clip_start + clip.visual_duration_samples();
                            if current_sample >= clip_start && current_sample < clip_end {
                                // This clip should be active; mute all others overlapping it
                                if app.project.tracks[ti].clips[ci].muted {
                                    let cs = app.project.tracks[ti].clips[ci].start_sample;
                                    let ce = cs + app.project.tracks[ti].clips[ci].visual_duration_samples();
                                    for (j, c) in app.project.tracks[ti].clips.iter_mut().enumerate() {
                                        let c_end = c.start_sample + c.visual_duration_samples();
                                        if cs < c_end && ce > c.start_sample {
                                            c.muted = j != ci;
                                        }
                                    }
                                    app.sync_project();
                                }
                                break;
                            }
                        }
                    }
                    // Update swipe state
                    app.swipe_comping = Some(crate::SwipeCompState {
                        track_idx: ti,
                        lane,
                        start_sample: swipe.start_sample,
                        current_sample,
                    });
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
            // Handle clip stretch dragging (Alt+drag right edge)
            else if let Some(ref stretch) = app.stretching_clip {
                if let Some(pos) = response.interact_pointer_pos {
                    let ti = stretch.track_idx;
                    let ci = stretch.clip_idx;
                    let orig_dur = stretch.original_duration;
                    let _orig_rate = stretch.original_rate;

                    if ti < app.project.tracks.len()
                        && ci < app.project.tracks[ti].clips.len()
                    {
                        let clip_start = app.project.tracks[ti].clips[ci].start_sample;
                        let clip_x_start = rect.min.x
                            + (clip_start as f64 / sample_rate) as f32 * pixels_per_second
                            - app.scroll_x;
                        // New visual width from mouse position
                        let new_visual_px = (pos.x - clip_x_start).max(20.0);
                        let new_visual_seconds = new_visual_px as f64 / pixels_per_second as f64;
                        let new_visual_samples = (new_visual_seconds * sample_rate) as u64;
                        // new_rate = original_duration / new_visual_duration
                        let new_rate = orig_dur as f32 / new_visual_samples.max(1) as f32;
                        // Clamp to reasonable range
                        let new_rate = new_rate.clamp(0.1, 8.0);
                        app.project.tracks[ti].clips[ci].playback_rate = new_rate;
                    }

                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
            // Handle clip trim dragging
            else if let Some(ref trim) = app.trimming_clip {
                if let Some(pos) = response.interact_pointer_pos {
                    let x_offset = pos.x - rect.min.x + app.scroll_x;
                    let sample = (x_offset as f64 / pixels_per_second as f64 * sample_rate) as u64;
                    let sample = app.snap_position(sample);
                    let ti = trim.track_idx;
                    let ci = trim.clip_idx;
                    let orig_start = trim.original_start;
                    let orig_dur = trim.original_duration;
                    let orig_end = orig_start + orig_dur;

                    if ti < app.project.tracks.len()
                        && ci < app.project.tracks[ti].clips.len()
                    {
                        match trim.edge {
                            crate::TrimEdge::Left => {
                                // Move start forward, reduce duration
                                let new_start = sample.min(orig_end.saturating_sub(256));
                                let new_start = new_start.max(0);
                                app.project.tracks[ti].clips[ci].start_sample = new_start;
                                if new_start < orig_end {
                                    app.project.tracks[ti].clips[ci].duration_samples =
                                        orig_end - new_start;
                                }
                            }
                            crate::TrimEdge::Right => {
                                // Keep start, change duration
                                let new_end = sample.max(orig_start + 256);
                                app.project.tracks[ti].clips[ci].duration_samples =
                                    new_end - orig_start;
                            }
                        }
                    }

                    // Show trim cursor
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
            // Handle selection edge dragging (resize selection)
            else if app.dragging_selection_edge > 0 {
                if let Some(pos) = response.interact_pointer_pos {
                    let x_offset = pos.x - rect.min.x + app.scroll_x;
                    let sample = (x_offset as f64 / pixels_per_second as f64 * sample_rate) as u64;
                    let snapped = app.snap_position(sample);
                    if app.dragging_selection_edge == 1 {
                        // Dragging left edge
                        app.selection_start = Some(snapped);
                    } else {
                        // Dragging right edge
                        app.selection_end = Some(snapped);
                    }
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
            // Handle selection range dragging (creating new selection)
            else if app.selecting {
                if let Some(pos) = response.interact_pointer_pos {
                    let x_offset = pos.x - rect.min.x + app.scroll_x;
                    let sample =
                        (x_offset as f64 / pixels_per_second as f64 * sample_rate) as u64;
                    app.selection_end = Some(app.snap_position(sample));
                }
            }
            // Handle multi-clip dragging
            else if let Some(ref multi_drag) = app.dragging_clips {
                if let Some(pos) = response.interact_pointer_pos {
                    let dx = pos.x - multi_drag.start_x;
                    let d_seconds = dx as f64 / pixels_per_second as f64;
                    let d_samples = (d_seconds * sample_rate) as i64;

                    let originals = multi_drag.originals.clone();
                    app.magnetic_snap_active = false;
                    app.clip_edge_snap_sample = None;
                    for &(ti, ci, orig_start) in &originals {
                        if ti < app.project.tracks.len()
                            && ci < app.project.tracks[ti].clips.len()
                        {
                            let new_start = (orig_start as i64 + d_samples).max(0) as u64;
                            let snapped = if app.ctrl_held {
                                new_start // Ctrl disables snap while dragging
                            } else {
                                // Try clip edge snap first, then grid snap
                                let (edge_s, edge_snap) = app.snap_to_clip_edges(new_start, ti, pixels_per_second, 5.0);
                                if edge_snap {
                                    app.clip_edge_snap_sample = Some(edge_s);
                                    edge_s
                                } else {
                                    let (s, did_snap) = app.magnetic_snap(new_start, pixels_per_second, 5.0);
                                    if did_snap {
                                        app.magnetic_snap_active = true;
                                        app.magnetic_snap_sample = s;
                                    }
                                    s
                                }
                            };
                            app.project.tracks[ti].clips[ci].start_sample = snapped;
                        }
                    }
                }
            }
            // Handle single clip dragging
            else if let Some(ref drag) = app.dragging_clip {
                if let Some(pos) = response.interact_pointer_pos {
                    let dx = pos.x - drag.start_x;
                    let d_seconds = dx as f64 / pixels_per_second as f64;
                    let d_samples = (d_seconds * sample_rate) as i64;
                    let new_start =
                        (drag.original_start_sample as i64 + d_samples).max(0) as u64;
                    let drag_ti = drag.track_idx;
                    let snapped = if app.ctrl_held {
                        app.magnetic_snap_active = false;
                        app.clip_edge_snap_sample = None;
                        new_start // Ctrl disables snap while dragging
                    } else {
                        // Try clip edge snap first (higher priority), then grid snap
                        let (edge_s, edge_snap) = app.snap_to_clip_edges(new_start, drag_ti, pixels_per_second, 5.0);
                        if edge_snap {
                            app.magnetic_snap_active = false;
                            app.clip_edge_snap_sample = Some(edge_s);
                            edge_s
                        } else {
                            app.clip_edge_snap_sample = None;
                            let (s, did_snap) = app.magnetic_snap(new_start, pixels_per_second, 5.0);
                            app.magnetic_snap_active = did_snap;
                            if did_snap { app.magnetic_snap_sample = s; }
                            s
                        }
                    };

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
            // Handle slip editing drag — shift audio content within clip boundaries
            else if let Some(ref slip) = app.slip_editing {
                if let Some(pos) = response.interact_pointer_pos {
                    let ti = slip.track_idx;
                    let ci = slip.clip_idx;
                    let dx = pos.x - slip.start_x;
                    let d_seconds = dx as f64 / pixels_per_second as f64;
                    let d_samples = (d_seconds * sample_rate) as i64;
                    if ti < app.project.tracks.len()
                        && ci < app.project.tracks[ti].clips.len()
                    {
                        // Get the source buffer length to clamp offset
                        let max_offset = if let jamhub_model::ClipSource::AudioBuffer { buffer_id } =
                            &app.project.tracks[ti].clips[ci].source
                        {
                            app.audio_buffers.get(buffer_id).map(|b| b.len() as u64).unwrap_or(u64::MAX)
                        } else {
                            u64::MAX
                        };
                        let clip_dur = app.project.tracks[ti].clips[ci].duration_samples;
                        let new_offset = (slip.original_content_offset as i64 - d_samples).max(0) as u64;
                        let new_offset = new_offset.min(max_offset.saturating_sub(clip_dur));
                        app.project.tracks[ti].clips[ci].content_offset = new_offset;
                    }
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
            // Handle rubber-band selection drag
            else if app.rubber_band_active {
                if let Some(pos) = response.interact_pointer_pos {
                    if let Some(origin) = app.rubber_band_origin {
                        let rb_rect = egui::Rect::from_two_pos(origin, pos);
                        app.selected_clips.clear();
                        for &(ti, ci, _, cr) in &clip_rects {
                            if rb_rect.intersects(cr) {
                                app.selected_clips.insert((ti, ci));
                            }
                        }
                    }
                }
            }
        }

        if response.drag_stopped() {
            app.magnetic_snap_active = false;
            app.clip_edge_snap_sample = None;
            if app.dragging_fade.is_some() {
                app.dragging_fade = None;
                app.sync_project();
            }
            if app.dragging_clip_gain.is_some() {
                app.dragging_clip_gain = None;
                app.sync_project();
            }
            if app.dragging_clips.is_some() {
                app.dragging_clips = None;
                app.sync_project();
            }
            if app.dragging_clip.is_some() {
                app.dragging_clip = None;
                app.sync_project();
            }
            if app.trimming_clip.is_some() {
                app.trimming_clip = None;
                app.sync_project();
            }
            if app.stretching_clip.is_some() {
                app.stretching_clip = None;
                app.sync_project();
            }
            if app.swipe_comping.is_some() {
                app.swipe_comping = None;
                app.sync_project();
            }
            if app.slip_editing.is_some() {
                app.slip_editing = None;
                app.sync_project();
            }
            if app.dragging_separator.is_some() {
                app.dragging_separator = None;
            }
            if app.dragging_selection_edge > 0 {
                app.dragging_selection_edge = 0;
                // Normalize and update loop
                if let (Some(s), Some(e)) = (app.selection_start, app.selection_end) {
                    let s1 = s.min(e);
                    let s2 = s.max(e);
                    app.selection_start = Some(s1);
                    app.selection_end = Some(s2);
                    if s2 > s1 + 100 {
                        app.loop_start = s1;
                        app.loop_end = s2;
                        app.loop_enabled = true;
                        app.send_command(jamhub_engine::EngineCommand::SetLoop {
                            enabled: true, start: s1, end: s2,
                        });
                    }
                }
            }
            if app.rubber_band_active {
                app.rubber_band_active = false;
                app.rubber_band_origin = None;
            }
            if app.selecting {
                app.selecting = false;
                // Normalize selection so start < end
                if let (Some(s), Some(e)) = (app.selection_start, app.selection_end) {
                    if s > e {
                        app.selection_start = Some(e);
                        app.selection_end = Some(s);
                    }
                    // If selection is tiny (click, not drag), clear it
                    if let (Some(s2), Some(e2)) = (app.selection_start, app.selection_end) {
                        if e2.saturating_sub(s2) < 100 {
                            app.selection_start = None;
                            app.selection_end = None;
                        } else {
                            // Auto-set as loop region
                            app.loop_start = s2;
                            app.loop_end = e2;
                            app.loop_enabled = true;
                            app.send_command(jamhub_engine::EngineCommand::SetLoop {
                                enabled: true,
                                start: s2,
                                end: e2,
                            });
                        }
                    }
                }
            }
        }

        // Scroll/zoom
        ui.input(|i| {
            if i.modifiers.command && i.modifiers.shift {
                // Ctrl/Cmd + Shift + scroll = vertical track height zoom
                let scroll = i.smooth_scroll_delta.y;
                if scroll != 0.0 {
                    app.track_height_zoom = (app.track_height_zoom * (1.0 + scroll * 0.005)).clamp(0.5, 3.0);
                }
            } else if i.modifiers.command {
                // Cmd + scroll = horizontal time zoom (anchored at mouse position)
                let scroll = i.smooth_scroll_delta.y;
                if scroll != 0.0 {
                    let old_pps = PIXELS_PER_SECOND_BASE * app.zoom;
                    let mouse_x = i.pointer.hover_pos().map_or(rect.center().x, |p| p.x);
                    let mouse_time = (mouse_x - rect.min.x + app.scroll_x) / old_pps;

                    app.zoom = (app.zoom * (1.0 + scroll * 0.005)).clamp(0.1, 10.0);

                    let new_pps = PIXELS_PER_SECOND_BASE * app.zoom;
                    app.scroll_x = (mouse_time * new_pps - (mouse_x - rect.min.x)).max(0.0);
                }
            } else {
                let scroll_x = i.smooth_scroll_delta.x - i.smooth_scroll_delta.y;
                if scroll_x != 0.0 {
                    app.scroll_x = (app.scroll_x - scroll_x).max(0.0);
                    // User is manually scrolling — suppress auto-follow temporarily
                    app.user_scrolling = true;
                }
            }
        });

        // Middle mouse drag also counts as user scrolling
        if response.dragged_by(egui::PointerButton::Middle) {
            app.user_scrolling = true;
        }

        // Clear user_scrolling flag when transport stops
        if app.transport_state() != jamhub_model::TransportState::Playing {
            app.user_scrolling = false;
        }

        let painter = ui.painter();

        // Background
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(22, 22, 26));

        // Ruler — premium gradient with clear hierarchy
        let ruler_rect =
            egui::Rect::from_min_size(rect.min, egui::vec2(available.x, RULER_HEIGHT));
        // Subtle vertical gradient: slightly lighter at top
        painter.rect_filled(ruler_rect, 0.0, egui::Color32::from_rgb(30, 30, 38));
        let ruler_top_half = egui::Rect::from_min_max(ruler_rect.min, egui::pos2(ruler_rect.max.x, ruler_rect.center().y));
        painter.rect_filled(ruler_top_half, 0.0, egui::Color32::from_rgba_premultiplied(255, 255, 255, 4));
        // Bottom edge accent line
        painter.line_segment(
            [egui::pos2(ruler_rect.min.x, ruler_rect.max.y), egui::pos2(ruler_rect.max.x, ruler_rect.max.y)],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(55, 55, 65)),
        );

        // Beat/bar grid
        let bpm = app.project.tempo.bpm;
        let beats_per_bar = app.project.time_signature.numerator as f64;
        let seconds_per_beat = 60.0 / bpm;
        let pixels_per_beat = seconds_per_beat as f32 * pixels_per_second;

        let start_beat = (app.scroll_x / pixels_per_beat).floor() as i32;
        let visible_beats = (available.x / pixels_per_beat).ceil() as i32 + 2;

        // Grid subdivision based on grid_division setting
        let grid_div = app.grid_division;
        let grid_subdiv = grid_div.subdivisions_per_beat();

        for b in start_beat..(start_beat + visible_beats) {
            if b < 0 {
                continue;
            }
            let x = rect.min.x + b as f32 * pixels_per_beat - app.scroll_x;
            let is_bar = b as f64 % beats_per_bar == 0.0;

            // Always draw beat and bar lines (unless grid is None)
            if grid_div != crate::GridDivision::None {
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
            } else if is_bar {
                // Even with grid=None, show bar lines as faint reference
                painter.line_segment(
                    [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                    egui::Stroke::new(0.5, egui::Color32::from_rgb(40, 40, 46)),
                );
            }

            // Sub-beat division lines based on grid division setting
            if grid_subdiv > 1.0 {
                let subdiv_count = grid_subdiv as i32;
                for sub in 1..subdiv_count {
                    let sub_x = x + (sub as f32 / subdiv_count as f32) * pixels_per_beat;
                    if sub_x >= rect.min.x && sub_x <= rect.max.x {
                        let alpha = if grid_subdiv >= 8.0 { 0.2 } else if grid_subdiv >= 4.0 { 0.25 } else { 0.3 };
                        painter.line_segment(
                            [egui::pos2(sub_x, tracks_y_start), egui::pos2(sub_x, rect.max.y)],
                            egui::Stroke::new(alpha, egui::Color32::from_rgb(40, 40, 48)),
                        );
                    }
                }
            }

            // Beat tick marks on the ruler bottom edge
            {
                let tick_x = x;
                if tick_x >= ruler_rect.min.x && tick_x <= ruler_rect.max.x {
                    let tick_len = if is_bar { 10.0 } else { 5.0 };
                    let tick_color = if is_bar {
                        egui::Color32::from_rgb(100, 100, 115)
                    } else {
                        egui::Color32::from_rgb(60, 60, 70)
                    };
                    painter.line_segment(
                        [egui::pos2(tick_x, ruler_rect.max.y - tick_len), egui::pos2(tick_x, ruler_rect.max.y)],
                        egui::Stroke::new(if is_bar { 1.0 } else { 0.5 }, tick_color),
                    );
                }

                // Subdivision tick marks within each beat
                if grid_subdiv > 1.0 {
                    let subdiv_count = grid_subdiv as i32;
                    for sub in 1..subdiv_count {
                        let sub_x = x + (sub as f32 / subdiv_count as f32) * pixels_per_beat;
                        if sub_x >= ruler_rect.min.x && sub_x <= ruler_rect.max.x {
                            painter.line_segment(
                                [egui::pos2(sub_x, ruler_rect.max.y - 3.0), egui::pos2(sub_x, ruler_rect.max.y)],
                                egui::Stroke::new(0.5, egui::Color32::from_rgb(48, 48, 56)),
                            );
                        }
                    }
                }
            }

            if is_bar {
                let bar = (b as f64 / beats_per_bar) as i32 + 1;
                let bar_time_sec = b as f64 * seconds_per_beat;
                let bar_min = (bar_time_sec / 60.0) as u32;
                let bar_sec = bar_time_sec % 60.0;

                // Highlight current bar with subtle amber background
                let current_pos = app.position_samples();
                let current_beat = app.project.tempo.beat_at_sample(current_pos, app.sample_rate() as f64);
                let current_bar = (current_beat / beats_per_bar).floor() as i32 + 1;
                if bar == current_bar {
                    let highlight_w = pixels_per_beat * beats_per_bar as f32;
                    let highlight_rect = egui::Rect::from_min_size(
                        egui::pos2(x, ruler_rect.min.y),
                        egui::vec2(highlight_w.min(ruler_rect.max.x - x), ruler_rect.height()),
                    );
                    painter.rect_filled(highlight_rect, 0.0, egui::Color32::from_rgba_premultiplied(240, 192, 64, 12));
                }

                // Bar number — bold, bright, prominent
                painter.text(
                    egui::pos2(x + 4.0, rect.min.y + 2.0),
                    egui::Align2::LEFT_TOP,
                    format!("{bar}"),
                    egui::FontId::new(14.0, egui::FontFamily::Proportional),
                    egui::Color32::from_rgb(220, 220, 235),
                );

                // Time display below bar number — dimmer, smaller, monospace feel
                let time_str = if bar_min > 0 {
                    format!("{bar_min}:{bar_sec:04.1}")
                } else {
                    format!("{bar_sec:.1}s")
                };
                painter.text(
                    egui::pos2(x + 4.0, rect.min.y + 18.0),
                    egui::Align2::LEFT_TOP,
                    time_str,
                    egui::FontId::new(8.5, egui::FontFamily::Monospace),
                    egui::Color32::from_rgb(85, 88, 100),
                );
            }
        }

        // Grid division label on the ruler (drawn with painter, no mutable ui borrow)
        {
            let grid_label = format!("Grid: {}", app.grid_division.label());
            let grid_label_pos = egui::pos2(rect.max.x - 70.0, ruler_rect.min.y + 4.0);
            painter.text(
                grid_label_pos,
                egui::Align2::LEFT_TOP,
                &grid_label,
                egui::FontId::proportional(9.0),
                egui::Color32::from_rgb(140, 140, 155),
            );

            // Hit test: click to cycle grid division
            let grid_btn_rect = egui::Rect::from_min_size(
                egui::pos2(rect.max.x - 72.0, ruler_rect.min.y + 1.0),
                egui::vec2(70.0, 16.0),
            );
            if response.clicked_by(egui::PointerButton::Primary) {
                if let Some(click_pos) = response.interact_pointer_pos {
                    if grid_btn_rect.contains(click_pos) {
                        use crate::GridDivision;
                        app.grid_division = match app.grid_division {
                            GridDivision::None => GridDivision::Bar,
                            GridDivision::Bar => GridDivision::Half,
                            GridDivision::Half => GridDivision::Beat,
                            GridDivision::Beat => GridDivision::Eighth,
                            GridDivision::Eighth => GridDivision::Sixteenth,
                            GridDivision::Sixteenth => GridDivision::ThirtySecond,
                            GridDivision::ThirtySecond => GridDivision::Triplet,
                            GridDivision::Triplet => GridDivision::None,
                        };
                        app.set_status(&format!("Grid: {}", app.grid_division.label()));
                    }
                }
            }
        }

        // Tempo change markers on the ruler
        {
            for tc in &app.project.tempo_map.changes {
                let tc_sec = tc.sample as f64 / sample_rate;
                let tc_x = rect.min.x + tc_sec as f32 * pixels_per_second - app.scroll_x;
                if tc_x >= rect.min.x - 10.0 && tc_x <= rect.max.x + 10.0 {
                    // Small tempo flag on the ruler
                    let flag_y = ruler_rect.max.y - 8.0;
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(tc_x, flag_y - 6.0),
                            egui::pos2(tc_x - 4.0, flag_y),
                            egui::pos2(tc_x + 4.0, flag_y),
                        ],
                        egui::Color32::from_rgb(255, 160, 60),
                        egui::Stroke::NONE,
                    ));
                    // Label with BPM
                    painter.text(
                        egui::pos2(tc_x + 5.0, flag_y - 6.0),
                        egui::Align2::LEFT_TOP,
                        format!("{:.0}", tc.bpm),
                        egui::FontId::proportional(8.0),
                        egui::Color32::from_rgb(255, 180, 80),
                    );
                }
            }
        }

        // === Saved regions — colored bars above the ruler ===
        {
            let region_bar_height = 8.0;
            for (ri, region) in app.project.regions.iter().enumerate() {
                let rc = egui::Color32::from_rgb(region.color[0], region.color[1], region.color[2]);
                let rx1 = rect.min.x + (region.start as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                let rx2 = rect.min.x + (region.end as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                if rx2 < rect.min.x || rx1 > rect.max.x {
                    continue;
                }
                let ry = ruler_rect.min.y - region_bar_height - 2.0 - (ri % 2) as f32 * (region_bar_height + 1.0);
                let region_rect = egui::Rect::from_min_max(
                    egui::pos2(rx1.max(rect.min.x), ry.max(rect.min.y)),
                    egui::pos2(rx2.min(rect.max.x), (ry + region_bar_height).max(rect.min.y)),
                );
                painter.rect_filled(region_rect, 3.0, rc.gamma_multiply(0.35));
                painter.rect_stroke(region_rect, 3.0, egui::Stroke::new(1.0, rc.gamma_multiply(0.7)), egui::StrokeKind::Outside);
                // Region name
                painter.with_clip_rect(region_rect).text(
                    egui::pos2(rx1.max(rect.min.x) + 3.0, ry.max(rect.min.y) + 0.5),
                    egui::Align2::LEFT_TOP,
                    &region.name,
                    egui::FontId::proportional(7.5),
                    rc,
                );
            }

            // Click on a region bar to activate it as the loop
            if response.clicked_by(egui::PointerButton::Primary) {
                if let Some(click_pos) = response.interact_pointer_pos {
                    let region_area_top = ruler_rect.min.y - (region_bar_height + 2.0) * 2.0;
                    if click_pos.y >= region_area_top && click_pos.y < ruler_rect.min.y {
                        for region in &app.project.regions {
                            let rx1 = rect.min.x + (region.start as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                            let rx2 = rect.min.x + (region.end as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                            if click_pos.x >= rx1 && click_pos.x <= rx2 {
                                app.loop_start = region.start;
                                app.loop_end = region.end;
                                app.loop_enabled = true;
                                app.send_command(jamhub_engine::EngineCommand::SetLoop {
                                    enabled: true,
                                    start: region.start,
                                    end: region.end,
                                });
                                app.set_status(&format!("Loop: {}", region.name));
                                break;
                            }
                        }
                    }
                }
            }

            // Right-click on the loop bar area to save current loop as named region
            if response.secondary_clicked() {
                if let Some(click_pos) = response.interact_pointer_pos {
                    if click_pos.y >= ruler_rect.min.y - 20.0 && click_pos.y < ruler_rect.min.y
                        && app.loop_enabled && app.loop_end > app.loop_start
                    {
                        app.region_name_input = Some(crate::RegionNameInput {
                            name: format!("Region {}", app.project.regions.len() + 1),
                            start: app.loop_start,
                            end: app.loop_end,
                        });
                    }
                }
            }
        }

        // Region name input dialog
        if app.region_name_input.is_some() {
            let mut apply = false;
            let mut cancel = false;
            egui::Window::new("Save Region")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    ui.label("Region name:");
                    if let Some(ref mut state) = app.region_name_input {
                        let resp = ui.text_edit_singleline(&mut state.name);
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            apply = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() { apply = true; }
                            if ui.button("Cancel").clicked() { cancel = true; }
                        });
                    }
                });
            if apply {
                if let Some(state) = app.region_name_input.take() {
                    let region_colors: &[[u8; 3]] = &[
                        [80, 160, 220], [160, 100, 200], [200, 140, 60],
                        [80, 200, 120], [200, 80, 120], [140, 200, 200],
                    ];
                    let color = region_colors[app.project.regions.len() % region_colors.len()];
                    app.project.regions.push(jamhub_model::Region {
                        id: uuid::Uuid::new_v4(),
                        name: state.name,
                        start: state.start,
                        end: state.end,
                        color,
                    });
                    app.set_status("Region saved");
                }
            } else if cancel {
                app.region_name_input = None;
            }
        }

        // Track lanes with take sub-lanes
        for (i, track) in app.project.tracks.iter().enumerate() {
            // Skip collapsed group tracks
            if is_track_collapsed(app, i) {
                continue;
            }
            let t_y = tracks_y_start + track_offsets[i];
            let t_h = track_height(track, app.track_height_zoom);
            let lane_rect = egui::Rect::from_min_size(
                egui::pos2(rect.min.x, t_y),
                egui::vec2(available.x, t_h),
            );

            let is_selected = app.selected_track == Some(i);
            let bg = if is_selected {
                egui::Color32::from_rgba_premultiplied(35, 35, 45, 60)
            } else if i % 2 == 0 {
                egui::Color32::from_rgba_premultiplied(28, 28, 34, 40)
            } else {
                egui::Color32::from_rgba_premultiplied(22, 22, 28, 40)
            };
            painter.rect_filled(lane_rect, 0.0, bg);

            // Track separator — clearer line
            painter.line_segment(
                [
                    egui::pos2(rect.min.x, t_y + t_h),
                    egui::pos2(rect.max.x, t_y + t_h),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(46, 46, 56)),
            );

            // Take lane separators and labels (only when expanded)
            let take_lanes = compute_take_lanes(track);
            let num_lanes = take_lanes.iter().map(|&(_, l)| l).max().unwrap_or(0) + 1;
            if num_lanes > 1 && track.lanes_expanded {
                for lane in 0..num_lanes {
                    let ly = t_y + lane as f32 * TAKE_LANE_HEIGHT;
                    // Separator line (skip first lane)
                    if lane > 0 {
                        painter.line_segment(
                            [egui::pos2(rect.min.x, ly), egui::pos2(rect.max.x, ly)],
                            egui::Stroke::new(
                                0.5,
                                egui::Color32::from_rgb(55, 50, 40),
                            ),
                        );
                    }
                    // Lane label — small "T1", "T2", etc. on the left side
                    let label = format!("T{}", lane + 1);
                    let lane_active = take_lanes.iter().any(|&(ci, l)| l == lane && !track.clips[ci].muted);
                    let label_color = if lane_active {
                        egui::Color32::from_rgb(220, 190, 70)
                    } else {
                        egui::Color32::from_rgb(90, 88, 82)
                    };
                    painter.text(
                        egui::pos2(rect.min.x + 3.0, ly + 2.0),
                        egui::Align2::LEFT_TOP,
                        &label,
                        egui::FontId::proportional(9.0),
                        label_color,
                    );

                    // Swipe comp visual indicator — highlight the lane being swiped
                    if let Some(ref swipe) = app.swipe_comping {
                        if swipe.track_idx == i && swipe.lane == lane {
                            let swipe_min = swipe.start_sample.min(swipe.current_sample);
                            let swipe_max = swipe.start_sample.max(swipe.current_sample);
                            let sx1 = rect.min.x + (swipe_min as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                            let sx2 = rect.min.x + (swipe_max as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                            let swipe_rect = egui::Rect::from_min_max(
                                egui::pos2(sx1.max(rect.min.x), ly),
                                egui::pos2(sx2.min(rect.max.x), ly + TAKE_LANE_HEIGHT),
                            );
                            painter.rect_filled(
                                swipe_rect,
                                0.0,
                                egui::Color32::from_rgba_premultiplied(220, 180, 50, 8),
                            );
                        }
                    }
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

            let track_color =
                egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);

            // Draw clips in their take lanes
            for &(ci, lane) in &take_lanes {
                let clip = &track.clips[ci];

                // Use custom clip color if set, otherwise fall back to track color
                // Apply type-based tinting: MIDI clips get purple tint, frozen clips get blue-gray
                let base_color = match clip.color {
                    Some(c) => egui::Color32::from_rgb(c[0], c[1], c[2]),
                    None => track_color,
                };
                let color = if track.frozen {
                    // Frozen: desaturated blue-gray tint
                    egui::Color32::from_rgb(120, 140, 170)
                } else if matches!(clip.source, ClipSource::Midi { .. }) {
                    // MIDI: blend 70% track color + 30% purple (140, 100, 220)
                    egui::Color32::from_rgb(
                        ((base_color.r() as u16 * 7 + 140 * 3) / 10) as u8,
                        ((base_color.g() as u16 * 7 + 100 * 3) / 10) as u8,
                        ((base_color.b() as u16 * 7 + 220 * 3) / 10) as u8,
                    )
                } else {
                    base_color
                };

                // When collapsed, only show active (non-muted) clips, all in lane 0
                let draw_lane = if track.lanes_expanded { lane } else { 0 };
                if !track.lanes_expanded && clip.muted {
                    continue; // hide inactive takes when collapsed
                }

                let cr = make_clip_rect(&track.clips[ci], draw_lane, tracks_y_start + track_offsets[i], sample_rate, pixels_per_second, app.scroll_x, rect.min.x);

                if cr.right() < rect.min.x || cr.left() > rect.max.x {
                    continue;
                }

                let is_clip_selected = app.selected_clips.contains(&(i, ci));
                let is_clip_muted = clip.muted;

                // Background — premium 8px radius with gradient fill
                let clip_radius = 8.0;
                let draw_color = if is_clip_muted {
                    egui::Color32::from_rgb(60, 60, 68)
                } else {
                    color
                };
                let bg_alpha = if is_clip_muted {
                    0.12
                } else if is_clip_selected {
                    0.32
                } else {
                    0.22
                };
                // Selected clip: gold outer glow (4px, 15% opacity)
                if is_clip_selected && !is_clip_muted {
                    let glow_rect = cr.expand(4.0);
                    painter.rect_filled(glow_rect, clip_radius + 4.0, egui::Color32::from_rgba_premultiplied(240, 192, 64, 15));
                    let glow_rect2 = cr.expand(2.0);
                    painter.rect_filled(glow_rect2, clip_radius + 2.0, egui::Color32::from_rgba_premultiplied(240, 192, 64, 25));
                }
                // Base fill
                painter.rect_filled(cr, clip_radius, draw_color.gamma_multiply(bg_alpha));
                // Subtle gradient: top slightly lighter for depth
                if !is_clip_muted {
                    let top_grad_rect = egui::Rect::from_min_max(
                        cr.min,
                        egui::pos2(cr.max.x, cr.min.y + cr.height() * 0.35),
                    );
                    painter.rect_filled(top_grad_rect, egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 },
                        egui::Color32::from_rgba_premultiplied(255, 255, 255, 8));
                    // Thin top highlight line
                    let highlight_rect = egui::Rect::from_min_max(
                        cr.min,
                        egui::pos2(cr.max.x, cr.min.y + 1.0),
                    );
                    painter.rect_filled(highlight_rect, egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 },
                        egui::Color32::from_rgba_premultiplied(255, 255, 255, 15));
                }

                // Clip content visualization
                match &clip.source {
                    ClipSource::AudioBuffer { buffer_id } => {
                        if let Some(peaks) = app.waveform_cache.get(buffer_id) {
                            let wc = if is_clip_muted {
                                egui::Color32::from_rgb(90, 90, 90)
                            } else {
                                egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2])
                            };
                            draw_waveform(painter, &peaks, cr, clip.duration_samples, wc, clip.content_offset, app.zoom, Some((rect.min.x, rect.max.x)), clip.gain_db, app.waveform_zoom, track.volume);
                        }
                    }
                    ClipSource::Midi { notes, .. } => {
                        // Draw MIDI notes as rectangles filling the clip area
                        let sr = app.sample_rate() as f64;
                        let bpm = app.project.tempo.bpm;
                        let ticks_per_second = bpm / 60.0 * 480.0;
                        let visual_dur = clip.visual_duration_samples().max(1) as f64;

                        // Inset for visual padding
                        let pad = 2.0;
                        let inner = egui::Rect::from_min_max(
                            egui::pos2(cr.min.x + pad, cr.min.y + pad),
                            egui::pos2(cr.max.x - pad, cr.max.y - pad),
                        );

                        if notes.is_empty() {
                            // Draw a faint miniature piano roll grid pattern
                            let grid_color = egui::Color32::from_rgba_premultiplied(
                                color.r(), color.g(), color.b(), 20,
                            );
                            let grid_color_accent = egui::Color32::from_rgba_premultiplied(
                                color.r(), color.g(), color.b(), 35,
                            );
                            // Horizontal lines (pitch rows) — ~12 rows for one octave feel
                            let row_count = 12;
                            let row_h = inner.height() / row_count as f32;
                            for r in 1..row_count {
                                let y = inner.min.y + r as f32 * row_h;
                                let sc = if r % 12 == 0 { grid_color_accent } else { grid_color };
                                painter.line_segment(
                                    [egui::pos2(inner.min.x, y), egui::pos2(inner.max.x, y)],
                                    egui::Stroke::new(0.5, sc),
                                );
                            }
                            // Vertical lines (beat divisions) — 4 beats
                            let beat_count = 4;
                            let beat_w = inner.width() / beat_count as f32;
                            for b in 1..beat_count {
                                let x = inner.min.x + b as f32 * beat_w;
                                painter.line_segment(
                                    [egui::pos2(x, inner.min.y), egui::pos2(x, inner.max.y)],
                                    egui::Stroke::new(0.5, grid_color_accent),
                                );
                            }
                            // Show "MIDI" label centered
                            painter.text(
                                inner.center(),
                                egui::Align2::CENTER_CENTER,
                                "MIDI",
                                egui::FontId::proportional(10.0),
                                egui::Color32::from_rgb(120, 120, 130),
                            );
                        } else {
                            // Use full 128-note range scaled to track height, or fit to actual range
                            let min_pitch = notes.iter().map(|n| n.pitch).min().unwrap_or(60).saturating_sub(4);
                            let max_pitch = notes.iter().map(|n| n.pitch).max().unwrap_or(72).saturating_add(4);
                            let pitch_range = (max_pitch - min_pitch).max(12) as f32;
                            let note_h = (inner.height() / pitch_range).max(1.5);

                            let note_color = if is_clip_muted {
                                egui::Color32::from_rgb(80, 80, 90)
                            } else {
                                egui::Color32::from_rgb(
                                    track.color[0].saturating_add(50),
                                    track.color[1].saturating_add(50),
                                    track.color[2].saturating_add(50),
                                )
                            };
                            let note_border = if is_clip_muted {
                                egui::Color32::from_rgb(60, 60, 70)
                            } else {
                                egui::Color32::from_rgb(
                                    track.color[0].saturating_add(80),
                                    track.color[1].saturating_add(80),
                                    track.color[2].saturating_add(80),
                                )
                            };

                            for note in notes {
                                let note_start_sec = note.start_tick as f64 / ticks_per_second;
                                let note_dur_sec = note.duration_ticks as f64 / ticks_per_second;
                                let note_start_sample = note_start_sec * sr;
                                let note_dur_sample = note_dur_sec * sr;

                                let x_start = inner.min.x + (note_start_sample / visual_dur) as f32 * inner.width();
                                let x_end = inner.min.x + ((note_start_sample + note_dur_sample) / visual_dur) as f32 * inner.width();
                                let y = inner.max.y - ((note.pitch - min_pitch) as f32 / pitch_range) * inner.height();

                                if x_end > inner.min.x && x_start < inner.max.x {
                                    let nr = egui::Rect::from_min_max(
                                        egui::pos2(x_start.max(inner.min.x), (y - note_h).max(inner.min.y)),
                                        egui::pos2(x_end.min(inner.max.x), y.min(inner.max.y)),
                                    );
                                    painter.rect_filled(nr, 1.0, note_color);
                                    if note_h >= 2.0 {
                                        painter.rect_stroke(nr, 1.0, egui::Stroke::new(0.5, note_border), egui::StrokeKind::Outside);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }

                // Muted clips: diagonal hash lines pattern
                if is_clip_muted {
                    let hash_color = egui::Color32::from_rgba_premultiplied(120, 120, 120, 40);
                    let spacing = 8.0;
                    let mut hx = cr.left() - cr.height();
                    while hx < cr.right() {
                        painter.with_clip_rect(cr).line_segment(
                            [egui::pos2(hx, cr.bottom()), egui::pos2(hx + cr.height(), cr.top())],
                            egui::Stroke::new(0.8, hash_color),
                        );
                        hx += spacing;
                    }
                }

                // Border — premium: track color 30% normally, gold 100% selected
                // Frozen track overlay — subtle blue tint over the clip
                if track.frozen && !is_clip_muted {
                    painter.rect_filled(
                        cr,
                        clip_radius,
                        egui::Color32::from_rgba_premultiplied(80, 130, 220, 25),
                    );
                    // Snowflake overlay centered on clip
                    painter.with_clip_rect(cr).text(
                        cr.center(),
                        egui::Align2::CENTER_CENTER,
                        "\u{2744}",
                        egui::FontId::proportional(16.0),
                        egui::Color32::from_rgba_premultiplied(140, 190, 255, 60),
                    );
                }

                let multi_selected = is_clip_selected && app.selected_clips.len() > 1;
                let border_w = if is_clip_selected { 2.0 } else { 1.0 };
                let border_c = if multi_selected {
                    egui::Color32::from_rgb(80, 200, 190) // teal for multi-select
                } else if is_clip_selected {
                    egui::Color32::from_rgb(240, 192, 64) // gold at full opacity
                } else if is_clip_muted {
                    egui::Color32::from_rgb(46, 46, 52)
                } else {
                    // Track color at 30% opacity
                    egui::Color32::from_rgba_premultiplied(
                        color.r(), color.g(), color.b(), 77
                    )
                };
                painter.rect_stroke(
                    cr,
                    clip_radius,
                    egui::Stroke::new(border_w, border_c),
                    egui::StrokeKind::Outside,
                );

                // Chord detection overlay
                if let Some(chords) = app.detected_chords.get(&clip.id) {
                    crate::analysis_tools::draw_chord_overlay(
                        painter,
                        cr,
                        clip.id,
                        clip.duration_samples,
                        app.sample_rate(),
                        chords,
                    );
                }

                // Loop markers — white dashed lines at each loop boundary
                if clip.loop_count > 1 && !is_clip_muted {
                    let single_loop_dur = clip.single_loop_visual_duration();
                    let loop_marker_color = egui::Color32::from_rgba_premultiplied(255, 255, 255, 130);
                    for lp in 1..clip.loop_count {
                        let marker_sample = single_loop_dur * lp as u64;
                        let marker_px = (marker_sample as f64 / sample_rate) as f32 * pixels_per_second;
                        let x = cr.left() + marker_px;
                        if x > cr.left() && x < cr.right() {
                            // Draw dashed line (segments)
                            let dash_len = 4.0;
                            let gap_len = 3.0;
                            let mut dy = cr.top();
                            while dy < cr.bottom() {
                                let dash_end = (dy + dash_len).min(cr.bottom());
                                painter.with_clip_rect(cr).line_segment(
                                    [egui::pos2(x, dy), egui::pos2(x, dash_end)],
                                    egui::Stroke::new(1.0, loop_marker_color),
                                );
                                dy += dash_len + gap_len;
                            }
                            // Small loop repeat indicator at top
                            painter.with_clip_rect(cr).text(
                                egui::pos2(x + 2.0, cr.top() + 1.0),
                                egui::Align2::LEFT_TOP,
                                "\u{21BB}",
                                egui::FontId::proportional(8.0),
                                loop_marker_color,
                            );
                        }
                    }
                }

                // Fade in/out visual overlays (triangular semi-transparent regions)
                if !is_clip_muted {
                    let fade_color = egui::Color32::from_rgba_premultiplied(0, 0, 0, 60);
                    let fade_line_color = egui::Color32::from_rgba_premultiplied(255, 255, 255, 100);

                    // Fade in overlay
                    if clip.fade_in_samples > 0 {
                        let fade_in_px = (clip.fade_in_samples as f64 / sample_rate) as f32 * pixels_per_second;
                        let fade_in_px = fade_in_px.min(cr.width());
                        // Dark triangle: top-left corner to fade_in point at bottom
                        // This represents the "silenced" portion being faded in
                        let p1 = egui::pos2(cr.left(), cr.top());      // top-left
                        let p2 = egui::pos2(cr.left(), cr.bottom());   // bottom-left
                        let p3 = egui::pos2(cr.left() + fade_in_px, cr.top()); // fade end at top
                        painter.with_clip_rect(cr).add(egui::Shape::convex_polygon(
                            vec![p1, p2, p3],
                            fade_color,
                            egui::Stroke::NONE,
                        ));
                        // Diagonal fade line
                        painter.with_clip_rect(cr).line_segment(
                            [egui::pos2(cr.left(), cr.bottom()), egui::pos2(cr.left() + fade_in_px, cr.top())],
                            egui::Stroke::new(1.0, fade_line_color),
                        );
                    }

                    // Fade out overlay
                    if clip.fade_out_samples > 0 {
                        let fade_out_px = (clip.fade_out_samples as f64 / sample_rate) as f32 * pixels_per_second;
                        let fade_out_px = fade_out_px.min(cr.width());
                        // Dark triangle: top-right corner to fade_out point at bottom
                        let p1 = egui::pos2(cr.right(), cr.top());       // top-right
                        let p2 = egui::pos2(cr.right(), cr.bottom());    // bottom-right
                        let p3 = egui::pos2(cr.right() - fade_out_px, cr.top()); // fade start at top
                        painter.with_clip_rect(cr).add(egui::Shape::convex_polygon(
                            vec![p1, p2, p3],
                            fade_color,
                            egui::Stroke::NONE,
                        ));
                        // Diagonal fade line
                        painter.with_clip_rect(cr).line_segment(
                            [egui::pos2(cr.right() - fade_out_px, cr.top()), egui::pos2(cr.right(), cr.bottom())],
                            egui::Stroke::new(1.0, fade_line_color),
                        );
                    }

                    // Fade handles (small squares at top corners when hovering)
                    if let Some(hover_pos) = ui.input(|inp| inp.pointer.hover_pos()) {
                        if cr.contains(hover_pos) {
                            let handle_size = 6.0;
                            let handle_zone = 20.0;

                            // Fade in handle (top-left area)
                            let fade_in_px = (clip.fade_in_samples as f64 / sample_rate) as f32 * pixels_per_second;
                            let fi_handle_x = cr.left() + fade_in_px;
                            if (hover_pos.x - fi_handle_x).abs() < handle_zone && (hover_pos.y - cr.top()) < handle_zone {
                                let handle_rect = egui::Rect::from_center_size(
                                    egui::pos2(fi_handle_x, cr.top() + handle_size),
                                    egui::vec2(handle_size * 2.0, handle_size * 2.0),
                                );
                                painter.rect_filled(handle_rect, 2.0, egui::Color32::from_rgb(255, 200, 80));
                                painter.rect_stroke(handle_rect, 2.0, egui::Stroke::new(1.0, egui::Color32::WHITE), egui::StrokeKind::Outside);
                            }

                            // Fade out handle (top-right area)
                            let fade_out_px = (clip.fade_out_samples as f64 / sample_rate) as f32 * pixels_per_second;
                            let fo_handle_x = cr.right() - fade_out_px;
                            if (hover_pos.x - fo_handle_x).abs() < handle_zone && (hover_pos.y - cr.top()) < handle_zone {
                                let handle_rect = egui::Rect::from_center_size(
                                    egui::pos2(fo_handle_x, cr.top() + handle_size),
                                    egui::vec2(handle_size * 2.0, handle_size * 2.0),
                                );
                                painter.rect_filled(handle_rect, 2.0, egui::Color32::from_rgb(255, 200, 80));
                                painter.rect_stroke(handle_rect, 2.0, egui::Stroke::new(1.0, egui::Color32::WHITE), egui::StrokeKind::Outside);
                            }
                        }
                    }
                }

                // Clip label — rounded pill tag at top-left with transparency
                // When clip is too narrow, show just the first letter
                let zoomed_out = app.zoom < 0.3;
                let speed_suffix = if (clip.playback_rate - 1.0).abs() > 0.01 {
                    format!(" [{:.2}x]", clip.playback_rate)
                } else {
                    String::new()
                };
                let transpose_suffix = if clip.transpose_semitones != 0 {
                    let sign = if clip.transpose_semitones > 0 { "+" } else { "" };
                    format!(" [{}{}st]", sign, clip.transpose_semitones)
                } else {
                    String::new()
                };
                let rev_suffix = if clip.reversed { " REV" } else { "" };
                let full_label = if is_clip_muted {
                    format!("{} (inactive){}{}{}", clip.name, speed_suffix, transpose_suffix, rev_suffix)
                } else {
                    format!("{}{}{}{}", clip.name, speed_suffix, transpose_suffix, rev_suffix)
                };
                // Determine whether to show full name or abbreviated
                let min_width_for_full = full_label.len() as f32 * 5.5 + 12.0;
                let label = if cr.width() < 24.0 {
                    String::new() // too narrow for anything
                } else if cr.width() < min_width_for_full {
                    clip.name.chars().next().map(|c| c.to_string()).unwrap_or_default()
                } else {
                    full_label.clone()
                };
                let text_color = if is_clip_muted {
                    egui::Color32::from_rgb(120, 120, 128)
                } else {
                    egui::Color32::from_rgb(242, 240, 236)
                };
                let font_size = if zoomed_out { 12.0 } else { 10.5 };
                // Glassmorphism pill tag behind clip name
                if !label.is_empty() {
                    if !is_clip_muted {
                        let tag_w = (label.len() as f32 * (if zoomed_out { 6.5 } else { 5.8 }) + 12.0).min(cr.width() - 6.0);
                        let tag_h = if zoomed_out { 18.0 } else { 16.0 };
                        let tag_rect = egui::Rect::from_min_size(
                            egui::pos2(cr.left() + 3.0, cr.top() + 3.0),
                            egui::vec2(tag_w, tag_h),
                        );
                        // Glassmorphism: semi-transparent background
                        painter.with_clip_rect(cr).rect_filled(tag_rect, 12.0, egui::Color32::from_rgba_premultiplied(0, 0, 0, if zoomed_out { 140 } else { 100 }));
                        // Subtle border for glass effect
                        painter.with_clip_rect(cr).rect_stroke(tag_rect, 12.0, egui::Stroke::new(0.5, egui::Color32::from_rgba_premultiplied(255, 255, 255, 20)), egui::StrokeKind::Outside);
                    }
                    painter.with_clip_rect(cr.shrink(2.0)).text(
                        egui::pos2(cr.left() + 7.0, cr.top() + 4.0),
                        egui::Align2::LEFT_TOP,
                        &label,
                        egui::FontId::proportional(font_size),
                        text_color,
                    );
                }

                // Slip edit indicator: show content offset when non-zero
                if clip.content_offset > 0 && !is_clip_muted {
                    let offset_label = format!("offset: {}s", clip.content_offset as f64 / sample_rate);
                    painter.with_clip_rect(cr).text(
                        egui::pos2(cr.left() + 5.0, cr.bottom() - 11.0),
                        egui::Align2::LEFT_TOP,
                        offset_label,
                        egui::FontId::proportional(8.0),
                        egui::Color32::from_rgb(180, 180, 200),
                    );
                }

                // Active indicator dot
                if !is_clip_muted && num_lanes > 1 {
                    painter.circle_filled(
                        egui::pos2(cr.right() - 8.0, cr.center().y),
                        3.0,
                        egui::Color32::from_rgb(80, 220, 80),
                    );
                }

                // --- Clip gain handle (Reaper-style: always-visible circle at top-right) ---
                if !is_clip_muted {
                    let clip_gain = clip.gain_db;
                    let gain_range = 24.0_f32;

                    // Gain handle is a small circle at top-right of the clip
                    // Y position maps gain: center = 0dB, top = +24dB, bottom = -24dB
                    let handle_area_top = cr.top() + 6.0;
                    let handle_area_bot = cr.top() + cr.height().min(40.0);
                    let handle_area_center = (handle_area_top + handle_area_bot) * 0.5;
                    let handle_y = handle_area_center - (clip_gain / gain_range) * (handle_area_bot - handle_area_top) * 0.5;
                    let handle_y = handle_y.clamp(handle_area_top, handle_area_bot);
                    let handle_x = cr.right() - 22.0;
                    let handle_radius = 4.5;

                    let hovering_gain = if let Some(hover_pos) = ui.input(|inp| inp.pointer.hover_pos()) {
                        let dx = hover_pos.x - handle_x;
                        let dy = hover_pos.y - handle_y;
                        (dx * dx + dy * dy).sqrt() < 10.0
                    } else {
                        false
                    };

                    // Always show the gain circle
                    let circle_color = if clip_gain > 0.01 {
                        egui::Color32::from_rgb(255, 180, 60) // orange for boost
                    } else if clip_gain < -0.01 {
                        egui::Color32::from_rgb(100, 180, 255) // blue for cut
                    } else {
                        egui::Color32::from_rgb(140, 140, 150) // gray for 0dB
                    };

                    let alpha = if hovering_gain { 1.0 } else { 0.7 };
                    let radius = if hovering_gain { handle_radius + 1.5 } else { handle_radius };

                    // Draw gain line across clip
                    if clip_gain.abs() > 0.01 || hovering_gain {
                        painter.with_clip_rect(cr).line_segment(
                            [egui::pos2(cr.left() + 4.0, handle_y), egui::pos2(cr.right() - 4.0, handle_y)],
                            egui::Stroke::new(if hovering_gain { 1.5 } else { 1.0 }, circle_color.gamma_multiply(alpha * 0.5)),
                        );
                    }

                    // Draw the circle handle
                    painter.with_clip_rect(cr).circle_filled(
                        egui::pos2(handle_x, handle_y),
                        radius,
                        circle_color.gamma_multiply(alpha),
                    );
                    if hovering_gain {
                        painter.with_clip_rect(cr).circle_stroke(
                            egui::pos2(handle_x, handle_y),
                            radius + 1.0,
                            egui::Stroke::new(1.0, egui::Color32::WHITE.gamma_multiply(0.5)),
                        );
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    }

                    if clip_gain.abs() > 0.01 {
                        let gain_label = format!("{:+.1} dB", clip_gain);
                        painter.with_clip_rect(cr).text(
                            egui::pos2(cr.right() - 4.0, handle_y - 2.0),
                            egui::Align2::RIGHT_BOTTOM,
                            gain_label,
                            egui::FontId::proportional(8.0),
                            egui::Color32::from_rgb(200, 200, 210),
                        );
                    }
                }
            }
        }

        // Empty state — premium welcome with radial glow and pill buttons
        {
            let has_any_clips = app.project.tracks.iter().any(|t| !t.clips.is_empty());
            if !has_any_clips && !app.project.tracks.is_empty() {
                let center = egui::pos2(
                    (rect.min.x + rect.max.x) * 0.5,
                    tracks_y_start + (rect.max.y - tracks_y_start) * 0.5,
                );

                // Radial gradient background glow — layered circles
                painter.circle_filled(center, 120.0, egui::Color32::from_rgba_premultiplied(240, 192, 64, 4));
                painter.circle_filled(center, 90.0, egui::Color32::from_rgba_premultiplied(240, 192, 64, 6));
                painter.circle_filled(center, 60.0, egui::Color32::from_rgba_premultiplied(240, 192, 64, 8));

                // Large musical note icon (60px)
                painter.text(
                    egui::pos2(center.x, center.y - 30.0),
                    egui::Align2::CENTER_CENTER,
                    "\u{266A}",
                    egui::FontId::proportional(60.0),
                    egui::Color32::from_rgba_premultiplied(240, 192, 64, 50),
                );

                // "Start creating" message (18px)
                painter.text(
                    egui::pos2(center.x, center.y + 20.0),
                    egui::Align2::CENTER_CENTER,
                    "Start creating",
                    egui::FontId::proportional(18.0),
                    egui::Color32::from_rgb(180, 178, 190),
                );

                // Subtitle
                painter.text(
                    egui::pos2(center.x, center.y + 42.0),
                    egui::Align2::CENTER_CENTER,
                    "Drop audio files here or use the buttons below",
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(90, 88, 100),
                );

                // Pill buttons: "Import Audio" and "Record"
                let btn_y = center.y + 66.0;
                let import_w = 110.0;
                let record_w = 90.0;
                let gap = 12.0;
                let total_w = import_w + record_w + gap;
                let start_x = center.x - total_w / 2.0;

                // Import Audio pill
                let import_rect = egui::Rect::from_min_size(
                    egui::pos2(start_x, btn_y),
                    egui::vec2(import_w, 30.0),
                );
                painter.rect_filled(import_rect, 15.0, egui::Color32::from_rgb(38, 40, 52));
                painter.rect_stroke(import_rect, 15.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 60, 75)), egui::StrokeKind::Outside);
                painter.text(
                    import_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "\u{1F4C2}  Import Audio",
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(180, 180, 200),
                );

                // Record pill
                let record_rect = egui::Rect::from_min_size(
                    egui::pos2(start_x + import_w + gap, btn_y),
                    egui::vec2(record_w, 30.0),
                );
                painter.rect_filled(record_rect, 15.0, egui::Color32::from_rgb(55, 30, 34));
                painter.rect_stroke(record_rect, 15.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 40, 45)), egui::StrokeKind::Outside);
                painter.text(
                    record_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "\u{23FA}  Record",
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(220, 100, 100),
                );
            }
        }

        // Loop region — skip fill if selection covers the same area (avoid double overlay)
        let selection_matches_loop = if let (Some(s), Some(e)) = (app.selection_start, app.selection_end) {
            let s1 = s.min(e);
            let s2 = s.max(e);
            s1 == app.loop_start && s2 == app.loop_end
        } else {
            false
        };
        if app.loop_enabled && app.loop_end > app.loop_start && !selection_matches_loop {
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
                egui::Color32::from_rgba_premultiplied(60, 100, 200, 2),
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

        // Selection range
        if let (Some(sel_s), Some(sel_e)) = (app.selection_start, app.selection_end) {
            let s1 = sel_s.min(sel_e);
            let s2 = sel_s.max(sel_e);
            let sx1 = rect.min.x + (s1 as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
            let sx2 = rect.min.x + (s2 as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
            let sel_rect = egui::Rect::from_min_max(
                egui::pos2(sx1.max(rect.min.x), rect.min.y),
                egui::pos2(sx2.min(rect.max.x), rect.max.y),
            );

            // Punch region indicator — distinct color when punch recording is enabled
            if app.punch_recording {
                painter.rect_filled(
                    sel_rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(255, 60, 60, 10),
                );
                // Punch region border
                painter.rect_stroke(
                    sel_rect,
                    0.0,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(220, 70, 70)),
                    egui::StrokeKind::Outside,
                );
                // "PUNCH IN" / "PUNCH OUT" labels at edges
                if sx1 >= rect.min.x && sx1 <= rect.max.x {
                    painter.text(
                        egui::pos2(sx1 + 2.0, rect.min.y + 2.0),
                        egui::Align2::LEFT_TOP,
                        "IN",
                        egui::FontId::proportional(9.0),
                        egui::Color32::from_rgb(255, 100, 100),
                    );
                }
                if sx2 >= rect.min.x && sx2 <= rect.max.x {
                    painter.text(
                        egui::pos2(sx2 - 2.0, rect.min.y + 2.0),
                        egui::Align2::RIGHT_TOP,
                        "OUT",
                        egui::FontId::proportional(9.0),
                        egui::Color32::from_rgb(255, 100, 100),
                    );
                }
            } else {
                painter.rect_filled(
                    sel_rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(80, 130, 255, 3),
                );
            }

            // Selection edges — thicker lines that serve as drag handles
            let edge_color = if app.punch_recording {
                egui::Color32::from_rgb(220, 70, 70)
            } else {
                egui::Color32::from_rgb(100, 150, 255)
            };
            let hover_pos = ui.ctx().pointer_hover_pos();
            for (idx, sx) in [sx1, sx2].iter().enumerate() {
                if *sx >= rect.min.x && *sx <= rect.max.x {
                    // Check if mouse is near this edge (for drag handle highlight)
                    // Only highlight when cursor is directly on the edge line (2px)
                    let near_edge = hover_pos.map_or(false, |p| {
                        (p.x - sx).abs() < 2.0 && p.y >= rect.min.y && p.y <= rect.max.y
                    });
                    let dragging_this = app.dragging_selection_edge == (idx as u8 + 1);
                    let thickness = if near_edge || dragging_this { 3.0 } else { 1.0 };
                    let color = if near_edge || dragging_this {
                        egui::Color32::from_rgb(180, 210, 255)
                    } else {
                        edge_color
                    };
                    painter.line_segment(
                        [egui::pos2(*sx, rect.min.y), egui::pos2(*sx, rect.max.y)],
                        egui::Stroke::new(thickness, color),
                    );
                    // Draw small triangular handle at top of edge
                    if near_edge || dragging_this {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                        let tri_size = 6.0;
                        let tri_y = rect.min.y;
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                egui::pos2(*sx, tri_y),
                                egui::pos2(sx - tri_size, tri_y + tri_size),
                                egui::pos2(sx + tri_size, tri_y + tri_size),
                            ],
                            color,
                            egui::Stroke::NONE,
                        ));
                    }
                }
            }
        }

        // Automation curves (when visible)
        if app.show_automation {
            let param = app.automation_param.clone();
            let (min_val, max_val) = param.range();

            for (i, track) in app.project.tracks.iter().enumerate() {
                if is_track_collapsed(app, i) {
                    continue;
                }
                let t_y = tracks_y_start + track_offsets[i];
                let t_h = track_height(track, app.track_height_zoom);

                if let Some(lane) = track.automation.iter().find(|l| l.parameter == param) {
                    if lane.points.len() >= 2 {
                        // Draw automation line
                        let points: Vec<egui::Pos2> = lane
                            .points
                            .iter()
                            .map(|p| {
                                let x = rect.min.x
                                    + (p.sample as f64 / sample_rate) as f32 * pixels_per_second
                                    - app.scroll_x;
                                let norm = (p.value - min_val) / (max_val - min_val);
                                let y = t_y + t_h * (1.0 - norm);
                                egui::pos2(x, y)
                            })
                            .collect();

                        for w in points.windows(2) {
                            painter.line_segment(
                                [w[0], w[1]],
                                egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 180, 50)),
                            );
                        }
                    }

                    // Draw automation points as dots
                    for p in &lane.points {
                        let x = rect.min.x
                            + (p.sample as f64 / sample_rate) as f32 * pixels_per_second
                            - app.scroll_x;
                        let norm = (p.value - min_val) / (max_val - min_val);
                        let y = t_y + t_h * (1.0 - norm);
                        if x >= rect.min.x && x <= rect.max.x {
                            painter.circle_filled(
                                egui::pos2(x, y),
                                4.0,
                                egui::Color32::from_rgb(255, 200, 50),
                            );
                            painter.circle_stroke(
                                egui::pos2(x, y),
                                4.0,
                                egui::Stroke::new(1.0, egui::Color32::WHITE),
                            );
                        }
                    }
                }
            }
        }

        // Cursor feedback — determine cursor icon based on what the mouse hovers over.
        // Only apply when no active drag/trim operation is in progress.
        if app.trimming_clip.is_none() && app.dragging_clip.is_none() && app.dragging_fade.is_none()
            && app.dragging_clips.is_none() && app.dragging_clip_gain.is_none()
            && app.stretching_clip.is_none() && app.slip_editing.is_none()
        {
            if let Some(hover_pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(hover_pos) && hover_pos.y > tracks_y_start {
                    let edge_zone = 8.0;
                    let fade_handle_zone = 20.0;

                    // Scan all clips once, determine what the cursor is over
                    let mut cursor_result = None; // None = not determined yet
                    for &(ti, ci, _, cr) in &clip_rects {
                        if !cr.contains(hover_pos) {
                            continue;
                        }
                        // Highest priority: fade handles (top corners)
                        if (hover_pos.y - cr.top()) < fade_handle_zone {
                            let clip = &app.project.tracks[ti].clips[ci];
                            let fade_in_px = (clip.fade_in_samples as f64 / sample_rate) as f32 * pixels_per_second;
                            let fi_handle_x = cr.left() + fade_in_px;
                            let fade_out_px = (clip.fade_out_samples as f64 / sample_rate) as f32 * pixels_per_second;
                            let fo_handle_x = cr.right() - fade_out_px;
                            if (hover_pos.x - fi_handle_x).abs() < fade_handle_zone
                                || (hover_pos.x - fo_handle_x).abs() < fade_handle_zone
                            {
                                cursor_result = Some(egui::CursorIcon::ResizeHorizontal);
                                break;
                            }
                        }
                        // Trim edges
                        if (hover_pos.x - cr.left()).abs() < edge_zone
                            || (hover_pos.x - cr.right()).abs() < edge_zone
                        {
                            cursor_result = Some(egui::CursorIcon::ResizeHorizontal);
                            break;
                        }
                        // Over clip body — grab cursor
                        cursor_result = Some(egui::CursorIcon::Grab);
                        break;
                    }

                    // If no clip was hit, check track separator hover
                    if cursor_result.is_none() && app.dragging_separator.is_none() {
                        let sep_zone = 4.0;
                        for (i, track) in app.project.tracks.iter().enumerate() {
                            if is_track_collapsed(app, i) {
                                continue;
                            }
                            let sep_y = tracks_y_start + track_offsets[i] + track_height(track, app.track_height_zoom);
                            if (hover_pos.y - sep_y).abs() < sep_zone && hover_pos.x >= rect.min.x && hover_pos.x <= rect.max.x {
                                cursor_result = Some(egui::CursorIcon::ResizeVertical);
                                break;
                            }
                        }
                    }

                    // If still nothing, we are over empty timeline area — crosshair
                    let cursor = cursor_result.unwrap_or(egui::CursorIcon::Crosshair);
                    ui.ctx().set_cursor_icon(cursor);
                }
            }
        }

        // Track separator drag — resize track height by dragging separator line
        if app.dragging_separator.is_some() {
            if response.dragged_by(egui::PointerButton::Primary) {
                if let Some(pos) = response.interact_pointer_pos {
                    if let Some(ti) = app.dragging_separator {
                        if ti < app.project.tracks.len() {
                            let track_top = tracks_y_start + track_offsets[ti];
                            let new_height = (pos.y - track_top).clamp(30.0, 300.0);
                            app.project.tracks[ti].custom_height = new_height / app.track_height_zoom;
                        }
                    }
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                }
            }
            if response.drag_stopped() {
                app.dragging_separator = None;
            }
        }

        // Markers on ruler — interactive: click to jump, right-click to rename/delete, drag to move
        {
            let marker_hit_zone = 10.0;

            // Collect marker screen positions for hit testing
            let marker_positions: Vec<(usize, f32)> = app.project.markers.iter().enumerate().map(|(mi, marker)| {
                let mx = rect.min.x + (marker.sample as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                (mi, mx)
            }).collect();

            // Handle active marker drag
            if let Some(drag_idx) = app.dragging_marker {
                if response.dragged_by(egui::PointerButton::Primary) {
                    if let Some(pos) = response.interact_pointer_pos {
                        let x_offset = pos.x - rect.min.x + app.scroll_x;
                        let seconds = x_offset as f64 / pixels_per_second as f64;
                        let new_sample = (seconds * sample_rate).max(0.0) as u64;
                        let snapped = app.snap_position(new_sample);
                        if drag_idx < app.project.markers.len() {
                            app.project.markers[drag_idx].sample = snapped;
                        }
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                }
                if response.drag_stopped() {
                    app.dragging_marker = None;
                }
            }

            // Check for hover/click on markers in the ruler
            if app.dragging_marker.is_none() {
                if let Some(hover_pos) = ui.input(|i| i.pointer.hover_pos()) {
                    if hover_pos.y >= ruler_rect.min.y && hover_pos.y <= ruler_rect.max.y {
                        for &(mi, mx) in &marker_positions {
                            if (hover_pos.x - mx).abs() < marker_hit_zone {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);

                                // Left click: jump playhead to marker position
                                if response.clicked_by(egui::PointerButton::Primary) {
                                    let sample = app.project.markers[mi].sample;
                                    app.send_command(jamhub_engine::EngineCommand::SetPosition(sample));
                                    app.set_status(&format!("Jumped to: {}", app.project.markers[mi].name));
                                }

                                // Begin marker drag
                                if response.drag_started_by(egui::PointerButton::Primary) {
                                    app.push_undo("Move marker");
                                    app.dragging_marker = Some(mi);
                                }

                                break;
                            }
                        }
                    }
                }
            }

            // Right-click on ruler: marker rename if on a marker, or context menu to add marker/tempo change
            if response.secondary_clicked() {
                if let Some(click_pos) = response.interact_pointer_pos {
                    if click_pos.y >= ruler_rect.min.y && click_pos.y <= ruler_rect.max.y {
                        let mut on_marker = false;
                        for &(mi, mx) in &marker_positions {
                            if (click_pos.x - mx).abs() < marker_hit_zone {
                                app.renaming_marker = Some((mi, app.project.markers[mi].name.clone()));
                                on_marker = true;
                                break;
                            }
                        }
                        // Right-click on empty ruler area: store position for context menu
                        if !on_marker {
                            let x_offset = click_pos.x - rect.min.x + app.scroll_x;
                            let seconds = x_offset as f64 / pixels_per_second as f64;
                            let sample = (seconds * sample_rate).max(0.0) as u64;
                            app.ruler_context_sample = Some(sample);
                        }
                    }
                }
            }

            // Ruler context menu popup (Add Marker / Add Tempo Change)
            if app.ruler_context_sample.is_some() {
                let popup_id = egui::Id::new("ruler_context_menu");
                ui.memory_mut(|mem| mem.open_popup(popup_id));
                egui::popup::popup_above_or_below_widget(ui, popup_id, &response, egui::AboveOrBelow::Below, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                    ui.set_min_width(160.0);
                    if ui.button("Add Marker").clicked() {
                        if let Some(sample) = app.ruler_context_sample.take() {
                            app.push_undo("Add marker");
                            let marker_count = app.project.markers.len() + 1;
                            app.project.markers.push(jamhub_model::Marker {
                                id: uuid::Uuid::new_v4(),
                                name: format!("Marker {}", marker_count),
                                sample: app.snap_position(sample),
                                color: [255, 200, 50],
                            });
                            app.set_status("Marker added");
                        }
                    }
                    ui.separator();
                    if ui.button("Add Tempo Change").clicked() {
                        if let Some(sample) = app.ruler_context_sample.take() {
                            let current_bpm = app.project.tempo_map.bpm_at(sample, app.project.tempo.bpm);
                            app.tempo_change_input = Some(crate::TempoChangeInput {
                                sample: app.snap_position(sample),
                                bpm_text: format!("{:.1}", current_bpm),
                            });
                        }
                    }
                    if !app.project.tempo_map.changes.is_empty() {
                        ui.separator();
                        if ui.button("Clear All Tempo Changes").clicked() {
                            app.push_undo("Clear tempo changes");
                            app.project.tempo_map.changes.clear();
                            app.set_status("Tempo changes cleared");
                            app.ruler_context_sample = None;
                        }
                    }
                });
                // Close context menu if nothing was clicked
                if !ui.memory(|mem| mem.is_popup_open(popup_id)) {
                    app.ruler_context_sample = None;
                }
            }

            // Draw markers
            for (mi, marker) in app.project.markers.iter().enumerate() {
                let mx = rect.min.x + (marker.sample as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                if mx >= rect.min.x - 20.0 && mx <= rect.max.x + 20.0 {
                    let mc = egui::Color32::from_rgb(marker.color[0], marker.color[1], marker.color[2]);
                    let is_being_dragged = app.dragging_marker == Some(mi);

                    // Vertical line through timeline
                    painter.line_segment(
                        [egui::pos2(mx, rect.min.y), egui::pos2(mx, rect.max.y)],
                        egui::Stroke::new(
                            if is_being_dragged { 1.5 } else { 1.0 },
                            mc.gamma_multiply(if is_being_dragged { 0.7 } else { 0.4 }),
                        ),
                    );

                    // Triangle flag on ruler
                    let tri = if is_being_dragged { 6.0 } else { 5.0 };
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(mx, ruler_rect.min.y),
                            egui::pos2(mx - tri, ruler_rect.min.y + tri * 1.5),
                            egui::pos2(mx + tri, ruler_rect.min.y + tri * 1.5),
                        ],
                        mc,
                        if is_being_dragged { egui::Stroke::new(1.0, egui::Color32::WHITE) } else { egui::Stroke::NONE },
                    ));

                    // Marker name label
                    let label_color = if is_being_dragged { egui::Color32::WHITE } else { mc };
                    painter.text(
                        egui::pos2(mx + 3.0, ruler_rect.min.y + 1.0),
                        egui::Align2::LEFT_TOP,
                        &marker.name,
                        egui::FontId::proportional(9.0),
                        label_color,
                    );
                }
            }
        }

        // Locators — numbered position markers on the ruler
        for (li, locator) in app.locators.iter().enumerate() {
            if let Some(pos) = locator {
                let lx = rect.min.x + (*pos as f64 / sample_rate) as f32 * pixels_per_second - app.scroll_x;
                if lx >= rect.min.x - 10.0 && lx <= rect.max.x + 10.0 {
                    let loc_color = egui::Color32::from_rgb(240, 192, 64); // amber/gold

                    // Thin dashed line through timeline
                    painter.line_segment(
                        [egui::pos2(lx, ruler_rect.max.y), egui::pos2(lx, rect.max.y)],
                        egui::Stroke::new(0.5, loc_color.gamma_multiply(0.3)),
                    );

                    // Number badge on ruler
                    let badge_size = 10.0;
                    let badge_rect = egui::Rect::from_min_size(
                        egui::pos2(lx - badge_size * 0.5, ruler_rect.max.y - badge_size - 2.0),
                        egui::vec2(badge_size, badge_size),
                    );
                    painter.rect_filled(badge_rect, 3.0, loc_color);
                    painter.text(
                        badge_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{}", li + 1),
                        egui::FontId::proportional(7.0),
                        egui::Color32::from_rgb(20, 20, 24),
                    );
                }
            }
        }

        // Marker rename/delete popup window
        if let Some((rename_mi, ref _buf)) = app.renaming_marker {
            if rename_mi < app.project.markers.len() {
                let mut open = true;
                egui::Window::new("Marker")
                    .collapsible(false)
                    .resizable(false)
                    .open(&mut open)
                    .fixed_size(egui::vec2(200.0, 70.0))
                    .show(ui.ctx(), |ui| {
                        let mut buf = if let Some((_, ref b)) = app.renaming_marker {
                            b.clone()
                        } else {
                            String::new()
                        };
                        let r = ui.text_edit_singleline(&mut buf);
                        app.renaming_marker = Some((rename_mi, buf.clone()));
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if !buf.is_empty() {
                                app.project.markers[rename_mi].name = buf.clone();
                            }
                            app.renaming_marker = None;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("OK").clicked() {
                                if !buf.is_empty() {
                                    app.project.markers[rename_mi].name = buf.clone();
                                }
                                app.renaming_marker = None;
                            }
                            if ui.button("Delete").clicked() {
                                app.push_undo("Delete marker");
                                app.project.markers.remove(rename_mi);
                                app.renaming_marker = None;
                            }
                        });
                    });
                if !open {
                    app.renaming_marker = None;
                }
            } else {
                app.renaming_marker = None;
            }
        }

        // Rubber-band selection rectangle
        if app.rubber_band_active {
            if let Some(origin) = app.rubber_band_origin {
                if let Some(current) = ui.input(|i| i.pointer.hover_pos()) {
                    let rb_rect = egui::Rect::from_two_pos(origin, current);
                    painter.rect_filled(
                        rb_rect,
                        0.0,
                        egui::Color32::from_rgba_premultiplied(100, 180, 255, 6),
                    );
                    painter.rect_stroke(
                        rb_rect,
                        0.0,
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 180, 255)),
                        egui::StrokeKind::Outside,
                    );
                }
            }
        }

        // Magnetic snap indicator — bright line when clip edge snaps to grid
        if app.magnetic_snap_active && (app.dragging_clip.is_some() || app.dragging_clips.is_some()) {
            let snap_sec = app.magnetic_snap_sample as f64 / sample_rate;
            let snap_x = rect.min.x + snap_sec as f32 * pixels_per_second - app.scroll_x;
            if snap_x >= rect.min.x && snap_x <= rect.max.x {
                painter.line_segment(
                    [egui::pos2(snap_x, rect.min.y), egui::pos2(snap_x, rect.max.y)],
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 220, 255)),
                );
            }
        }

        // Clip edge snap indicator — bright green line when snapping to another clip's edge
        if let Some(edge_sample) = app.clip_edge_snap_sample {
            if app.dragging_clip.is_some() || app.dragging_clips.is_some() {
                let snap_sec = edge_sample as f64 / sample_rate;
                let snap_x = rect.min.x + snap_sec as f32 * pixels_per_second - app.scroll_x;
                if snap_x >= rect.min.x && snap_x <= rect.max.x {
                    painter.line_segment(
                        [egui::pos2(snap_x, rect.min.y), egui::pos2(snap_x, rect.max.y)],
                        egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 255, 120)),
                    );
                }
            }
        }

        // Ruler hover preview line — shows where the playhead would go on click
        {
            app.ruler_hover_sample = None;
            if let Some(hover_pos) = ui.input(|i| i.pointer.hover_pos()) {
                if hover_pos.y >= ruler_rect.min.y && hover_pos.y <= ruler_rect.max.y
                    && hover_pos.x >= rect.min.x && hover_pos.x <= rect.max.x
                {
                    let x_offset = hover_pos.x - rect.min.x + app.scroll_x;
                    let seconds = x_offset as f64 / pixels_per_second as f64;
                    let sample = (seconds * sample_rate).max(0.0) as u64;
                    app.ruler_hover_sample = Some(sample);

                    // Draw the preview line (dimmed)
                    let preview_x = hover_pos.x;
                    painter.line_segment(
                        [egui::pos2(preview_x, ruler_rect.max.y), egui::pos2(preview_x, rect.max.y)],
                        egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(255, 80, 80, 80)),
                    );
                    // Small triangle at ruler bottom
                    let tri = 4.0;
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(preview_x, ruler_rect.max.y),
                            egui::pos2(preview_x - tri, ruler_rect.max.y - tri),
                            egui::pos2(preview_x + tri, ruler_rect.max.y - tri),
                        ],
                        egui::Color32::from_rgba_premultiplied(255, 80, 80, 80),
                        egui::Stroke::NONE,
                    ));
                }
            }
        }

        // Playhead — crisp 1px line with half-pixel offset for anti-aliased rendering
        let pos = app.position_samples();
        let pos_sec = pos as f64 / sample_rate;
        let playhead_x_raw =
            rect.min.x + pos_sec as f32 * pixels_per_second - app.scroll_x;
        // Snap to half-pixel for crisp 1px rendering on retina/non-retina displays
        let playhead_x = (playhead_x_raw * 2.0).round() / 2.0;
        let playhead_color = egui::Color32::from_rgb(245, 190, 50); // bright amber/gold

        if playhead_x >= rect.min.x && playhead_x <= rect.max.x {
            painter.line_segment(
                [
                    egui::pos2(playhead_x, rect.min.y),
                    egui::pos2(playhead_x, rect.max.y),
                ],
                egui::Stroke::new(1.0, playhead_color),
            );
            // Subtle glow for visibility against both light and dark clip backgrounds
            painter.line_segment(
                [
                    egui::pos2(playhead_x, rect.min.y),
                    egui::pos2(playhead_x, rect.max.y),
                ],
                egui::Stroke::new(3.0, egui::Color32::from_rgba_premultiplied(245, 190, 50, 40)),
            );
            let tri = 6.0;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(playhead_x, ruler_rect.max.y),
                    egui::pos2(playhead_x - tri, ruler_rect.max.y - tri),
                    egui::pos2(playhead_x + tri, ruler_rect.max.y - tri),
                ],
                playhead_color,
                egui::Stroke::NONE,
            ));
        }

        // Auto-follow playhead during playback
        if app.follow_playhead && !app.user_scrolling
            && app.transport_state() == jamhub_model::TransportState::Playing
        {
            let playhead_px = pos_sec as f32 * pixels_per_second;
            let view_left = app.scroll_x;

            // Smooth interpolation factor — frame-rate independent
            // At 60fps: 0.12 per frame gives ~7 frame catch-up. At 30fps: faster per frame.
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            let smooth = 1.0 - (-8.0 * dt).exp(); // exponential smoothing (~8 Hz bandwidth)

            // If playhead moves past 75% of visible area, scroll to keep it at 25% from left
            if playhead_px > view_left + available.x * 0.75 {
                let target = playhead_px - available.x * 0.25;
                app.scroll_x += (target - app.scroll_x) * smooth;
            } else if playhead_px < view_left {
                // Playhead went behind viewport (e.g. loop restart) — smooth transition
                let target = (playhead_px - available.x * 0.1).max(0.0);
                // Use faster smoothing for large jumps to avoid long catch-up
                let jump_smooth = 1.0 - (-15.0 * dt).exp();
                app.scroll_x += (target - app.scroll_x) * jump_smooth;
            }
        }

        // === MINIMAP / Overview bar ===

        if app.show_minimap {
            draw_minimap(app, ui, rect, available, pixels_per_second, sample_rate);
        }

        // Custom speed input dialog
        if app.speed_input.is_some() {
            let mut apply = false;
            let mut cancel = false;
            egui::Window::new("Custom Speed")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    ui.label("Enter playback rate (e.g. 1.25):");
                    if let Some(ref mut state) = app.speed_input {
                        let resp = ui.text_edit_singleline(&mut state.input_buf);
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            apply = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() {
                                apply = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel = true;
                            }
                        });
                    }
                });

            if apply {
                if let Some(ref state) = app.speed_input {
                    if let Ok(rate) = state.input_buf.parse::<f32>() {
                        let rate = rate.clamp(0.1, 8.0);
                        let ti = state.track_idx;
                        let ci = state.clip_idx;
                        if ti < app.project.tracks.len() && ci < app.project.tracks[ti].clips.len() {
                            app.push_undo("Set custom speed");
                            app.project.tracks[ti].clips[ci].playback_rate = rate;
                            app.sync_project();
                        }
                    }
                }
                app.speed_input = None;
            } else if cancel {
                app.speed_input = None;
            }
        }

        // Insert Silence dialog
        if app.insert_silence_input.is_some() {
            let mut apply = false;
            let mut cancel = false;
            egui::Window::new("Insert Silence at Playhead")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    if let Some(ref mut state) = app.insert_silence_input {
                        ui.horizontal(|ui| {
                            ui.label("Duration:");
                            let resp = ui.text_edit_singleline(&mut state.input_buf);
                            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                apply = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut state.use_bars, true, "Bars");
                            ui.radio_value(&mut state.use_bars, false, "Seconds");
                        });

                        // Show preview of duration in the other unit
                        if let Ok(val) = state.input_buf.parse::<f64>() {
                            if state.use_bars {
                                let beats_per_bar = app.project.time_signature.numerator as f64;
                                let bps = app.project.tempo.bpm / 60.0;
                                let secs = val * beats_per_bar / bps;
                                ui.label(format!("= {:.2}s", secs));
                            } else {
                                let bps = app.project.tempo.bpm / 60.0;
                                let beats_per_bar = app.project.time_signature.numerator as f64;
                                let bars = val * bps / beats_per_bar;
                                ui.label(format!("= {:.2} bars", bars));
                            }
                        }

                        ui.horizontal(|ui| {
                            if ui.button("Insert").clicked() {
                                apply = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel = true;
                            }
                        });
                    }
                });

            if apply {
                if let Some(ref state) = app.insert_silence_input {
                    if let Ok(val) = state.input_buf.parse::<f64>() {
                        let duration_samples = if state.use_bars {
                            // Convert bars to samples
                            let beats_per_bar = app.project.time_signature.numerator as f64;
                            let bps = app.project.tempo.bpm / 60.0;
                            let secs = val * beats_per_bar / bps;
                            (secs * sample_rate) as u64
                        } else {
                            (val * sample_rate) as u64
                        };
                        app.insert_silence(duration_samples);
                    }
                }
                app.insert_silence_input = None;
            } else if cancel {
                app.insert_silence_input = None;
            }
        }

        // Tempo change input dialog
        if let Some(ref mut tc_state) = app.tempo_change_input {
            let mut apply = false;
            let mut cancel = false;
            let pos_sec = tc_state.sample as f64 / sample_rate;
            let pos_label = format!("Position: {:.2}s", pos_sec);
            egui::Window::new("Add Tempo Change")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    ui.label(&pos_label);
                    ui.label("BPM:");
                    let resp = ui.text_edit_singleline(&mut tc_state.bpm_text);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        apply = true;
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Add").clicked() {
                            apply = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });

            if apply {
                if let Ok(bpm) = tc_state.bpm_text.parse::<f64>() {
                    let bpm = bpm.clamp(20.0, 999.0);
                    let sample = tc_state.sample;
                    app.push_undo("Add tempo change");
                    app.project.tempo_map.add_change(sample, bpm);
                    app.set_status(&format!("Tempo change added: {:.1} BPM", bpm));
                    app.sync_project();
                }
                app.tempo_change_input = None;
            } else if cancel {
                app.tempo_change_input = None;
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
    let clip_w = (clip.visual_duration_samples() as f64 / sample_rate) as f32 * pixels_per_second;
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
    content_offset: u64,
    zoom: f32,
    visible_x_range: Option<(f32, f32)>,
    gain_db: f32,
    waveform_zoom: f32,
    track_volume: f32,
) {
    let width = clip_rect.width();
    if width < 2.0 {
        return;
    }

    // When zoomed out far (zoom < 0.3), show simplified waveform
    let zoomed_out = zoom < 0.3;

    let samples_per_pixel = total_samples as f64 / width as f64;
    let peak_data = peaks.get_peaks_for_resolution(samples_per_pixel);
    let rms_data = peaks.get_rms_for_resolution(samples_per_pixel);
    let block_size = peaks.block_size_for_level(samples_per_pixel) as f64;

    // Calculate content_offset in peak-data indices
    let offset_blocks = if content_offset > 0 {
        (content_offset as f64 / block_size) as usize
    } else {
        0
    };

    let center_y = clip_rect.center().y;
    let half_height = clip_rect.height() * 0.4;

    // Only compute waveform peaks for the visible portion of the clip
    let (px_start, px_end) = if let Some((vis_left, vis_right)) = visible_x_range {
        let start = ((vis_left - clip_rect.min.x).max(0.0) as usize).min(width as usize);
        let end = ((vis_right - clip_rect.min.x).ceil().max(0.0) as usize).min(width as usize);
        (start, end)
    } else {
        (0, width as usize)
    };

    // When zoomed out far, reduce number of pixels sampled for performance
    let step = if zoomed_out { 2 } else { 1 };
    let num_visible = (px_end.saturating_sub(px_start)).min(2000);
    let mut peak_top: Vec<egui::Pos2> = Vec::with_capacity(num_visible / step + 2);
    let mut peak_bottom: Vec<egui::Pos2> = Vec::with_capacity(num_visible / step + 2);
    let mut rms_top: Vec<egui::Pos2> = Vec::with_capacity(num_visible / step + 2);
    let mut rms_bottom: Vec<egui::Pos2> = Vec::with_capacity(num_visible / step + 2);

    let mut px = px_start;
    while px < px_start + num_visible {
        let sample_start = px as f64 * samples_per_pixel;
        let sample_end = ((px + step) as f64 * samples_per_pixel).min(total_samples as f64);
        let peak_start = ((sample_start / block_size) as usize + offset_blocks).min(peak_data.len());
        let peak_end = (((sample_end / block_size) as usize + 1 + offset_blocks)).min(peak_data.len());

        if peak_start >= peak_data.len() {
            break;
        }

        let mut min = f32::MAX;
        let mut max = f32::MIN;
        let mut rms_max: f32 = 0.0;
        for pi in peak_start..peak_end {
            let (lo, hi) = peak_data[pi];
            if lo < min { min = lo; }
            if hi > max { max = hi; }
            if pi < rms_data.len() {
                rms_max = rms_max.max(rms_data[pi]);
            }
        }

        // Apply clip gain to waveform display
        if gain_db != 0.0 {
            let gain_linear = 10.0_f32.powf(gain_db / 20.0);
            min *= gain_linear;
            max *= gain_linear;
            rms_max *= gain_linear;
        }
        // Apply track volume to waveform display
        if (track_volume - 1.0).abs() > 0.001 {
            min *= track_volume;
            max *= track_volume;
            rms_max *= track_volume;
        }
        // Apply waveform vertical zoom (visual only)
        if waveform_zoom != 1.0 {
            min *= waveform_zoom;
            max *= waveform_zoom;
            rms_max *= waveform_zoom;
        }
        // Clamp to prevent overdraw
        min = min.max(-1.0);
        max = max.min(1.0);
        rms_max = rms_max.min(1.0);

        let x = clip_rect.min.x + px as f32;
        peak_top.push(egui::pos2(x, center_y - max * half_height));
        peak_bottom.push(egui::pos2(x, center_y - min * half_height));
        rms_top.push(egui::pos2(x, center_y - rms_max * half_height));
        rms_bottom.push(egui::pos2(x, center_y + rms_max * half_height));
        px += step;
    }

    let clipped = painter.with_clip_rect(clip_rect);

    // Center line (zero crossing) — subtle reference line
    clipped.line_segment(
        [
            egui::pos2(clip_rect.min.x, center_y),
            egui::pos2(clip_rect.max.x, center_y),
        ],
        egui::Stroke::new(0.5, color.gamma_multiply(0.20)),
    );

    if peak_top.len() >= 2 {
        if zoomed_out {
            // Simplified waveform: envelope only, thicker line, no individual detail
            let envelope_color = color.gamma_multiply(0.5);
            let mut envelope_polygon = peak_top.clone();
            let mut bottom_rev = peak_bottom.clone();
            bottom_rev.reverse();
            envelope_polygon.extend(bottom_rev);
            clipped.add(egui::Shape::convex_polygon(
                envelope_polygon,
                color.gamma_multiply(0.35),
                egui::Stroke::NONE,
            ));
            // Thicker envelope edge lines
            let thick_stroke = egui::Stroke::new(1.5, envelope_color);
            for w in peak_top.windows(2) {
                clipped.line_segment([w[0], w[1]], thick_stroke);
            }
            for w in peak_bottom.windows(2) {
                clipped.line_segment([w[0], w[1]], thick_stroke);
            }
        } else {
            // Peak envelope — gradient-like fill (slightly transparent)
            let mut peak_polygon = peak_top.clone();
            let mut bottom_rev = peak_bottom.clone();
            bottom_rev.reverse();
            peak_polygon.extend(bottom_rev);
            clipped.add(egui::Shape::convex_polygon(
                peak_polygon,
                color.gamma_multiply(0.40),
                egui::Stroke::NONE,
            ));

            // RMS envelope — brighter gradient core
            let mut rms_polygon = rms_top.clone();
            let mut rms_bot_rev = rms_bottom.clone();
            rms_bot_rev.reverse();
            rms_polygon.extend(rms_bot_rev);
            clipped.add(egui::Shape::convex_polygon(
                rms_polygon,
                color.gamma_multiply(0.65),
                egui::Stroke::NONE,
            ));

            // Bright highlight line along the top peak envelope
            let highlight_stroke = egui::Stroke::new(0.5, color.gamma_multiply(1.0));
            for w in peak_top.windows(2) {
                clipped.line_segment([w[0], w[1]], highlight_stroke);
            }

            // Anti-aliased waveform edge lines (top and bottom outlines)
            let top_stroke = egui::Stroke::new(1.0, color.gamma_multiply(0.85));
            for w in peak_top.windows(2) {
                clipped.line_segment([w[0], w[1]], top_stroke);
            }
            for w in peak_bottom.windows(2) {
                clipped.line_segment([w[0], w[1]], top_stroke);
            }
        }
    }
}

/// Draw a rotary knob (twister) — circular with an arc indicator.
fn draw_rotary_knob(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    value: f32, // 0.0 to 1.0
    color: egui::Color32,
    hovered: bool,
) {
    use std::f32::consts::PI;
    let v = value.clamp(0.0, 1.0);

    // Background circle
    let bg = if hovered {
        egui::Color32::from_rgb(48, 50, 58)
    } else {
        egui::Color32::from_rgb(34, 35, 42)
    };
    painter.circle_filled(center, radius, bg);
    painter.circle_stroke(center, radius, egui::Stroke::new(1.0, egui::Color32::from_rgb(55, 56, 65)));

    // Arc: from 135° (bottom-left) to 405° (bottom-right), 270° sweep
    let start_angle = 135.0_f32.to_radians(); // 7:30 position
    let sweep = 270.0_f32.to_radians();
    let end_angle = start_angle + sweep * v;

    // Draw inactive arc (dark)
    let arc_r = radius - 2.0;
    let segments = 32;
    for seg in 0..segments {
        let t0 = seg as f32 / segments as f32;
        let t1 = (seg + 1) as f32 / segments as f32;
        let a0 = start_angle + sweep * t0;
        let a1 = start_angle + sweep * t1;
        let p0 = egui::pos2(center.x + arc_r * a0.cos(), center.y + arc_r * a0.sin());
        let p1 = egui::pos2(center.x + arc_r * a1.cos(), center.y + arc_r * a1.sin());
        let c = if a0 < end_angle {
            color
        } else {
            egui::Color32::from_rgb(28, 29, 34)
        };
        painter.line_segment([p0, p1], egui::Stroke::new(2.0, c));
    }

    // Indicator dot at current position
    let dot_angle = end_angle;
    let dot_r = radius - 2.0;
    let dot_pos = egui::pos2(
        center.x + dot_r * dot_angle.cos(),
        center.y + dot_r * dot_angle.sin(),
    );
    painter.circle_filled(dot_pos, 2.5, egui::Color32::WHITE);

    // Center dot
    painter.circle_filled(center, 2.0, egui::Color32::from_rgb(80, 80, 90));
}

/// Draw a minimap overview bar at the bottom of the timeline area.
/// Shows the entire project as a miniature view with clip blocks,
/// and a highlighted rectangle for the currently visible portion.
fn draw_minimap(
    app: &mut DawApp,
    ui: &mut egui::Ui,
    timeline_rect: egui::Rect,
    available: egui::Vec2,
    pixels_per_second: f32,
    sample_rate: f64,
) {
    // Determine total project duration (end of last clip + some padding)
    let end_sample = app.project.tracks.iter()
        .flat_map(|t| t.clips.iter())
        .map(|c| c.start_sample + c.visual_duration_samples())
        .max()
        .unwrap_or(0);

    if end_sample == 0 { return; }

    let sr = sample_rate;
    let total_duration_sec = end_sample as f64 / sr;
    // Add 10% padding
    let padded_duration = total_duration_sec * 1.1;

    // Minimap rect: thin bar at bottom of timeline area
    let minimap_rect = egui::Rect::from_min_size(
        egui::pos2(timeline_rect.min.x, timeline_rect.max.y - MINIMAP_HEIGHT),
        egui::vec2(available.x, MINIMAP_HEIGHT),
    );

    let painter = ui.painter();

    // Background
    painter.rect_filled(minimap_rect, 0.0, egui::Color32::from_rgb(20, 20, 24));
    painter.rect_stroke(
        minimap_rect, 0.0,
        egui::Stroke::new(0.5, egui::Color32::from_rgb(45, 45, 52)),
        egui::StrokeKind::Inside,
    );

    let mm_width = minimap_rect.width();
    let px_per_sec_mm = mm_width as f64 / padded_duration;

    // Draw clip blocks as small colored rectangles
    let num_tracks = app.project.tracks.len().max(1);
    let track_h = (MINIMAP_HEIGHT - 4.0) / num_tracks as f32;

    for (ti, track) in app.project.tracks.iter().enumerate() {
        let color = egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);
        let ty = minimap_rect.min.y + 2.0 + ti as f32 * track_h;

        for clip in &track.clips {
            if clip.muted { continue; }
            let cx = minimap_rect.min.x + (clip.start_sample as f64 / sr * px_per_sec_mm) as f32;
            let cw = ((clip.visual_duration_samples() as f64 / sr * px_per_sec_mm) as f32).max(1.0);
            let clip_rect = egui::Rect::from_min_size(
                egui::pos2(cx, ty),
                egui::vec2(cw, (track_h - 1.0).max(1.0)),
            );
            painter.rect_filled(clip_rect, 1.0, color.gamma_multiply(0.6));
        }
    }

    // Draw the visible viewport rectangle
    let view_start_sec = app.scroll_x as f64 / pixels_per_second as f64;
    let view_end_sec = view_start_sec + available.x as f64 / pixels_per_second as f64;
    let vx1 = minimap_rect.min.x + (view_start_sec * px_per_sec_mm) as f32;
    let vx2 = minimap_rect.min.x + (view_end_sec * px_per_sec_mm) as f32;
    let vx1 = vx1.max(minimap_rect.min.x);
    let vx2 = vx2.min(minimap_rect.max.x);

    let viewport_rect = egui::Rect::from_min_max(
        egui::pos2(vx1, minimap_rect.min.y + 1.0),
        egui::pos2(vx2, minimap_rect.max.y - 1.0),
    );
    painter.rect_filled(viewport_rect, 1.0, egui::Color32::from_rgba_premultiplied(240, 192, 64, 20));
    painter.rect_stroke(
        viewport_rect, 1.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(240, 192, 64)),
        egui::StrokeKind::Outside,
    );

    // Playhead indicator in minimap
    let pos = app.position_samples();
    let pos_sec = pos as f64 / sr;
    let ph_x = minimap_rect.min.x + (pos_sec * px_per_sec_mm) as f32;
    if ph_x >= minimap_rect.min.x && ph_x <= minimap_rect.max.x {
        painter.line_segment(
            [egui::pos2(ph_x, minimap_rect.min.y), egui::pos2(ph_x, minimap_rect.max.y)],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 80, 80)),
        );
    }

    // Click/drag on minimap to navigate
    let mm_response = ui.interact(
        minimap_rect,
        ui.id().with("minimap_nav"),
        egui::Sense::click_and_drag(),
    );

    if mm_response.clicked() || mm_response.dragged() {
        if let Some(mpos) = mm_response.interact_pointer_pos {
            let click_fraction = ((mpos.x - minimap_rect.min.x) / mm_width).clamp(0.0, 1.0);
            let click_sec = click_fraction as f64 * padded_duration;
            // Center the viewport on the clicked position
            let half_view_sec = available.x as f64 / pixels_per_second as f64 / 2.0;
            let target_sec = (click_sec - half_view_sec).max(0.0);
            app.scroll_x = (target_sec as f32 * pixels_per_second).max(0.0);
            app.user_scrolling = true; // manual navigation
        }
    }
}
enum TrackAction {
    ToggleMute(usize),
    ToggleSolo(usize),
    /// Exclusive solo: un-solos all other tracks, then solos this one
    ToggleSoloExclusive(usize),
    ToggleArm(usize),
    SetVolume(usize, f32),
    SetPan(usize, f32),
    Select(usize),
    Delete(usize),
    Duplicate(usize),
    StartRename(usize),
    FinishRename(usize, String),
    ToggleLanes(usize),
    OpenFx,
    /// Create a new group from the selected track
    CreateGroupFromTrack(usize),
    /// Remove a track from its group
    RemoveFromGroup(usize),
    /// Toggle collapse/expand of a group
    ToggleGroup(Uuid),
    /// Toggle mute on all tracks in a group
    ToggleGroupMute(Uuid),
    /// Toggle solo on all tracks in a group
    ToggleGroupSolo(Uuid),
    /// Delete a group (ungroup tracks, don't delete them)
    DeleteGroup(Uuid),
    /// Move track up in arrangement order
    MoveUp(usize),
    /// Move track down in arrangement order
    MoveDown(usize),
    /// Freeze track (render effects offline)
    Freeze(usize),
    /// Unfreeze track (restore original clips and effects)
    Unfreeze(usize),
    /// Flatten comp — remove inactive takes, keep only active clips
    FlattenComp(usize),
    /// Set track height preset (0.0 = auto)
    SetHeight(usize, f32),
    /// Save track as a reusable template
    SaveAsTemplate(usize),
    /// Open the "Add from Template" picker
    AddFromTemplate,
    /// Open the color palette picker for a track
    OpenColorPalette(usize),
}
