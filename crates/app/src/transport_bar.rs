use eframe::egui;
use jamhub_engine::EngineCommand;
use jamhub_model::TransportState;

use crate::DawApp;

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;

        let state = app.transport_state();

        // Transport buttons
        if ui
            .button("⏮")
            .on_hover_text("Rewind to start (Home)")
            .clicked()
        {
            app.send_command(EngineCommand::SetPosition(0));
        }

        let stop_color = if state == TransportState::Stopped {
            egui::Color32::WHITE
        } else {
            egui::Color32::GRAY
        };
        if ui
            .add(egui::Button::new(
                egui::RichText::new("⏹").color(stop_color),
            ))
            .on_hover_text("Stop")
            .clicked()
        {
            app.send_command(EngineCommand::Stop);
        }

        let play_color = if state == TransportState::Playing {
            egui::Color32::from_rgb(80, 200, 80)
        } else {
            egui::Color32::GRAY
        };
        if ui
            .add(egui::Button::new(
                egui::RichText::new("▶").color(play_color),
            ))
            .on_hover_text("Play / Pause (Space)")
            .clicked()
        {
            app.send_command(EngineCommand::Play);
        }

        let rec_color = if app.is_recording {
            egui::Color32::from_rgb(220, 50, 50)
        } else {
            egui::Color32::GRAY
        };
        if ui
            .add(egui::Button::new(
                egui::RichText::new("⏺").color(rec_color),
            ))
            .on_hover_text("Record (R)")
            .clicked()
        {
            app.toggle_recording();
        }

        ui.separator();

        // Metronome
        let met_color = if app.metronome_enabled {
            egui::Color32::from_rgb(255, 200, 50)
        } else {
            egui::Color32::GRAY
        };
        if ui
            .add(egui::Button::new(
                egui::RichText::new("🔔").color(met_color),
            ))
            .on_hover_text("Metronome on/off (M)")
            .clicked()
        {
            app.metronome_enabled = !app.metronome_enabled;
            app.send_command(EngineCommand::SetMetronome(app.metronome_enabled));
        }

        ui.separator();

        // === TIME DISPLAY ===
        let pos = app.position_samples();
        let sr = app.sample_rate();
        let seconds = pos as f64 / sr as f64;
        let minutes = (seconds / 60.0) as u32;
        let secs = seconds % 60.0;

        ui.label(
            egui::RichText::new("Time")
                .small()
                .color(egui::Color32::GRAY),
        );
        ui.monospace(format!("{minutes:02}m {secs:05.2}s"));

        ui.separator();

        // === BAR.BEAT DISPLAY ===
        let beat = app.project.tempo.beat_at_sample(pos, sr as f64);
        let bar =
            (beat / app.project.time_signature.numerator as f64).floor() as u32 + 1;
        let beat_in_bar =
            (beat % app.project.time_signature.numerator as f64).floor() as u32 + 1;

        ui.label(
            egui::RichText::new("Bar")
                .small()
                .color(egui::Color32::GRAY),
        );
        ui.monospace(format!("{bar}.{beat_in_bar}"));

        ui.separator();

        // === BPM ===
        ui.label(
            egui::RichText::new("Tempo")
                .small()
                .color(egui::Color32::GRAY),
        );
        let mut bpm = app.project.tempo.bpm;
        let response = ui.add(
            egui::DragValue::new(&mut bpm)
                .range(20.0..=300.0)
                .speed(0.5)
                .suffix(" bpm"),
        );
        if response.changed() {
            app.project.tempo.bpm = bpm;
            app.sync_project();
        }

        ui.separator();

        // === TIME SIGNATURE ===
        ui.label(
            egui::RichText::new("Sig")
                .small()
                .color(egui::Color32::GRAY),
        );
        let mut num = app.project.time_signature.numerator as i32;
        ui.add(egui::DragValue::new(&mut num).range(1..=16).speed(0.1));
        ui.label("/");
        let mut den = app.project.time_signature.denominator as i32;
        ui.add(egui::DragValue::new(&mut den).range(1..=16).speed(0.1));
        if num != app.project.time_signature.numerator as i32
            || den != app.project.time_signature.denominator as i32
        {
            app.project.time_signature.numerator = num as u8;
            app.project.time_signature.denominator = den as u8;
            app.sync_project();
        }

        ui.separator();

        // === LOOP ===
        let loop_color = if app.loop_enabled {
            egui::Color32::from_rgb(80, 130, 220)
        } else {
            egui::Color32::GRAY
        };
        if ui
            .add(egui::Button::new(
                egui::RichText::new("⟳").color(loop_color),
            ))
            .on_hover_text("Loop on/off (L)")
            .clicked()
        {
            app.loop_enabled = !app.loop_enabled;
            if app.loop_enabled && app.loop_end == 0 {
                // Default loop: 4 bars
                let sr = app.sample_rate() as f64;
                let beats = app.project.time_signature.numerator as f64 * 4.0;
                app.loop_start = 0;
                app.loop_end = app.project.tempo.sample_at_beat(beats, sr);
            }
            app.send_command(EngineCommand::SetLoop {
                enabled: app.loop_enabled,
                start: app.loop_start,
                end: app.loop_end,
            });
        }

        ui.separator();

        // === MASTER VOLUME ===
        ui.label(
            egui::RichText::new("Master")
                .small()
                .color(egui::Color32::GRAY),
        );
        let mut mv = app.master_volume;
        if ui
            .add(
                egui::DragValue::new(&mut mv)
                    .range(0.0..=1.5)
                    .speed(0.01)
                    .suffix("")
                    .fixed_decimals(0)
                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
            )
            .changed()
        {
            app.master_volume = mv;
            app.send_command(EngineCommand::SetMasterVolume(mv));
        }

        ui.separator();

        // === ZOOM ===
        ui.label(
            egui::RichText::new("Zoom")
                .small()
                .color(egui::Color32::GRAY),
        );
        ui.add(
            egui::DragValue::new(&mut app.zoom)
                .range(0.1..=10.0)
                .speed(0.05)
                .suffix("x"),
        );

        // Right side
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Session indicator
            if app.session.is_connected() {
                ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "● Online");
                ui.separator();
            }
            ui.label(
                egui::RichText::new("JamHub")
                    .strong()
                    .color(egui::Color32::from_rgb(100, 180, 255)),
            );
        });
    });
}
