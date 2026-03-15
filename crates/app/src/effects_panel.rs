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
                                TrackEffect::Compressor { threshold_db, ratio, attack_ms, release_ms } => {
                                    if ui.add(egui::Slider::new(threshold_db, -60.0..=0.0).text("Threshold").suffix(" dB")).changed() { needs_sync = true; }
                                    if ui.add(egui::Slider::new(ratio, 1.0..=20.0).text("Ratio").suffix(":1")).changed() { needs_sync = true; }
                                    if ui.add(egui::Slider::new(attack_ms, 0.1..=100.0).text("Attack").suffix(" ms")).changed() { needs_sync = true; }
                                    if ui.add(egui::Slider::new(release_ms, 10.0..=1000.0).text("Release").suffix(" ms")).changed() { needs_sync = true; }
                                }
                                TrackEffect::EqBand { freq_hz, gain_db, q } => {
                                    if ui.add(egui::Slider::new(freq_hz, 20.0..=20000.0).logarithmic(true).text("Freq").suffix(" Hz")).changed() { needs_sync = true; }
                                    if ui.add(egui::Slider::new(gain_db, -24.0..=24.0).text("Gain").suffix(" dB")).changed() { needs_sync = true; }
                                    if ui.add(egui::Slider::new(q, 0.1..=10.0).text("Q")).changed() { needs_sync = true; }
                                }
                                TrackEffect::Chorus { rate_hz, depth, mix } => {
                                    if ui.add(egui::Slider::new(rate_hz, 0.1..=5.0).text("Rate").suffix(" Hz")).changed() { needs_sync = true; }
                                    if ui.add(egui::Slider::new(depth, 0.0..=1.0).text("Depth")).changed() { needs_sync = true; }
                                    if ui.add(egui::Slider::new(mix, 0.0..=1.0).text("Mix")).changed() { needs_sync = true; }
                                }
                                TrackEffect::Distortion { drive, mix } => {
                                    if ui.add(egui::Slider::new(drive, 0.0..=40.0).text("Drive").suffix(" dB")).changed() { needs_sync = true; }
                                    if ui.add(egui::Slider::new(mix, 0.0..=1.0).text("Mix")).changed() { needs_sync = true; }
                                }
                                TrackEffect::Vst3Plugin { ref name, ref path } => {
                                    ui.label(egui::RichText::new(format!("VST3: {name}")).color(egui::Color32::from_rgb(100, 200, 100)));
                                    ui.label(egui::RichText::new(path).small().color(egui::Color32::GRAY));
                                    ui.label(egui::RichText::new("Plugin loaded — audio passthrough").small().color(egui::Color32::from_rgb(200, 180, 100)));
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
            ui.label(egui::RichText::new("Add Effect:").small().color(egui::Color32::GRAY));

            let effects_to_add: &[(&str, TrackEffect)] = &[
                ("Gain", TrackEffect::Gain { db: 0.0 }),
                ("EQ Band", TrackEffect::EqBand { freq_hz: 1000.0, gain_db: 0.0, q: 1.0 }),
                ("Compressor", TrackEffect::Compressor { threshold_db: -20.0, ratio: 4.0, attack_ms: 10.0, release_ms: 100.0 }),
                ("Low Pass", TrackEffect::LowPass { cutoff_hz: 5000.0 }),
                ("High Pass", TrackEffect::HighPass { cutoff_hz: 100.0 }),
                ("Delay", TrackEffect::Delay { time_ms: 250.0, feedback: 0.3, mix: 0.3 }),
                ("Reverb", TrackEffect::Reverb { decay: 0.7, mix: 0.3 }),
                ("Chorus", TrackEffect::Chorus { rate_hz: 1.0, depth: 0.5, mix: 0.3 }),
                ("Distortion", TrackEffect::Distortion { drive: 12.0, mix: 0.5 }),
            ];

            // Render add buttons in rows of 3
            for chunk in effects_to_add.chunks(3) {
                ui.horizontal(|ui| {
                    for (name, effect) in chunk {
                        if ui.button(*name).clicked() {
                            app.push_undo(&format!("Add {name}"));
                            app.project.tracks[track_idx].effects.push(effect.clone());
                            needs_sync = true;
                        }
                    }
                });
            }

            ui.separator();
            if ui.button("Browse Plugins...").clicked() {
                app.fx_browser.show = true;
            }

            if needs_sync {
                app.sync_project();
            }
        });
    if !open {
        app.show_effects = false;
    }
}
