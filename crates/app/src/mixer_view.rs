use eframe::egui;

use crate::DawApp;

const CHANNEL_WIDTH: f32 = 90.0;

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    let mut needs_sync = false;

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
                                // Track name with color
                                ui.horizontal(|ui| {
                                    ui.colored_label(color, "█");
                                    ui.strong(&track.name);
                                });

                                ui.add_space(4.0);

                                // Clip count
                                ui.label(
                                    egui::RichText::new(format!("{} clips", track.clips.len()))
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );

                                ui.add_space(8.0);

                                // Fader
                                ui.label("Volume");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut track.volume, 0.0..=1.5)
                                            .vertical()
                                            .show_value(true),
                                    )
                                    .changed()
                                {
                                    needs_sync = true;
                                }

                                ui.add_space(4.0);

                                // Pan
                                ui.label("Pan");
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

                                ui.add_space(8.0);

                                // Buttons
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
                                        .clicked()
                                    {
                                        track.solo = !track.solo;
                                        needs_sync = true;
                                    }
                                });
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
                        ui.label("Volume");
                        let mut master_vol: f32 = 1.0;
                        ui.add(
                            egui::Slider::new(&mut master_vol, 0.0..=1.5)
                                .vertical()
                                .show_value(true),
                        );
                    });
                });
        });
    });

    if needs_sync {
        app.sync_project();
    }
}
