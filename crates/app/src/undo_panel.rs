use eframe::egui;
use std::time::Instant;

use crate::DawApp;

/// Format an Instant relative to now as a human-readable string.
fn relative_time(ts: Instant) -> String {
    let elapsed = ts.elapsed().as_secs();
    if elapsed < 5 {
        "just now".to_string()
    } else if elapsed < 60 {
        format!("{}s ago", elapsed)
    } else if elapsed < 3600 {
        let mins = elapsed / 60;
        if mins == 1 {
            "1 min ago".to_string()
        } else {
            format!("{} min ago", mins)
        }
    } else {
        let hours = elapsed / 3600;
        if hours == 1 {
            "1 hr ago".to_string()
        } else {
            format!("{} hrs ago", hours)
        }
    }
}

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_undo_history {
        return;
    }

    // Continuously repaint while this panel is open so relative timestamps update
    ctx.request_repaint_after(std::time::Duration::from_secs(5));

    let mut open = true;
    let accent = egui::Color32::from_rgb(90, 160, 255);
    let dim = egui::Color32::from_rgb(140, 140, 148);
    let undo_color = egui::Color32::from_rgb(200, 180, 100);
    let redo_color = egui::Color32::from_rgb(100, 180, 200);

    // Collect jump action outside the window closure to satisfy borrow checker
    let mut jump_action: Option<JumpAction> = None;
    let mut clear_requested = false;

    let undo_count = app.undo_manager.undo_count();
    let redo_count = app.undo_manager.redo_count();

    egui::Window::new("Undo History")
        .open(&mut open)
        .default_width(280.0)
        .min_width(220.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} undo / {} redo steps",
                        undo_count, redo_count
                    ))
                    .small()
                    .color(dim),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            undo_count > 0 || redo_count > 0,
                            egui::Button::new(
                                egui::RichText::new("Clear").small(),
                            ),
                        )
                        .clicked()
                    {
                        clear_requested = true;
                    }
                });
            });
            ui.separator();

            // Buttons
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        app.undo_manager.can_undo(),
                        egui::Button::new("Undo"),
                    )
                    .clicked()
                {
                    jump_action = Some(JumpAction::Undo);
                }
                if ui
                    .add_enabled(
                        app.undo_manager.can_redo(),
                        egui::Button::new("Redo"),
                    )
                    .clicked()
                {
                    jump_action = Some(JumpAction::Redo);
                }
            });

            ui.separator();

            // Scrollable history list
            egui::ScrollArea::vertical()
                .max_height(400.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    // Redo entries (future states) — shown dimmed, topmost = furthest in future
                    let redo_entries = app.undo_manager.redo_entries();
                    if !redo_entries.is_empty() {
                        ui.label(
                            egui::RichText::new("Redo (future)")
                                .small()
                                .color(dim),
                        );
                        // Show redo stack top-to-bottom (most recent undo at top)
                        for i in (0..redo_entries.len()).rev() {
                            let entry = &redo_entries[i];
                            let time_str = relative_time(entry.timestamp);
                            ui.horizontal(|ui| {
                                let label_text = egui::RichText::new(&entry.label)
                                    .color(redo_color);
                                if ui
                                    .add(egui::Label::new(label_text).sense(egui::Sense::click()))
                                    .clicked()
                                {
                                    jump_action = Some(JumpAction::JumpRedo(i));
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(time_str).small().color(dim),
                                        );
                                    },
                                );
                            });
                        }
                        ui.separator();
                    }

                    // Current state marker
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("\u{25b6} Current State")
                                .strong()
                                .color(accent),
                        );
                    });

                    // Undo entries (past states) — most recent at top
                    let undo_entries = app.undo_manager.undo_entries();
                    if !undo_entries.is_empty() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new("Undo (past)")
                                .small()
                                .color(dim),
                        );
                        for i in (0..undo_entries.len()).rev() {
                            let entry = &undo_entries[i];
                            let time_str = relative_time(entry.timestamp);
                            ui.horizontal(|ui| {
                                let label_text = egui::RichText::new(&entry.label)
                                    .color(undo_color);
                                if ui
                                    .add(egui::Label::new(label_text).sense(egui::Sense::click()))
                                    .clicked()
                                {
                                    jump_action = Some(JumpAction::JumpUndo(i));
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(time_str).small().color(dim),
                                        );
                                    },
                                );
                            });
                        }
                    }
                });

            ui.separator();
            ui.label(
                egui::RichText::new("Tip: Cmd+Z / Cmd+Shift+Z  •  Click entry to jump")
                    .small()
                    .color(dim),
            );
        });

    // Apply actions after the UI closure
    match jump_action {
        Some(JumpAction::Undo) => {
            app.undo();
        }
        Some(JumpAction::Redo) => {
            app.redo();
        }
        Some(JumpAction::JumpUndo(idx)) => {
            if let Some(project) = app.undo_manager.jump_to_undo(idx, &app.project) {
                app.project = project;
                app.sync_project();
                app.set_status("Jumped to undo state");
            }
        }
        Some(JumpAction::JumpRedo(idx)) => {
            if let Some(project) = app.undo_manager.jump_to_redo(idx, &app.project) {
                app.project = project;
                app.sync_project();
                app.set_status("Jumped to redo state");
            }
        }
        None => {}
    }

    if clear_requested {
        app.undo_manager.clear();
        app.set_status("Undo history cleared");
    }

    if !open {
        app.show_undo_history = false;
    }
}

enum JumpAction {
    Undo,
    Redo,
    JumpUndo(usize),
    JumpRedo(usize),
}
