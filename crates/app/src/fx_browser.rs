use eframe::egui;
use jamhub_engine::{VstCategory, VstPluginInfo, VstScanner};
use jamhub_model::{EffectSlot, TrackEffect};

use crate::DawApp;

/// User-created folder for organizing plugins.
/// Items are stored as string IDs: VST paths or "builtin:<name>" for built-in effects.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FxFolder {
    pub name: String,
    /// Legacy field — migrated to `items` on load
    #[serde(default)]
    pub plugin_paths: Vec<std::path::PathBuf>,
    /// Generic item IDs: "builtin:Gain", "/Library/.../Plugin.vst3", etc.
    #[serde(default)]
    pub items: Vec<String>,
}

impl FxFolder {
    pub fn contains_item(&self, id: &str) -> bool {
        self.items.contains(&id.to_string())
            || self.plugin_paths.iter().any(|p| p.to_string_lossy() == id)
    }

    pub fn add_item(&mut self, id: &str) {
        if !self.contains_item(id) {
            self.items.push(id.to_string());
        }
    }

    pub fn remove_item(&mut self, id: &str) {
        self.items.retain(|i| i != id);
        self.plugin_paths.retain(|p| p.to_string_lossy() != id);
    }
}

/// Make a folder item ID for a built-in effect.
pub fn builtin_id(name: &str) -> String {
    format!("builtin:{name}")
}

/// Make a folder item ID for a VST plugin.
pub fn vst_id(path: &std::path::Path) -> String {
    path.to_string_lossy().to_string()
}

pub struct FxBrowser {
    pub show: bool,
    pub plugins: Vec<VstPluginInfo>,
    pub scanned: bool,
    pub filter: String,
    pub category_filter: Option<VstCategory>,
    pub loaded_plugins: Vec<jamhub_engine::vst_loader::VstInstance>,
    pub load_status: Option<String>,
    /// User-created folders for organizing plugins
    pub folders: Vec<FxFolder>,
    /// Currently selected folder index (None = show all)
    pub selected_folder: Option<usize>,
    /// UI state for creating a new folder
    pub new_folder_name: String,
    pub show_new_folder: bool,
    /// UI state for renaming
    pub renaming_folder: Option<(usize, String)>,
    /// Which folder to add a plugin to (folder_idx when right-click menu is open)
    pub add_to_folder_plugin: Option<std::path::PathBuf>,
    /// Show built-in effects category
    pub show_builtin: bool,
    /// Filter by vendor name
    pub vendor_filter: Option<String>,
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
            folders: load_folders(),
            selected_folder: None,
            new_folder_name: String::new(),
            show_new_folder: false,
            renaming_folder: None,
            add_to_folder_plugin: None,
            show_builtin: false,
            vendor_filter: None,
        }
    }
}

impl FxBrowser {
    pub fn scan_if_needed(&mut self) {
        if !self.scanned {
            self.plugins = VstScanner::scan();
            for p in &mut self.plugins {
                p.category = jamhub_engine::vst_host::guess_category(&p.name);
                p.is_instrument = p.category == VstCategory::Instrument;
            }
            self.scanned = true;
        }
    }

    /// Scan for plugins and load all of them at startup.
    pub fn scan_and_load_all(&mut self, sample_rate: u32) {
        self.scan_if_needed();
        println!("FX Browser: loading {} plugins...", self.plugins.len());
        for p in &self.plugins {
            // Skip if already loaded
            if self.loaded_plugins.iter().any(|l| l.path == p.path) {
                continue;
            }
            let instance = jamhub_engine::vst3_host::Vst3Plugin::load(
                &p.path,
                sample_rate as f64,
                256,
            );
            self.loaded_plugins.push(
                jamhub_engine::vst_loader::VstInstance {
                    name: instance.name.clone(),
                    path: instance.path.clone(),
                    loaded: instance.loaded,
                    error: instance.error.clone(),
                    _lib: None,
                },
            );
        }
        let loaded_count = self.loaded_plugins.iter().filter(|p| p.loaded).count();
        println!("FX Browser: {loaded_count}/{} plugins loaded", self.plugins.len());
    }

    pub fn save_folders(&self) {
        save_folders(&self.folders);
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
        .default_size([520.0, 550.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Plugins");
                ui.label(
                    egui::RichText::new(format!("{} found", app.fx_browser.plugins.len()))
                        .small()
                        .color(egui::Color32::GRAY),
                );
                let loaded_count = app
                    .fx_browser
                    .loaded_plugins
                    .iter()
                    .filter(|p| p.loaded)
                    .count();
                if loaded_count > 0 {
                    ui.label(
                        egui::RichText::new(format!("{loaded_count} loaded"))
                            .small()
                            .color(egui::Color32::from_rgb(80, 200, 80)),
                    );
                }
            });

            ui.separator();

            // Two-panel layout: folders on left, plugins on right
            ui.columns(2, |cols| {
                // --- Left panel: Vendors & Folders ---
                cols[0].set_min_width(140.0);

                // "All" button
                let no_filter = app.fx_browser.category_filter.is_none()
                    && app.fx_browser.selected_folder.is_none()
                    && !app.fx_browser.show_builtin
                    && app.fx_browser.vendor_filter.is_none();

                if cols[0].selectable_label(no_filter, "All").clicked() {
                    app.fx_browser.category_filter = None;
                    app.fx_browser.selected_folder = None;
                    app.fx_browser.show_builtin = false;
                    app.fx_browser.vendor_filter = None;
                }

                cols[0].add_space(4.0);
                cols[0].label(
                    egui::RichText::new("By Developer")
                        .size(10.0)
                        .color(egui::Color32::GRAY),
                );

                // "JamHub" (built-in)
                if cols[0]
                    .selectable_label(app.fx_browser.show_builtin, "  JamHub")
                    .clicked()
                {
                    app.fx_browser.show_builtin = true;
                    app.fx_browser.category_filter = None;
                    app.fx_browser.selected_folder = None;
                    app.fx_browser.vendor_filter = None;
                }

                // Collect unique vendors from scanned plugins
                let vendors: Vec<String> = app.fx_browser.plugins.iter()
                    .map(|p| p.vendor.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();

                for vendor in &vendors {
                    let is_selected = app.fx_browser.vendor_filter.as_ref() == Some(vendor);
                    if cols[0]
                        .selectable_label(is_selected, format!("  {vendor}"))
                        .clicked()
                    {
                        app.fx_browser.vendor_filter = Some(vendor.clone());
                        app.fx_browser.category_filter = None;
                        app.fx_browser.selected_folder = None;
                        app.fx_browser.show_builtin = false;
                    }
                }

                cols[0].add_space(8.0);
                cols[0].separator();
                cols[0].label(
                    egui::RichText::new("Folders")
                        .small()
                        .color(egui::Color32::GRAY),
                );

                let mut folder_to_delete: Option<usize> = None;
                let folders_len = app.fx_browser.folders.len();

                for fi in 0..folders_len {
                    let is_selected = app.fx_browser.selected_folder == Some(fi);
                    let folder = &app.fx_browser.folders[fi];
                    let count = folder.items.len() + folder.plugin_paths.len();

                    // Handle renaming
                    if let Some((rename_idx, ref mut new_name)) = app.fx_browser.renaming_folder {
                        if rename_idx == fi {
                            let resp = cols[0].text_edit_singleline(new_name);
                            if resp.lost_focus() {
                                let final_name = new_name.clone();
                                app.fx_browser.folders[fi].name = final_name;
                                app.fx_browser.renaming_folder = None;
                                app.fx_browser.save_folders();
                            }
                            continue;
                        }
                    }

                    let label = format!("  {} ({})", folder.name, count);
                    let resp = cols[0].selectable_label(is_selected, &label);
                    if resp.clicked() {
                        app.fx_browser.selected_folder = Some(fi);
                        app.fx_browser.category_filter = None;
                        app.fx_browser.show_builtin = false;
                        app.fx_browser.vendor_filter = None;
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Rename").clicked() {
                            app.fx_browser.renaming_folder =
                                Some((fi, app.fx_browser.folders[fi].name.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Delete").clicked() {
                            folder_to_delete = Some(fi);
                            ui.close_menu();
                        }
                    });
                }

                if let Some(idx) = folder_to_delete {
                    app.fx_browser.folders.remove(idx);
                    if app.fx_browser.selected_folder == Some(idx) {
                        app.fx_browser.selected_folder = None;
                    }
                    app.fx_browser.save_folders();
                }

                // New folder button
                if app.fx_browser.show_new_folder {
                    cols[0].horizontal(|ui| {
                        let resp =
                            ui.text_edit_singleline(&mut app.fx_browser.new_folder_name);
                        if resp.lost_focus()
                            || ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            let name = app.fx_browser.new_folder_name.trim().to_string();
                            if !name.is_empty() {
                                app.fx_browser.folders.push(FxFolder {
                                    name,
                                    plugin_paths: Vec::new(),
                                    items: Vec::new(),
                                });
                                app.fx_browser.save_folders();
                            }
                            app.fx_browser.new_folder_name.clear();
                            app.fx_browser.show_new_folder = false;
                        }
                    });
                } else {
                    if cols[0].small_button("+ New Folder").clicked() {
                        app.fx_browser.show_new_folder = true;
                    }
                }

                cols[0].add_space(4.0);
                if cols[0].small_button("Rescan Plugins").clicked() {
                    app.fx_browser.scanned = false;
                    app.fx_browser.scan_if_needed();
                }

                // --- Right panel: Plugin list ---
                cols[1].horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut app.fx_browser.filter);
                });

                // Load status
                if let Some(ref status) = app.fx_browser.load_status {
                    let color = if status.starts_with("Loaded") || status.starts_with("Added") {
                        egui::Color32::from_rgb(80, 200, 80)
                    } else {
                        egui::Color32::from_rgb(220, 80, 80)
                    };
                    cols[1].colored_label(color, status);
                }

                cols[1].separator();

                let show_builtin = app.fx_browser.show_builtin
                    || (app.fx_browser.category_filter.is_none()
                        && app.fx_browser.selected_folder.is_none()
                        && app.fx_browser.vendor_filter.is_none());
                let filter_lower_for_builtin = app.fx_browser.filter.to_lowercase();

                // Build filtered plugin list
                let filter_lower = app.fx_browser.filter.to_lowercase();
                let selected_folder_idx = app.fx_browser.selected_folder;

                let visible_plugins: Vec<(String, std::path::PathBuf, String, bool)> = app
                    .fx_browser
                    .plugins
                    .iter()
                    .filter(|p| {
                        filter_lower.is_empty()
                            || p.name.to_lowercase().contains(&filter_lower)
                    })
                    .filter(|p| {
                        if let Some(fi) = selected_folder_idx {
                            let vid = vst_id(&p.path);
                            app.fx_browser.folders[fi].contains_item(&vid)
                        } else if let Some(ref vendor) = app.fx_browser.vendor_filter {
                            &p.vendor == vendor
                        } else if app.fx_browser.show_builtin {
                            false // hide VSTs when "JamHub" (built-in) is selected
                        } else {
                            true // "All" — show everything
                        }
                    })
                    .map(|p| {
                        let is_loaded = app
                            .fx_browser
                            .loaded_plugins
                            .iter()
                            .any(|l| l.path == p.path && l.loaded);
                        (p.name.clone(), p.path.clone(), p.format.clone(), is_loaded)
                    })
                    .collect();

                // Track which folder we're viewing (for remove-from-folder)
                let viewing_folder_idx = app.fx_browser.selected_folder;

                egui::ScrollArea::vertical().show(&mut cols[1], |ui| {
                    // --- Built-in effects ---
                    let show_builtin_in_list = show_builtin || selected_folder_idx.is_some();
                    if show_builtin_in_list {
                        let built_ins: &[(&str, jamhub_model::TrackEffect)] = &[
                            ("Gain", jamhub_model::TrackEffect::Gain { db: 0.0 }),
                            ("Parametric EQ", jamhub_model::TrackEffect::ParametricEq { bands: vec![
                                jamhub_model::EqBandParams { freq_hz: 80.0, gain_db: 0.0, q: 0.7, band_type: jamhub_model::EqBandType::LowShelf },
                                jamhub_model::EqBandParams { freq_hz: 1000.0, gain_db: 0.0, q: 1.0, band_type: jamhub_model::EqBandType::Peak },
                                jamhub_model::EqBandParams { freq_hz: 8000.0, gain_db: 0.0, q: 0.7, band_type: jamhub_model::EqBandType::HighShelf },
                            ]}),
                            ("EQ Band", jamhub_model::TrackEffect::EqBand { freq_hz: 1000.0, gain_db: 0.0, q: 1.0 }),
                            ("Compressor", jamhub_model::TrackEffect::Compressor { threshold_db: -20.0, ratio: 4.0, attack_ms: 10.0, release_ms: 100.0 }),
                            ("Low Pass", jamhub_model::TrackEffect::LowPass { cutoff_hz: 5000.0 }),
                            ("High Pass", jamhub_model::TrackEffect::HighPass { cutoff_hz: 100.0 }),
                            ("Delay", jamhub_model::TrackEffect::Delay { time_ms: 250.0, feedback: 0.3, mix: 0.3 }),
                            ("Reverb", jamhub_model::TrackEffect::Reverb { decay: 0.7, mix: 0.3 }),
                            ("Chorus", jamhub_model::TrackEffect::Chorus { rate_hz: 1.0, depth: 0.5, mix: 0.3 }),
                            ("Distortion", jamhub_model::TrackEffect::Distortion { drive: 12.0, mix: 0.5 }),
                        ];

                        for (label, effect) in built_ins {
                            let bid = builtin_id(label);

                            // Filter by search
                            if !filter_lower_for_builtin.is_empty()
                                && !label.to_lowercase().contains(&filter_lower_for_builtin)
                            {
                                continue;
                            }
                            // Filter by folder
                            if let Some(fi) = selected_folder_idx {
                                if !app.fx_browser.folders[fi].contains_item(&bid) {
                                    continue;
                                }
                            }

                            egui::Frame::default()
                                .inner_margin(egui::Margin::symmetric(4, 2))
                                .corner_radius(3.0)
                                .stroke(egui::Stroke::new(0.5, egui::Color32::from_rgb(45, 45, 55)))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("JS")
                                                .size(9.0)
                                                .color(egui::Color32::from_rgb(120, 140, 120)),
                                        );
                                        ui.strong(*label);
                                        if ui.small_button("+ Track").clicked() {
                                            if let Some(ti) = app.selected_track {
                                                if ti < app.project.tracks.len() {
                                                    app.push_undo(&format!("Add {label}"));
                                                    app.project.tracks[ti].effects.push(
                                                        EffectSlot::new(effect.clone()),
                                                    );
                                                    app.sync_project();
                                                    app.fx_browser.load_status =
                                                        Some(format!("Added {label} to track"));
                                                }
                                            }
                                        }

                                        // Folder button
                                        if !app.fx_browser.folders.is_empty() {
                                            let bid_clone = bid.clone();
                                            ui.menu_button(
                                                egui::RichText::new("F")
                                                    .size(9.0)
                                                    .color(egui::Color32::from_rgb(140, 140, 160)),
                                                |ui| {
                                                    ui.label(egui::RichText::new("Folders:").small().color(egui::Color32::GRAY));
                                                    for fi in 0..app.fx_browser.folders.len() {
                                                        let fname = app.fx_browser.folders[fi].name.clone();
                                                        let already_in = app.fx_browser.folders[fi].contains_item(&bid_clone);
                                                        if already_in {
                                                            if ui.button(format!("- {fname}")).clicked() {
                                                                app.fx_browser.folders[fi].remove_item(&bid_clone);
                                                                app.fx_browser.save_folders();
                                                                ui.close_menu();
                                                            }
                                                        } else {
                                                            if ui.button(format!("+ {fname}")).clicked() {
                                                                app.fx_browser.folders[fi].add_item(&bid_clone);
                                                                app.fx_browser.save_folders();
                                                                ui.close_menu();
                                                            }
                                                        }
                                                    }
                                                },
                                            );
                                        }

                                        // Remove from current folder
                                        if let Some(fi) = selected_folder_idx {
                                            if ui.small_button(
                                                egui::RichText::new("x").size(9.0).color(egui::Color32::from_rgb(180, 80, 80))
                                            ).on_hover_text("Remove from folder").clicked() {
                                                app.fx_browser.folders[fi].remove_item(&bid);
                                                app.fx_browser.save_folders();
                                            }
                                        }
                                    });
                                });
                            ui.add_space(1.0);
                        }

                        if !app.fx_browser.show_builtin && !visible_plugins.is_empty() {
                            ui.add_space(4.0);
                        }
                    }

                    // --- VST plugins ---
                    if visible_plugins.is_empty() && !show_builtin {
                        ui.label("No plugins match the current filter.");
                    }

                    for (plugin_name, plugin_path, format, is_loaded) in &visible_plugins {
                        let format_tag = if format.is_empty() { "?" } else { format.as_str() };

                        egui::Frame::default()
                            .inner_margin(egui::Margin::symmetric(4, 2))
                            .corner_radius(3.0)
                            .stroke(egui::Stroke::new(0.5, egui::Color32::from_rgb(45, 45, 55)))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format_tag)
                                            .size(9.0)
                                            .color(egui::Color32::from_rgb(120, 120, 140)),
                                    );

                                    if *is_loaded {
                                        ui.strong(
                                            egui::RichText::new(plugin_name)
                                                .color(egui::Color32::from_rgb(80, 200, 80)),
                                        );

                                        // Add to selected track
                                        if ui
                                            .small_button("+ Track")
                                            .on_hover_text(
                                                "Add to selected track's FX chain",
                                            )
                                            .clicked()
                                        {
                                            if let Some(ti) = app.selected_track {
                                                if ti < app.project.tracks.len() {
                                                    app.push_undo("Add VST plugin");
                                                    let slot =
                                                        EffectSlot::new(TrackEffect::Vst3Plugin {
                                                            path: plugin_path
                                                                .to_string_lossy()
                                                                .to_string(),
                                                            name: plugin_name.clone(),
                                                        });
                                                    let slot_id = slot.id;
                                                    app.project.tracks[ti].effects.push(slot);
                                                    app.send_command(
                                                        jamhub_engine::EngineCommand::LoadVst3 {
                                                            slot_id,
                                                            path: plugin_path.clone(),
                                                        },
                                                    );
                                                    app.sync_project();
                                                    app.fx_browser.load_status = Some(format!(
                                                        "Added {plugin_name} to track"
                                                    ));
                                                }
                                            }
                                        }
                                    } else {
                                        ui.strong(plugin_name);
                                        if ui
                                            .small_button("Load")
                                            .on_hover_text("Load this plugin into memory")
                                            .clicked()
                                        {
                                            let instance =
                                                jamhub_engine::vst3_host::Vst3Plugin::load(
                                                    plugin_path,
                                                    app.sample_rate() as f64,
                                                    256,
                                                );
                                            if instance.loaded {
                                                app.fx_browser.load_status =
                                                    Some(format!("Loaded: {plugin_name}"));
                                            } else {
                                                app.fx_browser.load_status = Some(format!(
                                                    "Failed: {}",
                                                    instance
                                                        .error
                                                        .as_deref()
                                                        .unwrap_or("unknown error")
                                                ));
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

                                    // Folder button — visible dropdown to add/remove from folders
                                    if !app.fx_browser.folders.is_empty() {
                                        ui.menu_button(
                                            egui::RichText::new("F")
                                                .size(9.0)
                                                .color(egui::Color32::from_rgb(140, 140, 160)),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new("Folders:")
                                                        .small()
                                                        .color(egui::Color32::GRAY),
                                                );
                                                let item_id = vst_id(plugin_path);
                                                for fi in 0..app.fx_browser.folders.len() {
                                                    let fname =
                                                        app.fx_browser.folders[fi].name.clone();
                                                    let already_in = app.fx_browser.folders[fi]
                                                        .contains_item(&item_id);
                                                    if already_in {
                                                        if ui
                                                            .button(format!("- {fname}"))
                                                            .on_hover_text("Remove from folder")
                                                            .clicked()
                                                        {
                                                            app.fx_browser.folders[fi]
                                                                .remove_item(&item_id);
                                                            app.fx_browser.save_folders();
                                                            ui.close_menu();
                                                        }
                                                    } else {
                                                        if ui
                                                            .button(format!("+ {fname}"))
                                                            .on_hover_text("Add to folder")
                                                            .clicked()
                                                        {
                                                            app.fx_browser.folders[fi]
                                                                .add_item(&item_id);
                                                            app.fx_browser.save_folders();
                                                            ui.close_menu();
                                                        }
                                                    }
                                                }
                                            },
                                        );
                                    }

                                    // Remove from current folder button (when viewing a folder)
                                    if let Some(fi) = viewing_folder_idx {
                                        if ui
                                            .small_button(
                                                egui::RichText::new("x")
                                                    .size(9.0)
                                                    .color(egui::Color32::from_rgb(180, 80, 80)),
                                            )
                                            .on_hover_text("Remove from this folder")
                                            .clicked()
                                        {
                                            let item_id = vst_id(plugin_path);
                                            app.fx_browser.folders[fi].remove_item(&item_id);
                                            app.fx_browser.save_folders();
                                        }
                                    }
                                });
                            });
                        ui.add_space(1.0);
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
        });

    if !open {
        app.fx_browser.show = false;
    }
}

// --- Folder persistence ---

fn folders_path() -> std::path::PathBuf {
    let config = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let dir = config.join("jamhub");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("fx_folders.json")
}

fn load_folders() -> Vec<FxFolder> {
    let path = folders_path();
    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_folders(folders: &[FxFolder]) {
    let path = folders_path();
    if let Ok(data) = serde_json::to_string_pretty(folders) {
        let _ = std::fs::write(&path, data);
    }
}
