use eframe::egui;

use crate::DawApp;

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_undo_history {
        return;
    }

    let mut open = true;
    egui::Window::new("Undo History")
        .open(&mut open)
        .default_width(250.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Action history — click to jump")
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.separator();

            // Show undo stack (most recent first)
            let can_undo = app.undo_manager.can_undo();
            let can_redo = app.undo_manager.can_redo();

            ui.horizontal(|ui| {
                if ui.add_enabled(can_undo, egui::Button::new("Undo")).clicked() {
                    app.undo();
                }
                if ui.add_enabled(can_redo, egui::Button::new("Redo")).clicked() {
                    app.redo();
                }
            });

            ui.separator();

            if let Some(label) = app.undo_manager.undo_label() {
                ui.label(
                    egui::RichText::new(format!("Next undo: {label}"))
                        .color(egui::Color32::from_rgb(200, 180, 100)),
                );
            }
            if let Some(label) = app.undo_manager.redo_label() {
                ui.label(
                    egui::RichText::new(format!("Next redo: {label}"))
                        .color(egui::Color32::from_rgb(100, 180, 200)),
                );
            }

            ui.separator();
            ui.label(
                egui::RichText::new("Tip: Cmd+Z to undo, Cmd+Shift+Z to redo")
                    .small()
                    .color(egui::Color32::GRAY),
            );
        });

    if !open {
        app.show_undo_history = false;
    }
}
