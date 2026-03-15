use eframe::egui;
use jamhub_engine::{VstCategory, VstPluginInfo, VstScanner};

use crate::DawApp;

pub struct FxBrowser {
    pub show: bool,
    pub plugins: Vec<VstPluginInfo>,
    pub scanned: bool,
    pub filter: String,
    pub category_filter: Option<VstCategory>,
    pub loaded_plugins: Vec<jamhub_engine::vst_loader::VstInstance>,
    pub load_status: Option<String>,
}

impl Default for FxBrowser {
    fn default() -> Self {
        Self {
            show: false,
            plugins: Vec::new(),
            scanned: false,
            filter: String::new(),
            category_filter: None,
            loaded_plugins: Vec::new(),
            load_status: None,
        }
    }
}

impl FxBrowser {
    pub fn scan_if_needed(&mut self) {
        if !self.scanned {
            self.plugins = VstScanner::scan();
            // Guess categories
            for p in &mut self.plugins {
                p.category = jamhub_engine::vst_host::guess_category(&p.name);
                p.is_instrument = p.category == VstCategory::Instrument;
            }
            self.scanned = true;
        }
    }
}

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.fx_browser.show {
        return;
    }

    app.fx_browser.scan_if_needed();

    let mut open = true;
    egui::Window::new("Plugin Browser")
        .open(&mut open)
        .default_size([400.0, 500.0])
        .show(ctx, |ui| {
            ui.heading("Installed Plugins");
            ui.label(
                egui::RichText::new(format!("{} plugins found", app.fx_browser.plugins.len()))
                    .small()
                    .color(egui::Color32::GRAY),
            );

            ui.separator();

            // Filter bar
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut app.fx_browser.filter);
            });

            ui.horizontal(|ui| {
                ui.label("Category:");
                if ui.selectable_label(app.fx_browser.category_filter.is_none(), "All").clicked() {
                    app.fx_browser.category_filter = None;
                }
                if ui.selectable_label(app.fx_browser.category_filter == Some(VstCategory::Effect), "Effects").clicked() {
                    app.fx_browser.category_filter = Some(VstCategory::Effect);
                }
                if ui.selectable_label(app.fx_browser.category_filter == Some(VstCategory::Instrument), "Instruments").clicked() {
                    app.fx_browser.category_filter = Some(VstCategory::Instrument);
                }
            });

            ui.separator();

            // Load status
            if let Some(ref status) = app.fx_browser.load_status {
                let color = if status.starts_with("Loaded") {
                    egui::Color32::from_rgb(80, 200, 80)
                } else {
                    egui::Color32::from_rgb(220, 80, 80)
                };
                ui.colored_label(color, status);
            }

            // Loaded count
            let loaded_count = app.fx_browser.loaded_plugins.iter().filter(|p| p.loaded).count();
            if loaded_count > 0 {
                ui.label(egui::RichText::new(format!("{loaded_count} plugin(s) loaded"))
                    .small().color(egui::Color32::from_rgb(80, 200, 80)));
            }

            ui.separator();

            if ui.button("Rescan Plugins").clicked() {
                app.fx_browser.scanned = false;
                app.fx_browser.scan_if_needed();
            }

            ui.separator();

            // Plugin list — collect filtered list first to avoid borrow issues
            let filter_lower = app.fx_browser.filter.to_lowercase();
            let visible_plugins: Vec<(String, std::path::PathBuf, VstCategory, bool)> =
                app.fx_browser.plugins.iter()
                    .filter(|p| filter_lower.is_empty() || p.name.to_lowercase().contains(&filter_lower))
                    .filter(|p| app.fx_browser.category_filter.as_ref().map_or(true, |c| &p.category == c))
                    .map(|p| {
                        let is_loaded = app.fx_browser.loaded_plugins.iter().any(|l| l.path == p.path && l.loaded);
                        (p.name.clone(), p.path.clone(), p.category.clone(), is_loaded)
                    })
                    .collect();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (plugin_name, plugin_path, category, is_loaded) in &visible_plugins {
                    let cat_icon = match category {
                        VstCategory::Instrument => "🎹",
                        VstCategory::Effect => "🎛",
                        VstCategory::Analyzer => "📊",
                        VstCategory::Unknown => "?",
                    };

                    ui.horizontal(|ui| {
                        ui.label(cat_icon);
                        if *is_loaded {
                            ui.strong(egui::RichText::new(plugin_name).color(egui::Color32::from_rgb(80, 200, 80)));
                        } else {
                            ui.strong(plugin_name);
                        }

                        if *is_loaded {
                            ui.label(egui::RichText::new("✓").small().color(egui::Color32::from_rgb(80, 200, 80)));
                            if ui.small_button("+ Track FX")
                                .on_hover_text("Add this plugin to the selected track's FX chain")
                                .clicked()
                            {
                                if let Some(ti) = app.selected_track {
                                    if ti < app.project.tracks.len() {
                                        app.push_undo("Add VST plugin");
                                        app.project.tracks[ti].effects.push(
                                            jamhub_model::TrackEffect::Vst3Plugin {
                                                path: plugin_path.to_string_lossy().to_string(),
                                                name: plugin_name.clone(),
                                            },
                                        );
                                        app.sync_project();
                                        app.fx_browser.load_status = Some(format!("Added {plugin_name} to track FX chain"));
                                    }
                                }
                            }
                        } else {
                            if ui.small_button("Load").on_hover_text("Load this plugin into memory").clicked() {
                                let instance = jamhub_engine::vst3_host::Vst3Plugin::load(
                                    plugin_path,
                                    app.sample_rate() as f64,
                                    256,
                                );
                                if instance.loaded {
                                    app.fx_browser.load_status = Some(format!("Loaded: {plugin_name}"));
                                } else {
                                    app.fx_browser.load_status = Some(format!("Failed: {}", instance.error.as_deref().unwrap_or("unknown error")));
                                }
                                app.fx_browser.loaded_plugins.push(
                                    jamhub_engine::vst_loader::VstInstance {
                                        name: instance.name.clone(),
                                        path: instance.path.clone(),
                                        loaded: instance.loaded,
                                        error: instance.error.clone(),
                                        _lib: None,
                                    },
                                );
                            }
                        }

                        ui.label(
                            egui::RichText::new(plugin_path.to_string_lossy().to_string())
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    });
                }

                if app.fx_browser.plugins.is_empty() {
                    ui.label("No plugins found. Check your VST directories.");
                    ui.label(
                        egui::RichText::new("macOS: /Library/Audio/Plug-Ins/VST3/")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                }
            });
        });

    if !open {
        app.fx_browser.show = false;
    }
}
