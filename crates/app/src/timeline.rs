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

    egui::SidePanel::left("track_headers")
        .exact_width(HEADER_WIDTH)
        .resizable(false)
        .show_inside(ui, |ui| {
            // Spacer for ruler
            ui.allocate_space(egui::vec2(HEADER_WIDTH, RULER_HEIGHT));
            ui.separator();

            let mut track_actions: Vec<TrackAction> = Vec::new();

            for (i, track) in app.project.tracks.iter().enumerate() {
                ui.push_id(i, |ui| {
                    let color =
                        egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);
                    let is_selected = app.selected_track == Some(i);

                    let header_response = ui.allocate_ui(
                        egui::vec2(HEADER_WIDTH, TRACK_HEIGHT),
                        |ui| {
                            if is_selected {
                                let rect = ui.max_rect();
                                ui.painter()
                                    .rect_filled(rect, 0.0, egui::Color32::from_rgb(45, 45, 55));
                            }
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.colored_label(color, "█");
                                    ui.strong(&track.name);
                                });

                                ui.horizontal(|ui| {
                                    let muted = track.muted;
                                    if ui
                                        .selectable_label(muted, "M")
                                        .on_hover_text("Mute")
                                        .clicked()
                                    {
                                        track_actions.push(TrackAction::ToggleMute(i));
                                    }
                                    let solo = track.solo;
                                    if ui
                                        .selectable_label(solo, "S")
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
                        },
                    );
                    if header_response.response.clicked() {
                        track_actions.push(TrackAction::Select(i));
                    }
                    ui.separator();
                });
            }

            // Apply track actions
            for action in track_actions {
                match action {
                    TrackAction::ToggleMute(i) => {
                        app.push_undo("Toggle mute");
                        app.project.tracks[i].muted = !app.project.tracks[i].muted;
                    }
                    TrackAction::ToggleSolo(i) => {
                        app.push_undo("Toggle solo");
                        app.project.tracks[i].solo = !app.project.tracks[i].solo;
                    }
                    TrackAction::ToggleArm(i) => {
                        app.project.tracks[i].armed = !app.project.tracks[i].armed;
                    }
                    TrackAction::SetVolume(i, v) => {
                        app.project.tracks[i].volume = v;
                    }
                    TrackAction::Select(i) => {
                        app.selected_track = Some(i);
                        continue;
                    }
                }
                app.sync_project();
            }

            ui.add_space(8.0);
            if ui.button("+ Add Track").clicked() {
                app.push_undo("Add track");
                let n = app.project.tracks.len() + 1;
                app.project.add_track(&format!("Track {n}"), TrackKind::Audio);
                app.sync_project();
            }
        });

    // Timeline area
    egui::CentralPanel::default().show_inside(ui, |ui| {
        let available = ui.available_size();

        // Handle horizontal scroll with mouse wheel
        let scroll_response = ui.interact(
            ui.max_rect(),
            ui.id().with("timeline_scroll"),
            egui::Sense::click_and_drag(),
        );
        if scroll_response.dragged_by(egui::PointerButton::Middle) {
            app.scroll_x -= scroll_response.drag_delta().x;
            app.scroll_x = app.scroll_x.max(0.0);
        }

        // Click on timeline to set position
        if scroll_response.clicked() {
            if let Some(pos) = scroll_response.interact_pointer_pos {
                let rect = ui.max_rect();
                let x_offset = pos.x - rect.min.x + app.scroll_x;
                let seconds = x_offset as f64 / pixels_per_second as f64;
                let sample_pos = (seconds * sample_rate) as u64;
                app.send_command(jamhub_engine::EngineCommand::SetPosition(sample_pos));
            }
        }

        // Scroll wheel for zoom
        ui.input(|i| {
            let scroll = i.smooth_scroll_delta.y;
            if scroll != 0.0 {
                app.zoom = (app.zoom * (1.0 + scroll * 0.002)).clamp(0.1, 10.0);
            }
        });

        let painter = ui.painter();
        let rect = ui.max_rect();

        // Background
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(30, 30, 35));

        // Ruler
        let ruler_rect =
            egui::Rect::from_min_size(rect.min, egui::vec2(available.x, RULER_HEIGHT));
        painter.rect_filled(ruler_rect, 0.0, egui::Color32::from_rgb(40, 40, 48));

        // Draw beat/bar markers
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
                painter.text(
                    egui::pos2(x + 4.0, rect.min.y + 4.0),
                    egui::Align2::LEFT_TOP,
                    format!("{bar}"),
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(160, 160, 170),
                );
            }
        }

        // Track lanes
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

            // Lane separator
            painter.line_segment(
                [
                    egui::pos2(rect.min.x, y + TRACK_HEIGHT),
                    egui::pos2(rect.max.x, y + TRACK_HEIGHT),
                ],
                egui::Stroke::new(0.5, egui::Color32::from_rgb(50, 50, 58)),
            );

            // Draw clips with waveforms
            let color =
                egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);
            for clip in &track.clips {
                let clip_start_sec = clip.start_sample as f64 / sample_rate;
                let clip_dur_sec = clip.duration_samples as f64 / sample_rate;
                let clip_x =
                    rect.min.x + clip_start_sec as f32 * pixels_per_second - app.scroll_x;
                let clip_w = clip_dur_sec as f32 * pixels_per_second;

                // Skip clips not visible
                if clip_x + clip_w < rect.min.x || clip_x > rect.max.x {
                    continue;
                }

                let clip_rect = egui::Rect::from_min_size(
                    egui::pos2(clip_x, y + 2.0),
                    egui::vec2(clip_w, TRACK_HEIGHT - 4.0),
                );

                // Clip background
                painter.rect_filled(clip_rect, 4.0, color.gamma_multiply(0.3));

                // Draw waveform
                if let ClipSource::AudioBuffer { buffer_id } = &clip.source {
                    if let Some(peaks) = app.waveform_cache.get(buffer_id) {
                        draw_waveform(
                            painter,
                            &peaks,
                            clip_rect,
                            clip.duration_samples,
                            color,
                        );
                    }
                }

                // Clip border
                painter.rect_stroke(
                    clip_rect,
                    4.0,
                    egui::Stroke::new(1.0, color),
                    egui::StrokeKind::Outside,
                );

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
    let half_height = clip_rect.height() * 0.45;

    let waveform_color = color.gamma_multiply(0.8);

    let num_pixels = width as usize;
    let mut points_top: Vec<egui::Pos2> = Vec::with_capacity(num_pixels + 2);
    let mut points_bottom: Vec<egui::Pos2> = Vec::with_capacity(num_pixels + 2);

    for px in 0..num_pixels {
        let sample_start = (px as f64 * samples_per_pixel) as f64;
        let sample_end = ((px + 1) as f64 * samples_per_pixel) as f64;

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

    // Draw as filled polygon: top line forward, bottom line backward
    if points_top.len() >= 2 {
        points_bottom.reverse();
        let mut polygon = points_top;
        polygon.extend(points_bottom);

        painter.with_clip_rect(clip_rect).add(egui::Shape::convex_polygon(
            polygon,
            waveform_color.gamma_multiply(0.5),
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
}
