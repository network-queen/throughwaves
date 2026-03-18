use eframe::egui;
use jamhub_model::{EqBandParams, EqBandType, MidiMappingTarget, TrackEffect, MAX_EQ_BANDS};

use crate::DawApp;

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_effects {
        return;
    }

    let track_idx = match app.selected_track {
        Some(i) if i < app.project.tracks.len() => i,
        _ => {
            let mut open = true;
            egui::Window::new("FX Chain").constrain(false)
                .open(&mut open)
                .default_width(260.0)
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new("Cannot add effect: select a track first")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(200, 160, 60)),
                    );
                });
            if !open {
                app.show_effects = false;
            }
            return;
        }
    };

    let mut open = true;
    egui::Window::new("FX Chain").constrain(false)
        .open(&mut open)
        .default_width(300.0)
        .min_width(280.0)
        .show(ctx, |ui| {
            // Header with track name and signal flow
            let track_name = app.project.tracks[track_idx].name.clone();
            let tc = app.project.tracks[track_idx].color;
            let track_color = egui::Color32::from_rgb(tc[0], tc[1], tc[2]);

            ui.horizontal(|ui| {
                // Track color dot
                let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(dot_rect.center(), 5.0, track_color);
                ui.label(egui::RichText::new(&track_name).size(13.0).strong().color(egui::Color32::from_rgb(220, 218, 212)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let fx_count = app.project.tracks[track_idx].effects.len();
                    ui.label(egui::RichText::new(format!("{} FX", fx_count)).size(10.0).color(egui::Color32::from_rgb(110, 110, 120)));
                });
            });
            ui.add_space(4.0);

            // Signal flow: IN label
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("IN").size(8.0).color(egui::Color32::from_rgb(80, 200, 140)));
                let (line_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 16.0, 1.0), egui::Sense::hover());
                ui.painter().rect_filled(line_rect, 0.0, egui::Color32::from_rgb(45, 50, 58));
            });
            ui.add_space(2.0);

            let mut needs_sync = false;
            let mut remove_idx: Option<usize> = None;
            let mut open_editor: Option<(uuid::Uuid, String)> = None;
            let mut close_editor: Option<uuid::Uuid> = None;
            let mut toggle_builtin_popup: Option<usize> = None;
            let mut bypass_toggled_slot: Option<usize> = None;
            let effects_len = app.project.tracks[track_idx].effects.len();

            let crashed_plugins = app.engine.as_ref()
                .map(|e| e.state.read().crashed_plugins.clone())
                .unwrap_or_default();

            let slot_info: Vec<(uuid::Uuid, bool, bool, String)> = app.project.tracks[track_idx]
                .effects
                .iter()
                .map(|s| {
                    let is_editor_open = if s.effect.is_vst() {
                        app.plugin_windows.is_open(&s.id)
                    } else {
                        app.builtin_fx_open.contains_key(&s.id)
                    };
                    let is_vst = s.effect.is_vst();
                    let vst_path = if let TrackEffect::Vst3Plugin { ref path, .. } = s.effect {
                        path.clone()
                    } else { String::new() };
                    (s.id, is_editor_open, is_vst, vst_path)
                })
                .collect();

            let mut move_up: Option<usize> = None;
            let mut move_down: Option<usize> = None;

            for i in 0..effects_len {
                ui.push_id(i, |ui| {
                    let (slot_id, is_open, is_vst, ref vst_path) = slot_info[i];
                    let slot = &mut app.project.tracks[track_idx].effects[i];
                    let is_enabled = slot.enabled;
                    let is_crashed = crashed_plugins.contains(&slot_id);
                    let effect_name = slot.name().to_string();

                    // Get mix value if effect has one
                    let mix_val = slot.effect.get_param("Mix");

                    let row_bg = if is_crashed {
                        egui::Color32::from_rgb(55, 22, 22)
                    } else if is_open {
                        egui::Color32::from_rgb(28, 34, 48)
                    } else if !is_enabled {
                        egui::Color32::from_rgb(22, 22, 28)
                    } else {
                        egui::Color32::from_rgb(28, 29, 38)
                    };

                    let slot_stroke = if is_open {
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 120, 200).gamma_multiply(0.5))
                    } else {
                        egui::Stroke::new(0.5, egui::Color32::from_rgb(44, 46, 58))
                    };

                    // Signal flow connector line before each slot
                    if i > 0 {
                        ui.horizontal(|ui| {
                            ui.add_space(18.0);
                            let (line_rect, _) = ui.allocate_exact_size(egui::vec2(2.0, 6.0), egui::Sense::hover());
                            ui.painter().rect_filled(line_rect, 0.0, egui::Color32::from_rgb(50, 55, 65));
                        });
                    }

                    egui::Frame::default()
                        .inner_margin(egui::Margin::symmetric(6, 5))
                        .fill(row_bg)
                        .corner_radius(6.0)
                        .stroke(slot_stroke)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;

                                // Slot number
                                ui.label(egui::RichText::new(format!("{}.", i + 1)).size(9.0)
                                    .color(egui::Color32::from_rgb(75, 75, 85)));

                                // Move up/down
                                let arrow_active = egui::Color32::from_rgb(150, 150, 165);
                                let arrow_dim = egui::Color32::from_rgb(50, 50, 58);
                                if i > 0 {
                                    if ui.add(egui::Button::new(egui::RichText::new("\u{25B2}").size(7.0).color(arrow_active))
                                        .frame(false).min_size(egui::vec2(12.0, 16.0))).on_hover_text("Move up").clicked() {
                                        move_up = Some(i);
                                    }
                                } else {
                                    ui.add(egui::Button::new(egui::RichText::new("\u{25B2}").size(7.0).color(arrow_dim))
                                        .frame(false).min_size(egui::vec2(12.0, 16.0)));
                                }
                                if i + 1 < effects_len {
                                    if ui.add(egui::Button::new(egui::RichText::new("\u{25BC}").size(7.0).color(arrow_active))
                                        .frame(false).min_size(egui::vec2(12.0, 16.0))).on_hover_text("Move down").clicked() {
                                        move_down = Some(i);
                                    }
                                } else {
                                    ui.add(egui::Button::new(egui::RichText::new("\u{25BC}").size(7.0).color(arrow_dim))
                                        .frame(false).min_size(egui::vec2(12.0, 16.0)));
                                }

                                // Bypass toggle — power icon style
                                let (pwr_rect, pwr_resp) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
                                if is_enabled && !is_crashed {
                                    ui.painter().circle_filled(pwr_rect.center(), 7.0, egui::Color32::from_rgb(40, 140, 60));
                                    ui.painter().circle_stroke(pwr_rect.center(), 7.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(60, 200, 80)));
                                    // Power symbol
                                    ui.painter().line_segment(
                                        [egui::pos2(pwr_rect.center().x, pwr_rect.center().y - 4.0),
                                         egui::pos2(pwr_rect.center().x, pwr_rect.center().y - 7.5)],
                                        egui::Stroke::new(1.5, egui::Color32::from_rgb(60, 200, 80)),
                                    );
                                } else {
                                    ui.painter().circle_stroke(pwr_rect.center(), 7.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(65, 65, 78)));
                                    ui.painter().line_segment(
                                        [egui::pos2(pwr_rect.center().x, pwr_rect.center().y - 4.0),
                                         egui::pos2(pwr_rect.center().x, pwr_rect.center().y - 7.5)],
                                        egui::Stroke::new(1.5, egui::Color32::from_rgb(65, 65, 78)),
                                    );
                                }
                                if pwr_resp.on_hover_text(if is_enabled { "Bypass" } else { "Enable" }).clicked() {
                                    slot.enabled = !slot.enabled;
                                    needs_sync = true;
                                    bypass_toggled_slot = Some(i);
                                }

                                // Effect name — clickable to open editor
                                let name_text = if is_crashed {
                                    format!("\u{26A0} {}", effect_name)
                                } else {
                                    effect_name.clone()
                                };
                                let name_color = if is_crashed {
                                    egui::Color32::from_rgb(230, 80, 80)
                                } else if !is_enabled {
                                    egui::Color32::from_rgb(85, 85, 100)
                                } else if is_open {
                                    egui::Color32::from_rgb(100, 170, 240)
                                } else {
                                    egui::Color32::from_rgb(215, 215, 225)
                                };
                                let name_rt = if !is_enabled {
                                    egui::RichText::new(&name_text).size(12.0).color(name_color).strikethrough()
                                } else {
                                    egui::RichText::new(&name_text).size(12.0).color(name_color)
                                };
                                let resp = ui.add(egui::Button::new(name_rt).frame(false));
                                if resp.on_hover_text(if is_vst {
                                    if is_open { "Close plugin editor" } else { "Open plugin editor" }
                                } else { "Edit parameters" }).clicked() {
                                    if is_vst {
                                        if is_open { close_editor = Some(slot_id); }
                                        else { open_editor = Some((slot_id, vst_path.clone())); }
                                    } else {
                                        toggle_builtin_popup = Some(i);
                                    }
                                }

                                // Right side: mix indicator + remove
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // Remove button
                                    if ui.add(egui::Button::new(
                                        egui::RichText::new("\u{2715}").size(9.0).color(egui::Color32::from_rgb(160, 70, 70)))
                                        .frame(false)).on_hover_text("Remove effect").clicked() {
                                        remove_idx = Some(i);
                                    }

                                    // Mix percentage if available
                                    if let Some(mix) = mix_val {
                                        ui.label(egui::RichText::new(format!("{:.0}%", mix * 100.0))
                                            .size(9.0).color(egui::Color32::from_rgb(90, 90, 105)));
                                    }

                                    // VST badge
                                    if is_vst {
                                        ui.label(egui::RichText::new("VST3").size(7.0)
                                            .color(egui::Color32::from_rgb(100, 180, 100)));
                                    }
                                });
                            });
                        });
                });
            }

            // Handle move
            if let Some(idx) = move_up {
                app.push_undo("Reorder effects");
                app.project.tracks[track_idx].effects.swap(idx, idx - 1);
                needs_sync = true;
            }
            if let Some(idx) = move_down {
                app.push_undo("Reorder effects");
                app.project.tracks[track_idx].effects.swap(idx, idx + 1);
                needs_sync = true;
            }

            // Signal flow: OUT label
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                if effects_len > 0 {
                    let (line_rect, _) = ui.allocate_exact_size(egui::vec2(2.0, 6.0), egui::Sense::hover());
                    ui.painter().rect_filled(line_rect, 0.0, egui::Color32::from_rgb(50, 55, 65));
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("OUT").size(8.0).color(egui::Color32::from_rgb(240, 180, 60)));
                let (line_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 16.0, 1.0), egui::Sense::hover());
                ui.painter().rect_filled(line_rect, 0.0, egui::Color32::from_rgb(45, 50, 58));
            });

            if effects_len == 0 {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("No effects in chain").size(11.0).color(egui::Color32::from_rgb(90, 90, 105)));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Use the FX Browser (Cmd+F) to add effects").size(9.0).color(egui::Color32::from_rgb(70, 70, 82)));
                });
                ui.add_space(8.0);
            }

            // --- Actions ---
            if let Some(idx) = remove_idx {
                app.push_undo("Remove effect");
                let slot = app.project.tracks[track_idx].effects.remove(idx);
                if slot.effect.is_vst() {
                    app.send_command(jamhub_engine::EngineCommand::UnloadVst3 {
                        slot_id: slot.id,
                    });
                    app.plugin_windows.destroy(&slot.id);
                }
                app.builtin_fx_open.remove(&slot.id);
                needs_sync = true;
            }
            if let Some(slot_id) = close_editor {
                app.plugin_windows.close(&slot_id);
            }
            if let Some((slot_id, path)) = open_editor {
                let path_buf = std::path::PathBuf::from(&path);

                // Check if this is a nih-plug plugin (uses egui internally, conflicts with our egui)
                let is_nihplug = DawApp::is_nihplug_egui_plugin(&path_buf);

                if is_nihplug {
                    // nih-plug plugins use egui for their UI which conflicts with our event loop.
                    // Show as a built-in parameter window instead.
                    app.set_status("Plugin uses egui UI — opening parameter view");
                    // Toggle the built-in popup for this slot
                    if app.builtin_fx_open.contains_key(&slot_id) {
                        app.builtin_fx_open.remove(&slot_id);
                    } else {
                        app.builtin_fx_open.insert(slot_id, app.builtin_fx_open.get(&slot_id).map(|c| c+1).unwrap_or(1));
                    }
                } else {
                    let mut editor_plugin = jamhub_engine::vst3_host::Vst3Plugin::load(
                        &path_buf,
                        app.sample_rate() as f64,
                        256,
                    );
                    if editor_plugin.has_editor {
                        if let Some(rx) = editor_plugin.param_change_rx.take() {
                            app.send_command(jamhub_engine::EngineCommand::AttachParamRx {
                                slot_id,
                                rx,
                            });
                        }
                        if !app.plugin_windows.open(slot_id, editor_plugin) {
                            app.set_status("Failed to open plugin UI");
                        }
                    } else {
                        app.set_status("This plugin has no editor UI");
                    }
                }
            }
            if let Some(idx) = toggle_builtin_popup {
                let sid = app.project.tracks[track_idx].effects[idx].id;
                if app.builtin_fx_open.contains_key(&sid) {
                    app.builtin_fx_open.remove(&sid);
                } else {
                    app.builtin_fx_open.insert(sid, app.builtin_fx_open.get(&sid).map(|c| c+1).unwrap_or(1));
                }
            }

            // --- Add FX --- prominent centered button with + icon
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                if ui.add_sized(
                    [ui.available_width() - 12.0, 32.0],
                    egui::Button::new(
                        egui::RichText::new("+   Add FX")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(190, 190, 210)),
                    )
                    .fill(egui::Color32::from_rgb(32, 34, 46))
                    .corner_radius(10.0)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(52, 54, 68))),
                ).clicked() {
                    app.fx_browser.show = true;
                }
            });

            // --- FX Chain Presets ---
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(
                    egui::RichText::new("Save Preset...")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(140, 180, 140)),
                )).on_hover_text("Save the current effect chain as a reusable preset").clicked() {
                    app.fx_preset_name_input = Some(crate::templates::FxPresetNameInput {
                        name: String::new(),
                    });
                }
                ui.menu_button(
                    egui::RichText::new("Load Preset...")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(140, 160, 200)),
                    |ui| {
                        // Default built-in presets
                        ui.label(
                            egui::RichText::new("Built-in")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(120, 120, 130)),
                        );
                        for preset in crate::templates::default_fx_presets() {
                            let fx_names: Vec<&str> = preset.effects.iter().map(|e| e.name()).collect();
                            let desc = fx_names.join(" > ");
                            if ui.button(format!("{} ({})", preset.name, desc)).clicked() {
                                app.push_undo("Load FX preset");
                                app.project.tracks[track_idx].effects = preset.effects.iter().map(|e| {
                                    jamhub_model::EffectSlot::new(e.effect.clone())
                                }).collect();
                                needs_sync = true;
                                app.set_status(&format!("Loaded preset: {}", preset.name));
                                ui.close_menu();
                            }
                        }
                        // User presets
                        let user_presets = crate::templates::load_fx_presets();
                        if !user_presets.is_empty() {
                            ui.separator();
                            ui.label(
                                egui::RichText::new("User Presets")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(120, 120, 130)),
                            );
                            let mut del_idx: Option<usize> = None;
                            for (pidx, preset) in user_presets.iter().enumerate() {
                                let fx_count = preset.effects.len();
                                ui.horizontal(|ui| {
                                    if ui.button(format!("{} ({} FX)", preset.name, fx_count)).clicked() {
                                        app.push_undo("Load FX preset");
                                        app.project.tracks[track_idx].effects = preset.effects.iter().map(|e| {
                                            jamhub_model::EffectSlot::new(e.effect.clone())
                                        }).collect();
                                        needs_sync = true;
                                        app.set_status(&format!("Loaded preset: {}", preset.name));
                                        ui.close_menu();
                                    }
                                    if ui.add(
                                        egui::Button::new(
                                            egui::RichText::new("x")
                                                .size(9.0)
                                                .color(egui::Color32::from_rgb(160, 60, 60)),
                                        ).frame(false),
                                    ).on_hover_text("Delete preset").clicked() {
                                        del_idx = Some(pidx);
                                        ui.close_menu();
                                    }
                                });
                            }
                            if let Some(di) = del_idx {
                                let mut presets = crate::templates::load_fx_presets();
                                if di < presets.len() {
                                    presets.remove(di);
                                    crate::templates::save_fx_presets(&presets);
                                    app.set_status("FX preset deleted");
                                }
                            }
                        }
                    },
                );
            });

            // Trigger loudness match measurement if a bypass was toggled
            if let Some(slot_i) = bypass_toggled_slot {
                crate::analysis_tools::on_effect_bypass_toggled(app, track_idx, slot_i);
            }

            if needs_sync {
                app.sync_project();
            }
        });
    if !open {
        app.show_effects = false;
    }

    // --- Built-in effect popup windows ---
    show_builtin_popups(app, ctx, track_idx);
}

/// Render floating parameter windows for built-in effects that are "open".
/// Cached VST3 parameter info for UI rendering.
struct Vst3ParamCache {
    params: Vec<(u32, String, f64)>, // (id, name, current_value)
    plugin: jamhub_engine::vst3_host::Vst3Plugin,
}

thread_local! {
    static VST3_PARAM_CACHES: std::cell::RefCell<std::collections::HashMap<uuid::Uuid, Vst3ParamCache>>
        = std::cell::RefCell::new(std::collections::HashMap::new());

    /// Per-slot preset name input state for the "Save Preset" dialog.
    static VST3_PRESET_NAME_INPUT: std::cell::RefCell<std::collections::HashMap<uuid::Uuid, String>>
        = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Access the VST3 parameter caches safely (UI thread only).
fn with_param_caches<R>(f: impl FnOnce(&mut std::collections::HashMap<uuid::Uuid, Vst3ParamCache>) -> R) -> R {
    VST3_PARAM_CACHES.with(|c| f(&mut *c.borrow_mut()))
}

fn show_builtin_popups(app: &mut DawApp, ctx: &egui::Context, track_idx: usize) {
    if track_idx >= app.project.tracks.len() {
        return;
    }

    let open_ids: Vec<(uuid::Uuid, u32)> = app.builtin_fx_open.iter().map(|(&k, &v)| (k, v)).collect();
    let mut needs_sync = false;

    let mut close_ids: Vec<uuid::Uuid> = Vec::new();
    for (slot_id, open_counter) in open_ids {
        let slot_idx = app.project.tracks[track_idx]
            .effects
            .iter()
            .position(|s| s.id == slot_id);
        let slot_idx = match slot_idx {
            Some(i) => i,
            None => {
                close_ids.push(slot_id);
                continue;
            }
        };

        let name = app.project.tracks[track_idx].effects[slot_idx].name().to_string();
        let is_vst = app.project.tracks[track_idx].effects[slot_idx].effect.is_vst();
        let mut is_open = true;

        if is_vst {
            // VST3 parameter UI — load params from plugin if not cached
            let vst_path = if let TrackEffect::Vst3Plugin { ref path, .. } =
                app.project.tracks[track_idx].effects[slot_idx].effect
            {
                Some(path.clone())
            } else {
                None
            };

            let needs_load = with_param_caches(|caches| !caches.contains_key(&slot_id));
            if needs_load {
                if let Some(ref path) = vst_path {
                    let plugin = jamhub_engine::vst3_host::Vst3Plugin::load(
                        &std::path::PathBuf::from(path),
                        app.sample_rate() as f64,
                        256,
                    );
                    let mut params = Vec::new();
                    for i in 0..plugin.get_parameter_count() {
                        if let Some(info) = plugin.get_parameter_info(i) {
                            let value = plugin.get_param_normalized(info.id);
                            params.push((info.id, info.name, value));
                        }
                    }
                    // Attach param rx to engine for syncing
                    if let Some(rx) = {
                        // We need a mutable reference but plugin is about to be moved
                        // Load another just for the rx
                        let mut p2 = jamhub_engine::vst3_host::Vst3Plugin::load(
                            &std::path::PathBuf::from(path),
                            app.sample_rate() as f64,
                            256,
                        );
                        p2.param_change_rx.take()
                    } {
                        app.send_command(jamhub_engine::EngineCommand::AttachParamRx {
                            slot_id,
                            rx,
                        });
                    }
                    with_param_caches(|caches| {
                        caches.insert(slot_id, Vst3ParamCache { params, plugin });
                    });
                }
            }

            let plugin_name_for_presets = name.clone();
            egui::Window::new(format!("{name}"))
                .id(egui::Id::new("fx_popup").with(slot_id).with(open_counter))
                .title_bar(true)
                .collapsible(false)
                .default_width(300.0)
                .resizable(true)
                .show(ctx, |ui| {
                    // Close button at top-right
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("\u{2715}").on_hover_text("Close").clicked() {
                                is_open = false;
                            }
                        });
                    });
                    // --- Preset Save/Load bar ---
                    ui.horizontal(|ui| {
                        // Save Preset button
                        if ui.add(egui::Button::new(
                            egui::RichText::new("Save Preset")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(140, 180, 140)),
                        )).on_hover_text("Save current parameters as a preset").clicked() {
                            VST3_PRESET_NAME_INPUT.with(|r| {
                                r.borrow_mut().insert(slot_id, String::new());
                            });
                        }

                        // Load Preset dropdown
                        let presets = crate::templates::load_plugin_presets(&plugin_name_for_presets);
                        ui.menu_button(
                            egui::RichText::new("Load Preset")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(140, 160, 200)),
                            |ui| {
                                if presets.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No saved presets")
                                            .size(10.0)
                                            .color(egui::Color32::from_rgb(100, 100, 110)),
                                    );
                                } else {
                                    let mut del_name: Option<String> = None;
                                    for preset in &presets {
                                        ui.horizontal(|ui| {
                                            if ui.button(&preset.name).clicked() {
                                                // Apply preset params to the cache
                                                with_param_caches(|caches| {
                                                    if let Some(cache) = caches.get_mut(&slot_id) {
                                                        for (id, _pname, value) in &mut cache.params {
                                                            if let Some(&pval) = preset.params.get(id) {
                                                                *value = pval;
                                                                cache.plugin.set_param_normalized(*id, pval);
                                                            }
                                                        }
                                                    }
                                                });
                                                ui.close_menu();
                                            }
                                            if ui.add(
                                                egui::Button::new(
                                                    egui::RichText::new("x")
                                                        .size(9.0)
                                                        .color(egui::Color32::from_rgb(160, 60, 60)),
                                                ).frame(false),
                                            ).on_hover_text("Delete preset").clicked() {
                                                del_name = Some(preset.name.clone());
                                                ui.close_menu();
                                            }
                                        });
                                    }
                                    if let Some(dn) = del_name {
                                        crate::templates::delete_plugin_preset(&plugin_name_for_presets, &dn);
                                    }
                                }
                            },
                        );
                    });

                    // --- Preset name input dialog (inline) ---
                    let mut save_action: Option<String> = None;
                    let mut cancel_save = false;
                    VST3_PRESET_NAME_INPUT.with(|r| {
                        let mut map = r.borrow_mut();
                        if let Some(input_name) = map.get_mut(&slot_id) {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Name:")
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(160, 160, 170)),
                                );
                                ui.text_edit_singleline(input_name);
                                if ui.button("OK").clicked() && !input_name.trim().is_empty() {
                                    save_action = Some(input_name.trim().to_string());
                                }
                                if ui.button("Cancel").clicked() {
                                    cancel_save = true;
                                }
                            });
                        }
                    });
                    if let Some(preset_name) = save_action {
                        // Collect params from cache and save
                        with_param_caches(|caches| {
                            if let Some(cache) = caches.get(&slot_id) {
                                let mut params = std::collections::HashMap::new();
                                for (id, _pname, value) in &cache.params {
                                    params.insert(*id, *value);
                                }
                                let vst_path_str = vst_path.as_ref().map(|p| p.as_str()).unwrap_or("");
                                let preset = crate::templates::PluginPreset {
                                    name: preset_name,
                                    plugin_path: vst_path_str.to_string(),
                                    params,
                                };
                                let _ = crate::templates::save_plugin_preset(&plugin_name_for_presets, &preset);
                            }
                        });
                        VST3_PRESET_NAME_INPUT.with(|r| { r.borrow_mut().remove(&slot_id); });
                    }
                    if cancel_save {
                        VST3_PRESET_NAME_INPUT.with(|r| { r.borrow_mut().remove(&slot_id); });
                    }

                    ui.separator();

                    // --- Parameter sliders ---
                    with_param_caches(|caches| {
                    if let Some(cache) = caches.get_mut(&slot_id) {
                        if cache.params.is_empty() {
                            ui.label("No parameters available");
                        } else {
                            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                                for (id, param_name, value) in &mut cache.params {
                                    let mut v = *value as f32;
                                    if ui.add(
                                        egui::Slider::new(&mut v, 0.0..=1.0)
                                            .text(param_name.as_str()),
                                    ).changed() {
                                        *value = v as f64;
                                        cache.plugin.set_param_normalized(*id, v as f64);
                                    }
                                }
                            });
                        }
                    }
                    }); // end with_param_caches
                });
        } else {
            // Built-in effect parameter UI
            let is_peq = matches!(
                app.project.tracks[track_idx].effects[slot_idx].effect,
                TrackEffect::ParametricEq { .. }
            );
            let default_w = if is_peq { 520.0 } else { 250.0 };
            egui::Window::new(format!("{name}"))
                .id(egui::Id::new("fx_builtin").with(slot_id).with(open_counter))
                .title_bar(true)
                .collapsible(false)
                .default_width(default_w)
                .resizable(is_peq)
                .show(ctx, |ui| {
                    // Close button
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("\u{2715}").on_hover_text("Close").clicked() {
                                is_open = false;
                            }
                        });
                    });
                    let effect = &mut app.project.tracks[track_idx].effects[slot_idx].effect;
                    show_effect_controls(ui, effect, &mut needs_sync, Some((track_idx, slot_idx)));
                });
        }

        if !is_open {
            close_ids.push(slot_id);
        }
    }
    for id in close_ids {
        app.builtin_fx_open.remove(&id);
        with_param_caches(|caches| { caches.remove(&id); });
    }

    if needs_sync {
        app.sync_project();
    }

    // Process any MIDI learn requests collected during effect controls rendering
    MIDI_LEARN_REQUESTS.with(|r| {
        let requests: Vec<MidiMappingTarget> = std::mem::take(&mut *r.borrow_mut());
        for target in requests {
            app.midi_learn_state = Some(crate::midi_mapping::MidiLearnState { target });
            app.set_status("MIDI Learn: move a knob/slider on your controller...");
        }
    });
}

/// Parameter controls for built-in effects.
/// `slot_ctx` is `Some((track_idx, slot_idx))` when MIDI learn context menus should be added.
fn show_effect_controls(
    ui: &mut egui::Ui,
    effect: &mut TrackEffect,
    needs_sync: &mut bool,
    slot_ctx: Option<(usize, usize)>,
) {
    // Helper: add a slider with optional MIDI learn context menu
    macro_rules! slider_with_learn {
        ($ui:expr, $slider:expr, $param_name:expr, $needs_sync:expr, $slot_ctx:expr) => {{
            let resp = $ui.add($slider);
            if resp.changed() {
                *$needs_sync = true;
            }
            if let Some((ti, si)) = $slot_ctx {
                resp.context_menu(|ui| {
                    if ui.button("MIDI Learn").clicked() {
                        MIDI_LEARN_REQUESTS.with(|r| {
                            r.borrow_mut().push(MidiMappingTarget::EffectParam {
                                track_idx: ti,
                                slot_idx: si,
                                param_name: $param_name.to_string(),
                            });
                        });
                        ui.close_menu();
                    }
                });
            }
        }};
    }

    match effect {
        TrackEffect::Gain { db } => {
            slider_with_learn!(ui, egui::Slider::new(db, -24.0..=24.0).suffix(" dB"), "Gain dB", needs_sync, slot_ctx);
        }
        TrackEffect::LowPass { cutoff_hz } | TrackEffect::HighPass { cutoff_hz } => {
            slider_with_learn!(ui, egui::Slider::new(cutoff_hz, 20.0..=20000.0).logarithmic(true).text("Cutoff").suffix(" Hz"), "Cutoff Hz", needs_sync, slot_ctx);
        }
        TrackEffect::Delay { time_ms, feedback, mix } => {
            slider_with_learn!(ui, egui::Slider::new(time_ms, 1.0..=2000.0).text("Time").suffix(" ms"), "Time ms", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(feedback, 0.0..=0.95).text("Feedback"), "Feedback", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(mix, 0.0..=1.0).text("Mix"), "Mix", needs_sync, slot_ctx);
        }
        TrackEffect::Reverb { decay, mix } => {
            slider_with_learn!(ui, egui::Slider::new(decay, 0.0..=0.99).text("Decay"), "Decay", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(mix, 0.0..=1.0).text("Mix"), "Mix", needs_sync, slot_ctx);
        }
        TrackEffect::Compressor { threshold_db, ratio, attack_ms, release_ms } => {
            slider_with_learn!(ui, egui::Slider::new(threshold_db, -60.0..=0.0).text("Thresh").suffix(" dB"), "Threshold dB", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(ratio, 1.0..=20.0).text("Ratio").suffix(":1"), "Ratio", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(attack_ms, 0.1..=100.0).text("Atk").suffix(" ms"), "Attack ms", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(release_ms, 10.0..=1000.0).text("Rel").suffix(" ms"), "Release ms", needs_sync, slot_ctx);
        }
        TrackEffect::EqBand { freq_hz, gain_db, q } => {
            slider_with_learn!(ui, egui::Slider::new(freq_hz, 20.0..=20000.0).logarithmic(true).text("Freq").suffix(" Hz"), "Freq Hz", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(gain_db, -24.0..=24.0).text("Gain").suffix(" dB"), "Gain dB (EQ)", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(q, 0.1..=10.0).text("Q"), "Q", needs_sync, slot_ctx);
        }
        TrackEffect::ParametricEq { bands } => {
            show_parametric_eq_ui(ui, bands, needs_sync);
        }
        TrackEffect::Chorus { rate_hz, depth, mix } => {
            slider_with_learn!(ui, egui::Slider::new(rate_hz, 0.1..=5.0).text("Rate").suffix(" Hz"), "Rate Hz", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(depth, 0.0..=1.0).text("Depth"), "Depth", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(mix, 0.0..=1.0).text("Mix"), "Mix", needs_sync, slot_ctx);
        }
        TrackEffect::Distortion { drive, mix } => {
            slider_with_learn!(ui, egui::Slider::new(drive, 0.0..=40.0).text("Drive").suffix(" dB"), "Drive", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(mix, 0.0..=1.0).text("Mix"), "Mix", needs_sync, slot_ctx);
        }
        TrackEffect::Limiter { threshold_db, ceiling_db, release_ms } => {
            slider_with_learn!(ui, egui::Slider::new(threshold_db, -30.0..=0.0).text("Threshold").suffix(" dB"), "Threshold dB", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(ceiling_db, -12.0..=0.0).text("Ceiling").suffix(" dB"), "Ceiling dB", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(release_ms, 1.0..=500.0).text("Release").suffix(" ms").logarithmic(true), "Release ms", needs_sync, slot_ctx);
        }
        TrackEffect::Gate { threshold_db, attack_ms, release_ms, range_db } => {
            slider_with_learn!(ui, egui::Slider::new(threshold_db, -80.0..=0.0).text("Threshold").suffix(" dB"), "Threshold dB", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(attack_ms, 0.1..=50.0).text("Attack").suffix(" ms").logarithmic(true), "Attack ms", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(release_ms, 5.0..=500.0).text("Release").suffix(" ms").logarithmic(true), "Release ms", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(range_db, -120.0..=0.0).text("Range").suffix(" dB"), "Range dB", needs_sync, slot_ctx);
        }
        TrackEffect::Phaser { rate_hz, depth, stages, mix } => {
            slider_with_learn!(ui, egui::Slider::new(rate_hz, 0.05..=5.0).text("Rate").suffix(" Hz"), "Rate Hz", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(depth, 0.0..=1.0).text("Depth"), "Depth", needs_sync, slot_ctx);
            let mut stages_f = *stages as f32;
            if ui.add(egui::Slider::new(&mut stages_f, 2.0..=12.0).text("Stages").step_by(2.0)).changed() {
                *stages = stages_f as u32;
                *needs_sync = true;
            }
            slider_with_learn!(ui, egui::Slider::new(mix, 0.0..=1.0).text("Mix"), "Mix", needs_sync, slot_ctx);
        }
        TrackEffect::Flanger { rate_hz, depth, feedback, mix } => {
            slider_with_learn!(ui, egui::Slider::new(rate_hz, 0.05..=5.0).text("Rate").suffix(" Hz"), "Rate Hz", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(depth, 0.0..=1.0).text("Depth"), "Depth", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(feedback, -0.95..=0.95).text("Feedback"), "Feedback", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(mix, 0.0..=1.0).text("Mix"), "Mix", needs_sync, slot_ctx);
        }
        TrackEffect::Tremolo { rate_hz, depth } => {
            slider_with_learn!(ui, egui::Slider::new(rate_hz, 0.1..=20.0).text("Rate").suffix(" Hz"), "Rate Hz", needs_sync, slot_ctx);
            slider_with_learn!(ui, egui::Slider::new(depth, 0.0..=1.0).text("Depth"), "Depth", needs_sync, slot_ctx);
        }
        TrackEffect::Vst3Plugin { .. } => {}
    }
}

// Thread-local to collect MIDI learn requests from within effect control rendering.
// Processed after the effects panel rendering is complete.
thread_local! {
    static MIDI_LEARN_REQUESTS: std::cell::RefCell<Vec<MidiMappingTarget>> =
        std::cell::RefCell::new(Vec::new());
}

/// Full parametric EQ UI with frequency response curve and band controls.
fn show_parametric_eq_ui(ui: &mut egui::Ui, bands: &mut Vec<EqBandParams>, needs_sync: &mut bool) {
    let sample_rate = 44100.0_f32;
    let plot_width = ui.available_width().max(400.0);
    let plot_height = 200.0;

    // --- Frequency response curve ---
    let (response, painter) = ui.allocate_painter(
        egui::vec2(plot_width, plot_height),
        egui::Sense::click(),
    );
    let rect = response.rect;

    // Background
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 22, 28));
    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 50, 60)), egui::StrokeKind::Inside);

    let freq_min: f32 = 20.0;
    let freq_max: f32 = 20000.0;
    let db_min: f32 = -24.0;
    let db_max: f32 = 24.0;

    // Helper closures for coordinate mapping
    let freq_to_x = |f: f32| -> f32 {
        let t = (f.log10() - freq_min.log10()) / (freq_max.log10() - freq_min.log10());
        rect.min.x + t * rect.width()
    };
    let db_to_y = |db: f32| -> f32 {
        let t = (db - db_max) / (db_min - db_max);
        rect.min.y + t * rect.height()
    };
    let x_to_freq = |x: f32| -> f32 {
        let t = (x - rect.min.x) / rect.width();
        10.0_f32.powf(freq_min.log10() + t * (freq_max.log10() - freq_min.log10()))
    };
    let y_to_db = |y: f32| -> f32 {
        let t = (y - rect.min.y) / rect.height();
        db_max + t * (db_min - db_max)
    };

    // Grid lines — key frequencies
    let grid_color = egui::Color32::from_rgb(35, 37, 44);
    let label_color = egui::Color32::from_rgb(80, 80, 95);
    let key_freqs = [20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0];
    for &f in &key_freqs {
        let x = freq_to_x(f);
        painter.line_segment(
            [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
            egui::Stroke::new(0.5, grid_color),
        );
        let label = if f >= 1000.0 {
            format!("{}k", (f / 1000.0) as u32)
        } else {
            format!("{}", f as u32)
        };
        painter.text(
            egui::pos2(x, rect.max.y - 12.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(8.0),
            label_color,
        );
    }
    // dB grid lines
    let key_dbs: [f32; 5] = [-24.0, -12.0, 0.0, 12.0, 24.0];
    for &db in &key_dbs {
        let y = db_to_y(db);
        let stroke = if db == 0.0 {
            egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 52, 60))
        } else {
            egui::Stroke::new(0.5, grid_color)
        };
        painter.line_segment(
            [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
            stroke,
        );
        painter.text(
            egui::pos2(rect.min.x + 3.0, y - 1.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{:+.0}", db),
            egui::FontId::proportional(8.0),
            label_color,
        );
    }

    // Draw combined frequency response curve
    let num_points = 256;
    let mut curve_points: Vec<egui::Pos2> = Vec::with_capacity(num_points);
    for i in 0..num_points {
        let t = i as f32 / (num_points - 1) as f32;
        let freq = 10.0_f32.powf(freq_min.log10() + t * (freq_max.log10() - freq_min.log10()));
        let db = jamhub_engine::effects::compute_eq_response(bands, freq, sample_rate);
        let x = rect.min.x + t * rect.width();
        let y = db_to_y(db.clamp(db_min, db_max));
        curve_points.push(egui::pos2(x, y));
    }

    // Fill under/over the 0dB line
    if curve_points.len() >= 2 {
        let zero_y = db_to_y(0.0);
        // Build a filled polygon from the curve to the 0dB line
        let mut fill_above: Vec<egui::Pos2> = Vec::new();
        let mut fill_below: Vec<egui::Pos2> = Vec::new();
        for &pt in &curve_points {
            if pt.y < zero_y {
                fill_above.push(pt);
            }
            if pt.y > zero_y {
                fill_below.push(pt);
            }
        }
        // Simple fill: just paint thin rectangles per pixel column
        for pair in curve_points.windows(2) {
            let x0 = pair[0].x;
            let x1 = pair[1].x;
            let y0 = pair[0].y;
            let y1 = pair[1].y;
            let mid_y = (y0 + y1) * 0.5;
            if mid_y < zero_y {
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(x0, mid_y), egui::pos2(x1.max(x0 + 0.5), zero_y)),
                    0.0,
                    egui::Color32::from_rgba_premultiplied(60, 160, 255, 20),
                );
            } else if mid_y > zero_y {
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(x0, zero_y), egui::pos2(x1.max(x0 + 0.5), mid_y)),
                    0.0,
                    egui::Color32::from_rgba_premultiplied(255, 100, 60, 20),
                );
            }
        }

        // Draw the response curve line
        let curve_stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 180, 255));
        for pair in curve_points.windows(2) {
            painter.line_segment([pair[0], pair[1]], curve_stroke);
        }
    }

    // Draw band points as draggable dots
    let band_colors = [
        egui::Color32::from_rgb(255, 100, 100),
        egui::Color32::from_rgb(255, 180, 60),
        egui::Color32::from_rgb(255, 255, 80),
        egui::Color32::from_rgb(80, 255, 80),
        egui::Color32::from_rgb(80, 220, 255),
        egui::Color32::from_rgb(120, 120, 255),
        egui::Color32::from_rgb(200, 100, 255),
        egui::Color32::from_rgb(255, 120, 200),
    ];

    for (bi, band) in bands.iter().enumerate() {
        let bx = freq_to_x(band.freq_hz);
        let by = db_to_y(band.gain_db);
        let color = band_colors[bi % band_colors.len()];
        // Outer ring
        painter.circle_stroke(egui::pos2(bx, by), 7.0, egui::Stroke::new(1.5, color));
        // Inner fill
        painter.circle_filled(egui::pos2(bx, by), 5.0, color.gamma_multiply(0.6));
        // Band number label
        painter.text(
            egui::pos2(bx, by - 12.0),
            egui::Align2::CENTER_BOTTOM,
            format!("{}", bi + 1),
            egui::FontId::proportional(9.0),
            color,
        );
    }

    // Handle click to add new band (if click wasn't on an existing band dot)
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let click_freq = x_to_freq(pos.x);
            let click_db = y_to_db(pos.y);
            // Check if click is near an existing band
            let near_existing = bands.iter().any(|b| {
                let bx = freq_to_x(b.freq_hz);
                let by = db_to_y(b.gain_db);
                (bx - pos.x).abs() < 12.0 && (by - pos.y).abs() < 12.0
            });
            if !near_existing && bands.len() < MAX_EQ_BANDS {
                bands.push(EqBandParams {
                    freq_hz: click_freq.clamp(20.0, 20000.0),
                    gain_db: click_db.clamp(-24.0, 24.0),
                    q: 1.0,
                    band_type: EqBandType::Peak,
                });
                *needs_sync = true;
            }
        }
    }

    ui.add_space(6.0);

    // --- Per-band controls ---
    let mut remove_band: Option<usize> = None;
    let num_bands = bands.len();

    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        for bi in 0..num_bands {
            let color = band_colors[bi % band_colors.len()];
            ui.push_id(bi, |ui| {
                egui::Frame::default()
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .fill(egui::Color32::from_rgb(28, 30, 36))
                    .corner_radius(3.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Band number indicator
                            let (dot_rect, _) = ui.allocate_exact_size(
                                egui::vec2(10.0, 16.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().circle_filled(dot_rect.center(), 4.0, color);

                            // Band type combo
                            let band = &mut bands[bi];
                            let current_type = band.band_type;
                            egui::ComboBox::from_id_salt(format!("eq_type_{bi}"))
                                .width(72.0)
                                .selected_text(current_type.name())
                                .show_ui(ui, |ui| {
                                    for bt in EqBandType::ALL {
                                        if ui.selectable_value(&mut bands[bi].band_type, bt, bt.name()).changed() {
                                            *needs_sync = true;
                                        }
                                    }
                                });

                            let band = &mut bands[bi];
                            // Freq
                            let freq_speed = band.freq_hz * 0.01;
                            if ui.add(
                                egui::DragValue::new(&mut band.freq_hz)
                                    .range(20.0..=20000.0)
                                    .speed(freq_speed)
                                    .suffix(" Hz")
                            ).changed() {
                                *needs_sync = true;
                            }
                            // Gain
                            if ui.add(
                                egui::DragValue::new(&mut band.gain_db)
                                    .range(-24.0..=24.0)
                                    .speed(0.1)
                                    .suffix(" dB")
                            ).changed() {
                                *needs_sync = true;
                            }
                            // Q
                            if ui.add(
                                egui::DragValue::new(&mut band.q)
                                    .range(0.1..=10.0)
                                    .speed(0.02)
                                    .prefix("Q ")
                            ).changed() {
                                *needs_sync = true;
                            }
                            // Remove
                            if ui.add(
                                egui::Button::new(
                                    egui::RichText::new("x").size(10.0).color(egui::Color32::from_rgb(160, 60, 60))
                                ).frame(false)
                            ).on_hover_text("Remove band").clicked() {
                                remove_band = Some(bi);
                            }
                        });
                    });
                ui.add_space(1.0);
            });
        }
    });

    if let Some(bi) = remove_band {
        bands.remove(bi);
        *needs_sync = true;
    }

    // Add band button
    if bands.len() < MAX_EQ_BANDS {
        if ui.add(
            egui::Button::new(
                egui::RichText::new("+ Add Band")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(140, 180, 220))
            )
        ).clicked() {
            bands.push(EqBandParams::default());
            *needs_sync = true;
        }
    }
}
