use eframe::egui;
use jamhub_model::{MidiMappingTarget, TrackKind};

use crate::DawApp;
use crate::midi_mapping;

const CHANNEL_WIDTH: f32 = 74.0;
const METER_HEIGHT: f32 = 120.0;

/// Per-track peak hold state for the mixer meters.
struct PeakHoldState {
    left_peak: f32,
    right_peak: f32,
    left_time: std::time::Instant,
    right_time: std::time::Instant,
}

// Thread-local to persist peak hold across frames without touching DawApp.
thread_local! {
    static PEAK_HOLDS: std::cell::RefCell<std::collections::HashMap<uuid::Uuid, PeakHoldState>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    let mut needs_sync = false;

    // Decay meters each frame
    if let Some(levels) = app.levels() {
        levels.decay(0.85);
    }

    // Collect send-related mutations to apply after the UI loop
    enum SendAction {
        Remove { track_idx: usize, send_idx: usize },
        Add { track_idx: usize, target_id: uuid::Uuid },
        SetLevel { track_idx: usize, send_idx: usize, level: f32 },
        TogglePreFader { track_idx: usize, send_idx: usize },
    }
    let mut send_actions: Vec<SendAction> = Vec::new();

    // Routing mutations deferred to avoid borrow conflicts
    enum RoutingAction {
        SetSidechain { track_idx: usize, sc_id: Option<uuid::Uuid> },
        SetInputChannel { track_idx: usize, channel: Option<u16> },
        SetOutputTarget { track_idx: usize, target: Option<uuid::Uuid> },
    }
    let mut routing_actions: Vec<RoutingAction> = Vec::new();

    // Master FX chain mutations
    enum MasterFxAction {
        Add(jamhub_model::EffectSlot),
        Remove(usize),
    }
    let mut master_fx_actions: Vec<MasterFxAction> = Vec::new();

    egui::ScrollArea::horizontal().show(ui, |ui| {
        ui.horizontal(|ui| {
            // Build a list of (track_id, track_name) for dropdowns
            let track_info: Vec<(uuid::Uuid, String, TrackKind)> = app
                .project
                .tracks
                .iter()
                .map(|t| (t.id, t.name.clone(), t.kind))
                .collect();

            let selected_track = app.selected_track;
            let levels_ref = app.levels().cloned();
            let pdc_snapshot = app.pdc_info().map(|p| p.read().clone());
            let mut midi_learn_requests: Vec<MidiMappingTarget> = Vec::new();
            for (i, track) in app.project.tracks.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    let color =
                        egui::Color32::from_rgb(track.color[0], track.color[1], track.color[2]);
                    let is_selected = selected_track == Some(i);

                    let stroke_color = if is_selected {
                        egui::Color32::from_rgb(235, 180, 60)
                    } else {
                        egui::Color32::from_rgb(40, 41, 48)
                    };

                    egui::Frame::default()
                        .inner_margin(egui::Margin::symmetric(5, 6))
                        .stroke(egui::Stroke::new(
                            if is_selected { 1.5 } else { 0.5 },
                            stroke_color,
                        ))
                        .corner_radius(6.0)
                        .show(ui, |ui| {
                            ui.set_width(CHANNEL_WIDTH);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;

                                // Track color dot + number + type badge
                                ui.horizontal(|ui| {
                                    // Color dot at top of strip
                                    let (_, dot_rect) = ui.allocate_space(egui::vec2(8.0, 8.0));
                                    ui.painter().circle_filled(dot_rect.center(), 4.0, color);

                                    ui.label(
                                        egui::RichText::new(format!("{}", i + 1))
                                            .size(9.0)
                                            .color(egui::Color32::from_rgb(100, 98, 94)),
                                    );
                                    let kind_label = match track.kind {
                                        TrackKind::Audio => "AUD",
                                        TrackKind::Midi => "MID",
                                        TrackKind::Bus => "BUS",
                                    };
                                    ui.label(
                                        egui::RichText::new(kind_label)
                                            .size(8.0)
                                            .color(if track.kind == TrackKind::Bus {
                                                egui::Color32::from_rgb(120, 175, 220)
                                            } else {
                                                egui::Color32::from_rgb(100, 98, 94)
                                            }),
                                    );
                                });

                                // Input selector
                                if track.kind == TrackKind::Bus {
                                    ui.label(
                                        egui::RichText::new("In: Sends")
                                            .size(8.0)
                                            .color(egui::Color32::from_rgb(120, 170, 220)),
                                    );
                                } else {
                                    let current_input_label = match track.input_channel {
                                        None => "Default In".to_string(),
                                        Some(ch) => format!("In {}", ch + 1),
                                    };
                                    egui::ComboBox::from_id_salt(("input_sel", i))
                                        .selected_text(
                                            egui::RichText::new(&current_input_label).size(8.0),
                                        )
                                        .width(CHANNEL_WIDTH - 12.0)
                                        .show_ui(ui, |ui| {
                                            if ui
                                                .selectable_label(
                                                    track.input_channel.is_none(),
                                                    "Default Input",
                                                )
                                                .on_hover_text("Use default audio input")
                                                .clicked()
                                            {
                                                routing_actions.push(
                                                    RoutingAction::SetInputChannel {
                                                        track_idx: i,
                                                        channel: None,
                                                    },
                                                );
                                            }
                                            for ch in 0u16..8 {
                                                if ui
                                                    .selectable_label(
                                                        track.input_channel == Some(ch),
                                                        format!("Input {}", ch + 1),
                                                    )
                                                    .clicked()
                                                {
                                                    routing_actions.push(
                                                        RoutingAction::SetInputChannel {
                                                            track_idx: i,
                                                            channel: Some(ch),
                                                        },
                                                    );
                                                }
                                            }
                                        });
                                }

                                // Output selector
                                {
                                    let current_output_label = match track.output_target {
                                        None => "Master".to_string(),
                                        Some(tid) => track_info
                                            .iter()
                                            .find(|(id, _, _)| *id == tid)
                                            .map(|(_, name, _)| name.clone())
                                            .unwrap_or_else(|| "?".to_string()),
                                    };
                                    let current_id = track.id;
                                    let current_target = track.output_target;
                                    egui::ComboBox::from_id_salt(("output_sel", i))
                                        .selected_text(
                                            egui::RichText::new(&current_output_label).size(8.0),
                                        )
                                        .width(CHANNEL_WIDTH - 12.0)
                                        .show_ui(ui, |ui| {
                                            if ui
                                                .selectable_label(
                                                    current_target.is_none(),
                                                    "Master",
                                                )
                                                .on_hover_text("Route to master output")
                                                .clicked()
                                            {
                                                routing_actions.push(
                                                    RoutingAction::SetOutputTarget {
                                                        track_idx: i,
                                                        target: None,
                                                    },
                                                );
                                            }
                                            for (tid, tname, tkind) in &track_info {
                                                if *tkind != TrackKind::Bus || *tid == current_id {
                                                    continue;
                                                }
                                                if ui
                                                    .selectable_label(
                                                        current_target == Some(*tid),
                                                        tname,
                                                    )
                                                    .clicked()
                                                {
                                                    routing_actions.push(
                                                        RoutingAction::SetOutputTarget {
                                                            track_idx: i,
                                                            target: Some(*tid),
                                                        },
                                                    );
                                                }
                                            }
                                        });
                                }

                                // Effects + clips info
                                if !track.effects.is_empty() {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "FX {}",
                                            track.effects.len()
                                        ))
                                        .size(8.0)
                                        .color(egui::Color32::from_rgb(160, 120, 220)),
                                    );
                                }


                                // PDC latency indicator
                                if let Some(ref pdc_state) = pdc_snapshot {
                                    if let Some(&lat) = pdc_state.track_latency.get(&track.id) {
                                        if lat > 0 {
                                            let ms = lat as f64 / pdc_state.sample_rate.max(1) as f64 * 1000.0;
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "PDC: {} samples ({:.1}ms)", lat, ms
                                                ))
                                                .size(7.0)
                                                .color(egui::Color32::from_rgb(120, 180, 220)),
                                            );
                                        }
                                    }
                                }

                                // Sidechain selector (if track has compressor)
                                {
                                    let has_compressor = track.effects.iter().any(|slot| {
                                        slot.enabled
                                            && matches!(
                                                slot.effect,
                                                jamhub_model::TrackEffect::Compressor { .. }
                                            )
                                    });
                                    if has_compressor {
                                        let sc_label = match track.sidechain_track_id {
                                            None => "SC: None".to_string(),
                                            Some(sc_id) => {
                                                let name = track_info
                                                    .iter()
                                                    .find(|(id, _, _)| *id == sc_id)
                                                    .map(|(_, n, _)| n.as_str())
                                                    .unwrap_or("?");
                                                format!("SC: {}", name)
                                            }
                                        };
                                        let current_id = track.id;
                                        let current_sc = track.sidechain_track_id;
                                        egui::ComboBox::from_id_salt(("sidechain", i))
                                            .selected_text(
                                                egui::RichText::new(&sc_label)
                                                    .size(8.0)
                                                    .color(egui::Color32::from_rgb(255, 180, 100)),
                                            )
                                            .width(CHANNEL_WIDTH - 12.0)
                                            .show_ui(ui, |ui| {
                                                if ui
                                                    .selectable_label(current_sc.is_none(), "None")
                                                    .on_hover_text("No sidechain source")
                                                    .clicked()
                                                {
                                                    routing_actions.push(
                                                        RoutingAction::SetSidechain {
                                                            track_idx: i,
                                                            sc_id: None,
                                                        },
                                                    );
                                                }
                                                for (tid, tname, _) in &track_info {
                                                    if *tid == current_id {
                                                        continue;
                                                    }
                                                    if ui
                                                        .selectable_label(
                                                            current_sc == Some(*tid),
                                                            tname,
                                                        )
                                                        .clicked()
                                                    {
                                                        routing_actions.push(
                                                            RoutingAction::SetSidechain {
                                                                track_idx: i,
                                                                sc_id: Some(*tid),
                                                            },
                                                        );
                                                    }
                                                }
                                            });
                                    }
                                }

                                ui.add_space(2.0);

                                // Fader + level meter side by side
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;

                                    // Volume fader
                                    let vol_resp = ui
                                        .add(
                                            egui::Slider::new(&mut track.volume, 0.0..=1.5)
                                                .vertical()
                                                .show_value(false),
                                        )
                                        .on_hover_text("Track volume fader — right-click to MIDI learn");
                                    if vol_resp.changed() {
                                        needs_sync = true;
                                    }
                                    vol_resp.context_menu(|ui| {
                                        if ui.button("MIDI Learn").clicked() {
                                            midi_learn_requests.push(MidiMappingTarget::TrackVolume(i));
                                            ui.close_menu();
                                        }
                                    });

                                    // Stereo level meter with peak hold
                                    draw_stereo_meter(
                                        ui,
                                        levels_ref.as_ref(),
                                        &track.id,
                                    );
                                });

                                // Volume percentage
                                ui.label(
                                    egui::RichText::new(format!("{:.0}%", track.volume * 100.0))
                                        .size(9.0)
                                        .color(egui::Color32::from_rgb(145, 142, 138)),
                                );

                                // Pan knob — arc visualization
                                ui.horizontal(|ui| {
                                    // Draw a small pan arc knob
                                    let knob_size = 24.0;
                                    let (_, knob_rect) = ui.allocate_space(egui::vec2(knob_size, knob_size));
                                    let center = knob_rect.center();
                                    let radius = knob_size * 0.4;

                                    // Background arc (full range)
                                    let arc_segments = 32;
                                    let start_angle = std::f32::consts::PI * 0.75;
                                    let end_angle = std::f32::consts::PI * 2.25;
                                    let bg_color = egui::Color32::from_rgb(36, 37, 44);
                                    for seg in 0..arc_segments {
                                        let a1 = start_angle + (end_angle - start_angle) * seg as f32 / arc_segments as f32;
                                        let a2 = start_angle + (end_angle - start_angle) * (seg + 1) as f32 / arc_segments as f32;
                                        let p1 = egui::pos2(center.x + radius * a1.cos(), center.y + radius * a1.sin());
                                        let p2 = egui::pos2(center.x + radius * a2.cos(), center.y + radius * a2.sin());
                                        ui.painter().line_segment([p1, p2], egui::Stroke::new(2.5, bg_color));
                                    }

                                    // Active arc (from center to pan position)
                                    let pan_val = track.pan;
                                    let center_angle = (start_angle + end_angle) / 2.0;
                                    let pan_angle = center_angle + pan_val * (end_angle - start_angle) / 2.0;
                                    let arc_color = egui::Color32::from_rgb(80, 200, 190);
                                    let (arc_start, arc_end) = if pan_val >= 0.0 {
                                        (center_angle, pan_angle)
                                    } else {
                                        (pan_angle, center_angle)
                                    };
                                    let active_segs = ((arc_end - arc_start).abs() / (end_angle - start_angle) * arc_segments as f32) as i32;
                                    for seg in 0..active_segs.max(1) {
                                        let a1 = arc_start + (arc_end - arc_start) * seg as f32 / active_segs.max(1) as f32;
                                        let a2 = arc_start + (arc_end - arc_start) * (seg + 1) as f32 / active_segs.max(1) as f32;
                                        let p1 = egui::pos2(center.x + radius * a1.cos(), center.y + radius * a1.sin());
                                        let p2 = egui::pos2(center.x + radius * a2.cos(), center.y + radius * a2.sin());
                                        ui.painter().line_segment([p1, p2], egui::Stroke::new(2.5, arc_color));
                                    }

                                    // Center dot
                                    ui.painter().circle_filled(center, 2.5, egui::Color32::from_rgb(180, 178, 174));

                                    // DragValue next to the knob
                                    let pan_resp = ui
                                        .add(
                                            egui::DragValue::new(&mut track.pan)
                                                .range(-1.0..=1.0)
                                                .speed(0.01)
                                                .fixed_decimals(2),
                                        )
                                        .on_hover_text("Pan position — right-click to MIDI learn");
                                    if pan_resp.changed() {
                                        needs_sync = true;
                                    }
                                    pan_resp.context_menu(|ui| {
                                        if ui.button("MIDI Learn").clicked() {
                                            midi_learn_requests.push(MidiMappingTarget::TrackPan(i));
                                            ui.close_menu();
                                        }
                                    });
                                });

                                ui.add_space(2.0);

                                // Mute / Solo — circular toggles
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    let btn_size = egui::vec2(22.0, 22.0);

                                    let mute_bg = if track.muted {
                                        egui::Color32::from_rgb(200, 160, 30)
                                    } else {
                                        egui::Color32::from_rgb(36, 37, 44)
                                    };
                                    let mute_tc = if track.muted {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(145, 142, 138)
                                    };
                                    if ui
                                        .add_sized(
                                            btn_size,
                                            egui::Button::new(
                                                egui::RichText::new("M").size(9.0).color(mute_tc),
                                            )
                                            .fill(mute_bg)
                                            .corner_radius(11.0),
                                        )
                                        .on_hover_text("Mute this track")
                                        .clicked()
                                    {
                                        track.muted = !track.muted;
                                        needs_sync = true;
                                    }

                                    let solo_bg = if track.solo {
                                        egui::Color32::from_rgb(50, 160, 60)
                                    } else {
                                        egui::Color32::from_rgb(36, 37, 44)
                                    };
                                    let solo_tc = if track.solo {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(145, 142, 138)
                                    };
                                    if ui
                                        .add_sized(
                                            btn_size,
                                            egui::Button::new(
                                                egui::RichText::new("S").size(9.0).color(solo_tc),
                                            )
                                            .fill(solo_bg)
                                            .corner_radius(11.0),
                                        )
                                        .on_hover_text("Solo this track")
                                        .clicked()
                                    {
                                        track.solo = !track.solo;
                                        needs_sync = true;
                                    }
                                });

                                // ---- Sends section ----
                                if track.kind == TrackKind::Audio || track.kind == TrackKind::Bus {
                                    ui.separator();
                                    ui.label(
                                        egui::RichText::new("Sends")
                                            .size(8.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(160, 160, 180)),
                                    );

                                    let mut remove_send_idx: Option<usize> = None;
                                    for (si, send) in track.sends.iter_mut().enumerate() {
                                        ui.push_id(("send", si), |ui| {
                                            let target_name = track_info
                                                .iter()
                                                .find(|(id, _, _)| *id == send.target_track_id)
                                                .map(|(_, name, _)| name.as_str())
                                                .unwrap_or("?");

                                            ui.label(
                                                egui::RichText::new(format!("-> {}", target_name))
                                                    .size(8.0)
                                                    .color(egui::Color32::from_rgb(120, 180, 230)),
                                            );

                                            let mut level = send.level;
                                            let level_text =
                                                format!("{:.0}%", level * 100.0);
                                            if ui
                                                .add(
                                                    egui::Slider::new(&mut level, 0.0..=1.0)
                                                        .show_value(false)
                                                        .text(level_text),
                                                )
                                                .on_hover_text("Send level")
                                                .changed()
                                            {
                                                send_actions.push(SendAction::SetLevel {
                                                    track_idx: i,
                                                    send_idx: si,
                                                    level,
                                                });
                                            }

                                            ui.horizontal(|ui| {
                                                let pf_label = if send.pre_fader {
                                                    "Pre"
                                                } else {
                                                    "Post"
                                                };
                                                if ui
                                                    .add(egui::Button::new(
                                                        egui::RichText::new(pf_label).size(8.0),
                                                    ))
                                                    .on_hover_text("Toggle pre/post fader send")
                                                    .clicked()
                                                {
                                                    send_actions.push(
                                                        SendAction::TogglePreFader {
                                                            track_idx: i,
                                                            send_idx: si,
                                                        },
                                                    );
                                                }

                                                if ui
                                                    .add(egui::Button::new(
                                                        egui::RichText::new("X")
                                                            .size(8.0)
                                                            .color(egui::Color32::from_rgb(
                                                                255, 100, 100,
                                                            )),
                                                    ))
                                                    .on_hover_text("Remove this send")
                                                    .clicked()
                                                {
                                                    remove_send_idx = Some(si);
                                                }
                                            });
                                        });
                                    }

                                    if let Some(si) = remove_send_idx {
                                        send_actions
                                            .push(SendAction::Remove { track_idx: i, send_idx: si });
                                    }

                                    let current_id = track.id;
                                    let existing_targets: Vec<uuid::Uuid> = track
                                        .sends
                                        .iter()
                                        .map(|s| s.target_track_id)
                                        .collect();

                                    ui.menu_button(
                                        egui::RichText::new("+ Send").size(9.0),
                                        |ui| {
                                            ui.set_min_width(90.0);
                                            let mut any_target = false;
                                            for (tid, tname, _tkind) in &track_info {
                                                if *tid == current_id
                                                    || existing_targets.contains(tid)
                                                {
                                                    continue;
                                                }
                                                any_target = true;
                                                if ui.button(tname).clicked() {
                                                    send_actions.push(SendAction::Add {
                                                        track_idx: i,
                                                        target_id: *tid,
                                                    });
                                                    ui.close_menu();
                                                }
                                            }
                                            if !any_target {
                                                ui.label(
                                                    egui::RichText::new("No targets")
                                                        .size(9.0)
                                                        .color(egui::Color32::GRAY),
                                                );
                                            }
                                        },
                                    );
                                }

                                // Track name at bottom — thin separator, clean text
                                ui.add_space(3.0);
                                // Subtle thin separator line
                                let (_, sep_rect) = ui.allocate_space(egui::vec2(CHANNEL_WIDTH, 1.0));
                                ui.painter().rect_filled(sep_rect, 0.0, egui::Color32::from_rgb(44, 45, 52));
                                ui.add_space(2.0);
                                ui.vertical_centered(|ui| {
                                    let name_text = if track.name.len() > 10 {
                                        format!("{}...", &track.name[..8])
                                    } else {
                                        track.name.clone()
                                    };
                                    ui.label(
                                        egui::RichText::new(name_text)
                                            .size(10.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(230, 228, 224)),
                                    );
                                });
                            });
                        });
                });
            }

            // Process deferred MIDI learn requests from the track loop
            for target in midi_learn_requests {
                app.midi_learn_state = Some(midi_mapping::MidiLearnState {
                    target,
                });
                app.set_status("MIDI Learn: move a knob/slider on your controller...");
            }

            // ============================================================
            // Master channel — wider strip with LUFS metering & FX chain
            // ============================================================
            let master_width = CHANNEL_WIDTH + 46.0; // wider than normal tracks
            egui::Frame::default()
                .inner_margin(egui::Margin::symmetric(7, 6))
                .stroke(egui::Stroke::new(
                    1.5,
                    egui::Color32::from_rgb(100, 90, 60),
                ))
                .corner_radius(6.0)
                .fill(egui::Color32::from_rgb(22, 23, 30))
                .show(ui, |ui| {
                    ui.set_width(master_width);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;

                        // Header
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("MASTER")
                                    .size(11.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(235, 180, 60)),
                            );
                            ui.label(
                                egui::RichText::new("Output")
                                    .size(8.0)
                                    .color(egui::Color32::from_rgb(100, 100, 110)),
                            );
                        });

                        ui.add_space(2.0);

                        // Master FX chain summary
                        {
                            let fx_count = app.project.master_effects.len();
                            if fx_count > 0 {
                                ui.label(
                                    egui::RichText::new(format!("FX {}", fx_count))
                                        .size(8.0)
                                        .color(egui::Color32::from_rgb(180, 140, 230)),
                                );
                            }

                            // Add master effect button
                            ui.menu_button(
                                egui::RichText::new("+ Master FX").size(8.0),
                                |ui| {
                                    ui.set_min_width(120.0);
                                    let effects: Vec<(&str, jamhub_model::TrackEffect)> = vec![
                                        ("Gain", jamhub_model::TrackEffect::Gain { db: 0.0 }),
                                        ("Low Pass", jamhub_model::TrackEffect::LowPass { cutoff_hz: 8000.0 }),
                                        ("High Pass", jamhub_model::TrackEffect::HighPass { cutoff_hz: 80.0 }),
                                        ("EQ Band", jamhub_model::TrackEffect::EqBand { freq_hz: 1000.0, gain_db: 0.0, q: 1.0 }),
                                        ("Compressor", jamhub_model::TrackEffect::Compressor {
                                            threshold_db: -12.0, ratio: 4.0, attack_ms: 10.0, release_ms: 100.0,
                                        }),
                                        ("Reverb", jamhub_model::TrackEffect::Reverb { decay: 0.5, mix: 0.2 }),
                                        ("Delay", jamhub_model::TrackEffect::Delay { time_ms: 250.0, feedback: 0.3, mix: 0.2 }),
                                        ("Chorus", jamhub_model::TrackEffect::Chorus { rate_hz: 1.0, depth: 0.5, mix: 0.3 }),
                                        ("Distortion", jamhub_model::TrackEffect::Distortion { drive: 6.0, mix: 0.5 }),
                                    ];
                                    for (name, effect) in effects {
                                        if ui.button(name).clicked() {
                                            master_fx_actions.push(MasterFxAction::Add(
                                                jamhub_model::EffectSlot::new(effect),
                                            ));
                                            ui.close_menu();
                                        }
                                    }
                                },
                            );

                            // List current master effects with remove buttons
                            let mut remove_idx: Option<usize> = None;
                            for (fi, slot) in app.project.master_effects.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let name = slot.name();
                                    let color = if slot.enabled {
                                        egui::Color32::from_rgb(170, 140, 220)
                                    } else {
                                        egui::Color32::from_rgb(80, 80, 90)
                                    };
                                    ui.label(
                                        egui::RichText::new(name).size(8.0).color(color),
                                    );
                                    if ui
                                        .add(egui::Button::new(
                                            egui::RichText::new("X")
                                                .size(7.0)
                                                .color(egui::Color32::from_rgb(255, 100, 100)),
                                        ))
                                        .on_hover_text("Remove master effect")
                                        .clicked()
                                    {
                                        remove_idx = Some(fi);
                                    }
                                });
                            }
                            if let Some(idx) = remove_idx {
                                master_fx_actions.push(MasterFxAction::Remove(idx));
                            }
                        }

                        ui.add_space(2.0);

                        // Master volume fader + level meter
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;

                            // Volume fader
                            let mut vol = app.master_volume;
                            let master_resp = ui
                                .add(
                                    egui::Slider::new(&mut vol, 0.0..=1.5)
                                        .vertical()
                                        .show_value(false),
                                )
                                .on_hover_text("Master volume — right-click to MIDI learn");
                            if master_resp.changed() {
                                app.master_volume = vol;
                                app.send_command(jamhub_engine::EngineCommand::SetMasterVolume(vol));
                            }
                            midi_mapping::midi_learn_context_menu(
                                app,
                                &master_resp,
                                MidiMappingTarget::MasterVolume,
                            );

                            // Stereo peak meter
                            if let Some(levels) = &levels_ref {
                                let (l, r) = levels.get_master_level();
                                let height = METER_HEIGHT;
                                let meter_w = 36.0;
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(meter_w, height),
                                    egui::Sense::hover(),
                                );
                                let painter = ui.painter();

                                painter.rect_filled(
                                    rect,
                                    2.0,
                                    egui::Color32::from_rgb(20, 20, 24),
                                );

                                let half_w = (meter_w - 3.0) / 2.0;
                                draw_meter_bar(painter, rect.min.x + 1.0, rect.min.y, half_w, height, l);
                                draw_meter_bar(painter, rect.min.x + half_w + 2.0, rect.min.y, half_w, height, r);

                                painter.line_segment(
                                    [
                                        egui::pos2(rect.min.x + half_w + 1.0, rect.min.y),
                                        egui::pos2(rect.min.x + half_w + 1.0, rect.max.y),
                                    ],
                                    egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 48)),
                                );

                                // Peak dB
                                let peak = l.max(r);
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{:.1} dB", to_db(peak)))
                                            .size(9.0)
                                            .color(if peak > 0.9 {
                                                egui::Color32::from_rgb(255, 80, 80)
                                            } else {
                                                egui::Color32::from_rgb(140, 140, 150)
                                            }),
                                    );
                                });
                            }
                        });

                        // Volume percentage
                        ui.label(
                            egui::RichText::new(format!("{:.0}%", app.master_volume * 100.0))
                                .size(9.0)
                                .color(egui::Color32::from_rgb(145, 142, 138)),
                        );

                        ui.add_space(2.0);
                        // Subtle separator
                        let (_, sep_rect) = ui.allocate_space(egui::vec2(master_width, 1.0));
                        ui.painter().rect_filled(sep_rect, 0.0, egui::Color32::from_rgb(50, 48, 44));

                        // ---- LUFS Loudness Metering ----
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new("LOUDNESS")
                                .size(8.0)
                                .strong()
                                .color(egui::Color32::from_rgb(180, 175, 165)),
                        );

                        if let Some(lufs_meter) = app.lufs() {
                            let readings = lufs_meter.read();

                            // Momentary LUFS
                            let m_lufs = readings.momentary;
                            let m_text = if m_lufs.is_finite() {
                                format!("{:.1}", m_lufs)
                            } else {
                                "-inf".to_string()
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("M:")
                                        .size(8.0)
                                        .color(egui::Color32::from_rgb(120, 120, 130)),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{} LUFS", m_text))
                                        .size(9.0)
                                        .strong()
                                        .color(lufs_color(m_lufs)),
                                );
                            });

                            // Short-term LUFS
                            let st_lufs = readings.short_term;
                            let st_text = if st_lufs.is_finite() {
                                format!("{:.1}", st_lufs)
                            } else {
                                "-inf".to_string()
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("S:")
                                        .size(8.0)
                                        .color(egui::Color32::from_rgb(120, 120, 130)),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{} LUFS", st_text))
                                        .size(9.0)
                                        .strong()
                                        .color(lufs_color(st_lufs)),
                                );
                            });

                            // Integrated LUFS
                            let i_lufs = readings.integrated;
                            let i_text = if i_lufs.is_finite() {
                                format!("{:.1}", i_lufs)
                            } else {
                                "-inf".to_string()
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("I:")
                                        .size(8.0)
                                        .color(egui::Color32::from_rgb(120, 120, 130)),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{} LUFS", i_text))
                                        .size(9.0)
                                        .strong()
                                        .color(lufs_color(i_lufs)),
                                );
                            });

                            // Reset Integrated button
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new("Reset Integrated")
                                        .size(8.0)
                                        .color(egui::Color32::from_rgb(180, 170, 150)),
                                ))
                                .on_hover_text("Reset integrated LUFS measurement")
                                .clicked()
                            {
                                app.send_command(jamhub_engine::EngineCommand::ResetLufs);
                            }

                            // Clipping / limiter suggestion
                            if readings.clipping {
                                ui.add_space(2.0);
                                ui.label(
                                    egui::RichText::new("CLIPPING!")
                                        .size(9.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(255, 60, 60)),
                                );
                                // Suggest adding a limiter if none is present
                                let has_limiter = app.project.master_effects.iter().any(|slot| {
                                    matches!(
                                        slot.effect,
                                        jamhub_model::TrackEffect::Compressor { ratio, .. } if ratio >= 10.0
                                    )
                                });
                                if !has_limiter {
                                    if ui
                                        .add(egui::Button::new(
                                            egui::RichText::new("+ Add Limiter")
                                                .size(8.0)
                                                .color(egui::Color32::from_rgb(255, 200, 100)),
                                        ))
                                        .on_hover_text(
                                            "Add a brick-wall limiter to the master bus to prevent clipping",
                                        )
                                        .clicked()
                                    {
                                        master_fx_actions.push(MasterFxAction::Add(
                                            jamhub_model::EffectSlot::new(
                                                jamhub_model::TrackEffect::Compressor {
                                                    threshold_db: -1.0,
                                                    ratio: 20.0,
                                                    attack_ms: 0.1,
                                                    release_ms: 50.0,
                                                },
                                            ),
                                        ));
                                    }
                                }
                            }

                            // Streaming target reference line
                            ui.add_space(1.0);
                            ui.label(
                                egui::RichText::new("Target: -14 LUFS")
                                    .size(7.0)
                                    .color(egui::Color32::from_rgb(80, 160, 80)),
                            );
                        }
                    });
                });
        });
    });

    // Apply send mutations
    for action in send_actions {
        match action {
            SendAction::Remove { track_idx, send_idx } => {
                app.project.tracks[track_idx].sends.remove(send_idx);
                needs_sync = true;
            }
            SendAction::Add { track_idx, target_id } => {
                app.project.tracks[track_idx]
                    .sends
                    .push(jamhub_model::TrackSend {
                        target_track_id: target_id,
                        level: 1.0,
                        pre_fader: false,
                    });
                needs_sync = true;
            }
            SendAction::SetLevel { track_idx, send_idx, level } => {
                app.project.tracks[track_idx].sends[send_idx].level = level;
                needs_sync = true;
            }
            SendAction::TogglePreFader { track_idx, send_idx } => {
                let pf = &mut app.project.tracks[track_idx].sends[send_idx].pre_fader;
                *pf = !*pf;
                needs_sync = true;
            }
        }
    }

    // Apply routing mutations
    for action in routing_actions {
        match action {
            RoutingAction::SetSidechain { track_idx, sc_id } => {
                app.project.tracks[track_idx].sidechain_track_id = sc_id;
                needs_sync = true;
            }
            RoutingAction::SetInputChannel { track_idx, channel } => {
                app.project.tracks[track_idx].input_channel = channel;
                needs_sync = true;
            }
            RoutingAction::SetOutputTarget { track_idx, target } => {
                app.project.tracks[track_idx].output_target = target;
                needs_sync = true;
            }
        }
    }

    // Apply master FX mutations
    for action in master_fx_actions {
        match action {
            MasterFxAction::Add(slot) => {
                app.project.master_effects.push(slot);
                needs_sync = true;
            }
            MasterFxAction::Remove(idx) => {
                if idx < app.project.master_effects.len() {
                    app.project.master_effects.remove(idx);
                    needs_sync = true;
                }
            }
        }
    }

    if needs_sync {
        app.sync_project();
    }
}

/// Color-code LUFS values: green around -14 (streaming target), yellow above -14, red above -9.
fn lufs_color(lufs: f64) -> egui::Color32 {
    if !lufs.is_finite() {
        return egui::Color32::from_rgb(80, 80, 90);
    }
    if lufs > -9.0 {
        egui::Color32::from_rgb(255, 60, 60)    // red — very loud
    } else if lufs > -14.0 {
        egui::Color32::from_rgb(235, 200, 60)   // yellow — above streaming target
    } else {
        egui::Color32::from_rgb(80, 200, 80)    // green — at or below target
    }
}

/// Draw a stereo level meter with peak hold indicators.
fn draw_stereo_meter(
    ui: &mut egui::Ui,
    levels: Option<&jamhub_engine::LevelMeters>,
    track_id: &uuid::Uuid,
) {
    let (l, r) = levels
        .map(|lm| lm.get_track_level(track_id))
        .unwrap_or((0.0, 0.0));

    let height = METER_HEIGHT;
    let total_w = 16.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, height), egui::Sense::hover());
    let painter = ui.painter();

    // Background
    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(20, 20, 24));

    let half_w = (total_w - 2.0) / 2.0;

    // Left channel bar
    draw_meter_bar(painter, rect.min.x + 0.5, rect.min.y, half_w, height, l);

    // Right channel bar
    draw_meter_bar(painter, rect.min.x + half_w + 1.5, rect.min.y, half_w, height, r);

    // Peak hold indicators
    PEAK_HOLDS.with(|holds| {
        let mut holds = holds.borrow_mut();
        let now = std::time::Instant::now();
        let state = holds.entry(*track_id).or_insert_with(|| PeakHoldState {
            left_peak: 0.0,
            right_peak: 0.0,
            left_time: now,
            right_time: now,
        });

        // Update peak hold: if new peak is higher, latch it
        if l >= state.left_peak {
            state.left_peak = l;
            state.left_time = now;
        } else if now.duration_since(state.left_time).as_millis() > 1500 {
            state.left_peak *= 0.95;
        }

        if r >= state.right_peak {
            state.right_peak = r;
            state.right_time = now;
        } else if now.duration_since(state.right_time).as_millis() > 1500 {
            state.right_peak *= 0.95;
        }

        // Draw peak hold lines (thin bright line that slowly falls)
        let lp = state.left_peak.clamp(0.0, 1.0);
        let rp = state.right_peak.clamp(0.0, 1.0);

        if lp > 0.01 {
            let y = rect.max.y - lp * height;
            painter.line_segment(
                [
                    egui::pos2(rect.min.x + 0.5, y),
                    egui::pos2(rect.min.x + half_w + 0.5, y),
                ],
                egui::Stroke::new(1.5, level_color(lp)),
            );
        }

        if rp > 0.01 {
            let y = rect.max.y - rp * height;
            painter.line_segment(
                [
                    egui::pos2(rect.min.x + half_w + 1.5, y),
                    egui::pos2(rect.max.x - 0.5, y),
                ],
                egui::Stroke::new(1.5, level_color(rp)),
            );
        }
    });

    // dB value at peak
    let peak = l.max(r);
    if peak > 0.001 {
        let db_text = format!("{:.0}", to_db(peak));
        painter.text(
            egui::pos2(rect.center().x, rect.max.y - 8.0),
            egui::Align2::CENTER_CENTER,
            db_text,
            egui::FontId::proportional(7.0),
            egui::Color32::from_rgb(180, 180, 190),
        );
    }
}

/// Draw a single meter bar with gradient coloring (green -> yellow -> red).
fn draw_meter_bar(painter: &egui::Painter, x: f32, y: f32, w: f32, height: f32, level: f32) {
    let bar_height = level.clamp(0.0, 1.0) * height;
    if bar_height < 1.0 {
        return;
    }

    // Draw the meter in segments for gradient effect
    let segments = 24;
    let seg_h = bar_height / segments as f32;
    for s in 0..segments {
        let seg_bottom = y + height - s as f32 * seg_h;
        let seg_top = seg_bottom - seg_h;
        let norm = s as f32 / segments as f32;
        let actual_level = norm * level.clamp(0.0, 1.0);

        let color = if actual_level > 0.9 {
            egui::Color32::from_rgb(255, 50, 50)
        } else if actual_level > 0.7 {
            let t = (actual_level - 0.7) / 0.2;
            egui::Color32::from_rgb(
                255,
                (200.0 - t * 150.0).max(50.0) as u8,
                50,
            )
        } else if actual_level > 0.4 {
            let t = (actual_level - 0.4) / 0.3;
            egui::Color32::from_rgb(
                (80.0 + t * 175.0) as u8,
                200,
                (80.0 - t * 30.0) as u8,
            )
        } else {
            egui::Color32::from_rgb(60, 180, 60)
        };

        let seg_rect = egui::Rect::from_min_max(
            egui::pos2(x, seg_top.max(y)),
            egui::pos2(x + w, seg_bottom.min(y + height)),
        );
        painter.rect_filled(seg_rect, 0.0, color);
    }
}

fn level_color(level: f32) -> egui::Color32 {
    if level > 0.9 {
        egui::Color32::from_rgb(255, 50, 50)
    } else if level > 0.7 {
        egui::Color32::from_rgb(255, 200, 50)
    } else {
        egui::Color32::from_rgb(80, 200, 80)
    }
}

fn to_db(level: f32) -> f32 {
    if level <= 0.0001 {
        -60.0
    } else {
        20.0 * level.log10()
    }
}
