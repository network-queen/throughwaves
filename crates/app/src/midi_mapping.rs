use eframe::egui;
use jamhub_model::{
    MacroAssignment, MacroControl, MidiMapping, MidiMappingTarget,
    apply_macro_value, apply_midi_cc_to_target,
};

use crate::DawApp;

/// Number of macro knobs.
pub const NUM_MACROS: usize = 8;

// ── MIDI Learn State ─────────────────────────────────────────────────

/// When we are in "MIDI learn" mode, this holds what we're waiting to map.
#[derive(Debug, Clone)]
pub struct MidiLearnState {
    pub target: MidiMappingTarget,
}

// ── MIDI Mapping Manager Window ──────────────────────────────────────

pub fn show_mapping_manager(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_midi_mappings {
        return;
    }

    let mut open = true;
    let mut remove_idx: Option<usize> = None;
    let mut start_learn = false;
    let mut clear_all = false;

    egui::Window::new("MIDI Mappings")
        .open(&mut open)
        .default_width(500.0)
        .resizable(true)
        .show(ctx, |ui| {
            // Status line: show learn state
            if app.midi_learn_state.is_some() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Waiting for MIDI CC...")
                            .size(13.0)
                            .color(egui::Color32::from_rgb(255, 200, 60)),
                    );
                    if ui.button("Cancel").clicked() {
                        app.midi_learn_state = None;
                    }
                });
                ui.add_space(4.0);
            }

            // Table header
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("CC#")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(160, 160, 170)),
                );
                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new("Ch")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(160, 160, 170)),
                );
                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new("Parameter")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(160, 160, 170)),
                );
            });
            ui.separator();

            // Mapping rows
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                let tracks = &app.project.tracks;
                for (i, mapping) in app.project.midi_mappings.iter().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{:3}", mapping.cc_number))
                                    .size(12.0)
                                    .monospace(),
                            );
                            ui.add_space(16.0);
                            ui.label(
                                egui::RichText::new(format!("{:2}", mapping.channel + 1))
                                    .size(12.0)
                                    .monospace(),
                            );
                            ui.add_space(16.0);
                            ui.label(
                                egui::RichText::new(mapping.target.label(tracks))
                                    .size(12.0),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("x")
                                                .size(10.0)
                                                .color(egui::Color32::from_rgb(180, 60, 60)),
                                        )
                                        .frame(false),
                                    )
                                    .on_hover_text("Remove mapping")
                                    .clicked()
                                {
                                    remove_idx = Some(i);
                                }
                            });
                        });
                    });
                }

                if app.project.midi_mappings.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("No MIDI mappings defined.")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(100, 100, 110)),
                    );
                    ui.add_space(8.0);
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Learn...").clicked() {
                    // Start a learn aimed at master volume as default; user can also
                    // right-click a specific parameter for targeted learn.
                    start_learn = true;
                }
                if ui.button("Clear All").clicked() {
                    clear_all = true;
                }
            });
        });

    if !open {
        app.show_midi_mappings = false;
    }

    if let Some(idx) = remove_idx {
        app.project.midi_mappings.remove(idx);
        app.dirty = true;
    }
    if clear_all {
        app.project.midi_mappings.clear();
        app.dirty = true;
        app.set_status("All MIDI mappings cleared");
    }
    if start_learn {
        app.midi_learn_state = Some(MidiLearnState {
            target: MidiMappingTarget::MasterVolume,
        });
        app.set_status("MIDI Learn: move a knob/slider on your controller...");
    }
}

// ── Macro Controls Panel ─────────────────────────────────────────────

/// Show macro knobs below the transport bar as a top panel.
pub fn show_macro_panel(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_macros {
        return;
    }

    egui::TopBottomPanel::top("macro_panel")
        .frame(
            egui::Frame::default()
                .fill(egui::Color32::from_rgb(24, 25, 30))
                .inner_margin(egui::Margin::symmetric(8, 3))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(38, 38, 46),
                )),
        )
        .exact_height(52.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("MACROS")
                        .size(9.0)
                        .color(egui::Color32::from_rgb(100, 100, 115)),
                );
                ui.add_space(4.0);

                let mut any_changed = false;

                // Ensure we have 8 macros
                while app.project.macros.len() < NUM_MACROS {
                    let n = app.project.macros.len() + 1;
                    app.project.macros.push(MacroControl {
                        name: format!("Macro {n}"),
                        value: 0.0,
                        assignments: Vec::new(),
                    });
                }

                for mi in 0..NUM_MACROS {
                    ui.push_id(mi, |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(48.0);

                            // Macro name (editable on double-click, but for now static)
                            let name = app.project.macros[mi].name.clone();
                            ui.label(
                                egui::RichText::new(&name)
                                    .size(8.5)
                                    .color(egui::Color32::from_rgb(140, 140, 155)),
                            );

                            // Knob as a horizontal slider for simplicity
                            let mut val = app.project.macros[mi].value;
                            let knob_resp = ui.add(
                                egui::Slider::new(&mut val, 0.0..=1.0)
                                    .show_value(false)
                                    .fixed_decimals(2),
                            );

                            if knob_resp.changed() {
                                app.project.macros[mi].value = val;
                                any_changed = true;
                            }

                            // Right-click context menu: assign parameter
                            knob_resp.context_menu(|ui| {
                                ui.label(
                                    egui::RichText::new("Macro Assignments")
                                        .size(11.0)
                                        .strong(),
                                );
                                ui.separator();

                                // Show current assignments
                                let tracks = &app.project.tracks;
                                let assigns = app.project.macros[mi].assignments.clone();
                                let mut remove_assign: Option<usize> = None;
                                for (ai, assign) in assigns.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(assign.target.label(tracks))
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "[{:.1} - {:.1}]",
                                                assign.min_value, assign.max_value
                                            ))
                                            .size(10.0)
                                            .color(egui::Color32::from_rgb(120, 120, 135)),
                                        );
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("x")
                                                        .size(9.0)
                                                        .color(egui::Color32::from_rgb(180, 60, 60)),
                                                )
                                                .frame(false),
                                            )
                                            .clicked()
                                        {
                                            remove_assign = Some(ai);
                                        }
                                    });
                                }
                                if let Some(ai) = remove_assign {
                                    app.project.macros[mi].assignments.remove(ai);
                                    app.dirty = true;
                                    ui.close_menu();
                                }

                                if assigns.is_empty() {
                                    ui.label(
                                        egui::RichText::new("(no assignments)")
                                            .size(10.0)
                                            .color(egui::Color32::from_rgb(100, 100, 110)),
                                    );
                                }

                                ui.separator();

                                // Quick-assign submenus
                                ui.menu_button("Assign Track Volume...", |ui| {
                                    for (ti, track) in app.project.tracks.iter().enumerate() {
                                        if ui.button(&track.name).clicked() {
                                            let target = MidiMappingTarget::TrackVolume(ti);
                                            let (lo, hi) = target.range(&app.project);
                                            app.project.macros[mi].assignments.push(
                                                MacroAssignment {
                                                    target,
                                                    min_value: lo,
                                                    max_value: hi,
                                                },
                                            );
                                            app.dirty = true;
                                            ui.close_menu();
                                        }
                                    }
                                });

                                ui.menu_button("Assign Track Pan...", |ui| {
                                    for (ti, track) in app.project.tracks.iter().enumerate() {
                                        if ui.button(&track.name).clicked() {
                                            let target = MidiMappingTarget::TrackPan(ti);
                                            app.project.macros[mi].assignments.push(
                                                MacroAssignment {
                                                    target,
                                                    min_value: -1.0,
                                                    max_value: 1.0,
                                                },
                                            );
                                            app.dirty = true;
                                            ui.close_menu();
                                        }
                                    }
                                });

                                // Collect effect info to avoid borrow conflict
                                let fx_info: Vec<(usize, String, Vec<(usize, String, Vec<&'static str>)>)> =
                                    app.project.tracks.iter().enumerate().map(|(ti, track)| {
                                        let effects: Vec<_> = track.effects.iter().enumerate()
                                            .filter_map(|(si, slot)| {
                                                let params = slot.effect.automatable_params();
                                                if params.is_empty() { None }
                                                else { Some((si, slot.name().to_string(), params)) }
                                            }).collect();
                                        (ti, track.name.clone(), effects)
                                    }).collect();

                                ui.menu_button("Assign Effect Param...", |ui| {
                                    for (ti, tname, effects) in &fx_info {
                                        ui.menu_button(tname, |ui| {
                                            for (si, ename, params) in effects {
                                                ui.menu_button(ename, |ui| {
                                                    for pname in params {
                                                        if ui.button(*pname).clicked() {
                                                            let target =
                                                                MidiMappingTarget::EffectParam {
                                                                    track_idx: *ti,
                                                                    slot_idx: *si,
                                                                    param_name: pname.to_string(),
                                                                };
                                                            let (lo, hi) =
                                                                target.range(&app.project);
                                                            app.project.macros[mi]
                                                                .assignments
                                                                .push(MacroAssignment {
                                                                    target,
                                                                    min_value: lo,
                                                                    max_value: hi,
                                                                });
                                                            app.dirty = true;
                                                            ui.close_menu();
                                                        }
                                                    }
                                                });
                                            }
                                        });
                                    }
                                });

                                if ui.button("Assign Master Volume").clicked() {
                                    app.project.macros[mi].assignments.push(MacroAssignment {
                                        target: MidiMappingTarget::MasterVolume,
                                        min_value: 0.0,
                                        max_value: 1.5,
                                    });
                                    app.dirty = true;
                                    ui.close_menu();
                                }
                            });
                        });
                    });
                }

                // Apply macro values to their targets if any knob changed
                if any_changed {
                    // We need to apply all macros since one may have changed
                    let macros_snapshot: Vec<MacroControl> =
                        app.project.macros.clone();
                    let mut master_vol = app.master_volume;
                    let mut synced = false;
                    for m in &macros_snapshot {
                        if apply_macro_value(m, &mut app.project, &mut master_vol) {
                            synced = true;
                        }
                    }
                    app.master_volume = master_vol;
                    if synced {
                        app.sync_project();
                        app.dirty = true;
                    }
                }
            });
        });
}

// ── MIDI CC Processing ───────────────────────────────────────────────

/// Process incoming MIDI CC events from the recorder buffer.
/// Should be called each frame in the update loop.
pub fn process_midi_cc(app: &mut DawApp) {
    // Peek at events from the MIDI recorder
    let events = app.midi_panel.recorder.peek_events();
    if events.is_empty() {
        return;
    }

    let mut needs_sync = false;

    for event in &events {
        // CC messages: status byte 0xBn where n = channel
        let is_cc = (event.status & 0xF0) == 0xB0;
        if !is_cc {
            continue;
        }

        let channel = event.status & 0x0F;
        let cc_number = event.note; // byte 2 = CC number
        let cc_value = event.velocity; // byte 3 = CC value

        // If we're in MIDI learn mode, capture the mapping
        if let Some(learn) = app.midi_learn_state.take() {
            // Check if this CC is already mapped; if so, update it
            if let Some(existing) = app
                .project
                .midi_mappings
                .iter_mut()
                .find(|m| m.cc_number == cc_number && m.channel == channel)
            {
                existing.target = learn.target.clone();
            } else {
                app.project.midi_mappings.push(MidiMapping {
                    cc_number,
                    channel,
                    target: learn.target.clone(),
                });
            }

            let label = learn.target.label(&app.project.tracks);
            app.set_status(&format!(
                "MIDI Mapped: CC{cc_number} (ch{}) -> {label}",
                channel + 1
            ));
            app.dirty = true;
            needs_sync = true;
            continue;
        }

        // Apply CC to any existing mappings
        let mappings: Vec<MidiMapping> = app.project.midi_mappings.clone();
        let mut master_vol = app.master_volume;
        for mapping in &mappings {
            if mapping.cc_number == cc_number && mapping.channel == channel {
                if apply_midi_cc_to_target(
                    &mapping.target,
                    cc_value,
                    &mut app.project,
                    &mut master_vol,
                ) {
                    needs_sync = true;
                }
            }
        }
        app.master_volume = master_vol;
    }

    if needs_sync {
        app.sync_project();
    }
}

// ── Context menu helper for MIDI learn on any parameter ──────────────

/// Show a "MIDI Learn" context menu entry for a parameter widget response.
/// Call this after any slider/knob UI widget: `midi_learn_context_menu(app, &response, target)`.
pub fn midi_learn_context_menu(
    app: &mut DawApp,
    response: &egui::Response,
    target: MidiMappingTarget,
) {
    response.context_menu(|ui| {
        if ui.button("MIDI Learn").clicked() {
            app.midi_learn_state = Some(MidiLearnState {
                target: target.clone(),
            });
            app.set_status("MIDI Learn: move a knob/slider on your controller...");
            ui.close_menu();
        }

        // Check if already mapped
        let existing = app
            .project
            .midi_mappings
            .iter()
            .position(|m| m.target == target);
        if let Some(idx) = existing {
            let cc = app.project.midi_mappings[idx].cc_number;
            let ch = app.project.midi_mappings[idx].channel + 1;
            ui.label(
                egui::RichText::new(format!("Mapped: CC{cc} ch{ch}"))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(120, 200, 120)),
            );
            if ui.button("Remove Mapping").clicked() {
                app.project.midi_mappings.remove(idx);
                app.dirty = true;
                app.set_status("MIDI mapping removed");
                ui.close_menu();
            }
        }
    });
}
