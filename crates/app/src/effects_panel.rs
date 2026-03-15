use eframe::egui;
use jamhub_model::TrackEffect;

use crate::DawApp;

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_effects {
        return;
    }

    let track_idx = match app.selected_track {
        Some(i) if i < app.project.tracks.len() => i,
        _ => return,
    };

    let mut open = true;
    egui::Window::new("Effects")
        .open(&mut open)
        .default_width(300.0)
        .show(ctx, |ui| {
            let track_name = app.project.tracks[track_idx].name.clone();
            ui.heading(format!("FX: {track_name}"));
            ui.separator();

            let mut needs_sync = false;
            let mut remove_idx: Option<usize> = None;

            let effects_len = app.project.tracks[track_idx].effects.len();
            for i in 0..effects_len {
                ui.push_id(i, |ui| {
                    let effect = &mut app.project.tracks[track_idx].effects[i];
                    egui::Frame::default()
                        .inner_margin(6.0)
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(60, 60, 70),
                        ))
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.strong(effect.name());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("X").clicked() {
                                            remove_idx = Some(i);
                                        }
                                    },
                                );
                            });

                            match effect {
                                TrackEffect::Gain { db } => {
                                    ui.horizontal(|ui| {
                                        ui.label("dB:");
                                        if ui
                                            .add(
                                                egui::Slider::new(db, -24.0..=24.0)
                                                    .suffix(" dB"),
                                            )
                                            .changed()
                                        {
                                            needs_sync = true;
                                        }
                                    });
                                }
                                TrackEffect::LowPass { cutoff_hz } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Cutoff:");
                                        if ui
                                            .add(
                                                egui::Slider::new(cutoff_hz, 20.0..=20000.0)
                                                    .logarithmic(true)
                                                    .suffix(" Hz"),
                                            )
                                            .changed()
                                        {
                                            needs_sync = true;
                                        }
                                    });
                                }
                                TrackEffect::HighPass { cutoff_hz } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Cutoff:");
                                        if ui
                                            .add(
                                                egui::Slider::new(cutoff_hz, 20.0..=20000.0)
                                                    .logarithmic(true)
                                                    .suffix(" Hz"),
                                            )
                                            .changed()
                                        {
                                            needs_sync = true;
                                        }
                                    });
                                }
                                TrackEffect::Delay {
                                    time_ms,
                                    feedback,
                                    mix,
                                } => {
                                    if ui
                                        .add(
                                            egui::Slider::new(time_ms, 1.0..=2000.0)
                                                .text("Time")
                                                .suffix(" ms"),
                                        )
                                        .changed()
                                    {
                                        needs_sync = true;
                                    }
                                    if ui
                                        .add(
                                            egui::Slider::new(feedback, 0.0..=0.95)
                                                .text("Feedback"),
                                        )
                                        .changed()
                                    {
                                        needs_sync = true;
                                    }
                                    if ui
                                        .add(egui::Slider::new(mix, 0.0..=1.0).text("Mix"))
                                        .changed()
                                    {
                                        needs_sync = true;
                                    }
                                }
                                TrackEffect::Reverb { decay, mix } => {
                                    if ui
                                        .add(
                                            egui::Slider::new(decay, 0.0..=0.99).text("Decay"),
                                        )
                                        .changed()
                                    {
                                        needs_sync = true;
                                    }
                                    if ui
                                        .add(egui::Slider::new(mix, 0.0..=1.0).text("Mix"))
                                        .changed()
                                    {
                                        needs_sync = true;
                                    }
                                }
                            }
                        });
                    ui.add_space(2.0);
                });
            }

            if let Some(idx) = remove_idx {
                app.push_undo("Remove effect");
                app.project.tracks[track_idx].effects.remove(idx);
                needs_sync = true;
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Add:");
                if ui.button("Gain").clicked() {
                    app.push_undo("Add gain");
                    app.project.tracks[track_idx]
                        .effects
                        .push(TrackEffect::Gain { db: 0.0 });
                    needs_sync = true;
                }
                if ui.button("LowPass").clicked() {
                    app.push_undo("Add low pass");
                    app.project.tracks[track_idx]
                        .effects
                        .push(TrackEffect::LowPass { cutoff_hz: 5000.0 });
                    needs_sync = true;
                }
                if ui.button("HighPass").clicked() {
                    app.push_undo("Add high pass");
                    app.project.tracks[track_idx]
                        .effects
                        .push(TrackEffect::HighPass { cutoff_hz: 100.0 });
                    needs_sync = true;
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Delay").clicked() {
                    app.push_undo("Add delay");
                    app.project.tracks[track_idx]
                        .effects
                        .push(TrackEffect::Delay {
                            time_ms: 250.0,
                            feedback: 0.3,
                            mix: 0.3,
                        });
                    needs_sync = true;
                }
                if ui.button("Reverb").clicked() {
                    app.push_undo("Add reverb");
                    app.project.tracks[track_idx]
                        .effects
                        .push(TrackEffect::Reverb {
                            decay: 0.7,
                            mix: 0.3,
                        });
                    needs_sync = true;
                }
            });

            if needs_sync {
                app.sync_project();
            }
        });
    if !open {
        app.show_effects = false;
    }
}
