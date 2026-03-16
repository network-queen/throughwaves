use eframe::egui;

use crate::DawApp;

/// A single keyboard shortcut entry for display.
struct ShortcutEntry {
    key: &'static str,
    description: &'static str,
    category: ShortcutCategory,
}

#[derive(Clone, Copy, PartialEq)]
enum ShortcutCategory {
    Transport,
    Recording,
    Editing,
    Navigation,
    View,
    Midi,
}

impl ShortcutCategory {
    fn label(self) -> &'static str {
        match self {
            Self::Transport => "Transport",
            Self::Recording => "Recording",
            Self::Editing => "Editing",
            Self::Navigation => "Navigation",
            Self::View => "View",
            Self::Midi => "MIDI",
        }
    }

    fn color(self) -> egui::Color32 {
        match self {
            Self::Transport => egui::Color32::from_rgb(100, 200, 140),
            Self::Recording => egui::Color32::from_rgb(220, 90, 90),
            Self::Editing => egui::Color32::from_rgb(100, 170, 255),
            Self::Navigation => egui::Color32::from_rgb(220, 190, 100),
            Self::View => egui::Color32::from_rgb(170, 140, 220),
            Self::Midi => egui::Color32::from_rgb(100, 220, 220),
        }
    }

    const ALL: [ShortcutCategory; 6] = [
        Self::Transport,
        Self::Recording,
        Self::Editing,
        Self::Navigation,
        Self::View,
        Self::Midi,
    ];
}

fn all_shortcuts() -> Vec<ShortcutEntry> {
    vec![
        // Transport
        ShortcutEntry { key: "Space", description: "Play / Stop", category: ShortcutCategory::Transport },
        ShortcutEntry { key: "Home", description: "Rewind to start", category: ShortcutCategory::Transport },
        ShortcutEntry { key: "M", description: "Toggle metronome", category: ShortcutCategory::Transport },
        ShortcutEntry { key: "L", description: "Toggle loop mode", category: ShortcutCategory::Transport },
        ShortcutEntry { key: "H", description: "Toggle follow playhead", category: ShortcutCategory::Transport },
        ShortcutEntry { key: "F", description: "Focus / scroll to playhead", category: ShortcutCategory::Transport },
        ShortcutEntry { key: "C", description: "Toggle count-in", category: ShortcutCategory::Transport },
        ShortcutEntry { key: "P", description: "Toggle punch in/out", category: ShortcutCategory::Transport },

        // Recording
        ShortcutEntry { key: "R", description: "Start / stop recording", category: ShortcutCategory::Recording },
        ShortcutEntry { key: "I", description: "Toggle input monitor", category: ShortcutCategory::Recording },
        ShortcutEntry { key: "T", description: "Toggle take lanes", category: ShortcutCategory::Recording },
        ShortcutEntry { key: "Shift+F", description: "Flatten comp (remove inactive takes)", category: ShortcutCategory::Recording },

        // Editing
        ShortcutEntry { key: "S", description: "Split clip at playhead", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Del / Backspace", description: "Delete selected clip or track", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Escape", description: "Clear selection", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "G", description: "Cycle snap mode", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Alt+Left", description: "Nudge clip(s) left", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Alt+Right", description: "Nudge clip(s) right", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+Z", description: "Undo", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+Shift+Z", description: "Redo", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+S", description: "Save project", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+D", description: "Duplicate track or clips", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+A", description: "Select all clips on track", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+C", description: "Copy selected clips", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+V", description: "Paste clips", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+I", description: "Import audio file", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+B", description: "Bounce track", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+Shift+B", description: "Bounce selection range", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+M", description: "Mute/unmute selected track", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+Shift+M", description: "Add marker at playhead", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+Shift+S", description: "Solo/unsolo selected track", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Cmd+J", description: "Consolidate clips", category: ShortcutCategory::Editing },
        ShortcutEntry { key: "Shift+R", description: "Toggle ripple editing", category: ShortcutCategory::Editing },

        // Navigation
        ShortcutEntry { key: "Up / Down", description: "Switch tracks", category: ShortcutCategory::Navigation },
        ShortcutEntry { key: "1-9", description: "Select track by number", category: ShortcutCategory::Navigation },
        ShortcutEntry { key: "[", description: "Jump to previous marker", category: ShortcutCategory::Navigation },
        ShortcutEntry { key: "]", description: "Jump to next marker", category: ShortcutCategory::Navigation },
        ShortcutEntry { key: "Z", description: "Zoom to fit / selection", category: ShortcutCategory::Navigation },
        ShortcutEntry { key: "Cmd+Scroll", description: "Zoom in / out", category: ShortcutCategory::Navigation },

        // View
        ShortcutEntry { key: "Cmd+E", description: "Toggle effects panel", category: ShortcutCategory::View },
        ShortcutEntry { key: "Cmd+F", description: "Toggle FX browser", category: ShortcutCategory::View },
        ShortcutEntry { key: "A", description: "Toggle automation lanes", category: ShortcutCategory::View },
        ShortcutEntry { key: "Q", description: "Toggle spectrum analyzer", category: ShortcutCategory::View },
        ShortcutEntry { key: "Tab", description: "Cycle views (Arrange/Mixer/Session)", category: ShortcutCategory::View },
        ShortcutEntry { key: "?", description: "Open this shortcuts panel", category: ShortcutCategory::View },

        // MIDI
        ShortcutEntry { key: "Cmd+P", description: "Toggle piano roll", category: ShortcutCategory::Midi },
        ShortcutEntry { key: "Double-click", description: "Rename track", category: ShortcutCategory::Midi },
        ShortcutEntry { key: "Right-click", description: "Context menus", category: ShortcutCategory::Midi },
    ]
}

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_shortcuts {
        return;
    }

    let mut open = true;
    egui::Window::new("Keyboard Shortcuts")
        .open(&mut open)
        .collapsible(true)
        .resizable(true)
        .default_width(520.0)
        .default_height(500.0)
        .show(ctx, |ui| {
            // Search filter
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Filter:").color(egui::Color32::from_rgb(140, 140, 150)));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app.shortcuts_filter)
                        .hint_text("Type to search shortcuts...")
                        .desired_width(250.0),
                );
                if response.changed() {
                    // Filter is applied below
                }
                if !app.shortcuts_filter.is_empty() {
                    if ui.small_button("Clear").clicked() {
                        app.shortcuts_filter.clear();
                    }
                }
            });

            ui.add_space(6.0);

            let all = all_shortcuts();
            let filter_lower = app.shortcuts_filter.to_lowercase();

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for category in ShortcutCategory::ALL {
                        let entries: Vec<&ShortcutEntry> = all
                            .iter()
                            .filter(|e| e.category == category)
                            .filter(|e| {
                                if filter_lower.is_empty() {
                                    true
                                } else {
                                    e.key.to_lowercase().contains(&filter_lower)
                                        || e.description.to_lowercase().contains(&filter_lower)
                                        || e.category.label().to_lowercase().contains(&filter_lower)
                                }
                            })
                            .collect();

                        if entries.is_empty() {
                            continue;
                        }

                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(category.label())
                                .size(13.0)
                                .strong()
                                .color(category.color()),
                        );
                        ui.add_space(2.0);

                        egui::Grid::new(format!("shortcuts_{}", category.label()))
                            .striped(true)
                            .min_col_width(120.0)
                            .spacing([20.0, 3.0])
                            .show(ui, |ui| {
                                for entry in &entries {
                                    ui.monospace(
                                        egui::RichText::new(entry.key)
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(220, 200, 140)),
                                    );
                                    ui.label(
                                        egui::RichText::new(entry.description)
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(190, 190, 200)),
                                    );
                                    ui.end_row();
                                }
                            });

                        ui.add_space(4.0);
                        ui.separator();
                    }
                });
        });

    if !open {
        app.show_shortcuts = false;
    }
}
