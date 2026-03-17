use eframe::egui;

use crate::DawApp;

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_about {
        return;
    }

    let mut open = true;
    egui::Window::new("About ThroughWaves")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(400.0)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);
                // Waveform logo
                let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
                crate::draw_waveform_logo(
                    ui.painter(), icon_rect.center(), 44.0,
                    egui::Color32::from_rgb(235, 180, 60),
                    egui::Color32::from_rgb(20, 18, 14),
                );
                ui.add_space(4.0);
                ui.heading(
                    egui::RichText::new("ThroughWaves")
                        .size(28.0)
                        .color(egui::Color32::from_rgb(240, 192, 64)),
                );
                ui.label("Professional Digital Audio Workstation");
                ui.add_space(3.0);
                ui.label(
                    egui::RichText::new("v1.0.0")
                        .color(egui::Color32::GRAY),
                );
                ui.add_space(15.0);
            });

            ui.separator();
            ui.add_space(5.0);

            ui.label("Make music together, from anywhere.");
            ui.add_space(10.0);

            egui::Grid::new("about_grid").show(ui, |ui| {
                ui.label(egui::RichText::new("Audio Engine").strong());
                ui.label("cpal + custom mixer with effects");
                ui.end_row();

                ui.label(egui::RichText::new("Formats").strong());
                ui.label("WAV, MP3, OGG, FLAC");
                ui.end_row();

                ui.label(egui::RichText::new("Effects").strong());
                ui.label("Gain, LowPass, HighPass, Delay, Reverb");
                ui.end_row();

                ui.label(egui::RichText::new("Collaboration").strong());
                ui.label("WebSocket real-time sessions");
                ui.end_row();

                ui.label(egui::RichText::new("Built with").strong());
                ui.label("Rust, egui, symphonia");
                ui.end_row();
            });

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Keyboard shortcuts:")
                        .strong(),
                );
            });
            ui.add_space(3.0);

            egui::Grid::new("shortcuts_grid")
                .striped(true)
                .show(ui, |ui| {
                    let shortcuts = [
                        ("Space", "Play / Stop"),
                        ("R", "Record"),
                        ("M", "Metronome"),
                        ("L", "Loop mode"),
                        ("Home", "Rewind"),
                        ("Up/Down", "Switch tracks"),
                        ("1-9", "Select track"),
                        ("Del", "Delete clip/track"),
                        ("Cmd+Z", "Undo"),
                        ("Cmd+Shift+Z", "Redo"),
                        ("Cmd+S", "Save"),
                        ("Cmd+D", "Duplicate track"),
                        ("Cmd+E", "Effects panel"),
                        ("Cmd+I", "Import audio"),
                        ("Cmd+Scroll", "Zoom"),
                        ("Double-click", "Rename track"),
                        ("Right-click", "Context menus"),
                    ];
                    for (key, desc) in &shortcuts {
                        ui.monospace(*key);
                        ui.label(*desc);
                        ui.end_row();
                    }
                });
        });

    if !open {
        app.show_about = false;
    }
}
