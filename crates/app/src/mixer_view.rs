use eframe::egui;

use crate::DawApp;

const CHANNEL_WIDTH: f32 = 80.0;

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (i, track) in app.project.tracks.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                let color = egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);

                egui::Frame::default()
                    .inner_margin(8.0)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 68)))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.set_width(CHANNEL_WIDTH);
                        ui.vertical(|ui| {
                            // Track name
                            ui.horizontal(|ui| {
                                ui.colored_label(color, "█");
                                ui.strong(&track.name);
                            });

                            ui.add_space(8.0);

                            // Fader (vertical slider)
                            ui.label("Vol");
                            ui.add(
                                egui::Slider::new(&mut track.volume, 0.0..=1.5)
                                    .vertical()
                                    .show_value(true),
                            );

                            ui.add_space(4.0);

                            // Pan knob
                            ui.label("Pan");
                            ui.add(
                                egui::DragValue::new(&mut track.pan)
                                    .range(-1.0..=1.0)
                                    .speed(0.01)
                                    .fixed_decimals(2),
                            );

                            ui.add_space(8.0);

                            // Buttons
                            ui.horizontal(|ui| {
                                if ui.selectable_label(track.muted, "M").clicked() {
                                    track.muted = !track.muted;
                                }
                                if ui.selectable_label(track.solo, "S").clicked() {
                                    track.solo = !track.solo;
                                }
                            });
                        });
                    });
            });
        }

        // Master channel
        egui::Frame::default()
            .inner_margin(8.0)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 100, 120)))
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.set_width(CHANNEL_WIDTH);
                ui.vertical(|ui| {
                    ui.strong("Master");
                    ui.add_space(8.0);
                    ui.label("Vol");
                    let mut master_vol: f32 = 1.0;
                    ui.add(
                        egui::Slider::new(&mut master_vol, 0.0..=1.5)
                            .vertical()
                            .show_value(true),
                    );
                });
            });
    });
}
