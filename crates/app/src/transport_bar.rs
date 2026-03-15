use eframe::egui;
use jamhub_engine::EngineCommand;
use jamhub_model::TransportState;

use crate::DawApp;

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;

        let state = app.transport_state();

        // Rewind
        if ui.button("⏮").on_hover_text("Rewind (Home)").clicked() {
            app.send_command(EngineCommand::SetPosition(0));
        }

        // Stop
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

        // Play
        let play_color = if state == TransportState::Playing {
            egui::Color32::from_rgb(80, 200, 80)
        } else {
            egui::Color32::GRAY
        };
        if ui
            .add(egui::Button::new(
                egui::RichText::new("▶").color(play_color),
            ))
            .on_hover_text("Play (Space)")
            .clicked()
        {
            app.send_command(EngineCommand::Play);
        }

        // Record
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

        // Metronome toggle
        let met_color = if app.metronome_enabled {
            egui::Color32::from_rgb(255, 200, 50)
        } else {
            egui::Color32::GRAY
        };
        if ui
            .add(egui::Button::new(
                egui::RichText::new("🔔").color(met_color),
            ))
            .on_hover_text("Metronome (M)")
            .clicked()
        {
            app.metronome_enabled = !app.metronome_enabled;
            app.send_command(EngineCommand::SetMetronome(app.metronome_enabled));
        }

        ui.separator();

        // Position display
        let pos = app.position_samples();
        let sr = app.sample_rate();
        let seconds = pos as f64 / sr as f64;
        let minutes = (seconds / 60.0) as u32;
        let secs = seconds % 60.0;
        let beat = app.project.tempo.beat_at_sample(pos, sr as f64);
        let bar = (beat / app.project.time_signature.numerator as f64).floor() as u32 + 1;
        let beat_in_bar =
            (beat % app.project.time_signature.numerator as f64).floor() as u32 + 1;

        ui.monospace(format!("{minutes:02}:{secs:05.2}"));
        ui.separator();
        ui.monospace(format!("{bar}.{beat_in_bar}"));

        ui.separator();

        // BPM
        ui.label("BPM:");
        let mut bpm = app.project.tempo.bpm;
        let response = ui.add(
            egui::DragValue::new(&mut bpm)
                .range(20.0..=300.0)
                .speed(0.5),
        );
        if response.changed() {
            app.project.tempo.bpm = bpm;
            app.sync_project();
        }

        ui.separator();

        // Time signature
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

        // Zoom
        ui.label("Zoom:");
        ui.add(
            egui::DragValue::new(&mut app.zoom)
                .range(0.1..=10.0)
                .speed(0.05),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Session indicator
            if app.session.is_connected() {
                ui.colored_label(
                    egui::Color32::from_rgb(80, 200, 80),
                    "● Online",
                );
            }
            ui.label(
                egui::RichText::new("JamHub")
                    .strong()
                    .color(egui::Color32::from_rgb(100, 180, 255)),
            );
        });
    });
}
