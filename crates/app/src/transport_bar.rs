use eframe::egui;
use jamhub_engine::EngineCommand;
use jamhub_model::TransportState;

use crate::DawApp;

/// Common time signature presets for the selector popup.
const TIME_SIG_PRESETS: &[(u8, u8)] = &[
    (4, 4), (3, 4), (2, 4), (6, 8), (5, 4), (7, 8), (12, 8),
];

pub fn show(app: &mut DawApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let state = app.transport_state();

        // === TRANSPORT BUTTONS — circular, larger, prominent ===
        let small_btn = egui::vec2(32.0, 32.0);
        let big_btn = egui::vec2(40.0, 40.0);
        let small_r = 15.0;
        let big_r = 19.0;

        // RTZ — return to zero
        let rtz_rect = ui.allocate_space(small_btn).1;
        let rtz_resp = ui.interact(rtz_rect, ui.id().with("rtz"), egui::Sense::click());
        ui.painter().circle_filled(rtz_rect.center(), small_r, egui::Color32::from_rgb(36, 37, 44));
        if rtz_resp.hovered() {
            ui.painter().circle_filled(rtz_rect.center(), small_r, egui::Color32::from_rgb(44, 45, 54));
            ui.painter().circle_stroke(rtz_rect.center(), small_r, egui::Stroke::new(1.5, egui::Color32::from_rgb(235, 180, 60)));
        }
        ui.painter().text(rtz_rect.center(), egui::Align2::CENTER_CENTER, "RTZ", egui::FontId::proportional(10.0), egui::Color32::from_rgb(200, 198, 194));
        if rtz_resp.on_hover_text("Return to zero [Home]").clicked() {
            app.send_command(EngineCommand::SetPosition(0));
            app.scroll_x = 0.0;
        }

        // Rewind
        let rewind_rect = ui.allocate_space(small_btn).1;
        let rewind_resp = ui.interact(rewind_rect, ui.id().with("rewind"), egui::Sense::click());
        ui.painter().circle_filled(rewind_rect.center(), small_r, egui::Color32::from_rgb(36, 37, 44));
        if rewind_resp.hovered() {
            ui.painter().circle_filled(rewind_rect.center(), small_r, egui::Color32::from_rgb(44, 45, 54));
            ui.painter().circle_stroke(rewind_rect.center(), small_r, egui::Stroke::new(1.5, egui::Color32::from_rgb(235, 180, 60)));
        }
        ui.painter().text(rewind_rect.center(), egui::Align2::CENTER_CENTER, "\u{23EE}", egui::FontId::proportional(16.0), egui::Color32::from_rgb(230, 228, 224));
        if rewind_resp.on_hover_text("Rewind to start [Home]").clicked() {
            app.send_command(EngineCommand::SetPosition(0));
        }

        // Stop
        let stop_active = state == TransportState::Stopped;
        let stop_rect = ui.allocate_space(small_btn).1;
        let stop_resp = ui.interact(stop_rect, ui.id().with("stop"), egui::Sense::click());
        let stop_bg = if stop_active { egui::Color32::from_rgb(62, 64, 78) } else { egui::Color32::from_rgb(36, 37, 44) };
        ui.painter().circle_filled(stop_rect.center(), small_r, stop_bg);
        if stop_resp.hovered() {
            ui.painter().circle_filled(stop_rect.center(), small_r, egui::Color32::from_rgb(48, 50, 60));
            ui.painter().circle_stroke(stop_rect.center(), small_r, egui::Stroke::new(1.5, egui::Color32::from_rgb(235, 180, 60)));
        }
        ui.painter().text(stop_rect.center(), egui::Align2::CENTER_CENTER, "\u{23F9}", egui::FontId::proportional(16.0), egui::Color32::WHITE);
        if stop_resp.on_hover_text("Stop playback").clicked() {
            app.send_command(EngineCommand::Stop);
        }

        // Play — large green/amber circle
        let playing = state == TransportState::Playing;
        let play_rect = ui.allocate_space(big_btn).1;
        let play_resp = ui.interact(play_rect, ui.id().with("play"), egui::Sense::click());
        let play_bg = if playing { egui::Color32::from_rgb(235, 180, 60) } else { egui::Color32::from_rgb(80, 200, 80) };
        // Subtle glow when playing
        if playing {
            ui.painter().circle_filled(play_rect.center(), big_r + 3.0, egui::Color32::from_rgba_premultiplied(235, 180, 60, 35));
        }
        ui.painter().circle_filled(play_rect.center(), big_r, play_bg);
        if play_resp.hovered() {
            ui.painter().circle_stroke(play_rect.center(), big_r, egui::Stroke::new(2.0, egui::Color32::WHITE));
        }
        ui.painter().text(play_rect.center(), egui::Align2::CENTER_CENTER, "\u{25B6}", egui::FontId::proportional(18.0), egui::Color32::WHITE);
        if play_resp.on_hover_text("Play [Space]").clicked() {
            app.send_command(EngineCommand::Play);
        }

        // Record — large red circle, pulsing glow when recording
        let rec_rect = ui.allocate_space(big_btn).1;
        let rec_resp = ui.interact(rec_rect, ui.id().with("record"), egui::Sense::click());
        let rec_bg = if app.is_recording {
            // Pulsing red — animate brightness
            let pulse = (ui.input(|i| i.time) * 2.5).sin() as f32 * 0.18 + 0.82;
            let r = (232.0 * pulse) as u8;
            egui::Color32::from_rgb(r, 30, 30)
        } else {
            egui::Color32::from_rgb(36, 37, 44)
        };
        // Pulsing outer glow when recording
        if app.is_recording {
            let glow_pulse = (ui.input(|i| i.time) * 2.5).sin() as f32 * 0.3 + 0.7;
            let glow_alpha = (50.0 * glow_pulse) as u8;
            ui.painter().circle_filled(rec_rect.center(), big_r + 4.0, egui::Color32::from_rgba_premultiplied(232, 50, 50, glow_alpha));
        }
        ui.painter().circle_filled(rec_rect.center(), big_r, rec_bg);
        if rec_resp.hovered() {
            ui.painter().circle_stroke(rec_rect.center(), big_r, egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 80, 80)));
        }
        let rec_dot_color = if app.is_recording { egui::Color32::WHITE } else { egui::Color32::from_rgb(232, 80, 80) };
        ui.painter().circle_filled(rec_rect.center(), 7.0, rec_dot_color);
        if rec_resp.on_hover_text("Record [R]\nRecords onto the selected track").clicked() {
            app.toggle_recording();
        }
        if app.is_recording {
            ui.ctx().request_repaint(); // keep pulsing
        }

        ui.add_space(8.0);

        // === TOGGLE STRIP — rounded pill buttons ===
        let ts = egui::vec2(28.0, 22.0);

        // Metronome toggle with right-click settings popup
        {
            let active = app.metronome_enabled;
            let accent = egui::Color32::from_rgb(200, 160, 30);
            let bg = if active { accent.gamma_multiply(0.25) } else { egui::Color32::from_rgb(34, 35, 42) };
            let tc = if active { accent } else { egui::Color32::from_rgb(140, 138, 132) };
            let resp = ui.add_sized(ts, egui::Button::new(
                egui::RichText::new("M").size(11.0).strong().color(tc)
            ).fill(bg).corner_radius(11.0));
            if resp.on_hover_text("Metronome [M]\nRight-click for settings").clicked() {
                app.metronome_enabled = !app.metronome_enabled;
                app.send_command(EngineCommand::SetMetronome(app.metronome_enabled));
            }
            if resp.secondary_clicked() {
                app.show_metronome_settings = true;
            }
        }

        // Metronome settings popup window
        if app.show_metronome_settings {
            let mut open = app.show_metronome_settings;
            egui::Window::new("Metronome Settings")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(220.0)
                .show(ui.ctx(), |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;

                    // Volume slider
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Volume").size(11.0));
                        let mut vol = app.metronome_volume;
                        if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0)
                            .show_value(false)
                            .trailing_fill(true)).changed() {
                            app.metronome_volume = vol;
                        }
                        ui.label(egui::RichText::new(format!("{:.0}%", app.metronome_volume * 100.0))
                            .size(10.0).color(egui::Color32::from_rgb(140, 140, 150)));
                    });

                    // Accent first beat
                    ui.checkbox(&mut app.metronome_accent_first_beat, "Accent first beat");

                    // Count-in bars
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Count-in bars").size(11.0));
                        for bars in [1u32, 2, 4] {
                            let selected = app.metronome_count_in_bars == bars;
                            let btn_color = if selected {
                                egui::Color32::from_rgb(200, 160, 30)
                            } else {
                                egui::Color32::from_rgb(140, 140, 150)
                            };
                            if ui.add(egui::Button::new(
                                egui::RichText::new(format!("{bars}")).color(btn_color).size(11.0)
                            ).fill(if selected {
                                egui::Color32::from_rgb(50, 45, 20)
                            } else {
                                egui::Color32::from_rgb(34, 35, 42)
                            }).corner_radius(6.0).min_size(egui::vec2(28.0, 20.0))).clicked() {
                                app.metronome_count_in_bars = bars;
                            }
                        }
                    });
                });
            app.show_metronome_settings = open;
        }

        toggle_pill(ui, "L", ts, app.loop_enabled,
            egui::Color32::from_rgb(80, 160, 220),
            "Loop [L]", || {
            app.loop_enabled = !app.loop_enabled;
            if app.loop_enabled && app.loop_end == 0 {
                let sr = app.sample_rate() as f64;
                let beats = app.project.time_signature.numerator as f64 * 4.0;
                app.loop_start = 0;
                app.loop_end = app.project.tempo.sample_at_beat(beats, sr);
            }
            app.send_command(EngineCommand::SetLoop {
                enabled: app.loop_enabled, start: app.loop_start, end: app.loop_end,
            });
        });

        toggle_pill(ui, "I", ts, app.input_monitor.is_enabled(),
            egui::Color32::from_rgb(200, 140, 40),
            "Input monitor [I]\nHear mic in real-time", || {
            app.toggle_input_monitor();
        });

        toggle_pill(ui, "A", ts, app.show_automation,
            egui::Color32::from_rgb(200, 160, 40),
            "Automation [A]\nClick timeline to add points", || {
            app.show_automation = !app.show_automation;
        });

        toggle_pill(ui, "C", ts, app.count_in_enabled,
            egui::Color32::from_rgb(210, 130, 50),
            "Count-in [C]\nPlay 1 bar of metronome before recording starts", || {
            app.count_in_enabled = !app.count_in_enabled;
        });

        toggle_pill(ui, "P", ts, app.punch_recording,
            egui::Color32::from_rgb(220, 70, 70),
            "Punch In/Out [P]\nRecord only within the time selection.\nStarts playback 1 bar before selection (pre-roll)", || {
            app.punch_recording = !app.punch_recording;
        });

        toggle_pill(ui, "F", ts, app.follow_playhead,
            egui::Color32::from_rgb(80, 170, 200),
            "Follow playhead [H]\nAuto-scroll timeline to keep playhead visible during playback", || {
            app.follow_playhead = !app.follow_playhead;
        });

        ui.separator();

        // === TIME DISPLAY — recessed panel with monospace font ===
        let pos = app.position_samples();
        let sr = app.sample_rate();
        let seconds = pos as f64 / sr as f64;
        let minutes = (seconds / 60.0) as u32;
        let secs = seconds % 60.0;
        let millis = ((secs - secs.floor()) * 1000.0) as u32;
        let beat = app.project.tempo.beat_at_sample(pos, sr as f64);
        let bar = (beat / app.project.time_signature.numerator as f64).floor() as u32 + 1;
        let beat_in_bar = (beat % app.project.time_signature.numerator as f64).floor() as u32 + 1;
        let ticks_per_beat = 480u32; // standard MIDI resolution
        let tick_frac = beat - beat.floor();
        let ticks = (tick_frac * ticks_per_beat as f64) as u32;

        egui::Frame::default()
            .fill(egui::Color32::from_rgb(12, 12, 16))
            .inner_margin(egui::Margin::symmetric(14, 5))
            .corner_radius(8.0)
            .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(40, 40, 50)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;

                    // Count-in countdown overlay
                    if let Some(beats_left) = app.count_in_beats_remaining {
                        ui.monospace(
                            egui::RichText::new(format!("{beats_left}..."))
                                .size(18.0)
                                .strong()
                                .color(egui::Color32::from_rgb(255, 100, 80)),
                        );
                    } else {
                        // MM:SS.ms format
                        ui.monospace(
                            egui::RichText::new(format!("{minutes:02}:{:02}.{millis:03}", secs as u32))
                                .size(16.0)
                                .color(egui::Color32::from_rgb(100, 220, 140)),
                        );
                        // Bars.Beats.Ticks format
                        ui.monospace(
                            egui::RichText::new(format!("{bar}.{beat_in_bar}.{ticks:03}"))
                                .size(16.0)
                                .color(egui::Color32::from_rgb(235, 200, 100)),
                        );
                    }

                    // Punch indicator
                    if app.punch_recording {
                        ui.monospace(
                            egui::RichText::new("PUNCH")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(255, 80, 80)),
                        );
                    }

                    // Ripple mode indicator
                    if app.ripple_mode {
                        ui.monospace(
                            egui::RichText::new("RIPPLE")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(255, 150, 70)),
                        );
                    }
                });
            });

        ui.separator();

        // === TEMPO / TAP / SIG / SNAP / MASTER ===
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                // BPM in a recessed pill container
                egui::Frame::default()
                    .fill(egui::Color32::from_rgb(18, 18, 24))
                    .inner_margin(egui::Margin::symmetric(10, 3))
                    .corner_radius(12.0)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 42, 52)))
                    .show(ui, |ui| {
                        let mut bpm = app.project.tempo.bpm;
                        if ui.add(egui::DragValue::new(&mut bpm).range(20.0..=300.0).speed(0.5).suffix(" bpm"))
                            .on_hover_text("Project tempo — drag or click to type").changed() {
                            app.project.tempo.bpm = bpm;
                            app.sync_project();
                        }
                    });

                // Tap tempo button
                let tap_bg = egui::Color32::from_rgb(44, 40, 54);
                if ui.add(egui::Button::new(
                    egui::RichText::new("TAP").size(10.0).color(egui::Color32::from_rgb(200, 170, 255)))
                    .fill(tap_bg)
                    .corner_radius(10.0))
                    .on_hover_text("Tap Tempo — click repeatedly to set BPM from tap timing")
                    .clicked()
                {
                    let now = std::time::Instant::now();
                    // Expire old taps (more than 2 seconds since last tap)
                    if let Some(last) = app.tap_tempo_times.last() {
                        if now.duration_since(*last).as_secs_f64() > 2.0 {
                            app.tap_tempo_times.clear();
                        }
                    }
                    app.tap_tempo_times.push(now);
                    // Keep only last 8 taps
                    if app.tap_tempo_times.len() > 8 {
                        app.tap_tempo_times.remove(0);
                    }
                    // Need at least 2 taps to calculate BPM
                    if app.tap_tempo_times.len() >= 2 {
                        let intervals: Vec<f64> = app.tap_tempo_times.windows(2)
                            .map(|w| w[1].duration_since(w[0]).as_secs_f64())
                            .collect();
                        let avg_interval = intervals.iter().sum::<f64>() / intervals.len() as f64;
                        let tap_bpm = (60.0 / avg_interval).clamp(20.0, 300.0);
                        app.project.tempo.bpm = (tap_bpm * 10.0).round() / 10.0; // round to 0.1
                        app.sync_project();
                        app.set_status(&format!("Tap tempo: {:.1} BPM", app.project.tempo.bpm));
                    }
                }

                ui.add_space(2.0);

                // Time signature — drag values + preset selector
                let mut num = app.project.time_signature.numerator as i32;
                let mut den = app.project.time_signature.denominator as i32;
                ui.add(egui::DragValue::new(&mut num).range(1..=16).speed(0.1));
                ui.label(egui::RichText::new("/").color(egui::Color32::from_rgb(110, 108, 104)));
                ui.add(egui::DragValue::new(&mut den).range(1..=16).speed(0.1));
                if num != app.project.time_signature.numerator as i32 || den != app.project.time_signature.denominator as i32 {
                    app.project.time_signature.numerator = num as u8;
                    app.project.time_signature.denominator = den as u8;
                    app.sync_project();
                }

                // Time signature preset button — cycle through common presets
                let cur_num = app.project.time_signature.numerator;
                let cur_den = app.project.time_signature.denominator;
                let sig_label = format!("{cur_num}/{cur_den}");
                let sig_popup_id = ui.id().with("time_sig_popup");
                let sig_btn_response = ui.add(egui::Button::new(
                    egui::RichText::new(format!("{sig_label} \u{25BC}")).size(10.0).color(egui::Color32::from_rgb(190, 188, 184)))
                    .fill(egui::Color32::from_rgb(36, 37, 44))
                    .corner_radius(10.0))
                    .on_hover_text("Time signature presets");
                if sig_btn_response.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(sig_popup_id));
                }
                let below = egui::AboveOrBelow::Below;
                egui::popup::popup_above_or_below_widget(ui, sig_popup_id, &sig_btn_response, below, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                    ui.set_min_width(80.0);
                    for &(n, d) in TIME_SIG_PRESETS {
                        let label = format!("{n}/{d}");
                        let selected = cur_num == n && cur_den == d;
                        if ui.selectable_label(selected, &label).clicked() {
                            app.project.time_signature.numerator = n;
                            app.project.time_signature.denominator = d;
                            app.sync_project();
                            app.set_status(&format!("Time signature: {label}"));
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                // Snap mode
                let snap_label = app.snap_mode.label();
                let snap_bg = if app.snap_mode != crate::SnapMode::Off {
                    egui::Color32::from_rgb(40, 52, 76)
                } else {
                    egui::Color32::from_rgb(36, 37, 44)
                };
                if ui.add(egui::Button::new(
                    egui::RichText::new(format!("Snap: {snap_label}")).small().color(egui::Color32::from_rgb(190, 198, 220)))
                    .fill(snap_bg)
                    .corner_radius(10.0))
                    .on_hover_text("Snap mode [G]\nFree / 1/2 Beat / Beat / Bar").clicked() {
                    app.snap_mode = app.snap_mode.next();
                }

                // Master vol
                let mut mv = app.master_volume;
                if ui.add(egui::DragValue::new(&mut mv).range(0.0..=1.5).speed(0.01)
                    .custom_formatter(|v, _| format!("Vol {:.0}%", v * 100.0)))
                    .on_hover_text("Master volume").changed() {
                    app.master_volume = mv;
                    app.send_command(EngineCommand::SetMasterVolume(mv));
                }
            });
        });

        // === RIGHT SIDE — logo + session ===
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.session.is_connected() {
                ui.colored_label(egui::Color32::from_rgb(80, 210, 100), "\u{25CF} Online");
                ui.separator();
            }
            ui.label(
                egui::RichText::new("JamHub")
                    .size(17.0)
                    .strong()
                    .color(egui::Color32::from_rgb(235, 180, 60)),
            );
        });
    });
}

fn toggle_pill(
    ui: &mut egui::Ui,
    label: &str,
    size: egui::Vec2,
    active: bool,
    active_color: egui::Color32,
    tooltip: &str,
    mut on_click: impl FnMut(),
) {
    let bg = if active { active_color } else { egui::Color32::from_rgb(32, 33, 40) };
    let text_color = if active { egui::Color32::WHITE } else { egui::Color32::from_rgb(145, 142, 138) };
    if ui.add_sized(size, egui::Button::new(
        egui::RichText::new(label).size(11.0).color(text_color))
        .fill(bg)
        .corner_radius(11.0))
        .on_hover_text(tooltip).clicked() {
        on_click();
    }
}
