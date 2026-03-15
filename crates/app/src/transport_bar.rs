use eframe::egui;
use jamhub_engine::EngineCommand;
use jamhub_model::TransportState;

use crate::DawApp;

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let state = app.transport_state();

        // === TRANSPORT BUTTONS ===
        let tb = egui::vec2(30.0, 22.0);

        transport_btn(ui, "⏮", tb, false, "Rewind to start [Home]", || {
            app.send_command(EngineCommand::SetPosition(0));
        });

        transport_btn(ui, "⏹", tb, state == TransportState::Stopped,
            "Stop playback", || {
            app.send_command(EngineCommand::Stop);
        });

        let playing = state == TransportState::Playing;
        if ui.add_sized(tb, egui::Button::new(
            egui::RichText::new("▶").size(14.0).color(egui::Color32::WHITE))
            .fill(if playing { egui::Color32::from_rgb(40, 140, 40) } else { egui::Color32::from_rgb(48, 48, 55) }))
            .on_hover_text("Play [Space]").clicked() {
            app.send_command(EngineCommand::Play);
        }

        if ui.add_sized(tb, egui::Button::new(
            egui::RichText::new("⏺").size(14.0).color(
                if app.is_recording { egui::Color32::WHITE } else { egui::Color32::from_rgb(220, 70, 70) }))
            .fill(if app.is_recording { egui::Color32::from_rgb(200, 35, 35) } else { egui::Color32::from_rgb(48, 48, 55) }))
            .on_hover_text("Record [R]\nRecords onto the selected track").clicked() {
            app.toggle_recording();
        }

        ui.add_space(6.0);

        // === TOGGLE STRIP ===
        let ts = egui::vec2(26.0, 22.0);

        toggle_btn(ui, "M", ts, app.metronome_enabled,
            egui::Color32::from_rgb(160, 130, 20),
            "Metronome [M]", || {
            app.metronome_enabled = !app.metronome_enabled;
            app.send_command(EngineCommand::SetMetronome(app.metronome_enabled));
        });

        toggle_btn(ui, "L", ts, app.loop_enabled,
            egui::Color32::from_rgb(50, 90, 180),
            "Loop [L]", || {
            app.loop_enabled = !app.loop_enabled;
            if app.loop_enabled && app.loop_end == 0 {
                let sr = app.sample_rate() as f64;
                let beats = app.project.time_signature.numerator as f64 * 4.0;
                app.loop_start = 0;
                app.loop_end = app.project.tempo.sample_at_beat(beats, sr);
            }
            app.send_command(EngineCommand::SetLoop {
                enabled: app.loop_enabled, start: app.loop_start, end: app.loop_end,
            });
        });

        toggle_btn(ui, "I", ts, app.input_monitor.is_enabled(),
            egui::Color32::from_rgb(170, 100, 20),
            "Input monitor [I]\nHear mic in real-time", || {
            app.toggle_input_monitor();
        });

        toggle_btn(ui, "A", ts, app.show_automation,
            egui::Color32::from_rgb(160, 120, 20),
            "Automation [A]\nClick timeline to add points", || {
            app.show_automation = !app.show_automation;
        });

        ui.separator();

        // === TIME DISPLAY ===
        let pos = app.position_samples();
        let sr = app.sample_rate();
        let seconds = pos as f64 / sr as f64;
        let minutes = (seconds / 60.0) as u32;
        let secs = seconds % 60.0;
        let beat = app.project.tempo.beat_at_sample(pos, sr as f64);
        let bar = (beat / app.project.time_signature.numerator as f64).floor() as u32 + 1;
        let beat_in_bar = (beat % app.project.time_signature.numerator as f64).floor() as u32 + 1;

        egui::Frame::default()
            .fill(egui::Color32::from_rgb(18, 18, 22))
            .inner_margin(egui::Margin::symmetric(10, 3))
            .corner_radius(5.0)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 48)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    ui.monospace(
                        egui::RichText::new(format!("{minutes:02}:{secs:05.2}"))
                            .size(15.0)
                            .color(egui::Color32::from_rgb(100, 220, 130)),
                    );
                    ui.monospace(
                        egui::RichText::new(format!("Bar {bar}.{beat_in_bar}"))
                            .size(15.0)
                            .color(egui::Color32::from_rgb(220, 190, 100)),
                    );
                });
            });

        ui.separator();

        // === TEMPO / SIG / SNAP / MASTER ===
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let mut bpm = app.project.tempo.bpm;
                if ui.add(egui::DragValue::new(&mut bpm).range(20.0..=300.0).speed(0.5).suffix(" bpm"))
                    .on_hover_text("Project tempo").changed() {
                    app.project.tempo.bpm = bpm;
                    app.sync_project();
                }

                let mut num = app.project.time_signature.numerator as i32;
                let mut den = app.project.time_signature.denominator as i32;
                ui.add(egui::DragValue::new(&mut num).range(1..=16).speed(0.1));
                ui.label(egui::RichText::new("/").color(egui::Color32::from_rgb(100, 100, 110)));
                ui.add(egui::DragValue::new(&mut den).range(1..=16).speed(0.1));
                if num != app.project.time_signature.numerator as i32 || den != app.project.time_signature.denominator as i32 {
                    app.project.time_signature.numerator = num as u8;
                    app.project.time_signature.denominator = den as u8;
                    app.sync_project();
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                // Snap mode
                let snap_label = app.snap_mode.label();
                let snap_bg = if app.snap_mode != crate::SnapMode::Off {
                    egui::Color32::from_rgb(45, 60, 90)
                } else {
                    egui::Color32::from_rgb(42, 42, 48)
                };
                if ui.add(egui::Button::new(
                    egui::RichText::new(format!("Snap: {snap_label}")).small().color(egui::Color32::from_rgb(180, 190, 210)))
                    .fill(snap_bg))
                    .on_hover_text("Snap mode [G]\nFree / 1/2 Beat / Beat / Bar").clicked() {
                    app.snap_mode = app.snap_mode.next();
                }

                // Master vol
                let mut mv = app.master_volume;
                if ui.add(egui::DragValue::new(&mut mv).range(0.0..=1.5).speed(0.01)
                    .custom_formatter(|v, _| format!("Vol {:.0}%", v * 100.0)))
                    .on_hover_text("Master volume").changed() {
                    app.master_volume = mv;
                    app.send_command(EngineCommand::SetMasterVolume(mv));
                }
            });
        });

        // === RIGHT SIDE — logo + session ===
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.session.is_connected() {
                ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "● Online");
                ui.separator();
            }
            ui.label(
                egui::RichText::new("JamHub")
                    .size(16.0)
                    .strong()
                    .color(egui::Color32::from_rgb(90, 160, 255)),
            );
        });
    });
}

fn transport_btn(
    ui: &mut egui::Ui,
    icon: &str,
    size: egui::Vec2,
    active: bool,
    tooltip: &str,
    mut on_click: impl FnMut(),
) {
    let bg = if active {
        egui::Color32::from_rgb(60, 60, 70)
    } else {
        egui::Color32::from_rgb(48, 48, 55)
    };
    if ui.add_sized(size, egui::Button::new(
        egui::RichText::new(icon).size(14.0).color(egui::Color32::WHITE)).fill(bg))
        .on_hover_text(tooltip).clicked() {
        on_click();
    }
}

fn toggle_btn(
    ui: &mut egui::Ui,
    label: &str,
    size: egui::Vec2,
    active: bool,
    active_color: egui::Color32,
    tooltip: &str,
    mut on_click: impl FnMut(),
) {
    let bg = if active { active_color } else { egui::Color32::from_rgb(38, 38, 44) };
    let text_color = if active { egui::Color32::WHITE } else { egui::Color32::from_rgb(140, 140, 148) };
    if ui.add_sized(size, egui::Button::new(
        egui::RichText::new(label).size(11.0).color(text_color)).fill(bg))
        .on_hover_text(tooltip).clicked() {
        on_click();
    }
}
