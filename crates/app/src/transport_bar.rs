use eframe::egui;
use jamhub_engine::EngineCommand;
use jamhub_model::TransportState;

use crate::DawApp;

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        let state = app.transport_state();
        let btn_size = egui::vec2(28.0, 20.0);

        // === TRANSPORT CONTROLS (grouped) ===
        ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::from_rgb(50, 50, 58);

        if ui.add_sized(btn_size, egui::Button::new("⏮")).on_hover_text("Rewind to start of project [Home]").clicked() {
            app.send_command(EngineCommand::SetPosition(0));
        }

        let stop_bg = if state == TransportState::Stopped { egui::Color32::from_rgb(70, 70, 80) } else { egui::Color32::from_rgb(50, 50, 58) };
        if ui.add_sized(btn_size, egui::Button::new(egui::RichText::new("⏹").color(egui::Color32::WHITE)).fill(stop_bg))
            .on_hover_text("Stop playback and stay at current position").clicked() {
            app.send_command(EngineCommand::Stop);
        }

        let play_bg = if state == TransportState::Playing { egui::Color32::from_rgb(30, 120, 30) } else { egui::Color32::from_rgb(50, 50, 58) };
        if ui.add_sized(btn_size, egui::Button::new(egui::RichText::new("▶").color(egui::Color32::WHITE)).fill(play_bg))
            .on_hover_text("Start playback from current position [Space]").clicked() {
            app.send_command(EngineCommand::Play);
        }

        let rec_bg = if app.is_recording { egui::Color32::from_rgb(180, 30, 30) } else { egui::Color32::from_rgb(50, 50, 58) };
        if ui.add_sized(btn_size, egui::Button::new(egui::RichText::new("⏺").color(
            if app.is_recording { egui::Color32::WHITE } else { egui::Color32::from_rgb(200, 80, 80) }
        )).fill(rec_bg))
            .on_hover_text("Record audio from microphone onto selected track [R]\nTrack is muted during recording to avoid feedback").clicked() {
            app.toggle_recording();
        }

        ui.add_space(4.0);

        // === TOGGLE BUTTONS (metronome, loop, monitor) ===
        let toggle_size = egui::vec2(24.0, 20.0);

        let met_bg = if app.metronome_enabled { egui::Color32::from_rgb(140, 120, 20) } else { egui::Color32::from_rgb(45, 45, 50) };
        if ui.add_sized(toggle_size, egui::Button::new(egui::RichText::new("M").small().color(egui::Color32::WHITE)).fill(met_bg))
            .on_hover_text("Metronome — click track for keeping time [M]\nAccent on beat 1, lighter on other beats").clicked() {
            app.metronome_enabled = !app.metronome_enabled;
            app.send_command(EngineCommand::SetMetronome(app.metronome_enabled));
        }

        let loop_bg = if app.loop_enabled { egui::Color32::from_rgb(40, 70, 150) } else { egui::Color32::from_rgb(45, 45, 50) };
        if ui.add_sized(toggle_size, egui::Button::new(egui::RichText::new("L").small().color(egui::Color32::WHITE)).fill(loop_bg))
            .on_hover_text("Loop mode — repeat playback between loop markers [L]\nBlue region shown on timeline").clicked() {
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
        }

        let mon_bg = if app.input_monitor.is_enabled() { egui::Color32::from_rgb(150, 90, 20) } else { egui::Color32::from_rgb(45, 45, 50) };
        if ui.add_sized(toggle_size, egui::Button::new(egui::RichText::new("I").small().color(egui::Color32::WHITE)).fill(mon_bg))
            .on_hover_text("Input monitoring — hear your microphone in real-time [I]\nUseful for monitoring while recording").clicked() {
            app.toggle_input_monitor();
        }

        ui.separator();

        // Automation toggle
        let auto_bg = if app.show_automation { egui::Color32::from_rgb(140, 100, 20) } else { egui::Color32::from_rgb(45, 45, 50) };
        if ui.add_sized(toggle_size, egui::Button::new(egui::RichText::new("A").small().color(egui::Color32::WHITE)).fill(auto_bg))
            .on_hover_text("Show automation lanes [A]\nClick on timeline to add control points\nAutomate volume, pan, or mute over time").clicked() {
            app.show_automation = !app.show_automation;
        }

        ui.separator();

        // === TIME DISPLAY (big, readable) ===
        let pos = app.position_samples();
        let sr = app.sample_rate();
        let seconds = pos as f64 / sr as f64;
        let minutes = (seconds / 60.0) as u32;
        let secs = seconds % 60.0;

        let beat = app.project.tempo.beat_at_sample(pos, sr as f64);
        let bar = (beat / app.project.time_signature.numerator as f64).floor() as u32 + 1;
        let beat_in_bar = (beat % app.project.time_signature.numerator as f64).floor() as u32 + 1;

        // Time in a dark box
        egui::Frame::default()
            .fill(egui::Color32::from_rgb(20, 20, 25))
            .inner_margin(egui::Margin::symmetric(8, 2))
            .corner_radius(3.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.monospace(egui::RichText::new(format!("{minutes:02}:{secs:05.2}")).size(14.0).color(egui::Color32::from_rgb(120, 220, 120)));
                    ui.label(egui::RichText::new("|").color(egui::Color32::from_rgb(60, 60, 70)));
                    ui.monospace(egui::RichText::new(format!("{bar}.{beat_in_bar}")).size(14.0).color(egui::Color32::from_rgb(200, 180, 100)));
                });
            });

        ui.separator();

        // === SNAP MODE ===
        let snap_label = format!("Snap: {}", app.snap_mode.label());
        let snap_bg = if app.snap_mode != crate::SnapMode::Off {
            egui::Color32::from_rgb(40, 60, 100)
        } else {
            egui::Color32::from_rgb(45, 45, 50)
        };
        if ui.add(egui::Button::new(egui::RichText::new(&snap_label).small().color(egui::Color32::WHITE)).fill(snap_bg))
            .on_hover_text("Snap mode — controls positioning precision [G]\nFree: sample-accurate\n1/2 Beat: eighth notes\nBeat: quarter notes\nBar: bar boundaries").clicked() {
            app.snap_mode = app.snap_mode.next();
        }

        ui.separator();

        // === TEMPO & TIME SIG (compact) ===
        let mut bpm = app.project.tempo.bpm;
        if ui.add(egui::DragValue::new(&mut bpm).range(20.0..=300.0).speed(0.5).suffix(" bpm")).changed() {
            app.project.tempo.bpm = bpm;
            app.sync_project();
        }

        let mut num = app.project.time_signature.numerator as i32;
        let mut den = app.project.time_signature.denominator as i32;
        ui.add(egui::DragValue::new(&mut num).range(1..=16).speed(0.1));
        ui.label("/");
        ui.add(egui::DragValue::new(&mut den).range(1..=16).speed(0.1));
        if num != app.project.time_signature.numerator as i32 || den != app.project.time_signature.denominator as i32 {
            app.project.time_signature.numerator = num as u8;
            app.project.time_signature.denominator = den as u8;
            app.sync_project();
        }

        ui.separator();

        // === MASTER VOL (with icon) ===
        ui.label(egui::RichText::new("Vol").small().color(egui::Color32::GRAY));
        let mut mv = app.master_volume;
        if ui.add(egui::DragValue::new(&mut mv).range(0.0..=1.5).speed(0.01)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed() {
            app.master_volume = mv;
            app.send_command(EngineCommand::SetMasterVolume(mv));
        }

        // === RIGHT SIDE ===
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.session.is_connected() {
                ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "● Online");
                ui.separator();
            }
            ui.label(egui::RichText::new("JamHub").strong().color(egui::Color32::from_rgb(100, 180, 255)));
        });
    });
}
