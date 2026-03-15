use eframe::egui;

use crate::DawApp;

const CHANNEL_WIDTH: f32 = 90.0;

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    let mut needs_sync = false;

    // Decay meters each frame
    if let Some(levels) = app.levels() {
        levels.decay(0.85);
    }

    egui::ScrollArea::horizontal().show(ui, |ui| {
        ui.horizontal(|ui| {
            for (i, track) in app.project.tracks.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    let color =
                        egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);
                    let is_selected = app.selected_track == Some(i);

                    let stroke_color = if is_selected {
                        egui::Color32::from_rgb(100, 180, 255)
                    } else {
                        egui::Color32::from_rgb(60, 60, 68)
                    };

                    egui::Frame::default()
                        .inner_margin(8.0)
                        .stroke(egui::Stroke::new(
                            if is_selected { 2.0 } else { 1.0 },
                            stroke_color,
                        ))
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.set_width(CHANNEL_WIDTH);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.colored_label(color, "█");
                                    ui.strong(&track.name);
                                });

                                ui.label(
                                    egui::RichText::new(format!("{} clips", track.clips.len()))
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );

                                // Effects indicator
                                if !track.effects.is_empty() {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "FX: {}",
                                            track.effects.len()
                                        ))
                                        .small()
                                        .color(egui::Color32::from_rgb(180, 140, 255)),
                                    );
                                }

                                ui.add_space(4.0);

                                // Volume fader
                                ui.label(
                                    egui::RichText::new("Volume")
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                                if ui
                                    .add(
                                        egui::Slider::new(&mut track.volume, 0.0..=1.5)
                                            .vertical()
                                            .show_value(false),
                                    )
                                    .changed()
                                {
                                    needs_sync = true;
                                }
                                ui.label(
                                    egui::RichText::new(format!("{:.0}%", track.volume * 100.0))
                                        .small(),
                                );

                                ui.add_space(2.0);

                                // Pan
                                ui.label(
                                    egui::RichText::new("Pan")
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut track.pan)
                                            .range(-1.0..=1.0)
                                            .speed(0.01)
                                            .fixed_decimals(2),
                                    )
                                    .changed()
                                {
                                    needs_sync = true;
                                }

                                ui.add_space(4.0);

                                // Mute / Solo
                                ui.horizontal(|ui| {
                                    let mute_color = if track.muted {
                                        egui::Color32::from_rgb(255, 180, 50)
                                    } else {
                                        ui.visuals().text_color()
                                    };
                                    if ui
                                        .add(egui::Button::new(
                                            egui::RichText::new("M").color(mute_color),
                                        ))
                                        .on_hover_text("Mute")
                                        .clicked()
                                    {
                                        track.muted = !track.muted;
                                        needs_sync = true;
                                    }

                                    let solo_color = if track.solo {
                                        egui::Color32::from_rgb(80, 200, 80)
                                    } else {
                                        ui.visuals().text_color()
                                    };
                                    if ui
                                        .add(egui::Button::new(
                                            egui::RichText::new("S").color(solo_color),
                                        ))
                                        .on_hover_text("Solo")
                                        .clicked()
                                    {
                                        track.solo = !track.solo;
                                        needs_sync = true;
                                    }
                                });

                                // Level meter placeholder (levels shown on master)
                                ui.add_space(4.0);
                            });
                        });
                });
            }

            // Master channel
            egui::Frame::default()
                .inner_margin(8.0)
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(100, 100, 120),
                ))
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.set_width(CHANNEL_WIDTH);
                    ui.vertical(|ui| {
                        ui.strong("Master");
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("Output")
                                .small()
                                .color(egui::Color32::GRAY),
                        );

                        // Master level meter
                        if let Some(levels) = app.levels() {
                            let (l, r) = levels.get_master_level();
                            let height = 120.0;
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(30.0, height),
                                egui::Sense::hover(),
                            );
                            let painter = ui.painter();

                            // Background
                            painter.rect_filled(
                                rect,
                                2.0,
                                egui::Color32::from_rgb(25, 25, 30),
                            );

                            // Left channel
                            let l_height = (l.clamp(0.0, 1.0) * height) as f32;
                            let l_rect = egui::Rect::from_min_max(
                                egui::pos2(rect.min.x, rect.max.y - l_height),
                                egui::pos2(rect.min.x + 13.0, rect.max.y),
                            );
                            painter.rect_filled(l_rect, 0.0, level_color(l));

                            // Right channel
                            let r_height = (r.clamp(0.0, 1.0) * height) as f32;
                            let r_rect = egui::Rect::from_min_max(
                                egui::pos2(rect.min.x + 17.0, rect.max.y - r_height),
                                egui::pos2(rect.max.x, rect.max.y),
                            );
                            painter.rect_filled(r_rect, 0.0, level_color(r));

                            // dB labels
                            ui.label(
                                egui::RichText::new(format!(
                                    "L:{:.0}dB R:{:.0}dB",
                                    to_db(l),
                                    to_db(r)
                                ))
                                .small()
                                .color(egui::Color32::GRAY),
                            );
                        }
                    });
                });
        });
    });

    if needs_sync {
        app.sync_project();
    }
}

fn draw_level_meter(
    ui: &mut egui::Ui,
    levels: Option<&jamhub_engine::LevelMeters>,
    track_id: &uuid::Uuid,
) {
    let (l, r) = levels
        .map(|lm| lm.get_track_level(track_id))
        .unwrap_or((0.0, 0.0));

    let height = 60.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, height), egui::Sense::hover());
    let painter = ui.painter();

    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(25, 25, 30));

    let avg = (l + r) / 2.0;
    let bar_height = (avg.clamp(0.0, 1.0) * height) as f32;
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + 2.0, rect.max.y - bar_height),
        egui::pos2(rect.max.x - 2.0, rect.max.y),
    );
    painter.rect_filled(bar_rect, 0.0, level_color(avg));
}

fn level_color(level: f32) -> egui::Color32 {
    if level > 0.9 {
        egui::Color32::from_rgb(255, 50, 50) // red — clipping
    } else if level > 0.7 {
        egui::Color32::from_rgb(255, 200, 50) // yellow — hot
    } else {
        egui::Color32::from_rgb(80, 200, 80) // green — normal
    }
}

fn to_db(level: f32) -> f32 {
    if level <= 0.0001 {
        -60.0
    } else {
        20.0 * level.log10()
    }
}
