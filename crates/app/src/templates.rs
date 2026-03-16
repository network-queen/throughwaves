use eframe::egui;
use jamhub_model::{EffectSlot, TrackEffect, TrackKind, TrackSend};
use serde::{Deserialize, Serialize};

use crate::DawApp;

// ── Track Templates ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackTemplate {
    pub name: String,
    pub track_kind: TrackKind,
    pub effects: Vec<EffectSlot>,
    pub sends: Vec<TrackSend>,
    pub color: [u8; 3],
    pub volume: f32,
    pub pan: f32,
}

fn track_templates_path() -> std::path::PathBuf {
    crate::config_dir().join("track_templates.json")
}

pub fn load_track_templates() -> Vec<TrackTemplate> {
    let path = track_templates_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(list) = serde_json::from_str::<Vec<TrackTemplate>>(&data) {
            return list;
        }
    }
    Vec::new()
}

pub fn save_track_templates(templates: &[TrackTemplate]) {
    let dir = crate::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(templates) {
        let _ = std::fs::write(track_templates_path(), json);
    }
}

// ── FX Chain Presets ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxPreset {
    pub name: String,
    pub effects: Vec<EffectSlot>,
}

fn fx_presets_path() -> std::path::PathBuf {
    crate::config_dir().join("fx_presets.json")
}

pub fn load_fx_presets() -> Vec<FxPreset> {
    let path = fx_presets_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(list) = serde_json::from_str::<Vec<FxPreset>>(&data) {
            return list;
        }
    }
    Vec::new()
}

pub fn save_fx_presets(presets: &[FxPreset]) {
    let dir = crate::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(presets) {
        let _ = std::fs::write(fx_presets_path(), json);
    }
}

/// Built-in default FX chain presets that are always available.
pub fn default_fx_presets() -> Vec<FxPreset> {
    vec![
        FxPreset {
            name: "Vocal Chain".into(),
            effects: vec![
                EffectSlot::new(TrackEffect::EqBand {
                    freq_hz: 200.0,
                    gain_db: -3.0,
                    q: 1.0,
                }),
                EffectSlot::new(TrackEffect::Compressor {
                    threshold_db: -18.0,
                    ratio: 4.0,
                    attack_ms: 10.0,
                    release_ms: 100.0,
                }),
                EffectSlot::new(TrackEffect::Reverb {
                    decay: 0.4,
                    mix: 0.2,
                }),
            ],
        },
        FxPreset {
            name: "Guitar Clean".into(),
            effects: vec![
                EffectSlot::new(TrackEffect::EqBand {
                    freq_hz: 800.0,
                    gain_db: 2.0,
                    q: 1.2,
                }),
                EffectSlot::new(TrackEffect::Chorus {
                    rate_hz: 1.5,
                    depth: 0.4,
                    mix: 0.3,
                }),
                EffectSlot::new(TrackEffect::Delay {
                    time_ms: 350.0,
                    feedback: 0.25,
                    mix: 0.2,
                }),
            ],
        },
        FxPreset {
            name: "Drum Bus".into(),
            effects: vec![
                EffectSlot::new(TrackEffect::Compressor {
                    threshold_db: -12.0,
                    ratio: 6.0,
                    attack_ms: 5.0,
                    release_ms: 60.0,
                }),
                EffectSlot::new(TrackEffect::EqBand {
                    freq_hz: 5000.0,
                    gain_db: 3.0,
                    q: 0.8,
                }),
                EffectSlot::new(TrackEffect::Distortion {
                    drive: 4.0,
                    mix: 0.15,
                }),
            ],
        },
        FxPreset {
            name: "Master Limiter".into(),
            effects: vec![
                EffectSlot::new(TrackEffect::Compressor {
                    threshold_db: -3.0,
                    ratio: 20.0,
                    attack_ms: 0.5,
                    release_ms: 50.0,
                }),
            ],
        },
    ]
}

// ── Color Palette ────────────────────────────────────────────────────

/// 16 preset colors for the track color palette (warm/cool tones in a grid).
pub const PALETTE_COLORS: &[([u8; 3], &str)] = &[
    // Row 1: warm reds/oranges
    ([220, 60, 60], "Red"),
    ([230, 100, 50], "Vermilion"),
    ([235, 150, 45], "Orange"),
    ([220, 195, 50], "Gold"),
    // Row 2: greens/teals
    ([120, 200, 60], "Lime"),
    ([60, 185, 75], "Green"),
    ([50, 180, 150], "Teal"),
    ([55, 170, 210], "Cyan"),
    // Row 3: blues/purples
    ([70, 120, 220], "Blue"),
    ([100, 90, 230], "Indigo"),
    ([155, 80, 220], "Violet"),
    ([200, 70, 200], "Magenta"),
    // Row 4: pinks/neutrals
    ([220, 80, 150], "Pink"),
    ([210, 140, 120], "Salmon"),
    ([160, 160, 160], "Gray"),
    ([220, 215, 200], "Cream"),
];

/// State for the track template naming dialog.
pub struct TemplateNameInput {
    pub name: String,
    pub track_idx: usize,
}

/// State for the FX preset naming dialog.
pub struct FxPresetNameInput {
    pub name: String,
}

/// State for a custom RGB color picker.
pub struct CustomColorInput {
    pub track_idx: usize,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Show the "Save Track Template" naming dialog.
pub fn show_template_name_dialog(app: &mut DawApp, ctx: &egui::Context) {
    if app.template_name_input.is_none() {
        return;
    }

    let mut close = false;
    let mut save = false;
    let mut name_buf = String::new();
    let mut tidx = 0;

    if let Some(ref input) = app.template_name_input {
        name_buf = input.name.clone();
        tidx = input.track_idx;
    }

    egui::Window::new("Save Track Template")
        .collapsible(false)
        .resizable(false)
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.label("Template name:");
            let resp = ui.text_edit_singleline(&mut name_buf);
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                save = true;
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if save && !name_buf.trim().is_empty() {
        if tidx < app.project.tracks.len() {
            let track = &app.project.tracks[tidx];
            let template = TrackTemplate {
                name: name_buf.trim().to_string(),
                track_kind: track.kind,
                effects: track.effects.clone(),
                sends: track.sends.clone(),
                color: track.color,
                volume: track.volume,
                pan: track.pan,
            };
            let mut templates = load_track_templates();
            templates.push(template);
            save_track_templates(&templates);
            app.set_status("Track template saved");
        }
        app.template_name_input = None;
    } else if close {
        app.template_name_input = None;
    } else {
        app.template_name_input = Some(TemplateNameInput {
            name: name_buf,
            track_idx: tidx,
        });
    }
}

/// Show the "Save FX Preset" naming dialog.
pub fn show_fx_preset_name_dialog(app: &mut DawApp, ctx: &egui::Context) {
    if app.fx_preset_name_input.is_none() {
        return;
    }

    let mut close = false;
    let mut save = false;
    let mut name_buf = String::new();

    if let Some(ref input) = app.fx_preset_name_input {
        name_buf = input.name.clone();
    }

    egui::Window::new("Save FX Preset")
        .collapsible(false)
        .resizable(false)
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.label("Preset name:");
            let resp = ui.text_edit_singleline(&mut name_buf);
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                save = true;
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if save && !name_buf.trim().is_empty() {
        let track_idx = match app.selected_track {
            Some(i) if i < app.project.tracks.len() => Some(i),
            _ => None,
        };
        if let Some(tidx) = track_idx {
            let effects = app.project.tracks[tidx].effects.clone();
            let preset = FxPreset {
                name: name_buf.trim().to_string(),
                effects,
            };
            let mut presets = load_fx_presets();
            presets.push(preset);
            save_fx_presets(&presets);
            app.set_status("FX preset saved");
        }
        app.fx_preset_name_input = None;
    } else if close {
        app.fx_preset_name_input = None;
    } else {
        app.fx_preset_name_input = Some(FxPresetNameInput { name: name_buf });
    }
}

/// Show the custom RGB color input dialog.
pub fn show_custom_color_dialog(app: &mut DawApp, ctx: &egui::Context) {
    if app.custom_color_input.is_none() {
        return;
    }

    let mut close = false;
    let mut apply = false;
    let mut r = 128u8;
    let mut g = 128u8;
    let mut b = 128u8;
    let mut tidx = 0;

    if let Some(ref input) = app.custom_color_input {
        r = input.r;
        g = input.g;
        b = input.b;
        tidx = input.track_idx;
    }

    egui::Window::new("Custom Track Color")
        .collapsible(false)
        .resizable(false)
        .default_width(220.0)
        .show(ctx, |ui| {
            // Preview swatch
            let preview_color = egui::Color32::from_rgb(r, g, b);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 4.0, preview_color);

            ui.add_space(4.0);

            let mut ri = r as i32;
            let mut gi = g as i32;
            let mut bi = b as i32;
            ui.horizontal(|ui| {
                ui.label("R");
                ui.add(egui::Slider::new(&mut ri, 0..=255));
            });
            ui.horizontal(|ui| {
                ui.label("G");
                ui.add(egui::Slider::new(&mut gi, 0..=255));
            });
            ui.horizontal(|ui| {
                ui.label("B");
                ui.add(egui::Slider::new(&mut bi, 0..=255));
            });
            r = ri as u8;
            g = gi as u8;
            b = bi as u8;

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    apply = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if apply {
        if tidx < app.project.tracks.len() {
            app.push_undo("Set track color");
            app.project.tracks[tidx].color = [r, g, b];
            app.sync_project();
        }
        app.custom_color_input = None;
    } else if close {
        app.custom_color_input = None;
    } else {
        app.custom_color_input = Some(CustomColorInput {
            track_idx: tidx,
            r,
            g,
            b,
        });
    }
}
