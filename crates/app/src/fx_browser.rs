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

            // Plugin list
            egui::ScrollArea::vertical().show(ui, |ui| {
                let filter_lower = app.fx_browser.filter.to_lowercase();

                for plugin in &app.fx_browser.plugins {
                    // Apply filters
                    if !filter_lower.is_empty() && !plugin.name.to_lowercase().contains(&filter_lower) {
                        continue;
                    }
                    if let Some(ref cat) = app.fx_browser.category_filter {
                        if &plugin.category != cat {
                            continue;
                        }
                    }

                    let cat_icon = match plugin.category {
                        VstCategory::Instrument => "🎹",
                        VstCategory::Effect => "🎛",
                        VstCategory::Analyzer => "📊",
                        VstCategory::Unknown => "?",
                    };

                    let plugin_path = plugin.path.clone();
                    let plugin_name = plugin.name.clone();
                    let is_loaded = app.fx_browser.loaded_plugins.iter().any(|l| l.path == plugin_path && l.loaded);

                    ui.horizontal(|ui| {
                        ui.label(cat_icon);
                        if is_loaded {
                            ui.strong(egui::RichText::new(&plugin_name).color(egui::Color32::from_rgb(80, 200, 80)));
                        } else {
                            ui.strong(&plugin_name);
                        }

                        if is_loaded {
                            ui.label(egui::RichText::new("loaded").small().color(egui::Color32::from_rgb(80, 200, 80)));
                        } else {
                            if ui.small_button("Load").on_hover_text("Load this plugin").clicked() {
                                let instance = jamhub_engine::vst_loader::VstInstance::load(&plugin_path);
                                if instance.loaded {
                                    app.fx_browser.load_status = Some(format!("Loaded: {plugin_name}"));
                                } else {
                                    app.fx_browser.load_status = Some(format!("Failed: {}", instance.error.as_deref().unwrap_or("unknown error")));
                                }
                                app.fx_browser.loaded_plugins.push(instance);
                            }
                        }

                        ui.label(
                            egui::RichText::new(&plugin.path.to_string_lossy().to_string())
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
