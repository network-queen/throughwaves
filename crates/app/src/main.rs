mod about;
mod analysis_tools;
mod audio_settings;
mod effects_panel;
mod fx_browser;
mod jam_session;
#[allow(dead_code)]
mod media_browser;
mod midi_mapping;
mod midi_panel;
mod mixer_view;
mod piano_roll;
#[allow(dead_code)]
mod platform_panel;
mod session_panel;
mod session_view;
mod spectrum;
mod timeline;
mod shortcuts_panel;
mod transport_bar;
mod undo;
mod plugin_window;
mod undo_panel;
mod project_info;
mod stem_separator;
#[allow(dead_code)]
mod version_control;
pub mod templates;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::fs;

use eframe::egui;
use jamhub_engine::{load_audio, EngineCommand, EngineHandle, ExportFormat, ExportOptions, InputMonitor, LevelMeters, Recorder, WaveformCache};
use jamhub_model::{Clip, ClipSource, Project, TrackKind, TransportState};
use uuid::Uuid;

use session_panel::SessionPanel;
use undo::UndoManager;

/// Draw a compact rounded info pill in the status bar with a colored indicator dot.
fn status_pill(ui: &mut egui::Ui, text: &str, color: egui::Color32, highlight: bool) {
    let pill_bg = if highlight {
        egui::Color32::from_rgba_premultiplied(color.r() / 4, color.g() / 4, color.b() / 4, 80)
    } else {
        egui::Color32::from_rgb(22, 22, 28)
    };
    egui::Frame::default()
        .fill(pill_bg)
        .inner_margin(egui::Margin::symmetric(7, 2))
        .corner_radius(10.0)
        .stroke(egui::Stroke::new(0.5, egui::Color32::from_rgb(40, 40, 50)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                // Colored indicator dot
                let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
                ui.painter().circle_filled(dot_rect.center(), 3.0, color);
                ui.label(egui::RichText::new(text).size(10.0).color(color));
            });
        });
}

fn setup_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    // Ultra-modern dark theme — deep rich backgrounds with warm blue/purple undertones
    let bg = egui::Color32::from_rgb(17, 17, 20);          // #111114 — deepest background
    let panel_bg = egui::Color32::from_rgb(24, 25, 30);    // #18191E — panel surfaces
    let _surface = egui::Color32::from_rgb(31, 32, 40);    // #1F2028 — elevated surfaces
    let widget_bg = egui::Color32::from_rgb(38, 39, 48);   // warm charcoal with blue undertone
    let widget_hover = egui::Color32::from_rgb(50, 52, 64); // hover state — visible lift
    let widget_active = egui::Color32::from_rgb(62, 64, 78); // active/pressed
    let accent = egui::Color32::from_rgb(240, 192, 64);    // #F0C040 — warm gold primary accent
    let selection = egui::Color32::from_rgb(80, 200, 190);  // soft teal — selection/highlights
    let text = egui::Color32::from_rgb(238, 236, 232);     // warm white — high contrast
    let text_dim = egui::Color32::from_rgb(128, 126, 135); // secondary text with purple undertone

    visuals.panel_fill = panel_bg;
    visuals.window_fill = egui::Color32::from_rgb(22, 23, 28);
    visuals.extreme_bg_color = bg;
    visuals.faint_bg_color = egui::Color32::from_rgb(26, 27, 33);

    // Widget styles — refined corners, smooth hover transitions
    visuals.widgets.noninteractive.bg_fill = panel_bg;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text_dim);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(10);

    visuals.widgets.inactive.bg_fill = widget_bg;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;

    visuals.widgets.hovered.bg_fill = widget_hover;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent.gamma_multiply(0.50));
    visuals.widgets.hovered.expansion = 1.0; // subtle grow on hover

    visuals.widgets.active.bg_fill = widget_active;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.open.bg_fill = widget_hover;
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(6);

    visuals.selection.bg_fill = selection.gamma_multiply(0.22);
    visuals.selection.stroke = egui::Stroke::new(1.5, selection);

    // Windows — glass-like with warm glow, matching the web platform
    visuals.window_fill = egui::Color32::from_rgb(20, 21, 26);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 32,
        spread: 4,
        color: egui::Color32::from_black_alpha(80),
    };
    visuals.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 46, 56));
    visuals.window_corner_radius = egui::CornerRadius::same(12);

    // Popup menus
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 2,
        color: egui::Color32::from_black_alpha(70),
    };

    // Separator
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, egui::Color32::from_rgb(38, 40, 50));

    ctx.set_visuals(visuals);

    // Typography & spacing — generous, readable, modern
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(7.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(16);
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: 3.0,
        handle_min_length: 24.0,
        bar_inner_margin: 1.0,
        bar_outer_margin: 1.0,
        floating: true,
        ..style.spacing.scroll
    };

    // Premium font sizes — larger body, spacious headings
    use egui::FontId;
    use egui::TextStyle;
    style.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
    style.text_styles.insert(TextStyle::Heading, FontId::proportional(20.0));
    style.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
    style.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
    style.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.5));

    ctx.set_style(style);
}

fn apply_theme(ctx: &egui::Context, theme: ThemeChoice) {
    let mut visuals = egui::Visuals::dark();

    // Ultra-modern palette — warm charcoal with blue/purple undertones, no flat grays
    let (bg, panel_bg, widget_bg, widget_hover, widget_active, accent, text, text_dim, win_fill, win_stroke_col) = match theme {
        ThemeChoice::Dark => (
            egui::Color32::from_rgb(17, 17, 20),      // #111114
            egui::Color32::from_rgb(24, 25, 30),      // #18191E
            egui::Color32::from_rgb(38, 39, 48),      // warm charcoal + blue
            egui::Color32::from_rgb(50, 52, 64),      // hover lift
            egui::Color32::from_rgb(62, 64, 78),      // active/pressed
            egui::Color32::from_rgb(240, 192, 64),    // #F0C040 warm gold
            egui::Color32::from_rgb(238, 236, 232),   // warm white
            egui::Color32::from_rgb(128, 126, 135),   // purple-tinted dim
            egui::Color32::from_rgb(22, 23, 28),
            egui::Color32::from_rgb(38, 39, 48),
        ),
        ThemeChoice::Darker => (
            egui::Color32::from_rgb(13, 13, 17),
            egui::Color32::from_rgb(19, 20, 26),
            egui::Color32::from_rgb(32, 33, 42),
            egui::Color32::from_rgb(44, 46, 58),
            egui::Color32::from_rgb(54, 56, 70),
            egui::Color32::from_rgb(230, 180, 55),
            egui::Color32::from_rgb(228, 226, 222),
            egui::Color32::from_rgb(118, 116, 125),
            egui::Color32::from_rgb(17, 17, 23),
            egui::Color32::from_rgb(34, 35, 44),
        ),
        ThemeChoice::Midnight => (
            egui::Color32::from_rgb(8, 8, 14),
            egui::Color32::from_rgb(12, 13, 20),
            egui::Color32::from_rgb(24, 25, 36),
            egui::Color32::from_rgb(36, 38, 52),
            egui::Color32::from_rgb(46, 48, 64),
            egui::Color32::from_rgb(220, 170, 50),
            egui::Color32::from_rgb(210, 208, 205),
            egui::Color32::from_rgb(108, 106, 118),
            egui::Color32::from_rgb(14, 14, 24),
            egui::Color32::from_rgb(28, 30, 42),
        ),
    };

    visuals.panel_fill = panel_bg;
    visuals.window_fill = win_fill;
    visuals.extreme_bg_color = bg;
    visuals.faint_bg_color = egui::Color32::from_rgb(
        ((panel_bg.r() as u16 + bg.r() as u16) / 2) as u8,
        ((panel_bg.g() as u16 + bg.g() as u16) / 2) as u8,
        ((panel_bg.b() as u16 + bg.b() as u16) / 2) as u8,
    );

    visuals.widgets.noninteractive.bg_fill = panel_bg;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text_dim);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(10);

    visuals.widgets.inactive.bg_fill = widget_bg;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;

    visuals.widgets.hovered.bg_fill = widget_hover;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent.gamma_multiply(0.50));
    visuals.widgets.hovered.expansion = 1.0;

    visuals.widgets.active.bg_fill = widget_active;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.open.bg_fill = widget_hover;
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(6);

    let teal = egui::Color32::from_rgb(80, 200, 190);
    visuals.selection.bg_fill = teal.gamma_multiply(0.22);
    visuals.selection.stroke = egui::Stroke::new(1.5, teal);

    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 24,
        spread: 0,
        color: egui::Color32::from_black_alpha(60),
    };
    visuals.window_stroke = egui::Stroke::new(1.0, win_stroke_col);
    visuals.window_corner_radius = egui::CornerRadius::same(10);

    ctx.set_visuals(visuals);

    // Scrollbar styling — ultra-thin floating, invisible until hover
    let mut style = (*ctx.style()).clone();
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: 3.0,
        handle_min_length: 24.0,
        bar_inner_margin: 1.0,
        bar_outer_margin: 1.0,
        floating: true,
        ..style.spacing.scroll
    };
    ctx.set_style(style);
}

/// Generate the ThroughWaves waveform icon as RGBA pixel data.
/// 3D look with depth, specular highlights, and light reflections.
fn generate_app_icon() -> egui::IconData {
    let size = 256u32;
    let sf = size as f32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = sf / 2.0;
    let corner_r = sf * 0.22;

    // Helper: blend (sr, sg, sb, sa) over pixel at idx
    let blend = |rgba: &mut Vec<u8>, idx: usize, sr: f32, sg: f32, sb: f32, sa: f32| {
        let bg_a = rgba[idx + 3] as f32 / 255.0;
        let out_a = sa + bg_a * (1.0 - sa);
        if out_a > 0.001 {
            rgba[idx]     = ((sr * sa + rgba[idx] as f32 * bg_a * (1.0 - sa)) / out_a).clamp(0.0, 255.0) as u8;
            rgba[idx + 1] = ((sg * sa + rgba[idx + 1] as f32 * bg_a * (1.0 - sa)) / out_a).clamp(0.0, 255.0) as u8;
            rgba[idx + 2] = ((sb * sa + rgba[idx + 2] as f32 * bg_a * (1.0 - sa)) / out_a).clamp(0.0, 255.0) as u8;
            rgba[idx + 3] = (out_a * 255.0).clamp(0.0, 255.0) as u8;
        }
    };

    // Rounded rect SDF
    let sdf = |fx: f32, fy: f32| -> f32 {
        let dx = (fx - center).abs() - (center - corner_r);
        let dy = (fy - center).abs() - (center - corner_r);
        dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - corner_r
    };

    // ── Pass 1: Drop shadow (offset down-right) ──
    let shadow_ox = sf * 0.02;
    let shadow_oy = sf * 0.03;
    let shadow_blur = sf * 0.06;
    for y in 0..size {
        for x in 0..size {
            let d = sdf(x as f32 - shadow_ox, y as f32 - shadow_oy);
            if d < shadow_blur {
                let a = ((1.0 - d / shadow_blur).clamp(0.0, 1.0) * 0.5 * 255.0) as u8;
                let idx = ((y * size + x) * 4) as usize;
                rgba[idx] = 0; rgba[idx + 1] = 0; rgba[idx + 2] = 0; rgba[idx + 3] = a;
            }
        }
    }

    // ── Pass 2: Background with 3D gradient + inner glow ──
    // Light source: top-left
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32;
            let fy = y as f32;
            let d = sdf(fx, fy);
            if d < 1.0 {
                let alpha = if d < -1.0 { 1.0 } else { 1.0 - d };
                // Base gradient: warm amber, darker at bottom-right
                let ny = fy / sf; // 0..1
                let nx = fx / sf;
                let t = (ny * 0.6 + nx * 0.3).min(1.0);
                let mut r = 245.0 - t * 55.0;
                let mut g = 195.0 - t * 65.0;
                let mut b = 70.0 - t * 35.0;

                // Inner bevel: bright top edge, dark bottom edge
                let edge_dist = (-d).min(sf * 0.04) / (sf * 0.04); // 0 at edge, 1 at 4% inside
                let bevel = 1.0 - edge_dist;
                let top_light = ((1.0 - ny) * bevel * 0.4).min(0.4);
                let bot_dark = (ny * bevel * 0.25).min(0.25);
                r = (r + top_light * 80.0 - bot_dark * 60.0).clamp(0.0, 255.0);
                g = (g + top_light * 60.0 - bot_dark * 50.0).clamp(0.0, 255.0);
                b = (b + top_light * 30.0 - bot_dark * 30.0).clamp(0.0, 255.0);

                let idx = ((y * size + x) * 4) as usize;
                blend(&mut rgba, idx, r, g, b, alpha);
            }
        }
    }

    // ── Pass 3: Glass highlight (top ellipse, white, fading) ──
    let hl_cx = center - sf * 0.05;
    let hl_cy = sf * 0.30;
    let hl_rx = sf * 0.35;
    let hl_ry = sf * 0.18;
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32;
            let fy = y as f32;
            if sdf(fx, fy) < -1.0 {
                let ex = (fx - hl_cx) / hl_rx;
                let ey = (fy - hl_cy) / hl_ry;
                let ed = ex * ex + ey * ey;
                if ed < 1.0 {
                    let a = (1.0 - ed).powi(2) * 0.30; // soft falloff
                    let idx = ((y * size + x) * 4) as usize;
                    blend(&mut rgba, idx, 255.0, 255.0, 255.0, a);
                }
            }
        }
    }

    // ── Pass 4: Waveform bars with 3D cylindrical shading ──
    let bar_heights: [f32; 5] = [0.30, 0.65, 0.50, 0.80, 0.22];
    let bar_width = sf * 0.09;
    let spacing = sf * 0.155;
    let start_x = center - 2.0 * spacing;
    let radius = sf * 0.42;

    for (i, &h_frac) in bar_heights.iter().enumerate() {
        let cx = start_x + i as f32 * spacing;
        let bar_h = radius * h_frac * 1.8;
        let top = center - bar_h / 2.0;
        let bot = center + bar_h / 2.0;
        let half_w = bar_width / 2.0;

        for y in 0..size {
            for x in 0..size {
                let fx = x as f32;
                let fy = y as f32;
                let ddx = (fx - cx).abs();
                if ddx > half_w + 2.0 || fy < top - 2.0 || fy > bot + 2.0 { continue; }

                // Capsule SDF
                let cap_d = if fy < top + half_w {
                    (fx - cx).hypot(fy - (top + half_w)) - half_w
                } else if fy > bot - half_w {
                    (fx - cx).hypot(fy - (bot - half_w)) - half_w
                } else {
                    ddx - half_w
                };

                if cap_d < 1.5 {
                    let fill_a = if cap_d < -0.5 { 1.0 } else { (1.0 - cap_d).clamp(0.0, 1.0) };

                    // Cylindrical 3D shading: lighter on left, darker on right
                    let nx_bar = ((fx - cx) / half_w).clamp(-1.0, 1.0); // -1 left, +1 right
                    let cylinder = 1.0 - nx_bar * nx_bar; // parabolic highlight in center
                    let ny_bar = if fy < top + half_w || fy > bot - half_w { 0.7 } else { 1.0 };

                    // Base dark color with cylindrical highlight
                    let base_r = 20.0 + cylinder * 45.0 * ny_bar;
                    let base_g = 16.0 + cylinder * 35.0 * ny_bar;
                    let base_b = 12.0 + cylinder * 20.0 * ny_bar;

                    // Specular highlight: bright spot on left side of each bar
                    let spec_x = (nx_bar + 0.4).clamp(0.0, 1.0); // shifted left
                    let spec = (1.0 - spec_x * 3.0).max(0.0).powi(3) * 0.5;

                    let r = (base_r + spec * 200.0).clamp(0.0, 255.0);
                    let g = (base_g + spec * 180.0).clamp(0.0, 255.0);
                    let b = (base_b + spec * 100.0).clamp(0.0, 255.0);

                    let idx = ((y * size + x) * 4) as usize;
                    blend(&mut rgba, idx, r, g, b, fill_a);
                }
            }
        }
    }

    // ── Pass 5: Specular sparkle dots (light reflections) ──
    let sparkles: [(f32, f32, f32); 6] = [
        (0.28, 0.22, 0.7),  // top-left bright
        (0.72, 0.25, 0.4),  // top-right medium
        (0.35, 0.75, 0.25), // bottom-left dim
        (0.50, 0.18, 0.5),  // top-center
        (0.65, 0.70, 0.2),  // bottom-right subtle
        (0.42, 0.35, 0.35), // near center
    ];
    for &(sx, sy, intensity) in &sparkles {
        let scx = sx * sf;
        let scy = sy * sf;
        if sdf(scx, scy) > -2.0 { continue; } // only inside the icon
        let sparkle_r = sf * 0.025;
        for y in 0..size {
            for x in 0..size {
                let dist = ((x as f32 - scx).powi(2) + (y as f32 - scy).powi(2)).sqrt();
                if dist < sparkle_r * 3.0 {
                    let a = (1.0 - dist / (sparkle_r * 3.0)).powi(3) * intensity;
                    if a > 0.01 {
                        let idx = ((y * size + x) * 4) as usize;
                        blend(&mut rgba, idx, 255.0, 252.0, 230.0, a);
                    }
                }
            }
        }
    }

    // ── Pass 6: Rim light (bright edge on top-left) ──
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32;
            let fy = y as f32;
            let d = sdf(fx, fy);
            if d > -3.0 && d < 0.5 {
                let edge = 1.0 - ((d + 1.5).abs() / 1.5).min(1.0);
                let angle = ((fy - center).atan2(fx - center) + std::f32::consts::PI) / (2.0 * std::f32::consts::PI);
                // Bright on top-left quadrant
                let rim = if angle > 0.5 && angle < 0.85 { edge * 0.6 } else { 0.0 };
                if rim > 0.01 {
                    let idx = ((y * size + x) * 4) as usize;
                    blend(&mut rgba, idx, 255.0, 248.0, 220.0, rim);
                }
            }
        }
    }

    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

fn main() -> eframe::Result<()> {
    // Single instance check: try to create a lock file
    let lock_path = config_dir().join(".lock");
    let lock_file = std::fs::File::create(&lock_path);
    let _lock = lock_file.ok().and_then(|f| {
        use std::os::unix::io::AsRawFd;
        let fd = f.as_raw_fd();
        let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if ret == 0 {
            Some(f) // hold the lock for app lifetime
        } else {
            eprintln!("ThroughWaves is already running.");
            std::process::exit(0);
        }
    });

    let icon = generate_app_icon();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("ThroughWaves — Collaborative DAW")
            .with_icon(std::sync::Arc::new(icon)),
        ..Default::default()
    };

    eframe::run_native(
        "ThroughWaves",
        options,
        Box::new(|cc| {
            setup_theme(&cc.egui_ctx);
            let mut app = DawApp::new();
            // Load saved platform credentials
            platform_panel::load_saved_credentials(&mut app.platform);
            // Apply saved preferences on startup
            apply_theme(&cc.egui_ctx, app.preferences.theme);
            cc.egui_ctx.set_pixels_per_point(app.preferences.ui_scale);
            Ok(Box::new(app))
        }),
    )
}

pub struct DawApp {
    pub project: Project,
    engine: Option<EngineHandle>,
    engine_error: Option<String>,
    pub view: View,
    /// Show mixer docked at the bottom of the arrange view (Reaper-style)
    pub show_mixer_panel: bool,
    pub zoom: f32,
    pub scroll_x: f32,
    recorder: Recorder,
    pub is_recording: bool,
    recording_start_pos: u64,
    pub status_message: Option<(String, std::time::Instant)>,
    pub selected_track: Option<usize>,
    pub selected_clips: HashSet<(usize, usize)>, // (track_idx, clip_idx) multi-select
    /// Rubber-band (marquee) selection state
    pub rubber_band_origin: Option<egui::Pos2>,
    pub rubber_band_active: bool,
    pub waveform_cache: WaveformCache,
    undo_manager: UndoManager,
    pub audio_buffers: HashMap<Uuid, Vec<f32>>,
    pub project_path: Option<PathBuf>,
    pub session: SessionPanel,
    pub jam: jam_session::JamSessionPanel,
    pub platform: platform_panel::PlatformPanel,
    pub metronome_enabled: bool,
    pub snap_mode: SnapMode,
    // Clip dragging state
    pub dragging_clip: Option<ClipDragState>,
    pub dragging_clips: Option<MultiClipDragState>,
    pub show_effects: bool,
    pub loop_enabled: bool,
    pub loop_start: u64,
    pub loop_end: u64,
    pub master_volume: f32,
    pub renaming_track: Option<(usize, String)>,
    pub show_piano_roll: bool,
    /// Set by piano roll when it handles Delete key, so main app skips delete
    pub piano_roll_consumed_delete: bool,
    /// Confirm track deletion dialog: (track_idx, track_name)
    pub confirm_delete_track: Option<(usize, String)>,
    pub show_about: bool,
    input_monitor: InputMonitor,
    pub resizing_track: Option<usize>,
    pub fx_browser: fx_browser::FxBrowser,
    pub media_browser: media_browser::MediaBrowser,
    pub audio_settings: audio_settings::AudioSettings,
    // Time selection range (for export selection, loop-to-selection, delete range)
    pub selection_start: Option<u64>,
    pub selection_end: Option<u64>,
    pub selecting: bool,
    /// Which selection edge is being dragged: 0 = none, 1 = left, 2 = right
    pub dragging_selection_edge: u8,
    // Automation editing
    pub show_automation: bool,
    pub automation_param: jamhub_model::AutomationParam,
    // Clip trim state
    pub trimming_clip: Option<ClipTrimState>,
    // Fade handle drag state
    pub dragging_fade: Option<FadeDragState>,
    // Clip gain drag state
    pub dragging_clip_gain: Option<ClipGainDragState>,
    // Live recording waveform — one per armed track: (track_idx, live_buffer_id)
    live_rec_buffer_ids: Vec<(usize, uuid::Uuid)>,
    live_rec_last_update: std::time::Instant,
    // Recording input level (peak amplitude 0.0..1.0, updated during recording)
    pub recording_input_level: f32,
    pub recording_input_peak_db: f32,
    pub show_undo_history: bool,
    // Clipboard
    clipboard_clips: Vec<(jamhub_model::Clip, Option<Vec<f32>>)>,
    // Project dirty flag
    pub dirty: bool,
    // Color picker
    pub color_picker_track: Option<usize>,
    pub midi_panel: midi_panel::MidiPanel,
    pub plugin_windows: plugin_window::PluginWindowManager,
    /// Set of built-in effect slot IDs whose parameter windows are open
    /// Maps slot_id → open_counter (counter ensures unique egui window ID each time)
    pub builtin_fx_open: std::collections::HashMap<Uuid, u32>,
    /// FX chain drag state: source index being dragged
    pub fx_drag_source: Option<usize>,
    // Count-in recording
    pub count_in_enabled: bool,
    pub count_in_beats_remaining: Option<u32>,
    count_in_position: u64,
    // Punch-in/out recording
    pub punch_recording: bool,
    /// Piano roll editing state
    pub piano_roll_state: piano_roll::PianoRollState,
    /// Spectrum analyzer state
    pub spectrum_analyzer: spectrum::SpectrumAnalyzer,
    /// Export dialog state
    pub export_format: ExportFormat,
    pub export_bit_depth: u16,
    pub export_sample_rate: u32,
    pub export_normalize: bool,
    /// Collapsed track group IDs
    pub collapsed_groups: std::collections::HashSet<Uuid>,
    /// Marker drag state: index of marker being dragged
    pub dragging_marker: Option<usize>,
    /// Marker rename state: (index, buffer)
    pub renaming_marker: Option<(usize, String)>,
    // Auto-save
    pub autosave_enabled: bool,
    last_autosave: std::time::Instant,
    // Autosave recovery dialog
    pub show_autosave_recovery: bool,
    pub autosave_recovery_path: Option<PathBuf>,
    // Recent projects
    pub recent_projects: Vec<RecentProject>,
    /// Auto-follow playhead during playback
    pub follow_playhead: bool,
    /// Whether the user is currently manually scrolling (suppresses auto-follow)
    pub user_scrolling: bool,
    /// Vertical zoom for track heights (multiplier)
    pub track_height_zoom: f32,
    /// Whether the minimap overview bar is visible
    pub show_minimap: bool,
    /// Whether the minimap is being dragged
    pub minimap_dragging: bool,
    /// Keyboard shortcuts panel
    pub show_shortcuts: bool,
    pub shortcuts_filter: String,
    /// Tempo tap button state
    pub tap_tempo_times: Vec<std::time::Instant>,
    /// Time signature preset selector
    pub time_sig_popup: bool,
    /// CPU usage estimate (fraction 0.0-1.0)
    pub cpu_usage: f32,
    /// Render timing for CPU estimate
    pub render_time_accum: f64,
    pub render_frame_count: u32,
    /// Clip stretch (Alt+drag right edge) state
    pub stretching_clip: Option<ClipStretchState>,
    /// Custom speed input dialog
    pub speed_input: Option<SpeedInputState>,
    // Template picker
    pub show_template_picker: bool,
    // User preferences
    pub preferences: UserPreferences,
    pub show_preferences: bool,
    // Welcome screen
    pub show_welcome: bool,
    /// Ripple editing mode — moving/deleting clips shifts subsequent clips
    pub ripple_mode: bool,
    /// Playhead position when playback started (for return-to-start-on-stop)
    pub play_start_position: u64,
    /// Project Info panel
    pub show_project_info: bool,
    pub project_info_name_buf: String,
    pub project_info_notes_buf: String,
    /// Audio Pool manager window
    pub show_audio_pool: bool,
    /// Audio Pool preview playback state
    pub audio_pool_preview_id: Option<Uuid>,
    /// Bounce progress indicator (0.0-1.0, None = not bouncing)
    pub bounce_progress: Option<f32>,
    /// Bounce cancellation flag
    pub bounce_cancelled: bool,
    /// Grid display division (independent of snap mode)
    pub grid_division: GridDivision,
    /// Whether Ctrl is currently held (for disabling magnetic snap)
    pub ctrl_held: bool,
    /// Whether the last drag operation magnetically snapped (for visual indicator)
    pub magnetic_snap_active: bool,
    /// The sample position of the most recent magnetic snap (for drawing indicator)
    pub magnetic_snap_sample: u64,
    /// Ruler context menu state: sample position where right-click occurred
    pub ruler_context_sample: Option<u64>,
    /// Tempo change input dialog state
    pub tempo_change_input: Option<TempoChangeInput>,
    /// Swipe comping state: drag across take lanes to select active take regions
    pub swipe_comping: Option<SwipeCompState>,
    /// Session clip launcher state
    pub session_view_state: session_view::SessionViewState,
    /// MIDI learn mode — waiting for CC input to map a parameter
    pub midi_learn_state: Option<midi_mapping::MidiLearnState>,
    /// Show MIDI Mapping Manager window
    pub show_midi_mappings: bool,
    /// Show Macro Controls panel below transport
    pub show_macros: bool,
    /// Locator memory positions (9 slots, Shift+1..9 to save, 1..9 to recall)
    pub locators: [Option<u64>; 9],
    /// Whether the locators panel is visible
    pub show_locators: bool,
    /// Slip editing state: Ctrl+drag to shift audio content within clip boundaries
    pub slip_editing: Option<SlipEditState>,
    /// Region naming dialog state
    pub region_name_input: Option<RegionNameInput>,
    /// Clip properties panel: (track_idx, clip_idx) of the clip being edited
    pub editing_clip: Option<(usize, usize)>,
    /// Inline clip name editing state: (track_idx, clip_idx, text buffer)
    pub renaming_clip: Option<(usize, usize, String)>,
    /// Track template naming dialog state
    pub template_name_input: Option<templates::TemplateNameInput>,
    /// FX preset naming dialog state
    pub fx_preset_name_input: Option<templates::FxPresetNameInput>,
    /// Custom RGB color picker dialog state
    pub custom_color_input: Option<templates::CustomColorInput>,
    /// Whether to show the "Add from Template" picker window
    pub show_track_template_picker: bool,
    /// Track index for which the color palette popup is shown
    pub color_palette_track: Option<usize>,
    /// Drag-and-drop state: info about files being hovered over the window from Finder
    pub finder_drop_state: Option<FinderDropState>,
    /// Whether layout has been loaded from disk on startup
    pub layout_loaded: bool,
    /// Track separator drag state: index of the track whose bottom edge is being dragged
    pub dragging_separator: Option<usize>,
    /// Track header drag-to-reorder: (source track index, current mouse Y, start Y, activated)
    pub dragging_track_reorder: Option<(usize, f32, f32, bool)>,
    /// Timer for periodic layout persistence
    pub last_layout_save: std::time::Instant,
    /// Insert Silence dialog state
    pub insert_silence_input: Option<InsertSilenceInput>,
    // ── Metronome settings ──
    /// Metronome volume (0.0 - 1.0)
    pub metronome_volume: f32,
    /// Accent the first beat of each bar
    pub metronome_accent_first_beat: bool,
    /// Count-in bars (1, 2, or 4)
    pub metronome_count_in_bars: u32,
    /// Whether the metronome settings popup is open
    pub show_metronome_settings: bool,
    /// Ruler hover preview position (sample position where the playhead would go)
    pub ruler_hover_sample: Option<u64>,
    /// Global FX bypass — when true, all effects on all tracks are skipped
    pub global_fx_bypass: bool,
    /// Saved per-slot enabled states before global bypass was engaged
    /// (track_id, slot_id) -> original enabled state
    pub pre_bypass_states: HashMap<(Uuid, Uuid), bool>,
    /// Snap-to-clip-edge: the sample position of the nearest clip edge snap (for visual indicator)
    pub clip_edge_snap_sample: Option<u64>,
    /// AI Stem Separation panel state
    pub stem_sep: stem_separator::StemSeparatorPanel,
    /// Show the Analysis Tools window (Reference Track, Correlation Meter, etc.)
    pub show_analysis: bool,
    /// Reference track for A/B comparison
    pub reference_track: Option<analysis_tools::ReferenceTrack>,
    /// A/B mode: when true, the engine plays the reference track instead of the project mix
    pub ab_mode: bool,
    /// Loudness matching: auto-compensate volume when toggling effects
    pub loudness_match_enabled: bool,
    /// Current loudness compensation amount in dB
    pub loudness_compensation_db: f32,
    /// Pending loudness match measurement state
    pub loudness_match_state: Option<analysis_tools::LoudnessMatchState>,
    /// Chord detection running flag
    pub chord_detection_running: bool,
    /// Detected chords per clip ID (for overlay rendering on timeline)
    pub detected_chords: HashMap<Uuid, Vec<analysis_tools::DetectedChord>>,
    /// Version control (branching) panel state
    pub version_panel: version_control::VersionControlPanel,
    /// Waveform vertical zoom — amplify waveform display without changing audio (1.0 = normal)
    pub waveform_zoom: f32,
    /// Beat flash timer — counts down from 1.0 to 0.0 on each beat for visual indicator
    pub beat_flash: f32,
    /// Last beat number (to detect beat changes)
    pub last_beat_num: u64,
}

/// State for slip-editing: shifting audio content within clip boundaries.
pub struct SlipEditState {
    pub track_idx: usize,
    pub clip_idx: usize,
    pub start_x: f32,
    pub original_content_offset: u64,
}

/// State for the region naming dialog.
pub struct RegionNameInput {
    pub name: String,
    pub start: u64,
    pub end: u64,
}

/// State for an ongoing swipe-comp drag gesture.
pub struct SwipeCompState {
    pub track_idx: usize,
    /// The lane (take) index being swiped on
    pub lane: usize,
    /// Sample position where the swipe started
    pub start_sample: u64,
    /// Current sample position of the drag
    pub current_sample: u64,
}

/// State for the tempo change input dialog.
pub struct TempoChangeInput {
    pub sample: u64,
    pub bpm_text: String,
}

pub struct ClipTrimState {
    pub track_idx: usize,
    pub clip_idx: usize,
    pub edge: TrimEdge,
    pub original_start: u64,
    pub original_duration: u64,
}

#[derive(PartialEq)]
pub enum TrimEdge {
    Left,
    Right,
}

pub struct ClipDragState {
    pub track_idx: usize,
    pub clip_idx: usize,
    pub start_x: f32,
    pub original_start_sample: u64,
}

/// State for dragging multiple selected clips at once
pub struct MultiClipDragState {
    pub start_x: f32,
    /// Original positions of all dragged clips: (track_idx, clip_idx, original_start_sample)
    pub originals: Vec<(usize, usize, u64)>,
}

/// State for dragging fade handles on clips
pub struct FadeDragState {
    pub track_idx: usize,
    pub clip_idx: usize,
    pub fade_edge: FadeEdge,
    pub original_fade_samples: u64,
}

/// State for dragging clip gain handle
pub struct ClipGainDragState {
    pub track_idx: usize,
    pub clip_idx: usize,
    pub start_y: f32,
    pub original_gain_db: f32,
}

#[derive(PartialEq)]
pub enum FadeEdge {
    FadeIn,
    FadeOut,
}

/// State for Alt+drag stretch handle on clip right edge
pub struct ClipStretchState {
    pub track_idx: usize,
    pub clip_idx: usize,
    pub original_duration: u64,
    pub original_rate: f32,
}

/// State for custom speed input dialog
pub struct SpeedInputState {
    pub track_idx: usize,
    pub clip_idx: usize,
    pub input_buf: String,
}

/// State for the Insert Silence dialog.
pub struct InsertSilenceInput {
    pub input_buf: String,
    /// If true, interpret input as bars; if false, interpret as seconds.
    pub use_bars: bool,
}

impl Default for InsertSilenceInput {
    fn default() -> Self {
        Self {
            input_buf: "1".into(),
            use_bars: true,
        }
    }
}

/// State for files being dragged over the window from Finder
pub struct FinderDropState {
    /// Pointer position of the drag
    pub pos: egui::Pos2,
    /// File names being dragged
    pub file_names: Vec<String>,
    /// File paths being dragged
    pub file_paths: Vec<PathBuf>,
}

#[derive(PartialEq)]
pub enum View {
    Arrange,
    Mixer,
    Session,
}

#[derive(PartialEq, Clone, Copy)]
pub enum SnapMode {
    Off,            // Free positioning, sample-accurate
    Beat,           // Snap to beats
    Bar,            // Snap to bars
    HalfBeat,       // Snap to half beats (8th notes in 4/4)
    Triplet,        // Snap to triplet grid (1/3 of a beat)
    Sixteenth,      // Snap to 1/16 note
    ThirtySecond,   // Snap to 1/32 note
    Marker,         // Snap to nearest marker position
}

impl SnapMode {
    pub fn label(&self) -> &str {
        match self {
            SnapMode::Off => "Free",
            SnapMode::Beat => "Beat",
            SnapMode::Bar => "Bar",
            SnapMode::HalfBeat => "1/2 Beat",
            SnapMode::Triplet => "Triplet",
            SnapMode::Sixteenth => "1/16",
            SnapMode::ThirtySecond => "1/32",
            SnapMode::Marker => "Marker",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            SnapMode::Off => SnapMode::HalfBeat,
            SnapMode::HalfBeat => SnapMode::Triplet,
            SnapMode::Triplet => SnapMode::Beat,
            SnapMode::Beat => SnapMode::Sixteenth,
            SnapMode::Sixteenth => SnapMode::ThirtySecond,
            SnapMode::ThirtySecond => SnapMode::Bar,
            SnapMode::Bar => SnapMode::Marker,
            SnapMode::Marker => SnapMode::Off,
        }
    }

    /// All available modes for UI display.
    pub fn all() -> &'static [SnapMode] {
        &[
            SnapMode::Off,
            SnapMode::HalfBeat,
            SnapMode::Triplet,
            SnapMode::Beat,
            SnapMode::Sixteenth,
            SnapMode::ThirtySecond,
            SnapMode::Bar,
            SnapMode::Marker,
        ]
    }
}

/// Grid division type — controls which lines are drawn on the timeline,
/// independent of the snap mode.
#[derive(PartialEq, Clone, Copy)]
pub enum GridDivision {
    None,
    Bar,           // 1/1 — bar lines only
    Half,          // 1/2
    Beat,          // 1/4 (beats)
    Eighth,        // 1/8
    Sixteenth,     // 1/16
    ThirtySecond,  // 1/32
    Triplet,       // Triplet (1/3 beat)
}

impl GridDivision {
    pub fn label(&self) -> &str {
        match self {
            GridDivision::None => "None",
            GridDivision::Bar => "1/1 (Bar)",
            GridDivision::Half => "1/2",
            GridDivision::Beat => "1/4 (Beat)",
            GridDivision::Eighth => "1/8",
            GridDivision::Sixteenth => "1/16",
            GridDivision::ThirtySecond => "1/32",
            GridDivision::Triplet => "Triplet",
        }
    }

    /// Returns subdivisions per beat for this grid division.
    /// Returns 0 for None, and for Bar returns a negative sentinel handled separately.
    pub fn subdivisions_per_beat(&self) -> f64 {
        match self {
            GridDivision::None => 0.0,
            GridDivision::Bar => -1.0, // sentinel: one line per bar
            GridDivision::Half => 0.5,
            GridDivision::Beat => 1.0,
            GridDivision::Eighth => 2.0,
            GridDivision::Sixteenth => 4.0,
            GridDivision::ThirtySecond => 8.0,
            GridDivision::Triplet => 3.0,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct RecentProject {
    pub path: PathBuf,
    pub last_opened: u64, // unix timestamp
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("jamhub")
}

fn autosave_dir() -> PathBuf {
    config_dir().join("autosave")
}

fn recent_projects_path() -> PathBuf {
    config_dir().join("recent.json")
}

fn load_recent_projects() -> Vec<RecentProject> {
    let path = recent_projects_path();
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(mut list) = serde_json::from_str::<Vec<RecentProject>>(&data) {
            // Remove entries for files that no longer exist, keep max 10
            list.retain(|r| r.path.exists());
            list.sort_by(|a, b| b.last_opened.cmp(&a.last_opened));
            list.truncate(10);
            return list;
        }
    }
    Vec::new()
}

fn save_recent_projects(list: &[RecentProject]) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(list) {
        let _ = fs::write(recent_projects_path(), json);
    }
}

fn add_to_recent_projects(recent: &mut Vec<RecentProject>, path: &PathBuf) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Remove existing entry for same path
    recent.retain(|r| r.path != *path);
    // Add at front
    recent.insert(0, RecentProject {
        path: path.clone(),
        last_opened: now,
    });
    recent.truncate(10);
    save_recent_projects(recent);
}

// ── Project Templates ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProjectTemplate {
    Empty,
    SingerSongwriter,
    Band,
    Electronic,
    Podcast,
}

impl ProjectTemplate {
    pub const ALL: [ProjectTemplate; 5] = [
        ProjectTemplate::Empty,
        ProjectTemplate::SingerSongwriter,
        ProjectTemplate::Band,
        ProjectTemplate::Electronic,
        ProjectTemplate::Podcast,
    ];

    pub fn label(&self) -> &str {
        match self {
            ProjectTemplate::Empty => "Empty",
            ProjectTemplate::SingerSongwriter => "Singer/Songwriter",
            ProjectTemplate::Band => "Band",
            ProjectTemplate::Electronic => "Electronic",
            ProjectTemplate::Podcast => "Podcast",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            ProjectTemplate::Empty => "Blank project with 2 audio tracks",
            ProjectTemplate::SingerSongwriter => "2 audio tracks + 1 bus (vocals & guitar)",
            ProjectTemplate::Band => "Drums, bass, guitar, vocal + mix bus",
            ProjectTemplate::Electronic => "4 MIDI + 2 audio + master bus",
            ProjectTemplate::Podcast => "2 audio tracks with compressor on each",
        }
    }

    pub fn apply(&self, project: &mut Project) {
        use jamhub_model::{TrackKind, EffectSlot, TrackEffect};
        project.tracks.clear();
        match self {
            ProjectTemplate::Empty => {
                project.add_track("Track 1", TrackKind::Audio);
            }
            ProjectTemplate::SingerSongwriter => {
                project.add_track("Vocals", TrackKind::Audio);
                project.add_track("Guitar", TrackKind::Audio);
                project.add_track("Mix Bus", TrackKind::Audio);
            }
            ProjectTemplate::Band => {
                project.add_track("Drums", TrackKind::Audio);
                project.add_track("Bass", TrackKind::Audio);
                project.add_track("Guitar", TrackKind::Audio);
                project.add_track("Vocal", TrackKind::Audio);
                project.add_track("Mix Bus", TrackKind::Audio);
            }
            ProjectTemplate::Electronic => {
                project.add_track("Synth 1", TrackKind::Midi);
                project.add_track("Synth 2", TrackKind::Midi);
                project.add_track("Drums", TrackKind::Midi);
                project.add_track("Bass", TrackKind::Midi);
                project.add_track("Audio 1", TrackKind::Audio);
                project.add_track("Audio 2", TrackKind::Audio);
                project.add_track("Master Bus", TrackKind::Audio);
            }
            ProjectTemplate::Podcast => {
                project.add_track("Host", TrackKind::Audio);
                project.add_track("Guest", TrackKind::Audio);
                // Add compressor to each track
                for track in &mut project.tracks {
                    track.effects.push(EffectSlot::new(TrackEffect::Compressor {
                        threshold_db: -18.0,
                        ratio: 3.0,
                        attack_ms: 10.0,
                        release_ms: 100.0,
                    }));
                }
            }
        }
    }
}

// ── User Preferences ──────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserPreferences {
    pub audio_buffer_size: u32,
    pub default_template: ProjectTemplate,
    pub autosave_interval_secs: u64, // 0 = disabled
    pub ui_scale: f32,
    pub theme: ThemeChoice,
    pub dont_show_welcome: bool,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            audio_buffer_size: 512,
            default_template: ProjectTemplate::Empty,
            autosave_interval_secs: 120,
            ui_scale: 1.0,
            theme: ThemeChoice::Dark,
            dont_show_welcome: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeChoice {
    Dark,
    Darker,
    Midnight,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::Dark, ThemeChoice::Darker, ThemeChoice::Midnight];

    pub fn label(&self) -> &str {
        match self {
            ThemeChoice::Dark => "Dark",
            ThemeChoice::Darker => "Darker",
            ThemeChoice::Midnight => "Midnight",
        }
    }
}

fn preferences_path() -> PathBuf {
    config_dir().join("preferences.json")
}

fn load_preferences() -> UserPreferences {
    let path = preferences_path();
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(prefs) = serde_json::from_str::<UserPreferences>(&data) {
            return prefs;
        }
    }
    UserPreferences::default()
}

fn save_preferences(prefs: &UserPreferences) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(prefs) {
        let _ = fs::write(preferences_path(), json);
    }
}

/// Persisted panel layout state — saved to ~/.config/jamhub/layout.json
#[derive(serde::Serialize, serde::Deserialize)]
struct LayoutState {
    show_effects: bool,
    show_piano_roll: bool,
    show_automation: bool,
    show_minimap: bool,
    show_media_browser: bool,
    show_spectrum: bool,
    /// 0 = Arrange, 1 = Mixer, 2 = Session
    view_mode: u8,
    zoom: f32,
    scroll_x: f32,
}

fn layout_path() -> PathBuf {
    config_dir().join("layout.json")
}

fn load_layout() -> Option<LayoutState> {
    let path = layout_path();
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_layout(app: &DawApp) {
    let state = LayoutState {
        show_effects: app.show_effects,
        show_piano_roll: app.show_piano_roll,
        show_automation: app.show_automation,
        show_minimap: app.show_minimap,
        show_media_browser: app.media_browser.show,
        show_spectrum: app.spectrum_analyzer.show,
        view_mode: match app.view {
            View::Arrange => 0,
            View::Mixer => 1,
            View::Session => 2,
        },
        zoom: app.zoom,
        scroll_x: app.scroll_x,
    };
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let _ = fs::write(layout_path(), json);
    }
}

/// Find autosave files to offer recovery on startup.
fn find_autosave_recovery() -> Option<PathBuf> {
    let dir = autosave_dir();
    if dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&dir) {
            // Look for project.json inside autosave subdirectories
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && p.join("project.json").exists() {
                    return Some(p);
                }
            }
        }
    }
    None
}

impl DawApp {
    fn new() -> Self {
        let engine = match EngineHandle::spawn() {
            Ok(h) => Some(h),
            Err(e) => {
                eprintln!("Engine init error: {e}");
                None
            }
        };

        let mut project = Project::default();
        project.created_at = chrono::Local::now().to_rfc3339();
        project.add_track("Track 1", TrackKind::Audio);

        let sample_rate = engine.as_ref()
            .map(|e| e.state.read().sample_rate)
            .unwrap_or(44100);

        if let Some(ref eng) = engine {
            eng.send(EngineCommand::UpdateProject(project.clone()));
        }

        let mut app = Self {
            project,
            engine_error: if engine.is_none() {
                Some("Failed to initialize audio engine".into())
            } else {
                None
            },
            engine,
            view: View::Arrange,
            show_mixer_panel: false,
            zoom: 1.0,
            scroll_x: 0.0,
            recorder: Recorder::new(),
            is_recording: false,
            recording_start_pos: 0,
            status_message: None,
            selected_track: Some(0),
            selected_clips: HashSet::new(),
            rubber_band_origin: None,
            rubber_band_active: false,
            waveform_cache: WaveformCache::new(),
            undo_manager: UndoManager::new(),
            audio_buffers: HashMap::new(),
            project_path: None,
            session: SessionPanel::default(),
            jam: jam_session::JamSessionPanel::default(),
            platform: platform_panel::PlatformPanel::default(),
            metronome_enabled: false,
            snap_mode: SnapMode::Off,
            dragging_clip: None,
            dragging_clips: None,
            show_effects: false,
            loop_enabled: false,
            loop_start: 0,
            loop_end: 0,
            master_volume: 1.0,
            renaming_track: None,
            show_piano_roll: false,
            piano_roll_consumed_delete: false,
            confirm_delete_track: None,
            show_about: false,
            input_monitor: InputMonitor::new(),
            resizing_track: None,
            fx_browser: {
                let mut fb = fx_browser::FxBrowser::default();
                fb.scan_and_load_all(sample_rate);
                fb
            },
            media_browser: media_browser::MediaBrowser::default(),
            audio_settings: audio_settings::AudioSettings::default(),
            selection_start: None,
            selection_end: None,
            selecting: false,
            dragging_selection_edge: 0,
            show_automation: false,
            automation_param: jamhub_model::AutomationParam::Volume,
            trimming_clip: None,
            dragging_fade: None,
            dragging_clip_gain: None,
            live_rec_buffer_ids: Vec::new(),
            live_rec_last_update: std::time::Instant::now(),
            recording_input_level: 0.0,
            recording_input_peak_db: -60.0,
            show_undo_history: false,
            clipboard_clips: Vec::new(),
            dirty: false,
            color_picker_track: None,
            midi_panel: midi_panel::MidiPanel::default(),
            plugin_windows: plugin_window::PluginWindowManager::default(),
            builtin_fx_open: std::collections::HashMap::new(),
            fx_drag_source: None,
            count_in_enabled: false,
            count_in_beats_remaining: None,
            count_in_position: 0,
            punch_recording: false,
            piano_roll_state: piano_roll::PianoRollState::default(),
            spectrum_analyzer: spectrum::SpectrumAnalyzer::new(),
            export_format: ExportFormat::Wav,
            export_bit_depth: 32,
            export_sample_rate: 0,
            export_normalize: false,
            collapsed_groups: std::collections::HashSet::new(),
            dragging_marker: None,
            renaming_marker: None,
            autosave_enabled: true,
            last_autosave: std::time::Instant::now(),
            show_autosave_recovery: find_autosave_recovery().is_some(),
            autosave_recovery_path: find_autosave_recovery(),
            recent_projects: load_recent_projects(),
            follow_playhead: true,
            user_scrolling: false,
            track_height_zoom: 1.0,
            show_minimap: true,
            minimap_dragging: false,
            show_shortcuts: false,
            shortcuts_filter: String::new(),
            tap_tempo_times: Vec::new(),
            time_sig_popup: false,
            cpu_usage: 0.0,
            render_time_accum: 0.0,
            render_frame_count: 0,
            stretching_clip: None,
            speed_input: None,
            show_template_picker: false,
            preferences: {
                let p = load_preferences();
                p
            },
            show_preferences: false,
            show_welcome: {
                let prefs = load_preferences();
                let recent = load_recent_projects();
                !prefs.dont_show_welcome && recent.is_empty()
            },
            ripple_mode: false,
            play_start_position: 0,
            show_project_info: false,
            project_info_name_buf: String::new(),
            project_info_notes_buf: String::new(),
            show_audio_pool: false,
            audio_pool_preview_id: None,
            bounce_progress: None,
            bounce_cancelled: false,
            grid_division: GridDivision::Beat,
            ctrl_held: false,
            magnetic_snap_active: false,
            magnetic_snap_sample: 0,
            ruler_context_sample: None,
            tempo_change_input: None,
            swipe_comping: None,
            session_view_state: session_view::SessionViewState::default(),
            midi_learn_state: None,
            show_midi_mappings: false,
            show_macros: true,
            locators: [None; 9],
            show_locators: false,
            slip_editing: None,
            region_name_input: None,
            editing_clip: None,
            renaming_clip: None,
            template_name_input: None,
            fx_preset_name_input: None,
            custom_color_input: None,
            show_track_template_picker: false,
            color_palette_track: None,
            finder_drop_state: None,
            layout_loaded: false,
            dragging_separator: None,
            dragging_track_reorder: None,
            last_layout_save: std::time::Instant::now(),
            insert_silence_input: None,
            metronome_volume: 0.8,
            metronome_accent_first_beat: true,
            metronome_count_in_bars: 1,
            show_metronome_settings: false,
            ruler_hover_sample: None,
            global_fx_bypass: false,
            pre_bypass_states: HashMap::new(),
            clip_edge_snap_sample: None,
            stem_sep: stem_separator::StemSeparatorPanel::default(),
            show_analysis: false,
            reference_track: None,
            ab_mode: false,
            loudness_match_enabled: false,
            loudness_compensation_db: 0.0,
            loudness_match_state: None,
            chord_detection_running: false,
            detected_chords: HashMap::new(),
            version_panel: version_control::VersionControlPanel::default(),
            waveform_zoom: 1.0,
            beat_flash: 0.0,
            last_beat_num: 0,
        };

        // Apply persisted layout
        if let Some(layout) = load_layout() {
            app.show_effects = layout.show_effects;
            app.show_piano_roll = layout.show_piano_roll;
            app.show_automation = layout.show_automation;
            app.show_minimap = layout.show_minimap;
            app.media_browser.show = layout.show_media_browser;
            app.spectrum_analyzer.show = layout.show_spectrum;
            app.view = match layout.view_mode {
                1 => View::Mixer,
                2 => View::Session,
                _ => View::Arrange,
            };
            app.zoom = layout.zoom.clamp(0.1, 20.0);
            app.scroll_x = layout.scroll_x.max(0.0);
            app.layout_loaded = true;
        }

        app
    }

    pub fn transport_state(&self) -> TransportState {
        self.engine
            .as_ref()
            .map(|e| e.state.read().transport)
            .unwrap_or(TransportState::Stopped)
    }

    pub fn position_samples(&self) -> u64 {
        self.engine
            .as_ref()
            .map(|e| e.state.read().position_samples)
            .unwrap_or(0)
    }

    pub fn sample_rate(&self) -> u32 {
        self.engine
            .as_ref()
            .map(|e| e.state.read().sample_rate)
            .unwrap_or(44100)
    }


    pub fn pdc_info(&self) -> Option<&jamhub_engine::PdcInfo> {
        self.engine.as_ref().map(|e| &e.pdc_info)
    }
    pub fn levels(&self) -> Option<&LevelMeters> {
        self.engine.as_ref().map(|e| &e.levels)
    }

    pub fn lufs(&self) -> Option<&jamhub_engine::LufsMeter> {
        self.engine.as_ref().map(|e| &e.lufs)
    }

    pub fn send_command(&self, cmd: EngineCommand) {
        if let Some(ref engine) = self.engine {
            engine.send(cmd);
        }
    }

    pub fn sync_project(&self) {
        self.send_command(EngineCommand::UpdateProject(self.project.clone()));
    }

    pub fn set_status(&mut self, msg: &str) {
        self.status_message = Some((msg.to_string(), std::time::Instant::now()));
    }

    /// Detect if a VST3 plugin is a nih-plug plugin (uses egui for UI).
    /// These conflict with our egui event loop when we try to embed their editor.
    /// Detect if a VST3 plugin uses nih-plug (any UI backend).
    /// nih-plug's window management conflicts with our egui event loop
    /// regardless of whether the plugin uses egui or vizia.
    pub fn is_nihplug_egui_plugin(path: &std::path::Path) -> bool {
        // Check plist for nih-plug bundle identifier
        let plist_path = path.join("Contents").join("Info.plist");
        if let Ok(content) = std::fs::read_to_string(&plist_path) {
            if content.contains("nih-plug") || content.contains("nih_plug") {
                return true;
            }
        }
        // Check binary for nih_plug symbols
        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
            let binary = path.join("Contents").join("MacOS").join(name);
            if let Ok(data) = std::fs::read(&binary) {
                let needle = b"nih_plug";
                if data.windows(needle.len()).any(|w| w == needle) {
                    return true;
                }
            }
        }
        false
    }

    pub fn push_undo(&mut self, label: &str) {
        self.undo_manager.push(label, &self.project);
        self.dirty = true;
    }

    /// Push an undo entry using a pre-captured project snapshot.
    /// Used when the caller has already mutated self.project and needs to
    /// record the state from *before* the mutation.
    pub fn undo_manager_push_with_snapshot(&mut self, label: &str, snapshot: Project) {
        self.undo_manager.push(label, &snapshot);
        self.dirty = true;
    }

    pub fn undo(&mut self) {
        if let Some(project) = self.undo_manager.undo(&self.project) {
            self.project = project;
            self.sync_project();
            self.set_status("Undo");
        }
    }

    pub fn redo(&mut self) {
        if let Some(project) = self.undo_manager.redo(&self.project) {
            self.project = project;
            self.sync_project();
            self.set_status("Redo");
        }
    }

    pub fn import_audio_file(&mut self, path: PathBuf) {
        let track_idx = self.selected_track.unwrap_or(0);
        if track_idx >= self.project.tracks.len() {
            self.set_status("No track selected");
            return;
        }

        match load_audio(&path) {
            Ok(data) => {
                self.push_undo("Import audio");

                let buffer_id = Uuid::new_v4();
                let position = self.position_samples();
                let file_name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Audio".to_string());

                let clip = Clip {
                    id: Uuid::new_v4(),
                    name: file_name.clone(),
                    start_sample: position,
                    duration_samples: data.duration_samples,
                    source: ClipSource::AudioBuffer { buffer_id },
                    muted: false, content_offset: 0, transpose_semitones: 0, reversed: false,
                    fade_in_samples: 0,
                    fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                    color: None,
                    playback_rate: 1.0,
                    preserve_pitch: false,
                    loop_count: 1,
                    gain_db: 0.0,
                    take_index: 0,
                };

                self.waveform_cache.insert(buffer_id, &data.samples);
                self.audio_buffers.insert(buffer_id, data.samples.clone());

                self.project.tracks[track_idx].clips.push(clip);

                self.send_command(EngineCommand::LoadAudioBuffer {
                    id: buffer_id,
                    samples: data.samples,
                });
                self.sync_project();
                self.set_status(&format!("Imported: {file_name}"));
            }
            Err(e) => {
                self.set_status(&format!("Import failed: {e}"));
            }
        }
    }

    /// Import an audio file at a specific track and sample position.
    /// If `track_idx` is None or out of range, a new track is created.
    pub fn import_audio_file_at(&mut self, path: PathBuf, track_idx: Option<usize>, start_sample: u64) {
        let target_track = match track_idx {
            Some(idx) if idx < self.project.tracks.len() => idx,
            _ => {
                // Create a new track
                let n = self.project.tracks.len() + 1;
                self.project.add_track(&format!("Track {n}"), TrackKind::Audio);
                self.project.tracks.len() - 1
            }
        };

        match load_audio(&path) {
            Ok(data) => {
                self.push_undo("Import audio (drop)");

                let buffer_id = Uuid::new_v4();
                let file_name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Audio".to_string());

                let clip = Clip {
                    id: Uuid::new_v4(),
                    name: file_name.clone(),
                    start_sample,
                    duration_samples: data.duration_samples,
                    source: ClipSource::AudioBuffer { buffer_id },
                    muted: false, content_offset: 0, transpose_semitones: 0, reversed: false,
                    fade_in_samples: 0,
                    fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                    color: None,
                    playback_rate: 1.0,
                    preserve_pitch: false,
                    loop_count: 1,
                    gain_db: 0.0,
                    take_index: 0,
                };

                self.waveform_cache.insert(buffer_id, &data.samples);
                self.audio_buffers.insert(buffer_id, data.samples.clone());

                self.project.tracks[target_track].clips.push(clip);

                self.send_command(EngineCommand::LoadAudioBuffer {
                    id: buffer_id,
                    samples: data.samples,
                });
                self.selected_track = Some(target_track);
                self.sync_project();
                self.set_status(&format!("Imported: {file_name}"));
            }
            Err(e) => {
                self.set_status(&format!("Import failed: {e}"));
            }
        }
    }

    pub fn toggle_recording(&mut self) {
        if self.is_recording {
            // === STOP RECORDING ===
            self.is_recording = false;
            self.recording_input_level = 0.0;
            self.recording_input_peak_db = -60.0;

            // Collect armed track indices before mutating
            let armed_tracks: Vec<usize> = self.project.tracks.iter().enumerate()
                .filter(|(_, t)| t.armed)
                .map(|(i, _)| i)
                .collect();

            // If we were in count-in phase, just cancel without saving
            if self.count_in_beats_remaining.is_some() {
                self.count_in_beats_remaining = None;
                self.send_command(EngineCommand::Stop);
                self.send_command(EngineCommand::SetMetronome(self.metronome_enabled));
                for &ti in &armed_tracks {
                    if ti < self.project.tracks.len() {
                        self.project.tracks[ti].muted = false;
                        self.project.tracks[ti].armed = false;
                    }
                }
                self.live_rec_buffer_ids.clear();
                self.sync_project();
                self.set_status("Count-in cancelled");
                return;
            }

            // 1. Stop the recorder FIRST to get captured audio
            let result = self.recorder.stop();

            // 2. Stop the engine AFTER getting recording data
            self.send_command(EngineCommand::Stop);

            // 3. Remove the live recording placeholder clips from ALL armed tracks
            let live_ids: Vec<(usize, uuid::Uuid)> = std::mem::take(&mut self.live_rec_buffer_ids);
            for (ti, live_id) in &live_ids {
                if *ti < self.project.tracks.len() {
                    self.project.tracks[*ti]
                        .clips
                        .retain(|c| {
                            if let ClipSource::AudioBuffer { buffer_id } = &c.source {
                                *buffer_id != *live_id
                            } else {
                                true
                            }
                        });
                }
            }

            // Unmute and disarm all armed tracks
            for &ti in &armed_tracks {
                if ti < self.project.tracks.len() {
                    self.project.tracks[ti].muted = false;
                    self.project.tracks[ti].armed = false;
                }
            }

            if result.samples.is_empty() {
                self.sync_project();
                self.set_status("Recording was empty");
                return;
            }

            if armed_tracks.is_empty() {
                self.sync_project();
                return;
            }

            self.push_undo("Record audio");

            // 3. Resample to engine sample rate if needed
            let engine_sr = self.sample_rate();
            let samples = if result.sample_rate != engine_sr {
                jamhub_engine::resample(&result.samples, result.sample_rate, engine_sr)
            } else {
                result.samples
            };

            // 4. For punch recording, trim audio to only the punch region
            let samples = if self.punch_recording {
                if let (Some(sel_s), Some(sel_e)) = (self.selection_start, self.selection_end) {
                    let punch_start = sel_s.min(sel_e);
                    let punch_end = sel_s.max(sel_e);
                    let punch_len = (punch_end - punch_start) as usize;
                    if samples.len() > punch_len {
                        samples[..punch_len].to_vec()
                    } else {
                        samples
                    }
                } else {
                    samples
                }
            } else {
                samples
            };

            // Duration is the buffer length — this is the actual audio data
            let rec_start = self.recording_start_pos;
            let duration = samples.len() as u64;

            // Load the shared audio buffer into the engine once
            let shared_buffer_id = Uuid::new_v4();

            // Build waveform for display (shared across all clips)
            self.waveform_cache.insert(shared_buffer_id, &samples);
            self.audio_buffers.insert(shared_buffer_id, samples.clone());

            // Load buffer into engine FIRST
            self.send_command(EngineCommand::LoadAudioBuffer {
                id: shared_buffer_id,
                samples,
            });

            // Create a clip on EACH armed track with the same shared buffer
            for &track_idx in &armed_tracks {
                if track_idx >= self.project.tracks.len() {
                    continue;
                }

                // Auto-mute older overlapping clips (takes behavior)
                for existing_clip in &mut self.project.tracks[track_idx].clips {
                    let existing_end = existing_clip.start_sample + existing_clip.duration_samples;
                    let new_end = rec_start + duration;
                    if rec_start < existing_end && new_end > existing_clip.start_sample {
                        existing_clip.muted = true;
                    }
                }

                // Count overlapping takes for naming
                let take_num = self.project.tracks[track_idx]
                    .clips
                    .iter()
                    .filter(|c| {
                        let c_end = c.start_sample + c.duration_samples;
                        rec_start < c_end && (rec_start + duration) > c.start_sample
                    })
                    .count()
                    + 1;

                let clip = Clip {
                    id: Uuid::new_v4(),
                    name: format!("Take {}", take_num),
                    start_sample: rec_start,
                    duration_samples: duration,
                    source: ClipSource::AudioBuffer { buffer_id: shared_buffer_id },
                    muted: false, content_offset: 0, transpose_semitones: 0, reversed: false,
                    fade_in_samples: 0,
                    fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                    color: None,
                    playback_rate: 1.0,
                    preserve_pitch: false,
                    loop_count: 1,
                    gain_db: 0.0,
                    take_index: take_num as u32 - 1,
                };

                // Auto-expand take lanes when recording creates overlapping takes
                if take_num > 1 {
                    self.project.tracks[track_idx].lanes_expanded = true;
                }

                self.project.tracks[track_idx].clips.push(clip);
            }

            self.sync_project();

            // Scroll view to show the recorded clip
            self.scroll_x = 0.0;
            let min_zoom = 0.3;
            if self.zoom < min_zoom {
                self.zoom = min_zoom;
            }

            // Rewind playhead to start of clip for immediate playback
            self.send_command(EngineCommand::SetPosition(rec_start));

            let track_label = if armed_tracks.len() > 1 {
                format!("{} tracks", armed_tracks.len())
            } else {
                "1 track".to_string()
            };
            self.set_status(&format!(
                "Recorded on {} ({:.1}s) — press Space to play",
                track_label,
                duration as f64 / engine_sr as f64
            ));
        } else {
            // === START RECORDING ===
            if self.project.tracks.is_empty() {
                self.set_status("Cannot record: no tracks in project");
                return;
            }

            // Arm the selected track if no tracks are armed yet
            let track_idx = self.selected_track.unwrap_or(0);
            let any_armed = self.project.tracks.iter().any(|t| t.armed);
            if !any_armed && track_idx < self.project.tracks.len() {
                self.project.tracks[track_idx].armed = true;
            }

            // Mute all armed tracks while recording so old takes don't
            // play back through speakers (prevents feedback/confusion)
            for t in &mut self.project.tracks {
                if t.armed {
                    t.muted = true;
                }
            }
            self.sync_project();

            // Pre-roll: if punch recording with a selection, start 1 bar before selection
            let mut start_pos = self.position_samples();
            if self.punch_recording {
                if let (Some(sel_s), Some(sel_e)) = (self.selection_start, self.selection_end) {
                    let punch_start = sel_s.min(sel_e);
                    let sr = self.sample_rate() as f64;
                    let beats_per_bar = self.project.time_signature.numerator as u64;
                    let samples_per_bar = self.project.tempo.samples_per_beat(sr) as u64
                        * beats_per_bar;
                    start_pos = punch_start.saturating_sub(samples_per_bar);
                    self.send_command(EngineCommand::SetPosition(start_pos));
                }
            }

            // Store the current playhead position BEFORE starting
            self.recording_start_pos = start_pos;

            // Count-in: play metronome beats before actual recording begins
            if self.count_in_enabled {
                let beats = self.project.time_signature.numerator as u32;
                self.count_in_beats_remaining = Some(beats);
                self.count_in_position = 0;
                // Enable metronome for count-in
                self.send_command(EngineCommand::SetMetronome(true));
                // Play from position 0 so metronome beats align cleanly
                self.send_command(EngineCommand::SetPosition(0));
                self.send_command(EngineCommand::Play);
                self.is_recording = true; // mark so UI shows recording state
                self.set_status(&format!("Count-in: {}...", beats));
                return; // actual recording starts after count-in finishes in update()
            }

            self.start_actual_recording(track_idx);
        }
    }

    /// Called after count-in finishes, or immediately if count-in is disabled.
    fn start_actual_recording(&mut self, track_idx: usize) {
        // If punch recording, set position to pre-roll start
        if self.punch_recording {
            if let (Some(sel_s), Some(sel_e)) = (self.selection_start, self.selection_end) {
                let punch_start = sel_s.min(sel_e);
                let sr = self.sample_rate() as f64;
                let beats_per_bar = self.project.time_signature.numerator as u64;
                let samples_per_bar = self.project.tempo.samples_per_beat(sr) as u64
                    * beats_per_bar;
                let pre_roll_pos = punch_start.saturating_sub(samples_per_bar);
                self.recording_start_pos = punch_start; // actual recording starts at punch point
                self.send_command(EngineCommand::SetPosition(pre_roll_pos));
            }
        }

        match self.recorder.start() {
            Ok(()) => {
                self.is_recording = true;
                self.send_command(EngineCommand::Play);

                // Restore metronome to user preference
                self.send_command(EngineCommand::SetMetronome(self.metronome_enabled));

                // Create a live placeholder clip for waveform display
                let live_id = Uuid::new_v4();
                self.live_rec_buffer_ids = vec![(track_idx, live_id)];
                let rec_start = if self.punch_recording {
                    if let (Some(sel_s), Some(sel_e)) = (self.selection_start, self.selection_end) {
                        sel_s.min(sel_e)
                    } else {
                        self.recording_start_pos
                    }
                } else {
                    self.recording_start_pos
                };
                let live_clip = Clip {
                    id: Uuid::new_v4(),
                    name: "Recording...".into(),
                    start_sample: rec_start,
                    duration_samples: 1, // will grow
                    source: ClipSource::AudioBuffer { buffer_id: live_id },
                    muted: false, content_offset: 0, transpose_semitones: 0, reversed: false,
                    fade_in_samples: 0,
                    fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                    color: None,
                    playback_rate: 1.0,
                    preserve_pitch: false,
                    loop_count: 1,
                    gain_db: 0.0,
                    take_index: 0,
                };
                if track_idx < self.project.tracks.len() {
                    self.project.tracks[track_idx].clips.push(live_clip);
                }

                if self.punch_recording {
                    self.set_status("Punch recording...");
                } else {
                    self.set_status("Recording...");
                }
            }
            Err(e) => {
                // Undo mute on failure
                if track_idx < self.project.tracks.len() {
                    self.project.tracks[track_idx].muted = false;
                    self.project.tracks[track_idx].armed = false;
                    self.sync_project();
                }
                let err_str = format!("{e}");
                let msg = if err_str.contains("input") || err_str.contains("device") || err_str.contains("stream") {
                    format!("Cannot record: no input device available ({e})")
                } else {
                    format!("Cannot record: {e}")
                };
                self.set_status(&msg);
            }
        }
    }

    pub fn delete_selected_clips(&mut self) {
        if self.selected_clips.is_empty() {
            return;
        }
        self.push_undo("Delete clips");
        // Group by track, sort clip indices in reverse to remove from end first
        let mut by_track: HashMap<usize, Vec<usize>> = HashMap::new();
        for &(ti, ci) in &self.selected_clips {
            by_track.entry(ti).or_default().push(ci);
        }
        let count = self.selected_clips.len();

        // In ripple mode, compute the gap each deleted clip leaves, then shift subsequent clips
        if self.ripple_mode {
            for (ti, mut cis) in by_track {
                cis.sort_unstable();
                if ti >= self.project.tracks.len() {
                    continue;
                }
                // Process forward: for each deleted clip, shift all later clips left
                let mut total_shift: u64 = 0;
                let mut removed_indices: Vec<usize> = Vec::new();
                for &ci in &cis {
                    if ci < self.project.tracks[ti].clips.len() {
                        let clip = &self.project.tracks[ti].clips[ci];
                        total_shift += clip.visual_duration_samples();
                    }
                    removed_indices.push(ci);
                }
                // Remove clips in reverse order
                removed_indices.sort_unstable();
                // Find the earliest start position of deleted clips
                let earliest_start = removed_indices.iter()
                    .filter_map(|&ci| {
                        if ci < self.project.tracks[ti].clips.len() {
                            Some(self.project.tracks[ti].clips[ci].start_sample)
                        } else {
                            None
                        }
                    })
                    .min()
                    .unwrap_or(0);
                // Remove in reverse
                for &ci in removed_indices.iter().rev() {
                    if ci < self.project.tracks[ti].clips.len() {
                        self.project.tracks[ti].clips.remove(ci);
                    }
                }
                // Shift all clips after the deleted ones to the left
                for clip in &mut self.project.tracks[ti].clips {
                    if clip.start_sample >= earliest_start {
                        clip.start_sample = clip.start_sample.saturating_sub(total_shift);
                    }
                }
            }
        } else {
            for (ti, mut cis) in by_track {
                cis.sort_unstable();
                cis.reverse();
                for ci in cis {
                    if ti < self.project.tracks.len()
                        && ci < self.project.tracks[ti].clips.len()
                    {
                        self.project.tracks[ti].clips.remove(ci);
                    }
                }
            }
        }
        self.selected_clips.clear();
        self.sync_project();
        self.set_status(&format!("{} clip(s) deleted", count));
    }

    /// Remove audio buffers that are no longer referenced by any clip or frozen track.
    /// Returns the number of buffers removed.
    pub fn cleanup_unused_audio(&mut self) -> usize {
        let referenced: HashSet<Uuid> = self.project.tracks.iter()
            .flat_map(|t| {
                let clip_ids = t.clips.iter().filter_map(|c| {
                    if let ClipSource::AudioBuffer { buffer_id } = &c.source {
                        Some(*buffer_id)
                    } else {
                        None
                    }
                });
                let frozen_id = t.frozen_buffer_id.into_iter();
                clip_ids.chain(frozen_id)
            })
            .collect();

        let orphans: Vec<Uuid> = self.audio_buffers.keys()
            .filter(|id| !referenced.contains(id))
            .copied()
            .collect();

        let count = orphans.len();
        for id in orphans {
            self.audio_buffers.remove(&id);
            self.waveform_cache.remove(id);
        }
        count
    }

    /// Backward-compatible: check if any clips are selected
    pub fn has_selected_clips(&self) -> bool {
        !self.selected_clips.is_empty()
    }

    pub fn delete_selected_track(&mut self) {
        if let Some(track_idx) = self.selected_track {
            if track_idx < self.project.tracks.len() {
                let name = self.project.tracks[track_idx].name.clone();
                self.confirm_delete_track = Some((track_idx, name));
            }
        }
    }

    /// Actually perform track deletion (called after user confirms)
    fn do_delete_track(&mut self, track_idx: usize) {
        if track_idx < self.project.tracks.len() {
            self.push_undo("Delete track");
            self.project.tracks.remove(track_idx);
            if self.project.tracks.is_empty() {
                self.selected_track = None;
            } else {
                self.selected_track = Some(track_idx.min(self.project.tracks.len() - 1));
            }
            self.selected_clips.clear();
            self.sync_project();
            self.set_status("Track deleted");
        }
    }

    pub fn duplicate_selected_track(&mut self) {
        if let Some(track_idx) = self.selected_track {
            if track_idx < self.project.tracks.len() {
                self.push_undo("Duplicate track");
                let mut new_track = self.project.tracks[track_idx].clone();
                new_track.id = Uuid::new_v4();
                // Append "(copy)" only if not already a copy
                if new_track.name.ends_with("(copy)") {
                    new_track.name = format!("{} 2", new_track.name);
                } else {
                    new_track.name = format!("{} (copy)", new_track.name);
                }
                // Regenerate clip IDs so they are unique, but keep buffer_id references
                // shared so audio data is not duplicated in memory.
                for clip in new_track.clips.iter_mut() {
                    clip.id = Uuid::new_v4();
                    // ClipSource::AudioBuffer { buffer_id } stays the same — shared reference
                }
                // Regenerate effect slot IDs
                for slot in new_track.effects.iter_mut() {
                    slot.id = Uuid::new_v4();
                }
                if let Some(ref mut inst) = new_track.instrument_plugin {
                    inst.id = Uuid::new_v4();
                }
                // Session clips also get new IDs
                for sc in new_track.session_clips.iter_mut().flatten() {
                    sc.clip_id = Uuid::new_v4();
                }
                self.project.tracks.insert(track_idx + 1, new_track);
                self.selected_track = Some(track_idx + 1);
                self.sync_project();
                self.set_status("Track duplicated with all content");
            }
        }
    }

    /// Split the selected clip at the current playhead position.
    /// Split ALL clips on the selected track at the playhead position.
    pub fn split_clip_at_playhead(&mut self) {
        let pos = self.position_samples();
        let track_idx = self.selected_track.unwrap_or(0);
        if track_idx >= self.project.tracks.len() {
            self.set_status("No track selected");
            return;
        }

        // Find all clips that the playhead crosses
        let mut to_split: Vec<usize> = Vec::new();
        for (ci, clip) in self.project.tracks[track_idx].clips.iter().enumerate() {
            let clip_end = clip.start_sample + clip.duration_samples;
            if pos > clip.start_sample && pos < clip_end {
                to_split.push(ci);
            }
        }

        if to_split.is_empty() {
            self.set_status("No clips at playhead on this track");
            return;
        }

        self.push_undo("Split clips");

        // Process in reverse order so indices stay valid when inserting

        for &ci in to_split.iter().rev() {
            let clip_start = self.project.tracks[track_idx].clips[ci].start_sample;
            let clip_duration = self.project.tracks[track_idx].clips[ci].duration_samples;
            let clip_name = self.project.tracks[track_idx].clips[ci].name.clone();
            let clip_source = self.project.tracks[track_idx].clips[ci].source.clone();
            let clip_muted = self.project.tracks[track_idx].clips[ci].muted;
            let split_offset = pos - clip_start;

            let clip_color = self.project.tracks[track_idx].clips[ci].color;
            let clip_rate = self.project.tracks[track_idx].clips[ci].playback_rate;
            let clip_preserve_pitch = self.project.tracks[track_idx].clips[ci].preserve_pitch;
            let mut right_clip = Clip {
                id: Uuid::new_v4(),
                name: clip_name.clone(),
                start_sample: pos,
                duration_samples: clip_duration - split_offset,
                source: clip_source.clone(),
                muted: clip_muted,
                fade_in_samples: 0,
                fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                color: clip_color,
                playback_rate: clip_rate,
                preserve_pitch: clip_preserve_pitch,
                loop_count: 1,
                gain_db: 0.0,
                take_index: 0,
                content_offset: 0,
                transpose_semitones: 0,
                reversed: false,
            };

            if let ClipSource::AudioBuffer { buffer_id } = &clip_source {
                let buf_data = self.audio_buffers.get(buffer_id).cloned();
                if let Some(buf) = buf_data {
                    // Snap split point to nearest zero crossing to avoid clicks/pops
                    let raw_split = (split_offset as usize).min(buf.len());
                    let split_at = find_nearest_zero_crossing(&buf, raw_split, 256);
                    let right_samples = buf[split_at..].to_vec();
                    let left_samples = buf[..split_at].to_vec();

                    let right_id = Uuid::new_v4();
                    let left_id = Uuid::new_v4();

                    right_clip.source = ClipSource::AudioBuffer { buffer_id: right_id };
                    right_clip.duration_samples = right_samples.len() as u64;

                    self.waveform_cache.insert(right_id, &right_samples);
                    self.waveform_cache.insert(left_id, &left_samples);
                    self.send_command(EngineCommand::LoadAudioBuffer { id: right_id, samples: right_samples.clone() });
                    self.send_command(EngineCommand::LoadAudioBuffer { id: left_id, samples: left_samples.clone() });
                    self.audio_buffers.insert(right_id, right_samples);
                    self.audio_buffers.insert(left_id, left_samples);

                    self.project.tracks[track_idx].clips[ci].source =
                        ClipSource::AudioBuffer { buffer_id: left_id };
                }
            }

            self.project.tracks[track_idx].clips[ci].duration_samples = split_offset;

            // Insert right half immediately after left half to preserve take ordering
            self.project.tracks[track_idx].clips.insert(ci + 1, right_clip);

            // Adjust indices in to_split since we inserted a clip
            // (we're iterating in reverse, so earlier indices are unaffected)
        }

        // Don't change selection — user's current state stays as-is
        self.sync_project();
        self.set_status(&format!("Split {} clip(s) at playhead", to_split.len()));
    }

    /// Split clips on ALL tracks at the playhead position (Reaper-style).
    /// Called when pressing S with no clip selected.
    pub fn split_all_tracks_at_playhead(&mut self) {
        let pos = self.position_samples();
        let mut total_splits = 0usize;

        // Collect all (track_idx, clip_idx) pairs that need splitting
        let mut splits: Vec<(usize, Vec<usize>)> = Vec::new();
        for (ti, track) in self.project.tracks.iter().enumerate() {
            let mut to_split = Vec::new();
            for (ci, clip) in track.clips.iter().enumerate() {
                let clip_end = clip.start_sample + clip.duration_samples;
                if pos > clip.start_sample && pos < clip_end {
                    to_split.push(ci);
                }
            }
            if !to_split.is_empty() {
                splits.push((ti, to_split));
            }
        }

        if splits.is_empty() {
            self.set_status("No clips at playhead on any track");
            return;
        }

        self.push_undo("Split all tracks at playhead");

        for (ti, to_split) in &splits {
            let ti = *ti;
            for &ci in to_split.iter().rev() {
                let clip_start = self.project.tracks[ti].clips[ci].start_sample;
                let clip_duration = self.project.tracks[ti].clips[ci].duration_samples;
                let clip_name = self.project.tracks[ti].clips[ci].name.clone();
                let clip_source = self.project.tracks[ti].clips[ci].source.clone();
                let clip_muted = self.project.tracks[ti].clips[ci].muted;
                let split_offset = pos - clip_start;
                let clip_color = self.project.tracks[ti].clips[ci].color;
                let clip_rate = self.project.tracks[ti].clips[ci].playback_rate;
                let clip_preserve_pitch = self.project.tracks[ti].clips[ci].preserve_pitch;

                let mut right_clip = Clip {
                    id: Uuid::new_v4(),
                    name: clip_name.clone(),
                    start_sample: pos,
                    duration_samples: clip_duration - split_offset,
                    source: clip_source.clone(),
                    muted: clip_muted,
                    fade_in_samples: 0,
                    fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                    color: clip_color,
                    playback_rate: clip_rate,
                    preserve_pitch: clip_preserve_pitch,
                    loop_count: 1,
                    gain_db: 0.0,
                    take_index: 0,
                    content_offset: 0,
                    transpose_semitones: 0,
                    reversed: false,
                };

                if let ClipSource::AudioBuffer { buffer_id } = &clip_source {
                    let buf_data = self.audio_buffers.get(buffer_id).cloned();
                    if let Some(buf) = buf_data {
                        let raw_split = (split_offset as usize).min(buf.len());
                        let split_at = find_nearest_zero_crossing(&buf, raw_split, 256);
                        let right_samples = buf[split_at..].to_vec();
                        let left_samples = buf[..split_at].to_vec();

                        let right_id = Uuid::new_v4();
                        let left_id = Uuid::new_v4();

                        right_clip.source = ClipSource::AudioBuffer { buffer_id: right_id };
                        right_clip.duration_samples = right_samples.len() as u64;

                        self.waveform_cache.insert(right_id, &right_samples);
                        self.waveform_cache.insert(left_id, &left_samples);
                        self.send_command(EngineCommand::LoadAudioBuffer { id: right_id, samples: right_samples.clone() });
                        self.send_command(EngineCommand::LoadAudioBuffer { id: left_id, samples: left_samples.clone() });
                        self.audio_buffers.insert(right_id, right_samples);
                        self.audio_buffers.insert(left_id, left_samples);

                        self.project.tracks[ti].clips[ci].source =
                            ClipSource::AudioBuffer { buffer_id: left_id };
                    }
                }

                self.project.tracks[ti].clips[ci].duration_samples = split_offset;
                self.project.tracks[ti].clips.insert(ci + 1, right_clip);
                total_splits += 1;
            }
        }

        self.sync_project();
        self.set_status(&format!("Split {} clip(s) across all tracks at playhead", total_splits));
    }

    /// Insert silence at the playhead: shift all clips after the playhead forward
    /// by the given duration in samples.
    pub fn insert_silence(&mut self, duration_samples: u64) {
        if duration_samples == 0 {
            self.set_status("Insert silence: duration is zero");
            return;
        }

        let pos = self.position_samples();
        self.push_undo("Insert silence");

        // First, split any clips that span the playhead position
        for ti in 0..self.project.tracks.len() {
            let mut splits_needed: Vec<usize> = Vec::new();
            for (ci, clip) in self.project.tracks[ti].clips.iter().enumerate() {
                let clip_end = clip.start_sample + clip.duration_samples;
                if pos > clip.start_sample && pos < clip_end {
                    splits_needed.push(ci);
                }
            }
            // Split in reverse order to keep indices valid
            for &ci in splits_needed.iter().rev() {
                let clip = &self.project.tracks[ti].clips[ci];
                let split_offset = pos - clip.start_sample;
                let clip_source = clip.source.clone();
                let clip_duration = clip.duration_samples;
                let clip_name = clip.name.clone();
                let clip_muted = clip.muted;
                let clip_color = clip.color;
                let clip_rate = clip.playback_rate;
                let clip_preserve_pitch = clip.preserve_pitch;

                let mut right_clip = Clip {
                    id: Uuid::new_v4(),
                    name: clip_name,
                    start_sample: pos,
                    duration_samples: clip_duration - split_offset,
                    source: clip_source.clone(),
                    muted: clip_muted,
                    fade_in_samples: 0,
                    fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                    color: clip_color,
                    playback_rate: clip_rate,
                    preserve_pitch: clip_preserve_pitch,
                    loop_count: 1,
                    gain_db: 0.0,
                    take_index: 0,
                    content_offset: 0,
                    transpose_semitones: 0,
                    reversed: false,
                };

                if let ClipSource::AudioBuffer { buffer_id } = &clip_source {
                    if let Some(buf) = self.audio_buffers.get(buffer_id).cloned() {
                        let raw_split = (split_offset as usize).min(buf.len());
                        let split_at = find_nearest_zero_crossing(&buf, raw_split, 256);
                        let right_samples = buf[split_at..].to_vec();
                        let left_samples = buf[..split_at].to_vec();
                        let right_id = Uuid::new_v4();
                        let left_id = Uuid::new_v4();
                        right_clip.source = ClipSource::AudioBuffer { buffer_id: right_id };
                        right_clip.duration_samples = right_samples.len() as u64;
                        self.waveform_cache.insert(right_id, &right_samples);
                        self.waveform_cache.insert(left_id, &left_samples);
                        self.send_command(EngineCommand::LoadAudioBuffer { id: right_id, samples: right_samples.clone() });
                        self.send_command(EngineCommand::LoadAudioBuffer { id: left_id, samples: left_samples.clone() });
                        self.audio_buffers.insert(right_id, right_samples);
                        self.audio_buffers.insert(left_id, left_samples);
                        self.project.tracks[ti].clips[ci].source =
                            ClipSource::AudioBuffer { buffer_id: left_id };
                    }
                }
                self.project.tracks[ti].clips[ci].duration_samples = split_offset;
                self.project.tracks[ti].clips.insert(ci + 1, right_clip);
            }
        }

        // Now shift all clips that start at or after the playhead
        for track in &mut self.project.tracks {
            for clip in &mut track.clips {
                if clip.start_sample >= pos {
                    clip.start_sample += duration_samples;
                }
            }
        }

        // Also shift markers
        for marker in &mut self.project.markers {
            if marker.sample >= pos {
                marker.sample += duration_samples;
            }
        }

        // Shift regions
        for region in &mut self.project.regions {
            if region.start >= pos {
                region.start += duration_samples;
                region.end += duration_samples;
            } else if region.end > pos {
                region.end += duration_samples;
            }
        }

        // Shift automation points
        for track in &mut self.project.tracks {
            for lane in &mut track.automation {
                for point in &mut lane.points {
                    if point.sample >= pos {
                        point.sample += duration_samples;
                    }
                }
            }
        }

        self.sync_project();
        let secs = duration_samples as f64 / self.sample_rate() as f64;
        self.set_status(&format!("Inserted {:.2}s of silence at playhead", secs));
    }

    /// Remove time at the current selection: delete the time range from all tracks,
    /// trim clips that partially overlap, and shift remaining clips left.
    pub fn remove_time_selection(&mut self) {
        let (sel_start, sel_end) = match (self.selection_start, self.selection_end) {
            (Some(s), Some(e)) => (s.min(e), s.max(e)),
            _ => {
                self.set_status("No time selection — select a range first");
                return;
            }
        };

        let range_len = sel_end - sel_start;
        if range_len == 0 {
            self.set_status("Selection has zero length");
            return;
        }

        self.push_undo("Remove time at selection");

        for track in &mut self.project.tracks {
            let mut new_clips = Vec::new();
            for clip in &track.clips {
                let clip_end = clip.start_sample + clip.duration_samples;

                if clip_end <= sel_start {
                    // Clip is entirely before the selection — keep as-is
                    new_clips.push(clip.clone());
                } else if clip.start_sample >= sel_end {
                    // Clip is entirely after the selection — shift left
                    let mut shifted = clip.clone();
                    shifted.start_sample -= range_len;
                    new_clips.push(shifted);
                } else if clip.start_sample >= sel_start && clip_end <= sel_end {
                    // Clip is entirely within the selection — remove it
                    // (don't add to new_clips)
                } else if clip.start_sample < sel_start && clip_end > sel_end {
                    // Clip spans the entire selection — trim the middle out
                    // Keep left portion
                    let mut left = clip.clone();
                    left.duration_samples = sel_start - clip.start_sample;
                    new_clips.push(left);
                    // Keep right portion, shifted left
                    let mut right = clip.clone();
                    right.id = Uuid::new_v4();
                    right.start_sample = sel_start;
                    let right_offset = sel_end - clip.start_sample;
                    right.duration_samples = clip.duration_samples - right_offset;
                    right.content_offset = clip.content_offset + right_offset;
                    new_clips.push(right);
                } else if clip.start_sample < sel_start && clip_end > sel_start {
                    // Clip starts before selection, ends within it — trim right edge
                    let mut trimmed = clip.clone();
                    trimmed.duration_samples = sel_start - clip.start_sample;
                    new_clips.push(trimmed);
                } else if clip.start_sample < sel_end && clip_end > sel_end {
                    // Clip starts within selection, ends after it — trim left edge and shift
                    let mut trimmed = clip.clone();
                    let trim_amount = sel_end - clip.start_sample;
                    trimmed.start_sample = sel_start;
                    trimmed.duration_samples = clip.duration_samples - trim_amount;
                    trimmed.content_offset = clip.content_offset + trim_amount;
                    new_clips.push(trimmed);
                }
            }
            track.clips = new_clips;
        }

        // Shift markers
        self.project.markers.retain(|m| m.sample < sel_start || m.sample >= sel_end);
        for marker in &mut self.project.markers {
            if marker.sample >= sel_end {
                marker.sample -= range_len;
            }
        }

        // Shift regions
        self.project.regions.retain(|r| !(r.start >= sel_start && r.end <= sel_end));
        for region in &mut self.project.regions {
            if region.start >= sel_end {
                region.start -= range_len;
                region.end -= range_len;
            } else if region.end > sel_start {
                // Partially overlapping region — trim
                if region.start < sel_start {
                    region.end = sel_start;
                }
            }
        }

        // Shift automation points
        for track in &mut self.project.tracks {
            for lane in &mut track.automation {
                lane.points.retain(|p| p.sample < sel_start || p.sample >= sel_end);
                for point in &mut lane.points {
                    if point.sample >= sel_end {
                        point.sample -= range_len;
                    }
                }
            }
        }

        // Clear selection
        self.selection_start = None;
        self.selection_end = None;
        self.selected_clips.clear();

        self.sync_project();
        let secs = range_len as f64 / self.sample_rate() as f64;
        self.set_status(&format!("Removed {:.2}s of time", secs));
    }

    /// Crop to selection: remove everything outside the time selection,
    /// trim clips that extend beyond, and move everything to start at time 0.
    pub fn crop_to_selection(&mut self) {
        let (sel_start, sel_end) = match (self.selection_start, self.selection_end) {
            (Some(s), Some(e)) => (s.min(e), s.max(e)),
            _ => {
                self.set_status("No time selection — select a range first");
                return;
            }
        };

        if sel_end <= sel_start {
            self.set_status("Selection has zero length");
            return;
        }

        self.push_undo("Crop to selection");

        for track in &mut self.project.tracks {
            let mut new_clips = Vec::new();
            for clip in &track.clips {
                let clip_end = clip.start_sample + clip.duration_samples;

                // Skip clips entirely outside the selection
                if clip_end <= sel_start || clip.start_sample >= sel_end {
                    continue;
                }

                let mut kept = clip.clone();

                // Trim left edge if clip starts before selection
                if kept.start_sample < sel_start {
                    let trim = sel_start - kept.start_sample;
                    kept.content_offset += trim;
                    kept.duration_samples -= trim;
                    kept.start_sample = sel_start;
                }

                // Trim right edge if clip ends after selection
                let kept_end = kept.start_sample + kept.duration_samples;
                if kept_end > sel_end {
                    kept.duration_samples = sel_end - kept.start_sample;
                }

                // Shift to start at time 0
                kept.start_sample -= sel_start;

                new_clips.push(kept);
            }
            track.clips = new_clips;
        }

        // Filter and shift markers
        self.project.markers.retain(|m| m.sample >= sel_start && m.sample < sel_end);
        for marker in &mut self.project.markers {
            marker.sample -= sel_start;
        }

        // Filter and shift regions
        self.project.regions.retain(|r| r.end > sel_start && r.start < sel_end);
        for region in &mut self.project.regions {
            region.start = region.start.saturating_sub(sel_start);
            region.end = (region.end - sel_start).min(sel_end - sel_start);
        }

        // Shift automation points
        for track in &mut self.project.tracks {
            for lane in &mut track.automation {
                lane.points.retain(|p| p.sample >= sel_start && p.sample < sel_end);
                for point in &mut lane.points {
                    point.sample -= sel_start;
                }
            }
        }

        // Clear selection and move playhead to 0
        self.selection_start = None;
        self.selection_end = None;
        self.selected_clips.clear();
        self.send_command(EngineCommand::SetPosition(0));

        self.sync_project();
        let secs = (sel_end - sel_start) as f64 / self.sample_rate() as f64;
        self.set_status(&format!("Cropped to {:.2}s selection", secs));
    }

    /// Flatten comp: remove all muted clips (inactive takes) from the selected track,
    /// keeping only the active (unmuted) clips. This produces a clean single-take track.
    pub fn flatten_comp(&mut self, track_idx: usize) {
        if track_idx >= self.project.tracks.len() {
            return;
        }
        let track = &self.project.tracks[track_idx];
        let muted_count = track.clips.iter().filter(|c| c.muted).count();
        if muted_count == 0 {
            self.set_status("No inactive takes to flatten");
            return;
        }

        self.push_undo("Flatten comp");

        // Remove all muted clips (inactive takes)
        self.project.tracks[track_idx]
            .clips
            .retain(|c| !c.muted);

        // Collapse lanes since there's only one take now
        self.project.tracks[track_idx].lanes_expanded = false;
        self.project.tracks[track_idx].custom_height = 0.0;

        // Reset take_index on remaining clips
        for clip in &mut self.project.tracks[track_idx].clips {
            clip.take_index = 0;
        }

        self.sync_project();
        self.set_status(&format!(
            "Flattened comp — removed {} inactive take(s)",
            muted_count
        ));
    }

    /// Bounce/freeze selected track: render all effects to a new audio buffer.
    pub fn bounce_selected_track(&mut self) {
        let track_idx = match self.selected_track {
            Some(i) if i < self.project.tracks.len() => i,
            _ => {
                self.set_status("Cannot bounce: select a track first");
                return;
            }
        };
        if self.project.tracks[track_idx].clips.is_empty() {
            self.set_status("Cannot bounce: track has no clips");
            return;
        }

        let sr = self.sample_rate();
        match jamhub_engine::bounce_track(
            &self.project,
            track_idx,
            &self.audio_buffers,
            sr,
        ) {
            Ok(samples) => {
                self.push_undo("Bounce track");
                let buffer_id = Uuid::new_v4();
                let duration = samples.len() as u64;

                self.waveform_cache.insert(buffer_id, &samples);
                self.send_command(EngineCommand::LoadAudioBuffer {
                    id: buffer_id,
                    samples: samples.clone(),
                });
                self.audio_buffers.insert(buffer_id, samples);

                // Replace all clips with a single bounced clip, clear effects
                let bounced_name = format!("{} (bounced)", self.project.tracks[track_idx].name);
                self.project.tracks[track_idx].clips.clear();
                self.project.tracks[track_idx].clips.push(Clip {
                    id: Uuid::new_v4(),
                    name: bounced_name,
                    start_sample: 0,
                    duration_samples: duration,
                    source: ClipSource::AudioBuffer { buffer_id },
                    muted: false, content_offset: 0, transpose_semitones: 0, reversed: false,
                    fade_in_samples: 0,
                    fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                    color: None,
                    playback_rate: 1.0,
                    preserve_pitch: false,
                    loop_count: 1,
                    gain_db: 0.0,
                    take_index: 0,
                });
                // Unload any VST instances for this track's effects
                for slot in &self.project.tracks[track_idx].effects {
                    if slot.effect.is_vst() {
                        self.send_command(jamhub_engine::EngineCommand::UnloadVst3 {
                            slot_id: slot.id,
                        });
                    }
                }
                self.project.tracks[track_idx].effects.clear();
                self.sync_project();
                self.set_status("Track bounced — effects baked in");
            }
            Err(e) => self.set_status(&format!("Bounce failed: {e}")),
        }
    }

    /// Bounce a single clip in place: render it through the track's effect chain
    /// and replace the clip with the rendered audio. Effects are NOT removed.
    pub fn bounce_clip_in_place(&mut self, track_idx: usize, clip_idx: usize) {
        if track_idx >= self.project.tracks.len() {
            self.set_status("Invalid track index");
            return;
        }
        if clip_idx >= self.project.tracks[track_idx].clips.len() {
            self.set_status("Invalid clip index");
            return;
        }
        if self.project.tracks[track_idx].effects.is_empty() {
            self.set_status("No effects on track — nothing to bounce");
            return;
        }

        let clip = &self.project.tracks[track_idx].clips[clip_idx];
        let clip_start = clip.start_sample;
        let clip_duration = clip.visual_duration_samples();
        let clip_name = clip.name.clone();

        if clip_duration == 0 {
            self.set_status("Clip has zero duration");
            return;
        }

        // Build a temporary project with only this track and only this clip
        let sr = self.sample_rate();
        let mut temp_project = self.project.clone();
        let mut temp_track = temp_project.tracks[track_idx].clone();
        // Keep only the target clip (unmuted), clear all others
        let single_clip = temp_track.clips[clip_idx].clone();
        temp_track.clips = vec![single_clip];
        temp_track.clips[0].muted = false;
        temp_track.muted = false;
        temp_track.solo = false;
        temp_track.volume = 1.0;
        temp_track.pan = 0.0;
        temp_project.tracks = vec![temp_track];

        // Render through the mixer (includes effect chain)
        let block_size: usize = 1024;
        let total_samples = clip_duration + (sr as u64); // 1s tail for effects
        let mut mixer = jamhub_engine::Mixer::new(sr, 1); // mono render

        let mut output = Vec::new();
        let mut pos: u64 = clip_start;
        let end = clip_start + total_samples;
        while pos < end {
            let block = mixer.render_block(&temp_project, pos, block_size, &self.audio_buffers);
            output.extend_from_slice(&block);
            pos += block_size as u64;
        }

        // Trim trailing silence from the effects tail
        // Find last non-silent sample (threshold: -80dB ~ 0.0001)
        let mut trim_len = output.len();
        while trim_len > clip_duration as usize {
            if output[trim_len - 1].abs() > 0.0001 {
                break;
            }
            trim_len -= 1;
        }
        // Keep at least the original clip duration
        trim_len = trim_len.max(clip_duration as usize);
        output.truncate(trim_len);

        self.push_undo("Bounce clip in place");

        let buffer_id = Uuid::new_v4();
        let duration = output.len() as u64;

        self.waveform_cache.insert(buffer_id, &output);
        self.send_command(EngineCommand::LoadAudioBuffer {
            id: buffer_id,
            samples: output.clone(),
        });
        self.audio_buffers.insert(buffer_id, output);

        // Replace the clip with the bounced version at the same position
        self.project.tracks[track_idx].clips[clip_idx] = Clip {
            id: Uuid::new_v4(),
            name: format!("{} (bounced)", clip_name),
            start_sample: clip_start,
            duration_samples: duration,
            source: ClipSource::AudioBuffer { buffer_id },
            muted: false,
            content_offset: 0,
            transpose_semitones: 0,
            reversed: false,
            fade_in_samples: 0,
            fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
            color: None,
            playback_rate: 1.0,
            preserve_pitch: false,
            loop_count: 1,
            gain_db: 0.0,
            take_index: 0,
        };

        self.sync_project();
        self.set_status(&format!("Bounced in place: {} — effects baked into clip", clip_name));
    }

    /// Freeze selected track: render effects offline, disable processing, save CPU.
    pub fn freeze_selected_track(&mut self) {
        let track_idx = self.selected_track.unwrap_or(0);
        if track_idx >= self.project.tracks.len() { return; }
        if self.project.tracks[track_idx].frozen {
            self.set_status("Track is already frozen");
            return;
        }
        if self.project.tracks[track_idx].clips.is_empty() {
            self.set_status("No clips to freeze");
            return;
        }
        let sr = self.sample_rate();
        self.bounce_progress = Some(0.0);
        self.bounce_cancelled = false;
        match jamhub_engine::bounce_track_with_progress(
            &self.project, track_idx, &self.audio_buffers, sr,
            &mut |_frac| true,
        ) {
            Ok(samples) => {
                self.push_undo("Freeze track");
                let buffer_id = Uuid::new_v4();
                let duration = samples.len() as u64;
                self.waveform_cache.insert(buffer_id, &samples);
                self.send_command(EngineCommand::LoadAudioBuffer { id: buffer_id, samples: samples.clone() });
                self.audio_buffers.insert(buffer_id, samples);
                let original_clips = self.project.tracks[track_idx].clips.clone();
                let original_effects = self.project.tracks[track_idx].effects.clone();
                let frozen_name = format!("{} (frozen)", self.project.tracks[track_idx].name);
                self.project.tracks[track_idx].clips.clear();
                self.project.tracks[track_idx].clips.push(Clip {
                    id: Uuid::new_v4(),
                    name: frozen_name,
                    start_sample: 0, duration_samples: duration,
                    source: ClipSource::AudioBuffer { buffer_id },
                    muted: false, fade_in_samples: 0, fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                    color: None, playback_rate: 1.0, preserve_pitch: false, loop_count: 1, gain_db: 0.0, take_index: 0, content_offset: 0, transpose_semitones: 0, reversed: false,
                });
                self.project.tracks[track_idx].frozen = true;
                self.project.tracks[track_idx].frozen_buffer_id = Some(buffer_id);
                self.project.tracks[track_idx].pre_freeze_clips = Some(original_clips);
                self.project.tracks[track_idx].pre_freeze_effects = Some(original_effects);
                for slot in self.project.tracks[track_idx].effects.iter_mut() { slot.enabled = false; }
                self.sync_project();
                self.set_status("Track frozen — effects baked, CPU saved");
            }
            Err(e) => self.set_status(&format!("Freeze failed: {e}")),
        }
        self.bounce_progress = None;
    }

    /// Unfreeze selected track: restore original clips and re-enable effects.
    pub fn unfreeze_selected_track(&mut self) {
        let track_idx = self.selected_track.unwrap_or(0);
        if track_idx >= self.project.tracks.len() { return; }
        if !self.project.tracks[track_idx].frozen {
            self.set_status("Track is not frozen");
            return;
        }
        self.push_undo("Unfreeze track");
        if let Some(original_clips) = self.project.tracks[track_idx].pre_freeze_clips.take() {
            self.project.tracks[track_idx].clips = original_clips;
        }
        if let Some(original_effects) = self.project.tracks[track_idx].pre_freeze_effects.take() {
            self.project.tracks[track_idx].effects = original_effects;
            for slot in self.project.tracks[track_idx].effects.iter_mut() { slot.enabled = true; }
        }
        self.project.tracks[track_idx].frozen = false;
        self.project.tracks[track_idx].frozen_buffer_id = None;
        self.sync_project();
        self.set_status("Track unfrozen — original clips and effects restored");
    }

    /// Bounce a selection range on the selected track.
    pub fn bounce_selection_range(&mut self) {
        let track_idx = self.selected_track.unwrap_or(0);
        if track_idx >= self.project.tracks.len() { return; }
        let (range_start, range_end) = match (self.selection_start, self.selection_end) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => { self.set_status("No selection range — select a region first"); return; }
        };
        let sr = self.sample_rate();
        self.bounce_progress = Some(0.0);
        match jamhub_engine::bounce_track_range(
            &self.project, track_idx, &self.audio_buffers, sr,
            range_start, range_end, &mut |_frac| true,
        ) {
            Ok(samples) => {
                self.push_undo("Bounce selection");
                let buffer_id = Uuid::new_v4();
                let duration = samples.len() as u64;
                self.waveform_cache.insert(buffer_id, &samples);
                self.send_command(EngineCommand::LoadAudioBuffer { id: buffer_id, samples: samples.clone() });
                self.audio_buffers.insert(buffer_id, samples);
                let bounced_name = format!("{} (bounced range)", self.project.tracks[track_idx].name);
                self.project.tracks[track_idx].clips.push(Clip {
                    id: Uuid::new_v4(),
                    name: bounced_name,
                    start_sample: range_start, duration_samples: duration,
                    source: ClipSource::AudioBuffer { buffer_id },
                    muted: false, fade_in_samples: 0, fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                    color: None, playback_rate: 1.0, preserve_pitch: false, loop_count: 1, gain_db: 0.0, take_index: 0, content_offset: 0, transpose_semitones: 0, reversed: false,
                });
                self.sync_project();
                self.set_status(&format!("Selection bounced ({:.1}s)", duration as f64 / sr as f64));
            }
            Err(e) => self.set_status(&format!("Bounce selection failed: {e}")),
        }
        self.bounce_progress = None;
    }

    /// Show the "Add from Template" picker window.
    pub fn show_track_template_picker_window(&mut self, ctx: &egui::Context) {
        if !self.show_track_template_picker {
            return;
        }
        let mut open = true;
        let mut add_template: Option<templates::TrackTemplate> = None;
        let mut delete_idx: Option<usize> = None;

        egui::Window::new("Add Track from Template")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                let user_templates = templates::load_track_templates();
                if user_templates.is_empty() {
                    ui.label(
                        egui::RichText::new("No saved templates yet.")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(130, 130, 140)),
                    );
                    ui.label(
                        egui::RichText::new("Right-click a track header > \"Save as Template...\" to create one.")
                            .size(10.0)
                            .color(egui::Color32::from_rgb(110, 110, 120)),
                    );
                } else {
                    ui.label("Saved templates:");
                    ui.add_space(4.0);
                    for (idx, tpl) in user_templates.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let kind_label = match tpl.track_kind {
                                TrackKind::Audio
                                | TrackKind::Bus => "Audio",
                                TrackKind::Midi => "MIDI",
                                TrackKind::Folder => "Folder",
                            };
                            let fx_count = tpl.effects.len();
                            let c = egui::Color32::from_rgb(tpl.color[0], tpl.color[1], tpl.color[2]);
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 5.0, c);
                            let label = format!("{} ({}, {} FX)", tpl.name, kind_label, fx_count);
                            if ui.button(&label).clicked() {
                                add_template = Some(tpl.clone());
                            }
                            if ui.add(
                                egui::Button::new(
                                    egui::RichText::new("x")
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(160, 60, 60)),
                                ).frame(false),
                            ).on_hover_text("Delete template").clicked() {
                                delete_idx = Some(idx);
                            }
                        });
                    }
                }
            });

        if let Some(tpl) = add_template {
            self.push_undo("Add track from template");
            let id = uuid::Uuid::new_v4();
            let track = jamhub_model::Track {
                id,
                name: tpl.name.clone(),
                kind: tpl.track_kind,
                clips: Vec::new(),
                volume: tpl.volume,
                pan: tpl.pan,
                muted: false,
                solo: false,
                armed: false,
                color: tpl.color,
                effects: tpl.effects.iter().map(|e| {
                    jamhub_model::EffectSlot::new(e.effect.clone())
                }).collect(),
                lanes_expanded: false,
                custom_height: 0.0,
                automation: Vec::new(),
                sends: tpl.sends.clone(),
                group_id: None,
                frozen: false,
                frozen_buffer_id: None,
                pre_freeze_clips: None,
                pre_freeze_effects: None,
                sidechain_track_id: None,
                input_channel: None,
                output_target: None,
                session_clips: Vec::new(),
                synth_wave: "Saw".to_string(),
                synth_attack: 10.0,
                synth_decay: 100.0,
                synth_sustain: 0.7,
                synth_release: 200.0,
                synth_cutoff: 8000.0,
                instrument_plugin: None,
                phase_inverted: false,
                mono: false,
            };
            self.project.tracks.push(track);
            self.selected_track = Some(self.project.tracks.len() - 1);
            self.sync_project();
            self.set_status(&format!("Added track from template: {}", tpl.name));
            self.show_track_template_picker = false;
        }

        if let Some(idx) = delete_idx {
            let mut user_templates = templates::load_track_templates();
            if idx < user_templates.len() {
                user_templates.remove(idx);
                templates::save_track_templates(&user_templates);
                self.set_status("Template deleted");
            }
        }

        if !open {
            self.show_track_template_picker = false;
        }
    }

    /// Show a color palette popup window for a track.
    pub fn show_color_palette_popup(&mut self, ctx: &egui::Context) {
        let track_idx = match self.color_palette_track {
            Some(i) if i < self.project.tracks.len() => i,
            _ => {
                self.color_palette_track = None;
                return;
            }
        };

        let mut open = true;
        let mut chosen_color: Option<[u8; 3]> = None;
        let mut open_custom = false;

        egui::Window::new("Track Color")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Choose a color:")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(180, 180, 190)),
                );
                ui.add_space(4.0);

                // 4x4 color grid
                let colors = templates::PALETTE_COLORS;
                for row in 0..4 {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for col in 0..4 {
                            let idx = row * 4 + col;
                            if idx < colors.len() {
                                let (rgb, name) = colors[idx];
                                let c = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                                let current = self.project.tracks[track_idx].color;
                                let is_current = current == rgb;

                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(28.0, 28.0),
                                    egui::Sense::click(),
                                );
                                ui.painter().rect_filled(rect, 4.0, c);
                                if is_current {
                                    ui.painter().rect_stroke(
                                        rect,
                                        4.0,
                                        egui::Stroke::new(2.0, egui::Color32::WHITE),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                                if resp.on_hover_text(name).clicked() {
                                    chosen_color = Some(rgb);
                                }
                            }
                        }
                    });
                }

                ui.add_space(6.0);
                ui.separator();
                if ui.button("Custom RGB...").clicked() {
                    open_custom = true;
                }
            });

        if let Some(color) = chosen_color {
            self.push_undo("Set track color");
            self.project.tracks[track_idx].color = color;
            self.sync_project();
            self.color_palette_track = None;
        }

        if open_custom {
            let current = self.project.tracks[track_idx].color;
            self.custom_color_input = Some(templates::CustomColorInput {
                track_idx,
                r: current[0],
                g: current[1],
                b: current[2],
            });
            self.color_palette_track = None;
        }

        if !open {
            self.color_palette_track = None;
        }
    }

    /// Show the Audio Pool manager window.
    pub fn show_audio_pool_window(&mut self, ctx: &egui::Context) {
        if !self.show_audio_pool { return; }
        let mut open = self.show_audio_pool;
        egui::Window::new("Audio Pool")
            .open(&mut open)
            .default_size([620.0, 450.0])
            .resizable(true)
            .show(ctx, |ui| {
                struct BufInfo { id: Uuid, name: String, samples: usize, sample_rate: u32, used_by: Vec<String> }
                let sr = self.sample_rate();
                let mut infos: Vec<BufInfo> = Vec::new();
                for (&buf_id, buf) in &self.audio_buffers {
                    let mut used_by = Vec::new();
                    for track in &self.project.tracks {
                        for clip in &track.clips {
                            if let ClipSource::AudioBuffer { buffer_id } = &clip.source {
                                if *buffer_id == buf_id { used_by.push(clip.name.clone()); }
                            }
                        }
                        if track.frozen_buffer_id == Some(buf_id) {
                            used_by.push(format!("{} (frozen)", track.name));
                        }
                    }
                    let name = self.project.tracks.iter()
                        .flat_map(|t| t.clips.iter())
                        .find(|c| matches!(&c.source, ClipSource::AudioBuffer { buffer_id } if *buffer_id == buf_id))
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| format!("Buffer {}", &buf_id.to_string()[..8]));
                    infos.push(BufInfo { id: buf_id, name, samples: buf.len(), sample_rate: sr, used_by });
                }
                infos.sort_by(|a, b| {
                    (a.used_by.is_empty() as u8).cmp(&(b.used_by.is_empty() as u8))
                        .then_with(|| a.name.cmp(&b.name))
                });
                let total_samples: usize = infos.iter().map(|b| b.samples).sum();
                let total_mb = (total_samples * 4) as f64 / (1024.0 * 1024.0);
                let orphan_count = infos.iter().filter(|b| b.used_by.is_empty()).count();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{} buffers | {:.1} MB total", infos.len(), total_mb)).strong());
                    if orphan_count > 0 {
                        ui.label(egui::RichText::new(format!(" | {} orphaned", orphan_count))
                            .color(egui::Color32::from_rgb(220, 160, 60)));
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("audio_pool_grid").striped(true).min_col_width(60.0).show(ui, |ui| {
                        ui.label(egui::RichText::new("Name").strong());
                        ui.label(egui::RichText::new("Duration").strong());
                        ui.label(egui::RichText::new("Size").strong());
                        ui.label(egui::RichText::new("Rate").strong());
                        ui.label(egui::RichText::new("Used By").strong());
                        ui.label(egui::RichText::new("").strong());
                        ui.end_row();
                        let mut to_delete: Vec<Uuid> = Vec::new();
                        for info in &infos {
                            let is_orphan = info.used_by.is_empty();
                            let text_color = if is_orphan {
                                egui::Color32::from_rgb(220, 160, 60)
                            } else {
                                egui::Color32::from_rgb(210, 210, 215)
                            };
                            ui.label(egui::RichText::new(&info.name).color(text_color));
                            let dur_s = info.samples as f64 / info.sample_rate as f64;
                            ui.label(egui::RichText::new(format!("{:.2}s", dur_s)).color(text_color));
                            let size_kb = (info.samples * 4) as f64 / 1024.0;
                            if size_kb > 1024.0 {
                                ui.label(egui::RichText::new(format!("{:.1} MB", size_kb / 1024.0)).color(text_color));
                            } else {
                                ui.label(egui::RichText::new(format!("{:.0} KB", size_kb)).color(text_color));
                            }
                            ui.label(egui::RichText::new(format!("{} Hz", info.sample_rate)).color(text_color));
                            let used_str = if is_orphan { "orphaned".into() } else { info.used_by.join(", ") };
                            ui.label(egui::RichText::new(&used_str).color(text_color).size(11.0));
                            ui.horizontal(|ui| {
                                let is_previewing = self.audio_pool_preview_id == Some(info.id);
                                if ui.small_button(if is_previewing { "Stop" } else { "Play" }).clicked() {
                                    if is_previewing {
                                        self.audio_pool_preview_id = None;
                                        self.send_command(EngineCommand::Stop);
                                    } else {
                                        self.audio_pool_preview_id = Some(info.id);
                                        self.send_command(EngineCommand::SetPosition(0));
                                    }
                                }
                                if is_orphan && ui.small_button("Del").on_hover_text("Remove unused buffer").clicked() {
                                    to_delete.push(info.id);
                                }
                            });
                            ui.end_row();
                        }
                        for del_id in &to_delete {
                            self.audio_buffers.remove(del_id);
                            self.waveform_cache.remove(*del_id);
                        }
                        if !to_delete.is_empty() {
                            self.set_status(&format!("Removed {} buffer(s)", to_delete.len()));
                        }
                    });
                });
                ui.separator();
                if orphan_count > 0 {
                    if ui.button(format!("Delete All Orphaned ({})", orphan_count)).clicked() {
                        let orphan_ids: Vec<Uuid> = infos.iter()
                            .filter(|b| b.used_by.is_empty()).map(|b| b.id).collect();
                        let count = orphan_ids.len();
                        for id in orphan_ids {
                            self.audio_buffers.remove(&id);
                            self.waveform_cache.remove(id);
                        }
                        self.set_status(&format!("Removed {} orphaned buffer(s)", count));
                    }
                }
            });
        self.show_audio_pool = open;
    }

    pub fn toggle_input_monitor(&mut self) {
        match self.input_monitor.toggle() {
            Ok(true) => self.set_status("Input monitoring ON — you can hear your mic"),
            Ok(false) => self.set_status("Input monitoring OFF"),
            Err(e) => self.set_status(&format!("Monitor failed: {e}")),
        }
    }

    pub fn export_mixdown(&mut self) {
        // Check if there's any audio content to export
        let has_content = self.project.tracks.iter().any(|t| !t.clips.is_empty());
        if !has_content {
            self.set_status("Export failed: no audio to export — add clips first");
            return;
        }

        let fmt = self.export_format;
        let ext = fmt.extension();
        let filter_label = match fmt {
            ExportFormat::Wav => "WAV Audio",
            ExportFormat::Flac => "FLAC Audio",
            ExportFormat::Aiff => "AIFF Audio",
        };

        let filename = format!("mixdown.{ext}");
        if let Some(path) = rfd::FileDialog::new()
            .set_title(&format!("Export Mixdown ({} {}‑bit)", fmt.label(), self.export_bit_depth))
            .add_filter(filter_label, &[ext])
            .add_filter("WAV Audio", &["wav"])
            .add_filter("FLAC Audio", &["flac"])
            .add_filter("AIFF Audio", &["aiff"])
            .set_file_name(&filename)
            .save_file()
        {
            let sr = self.sample_rate();
            let options = ExportOptions {
                normalize: self.export_normalize,
                bit_depth: self.export_bit_depth,
                channels: 2,
                tail_seconds: 1.0,
                format: fmt,
                sample_rate: if self.export_sample_rate > 0 { self.export_sample_rate } else { 0 },
            };
            self.set_status(&format!("Exporting {}...", path.file_name().unwrap_or_default().to_string_lossy()));
            let start_time = std::time::Instant::now();
            match jamhub_engine::export_with_options(&path, &self.project, &self.audio_buffers, sr, &options) {
                Ok(()) => {
                    let elapsed = start_time.elapsed().as_secs_f32();
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    self.set_status(&format!("Exported: {} ({:.1}s)", filename, elapsed));
                }
                Err(e) => self.set_status(&format!("Export failed: {e}")),
            }
        }
    }

    /// Apply an offline operation to the selected clip's audio buffer.
    fn apply_clip_operation(&mut self, op_name: &str, op: fn(&mut Vec<f32>, u32)) {
        let (ti, ci) = match self.selected_clips.iter().next() {
            Some(&tc) => tc,
            None => {
                self.set_status("No clip selected");
                return;
            }
        };
        if ti >= self.project.tracks.len()
            || ci >= self.project.tracks[ti].clips.len()
        {
            return;
        }
        if let ClipSource::AudioBuffer { buffer_id } =
            &self.project.tracks[ti].clips[ci].source
        {
            let buf_data = self.audio_buffers.get(buffer_id).cloned();
            if let Some(mut buf) = buf_data {
                self.push_undo(op_name);
                let sr = self.sample_rate();
                op(&mut buf, sr);

                // Update everything
                let new_id = Uuid::new_v4();
                self.waveform_cache.insert(new_id, &buf);
                self.send_command(EngineCommand::LoadAudioBuffer {
                    id: new_id,
                    samples: buf.clone(),
                });
                self.project.tracks[ti].clips[ci].duration_samples = buf.len() as u64;
                self.project.tracks[ti].clips[ci].source =
                    ClipSource::AudioBuffer { buffer_id: new_id };
                self.audio_buffers.insert(new_id, buf);
                self.sync_project();
                self.set_status(&format!("{op_name} applied"));
            }
        }
    }

    pub fn reverse_clip(&mut self) {
        self.apply_clip_operation("Reverse", |buf, _| {
            jamhub_engine::clip_ops::reverse(buf);
        });
    }

    pub fn normalize_clip(&mut self) {
        self.apply_clip_operation("Normalize", |buf, _| {
            jamhub_engine::clip_ops::normalize(buf);
        });
    }

    pub fn fade_in_clip(&mut self) {
        self.apply_clip_operation("Fade In", |buf, sr| {
            let fade = (sr as f32 * 0.1) as usize; // 100ms fade
            jamhub_engine::clip_ops::fade_in(buf, fade);
        });
    }

    pub fn fade_out_clip(&mut self) {
        self.apply_clip_operation("Fade Out", |buf, sr| {
            let fade = (sr as f32 * 0.1) as usize;
            jamhub_engine::clip_ops::fade_out(buf, fade);
        });
    }

    pub fn invert_clip(&mut self) {
        self.apply_clip_operation("Invert Phase", |buf, _| {
            jamhub_engine::clip_ops::invert(buf);
        });
    }

    pub fn gain_up_clip(&mut self) {
        self.apply_clip_operation("Gain +3dB", |buf, _| {
            jamhub_engine::clip_ops::apply_gain_db(buf, 3.0);
        });
    }

    pub fn gain_down_clip(&mut self) {
        self.apply_clip_operation("Gain -3dB", |buf, _| {
            jamhub_engine::clip_ops::apply_gain_db(buf, -3.0);
        });
    }

    pub fn silence_clip(&mut self) {
        self.apply_clip_operation("Silence", |buf, _| {
            jamhub_engine::clip_ops::silence(buf);
        });
    }

    pub fn export_stems(&mut self) {
        let has_content = self.project.tracks.iter().any(|t| !t.clips.is_empty());
        if !has_content {
            self.set_status("Export failed: no audio to export — add clips first");
            return;
        }

        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Export Stems — Choose Directory")
            .pick_folder()
        {
            let sr = self.sample_rate();
            let options = ExportOptions {
                normalize: self.export_normalize,
                bit_depth: self.export_bit_depth,
                channels: 2,
                tail_seconds: 1.0,
                format: self.export_format,
                sample_rate: if self.export_sample_rate > 0 { self.export_sample_rate } else { 0 },
            };
            let start_time = std::time::Instant::now();
            self.set_status("Exporting stems...");
            let result = jamhub_engine::export_stems(
                &dir,
                &self.project,
                &self.audio_buffers,
                sr,
                &options,
                |_current, _total| {
                    // Progress callback — could be used for UI progress bar in future
                },
            );
            match result {
                Ok(res) => {
                    let count = res.stems.len();
                    let elapsed = start_time.elapsed().as_secs_f32();
                    self.set_status(&format!("Exported {count} stems to {} ({:.1}s)", dir.display(), elapsed));
                }
                Err(e) => self.set_status(&format!("Stem export failed: {e}")),
            }
        }
    }

    pub fn copy_selected_clips(&mut self) {
        if self.selected_clips.is_empty() {
            return;
        }
        self.clipboard_clips.clear();
        for &(ti, ci) in &self.selected_clips {
            if ti < self.project.tracks.len() && ci < self.project.tracks[ti].clips.len() {
                let clip = self.project.tracks[ti].clips[ci].clone();
                let buf = if let ClipSource::AudioBuffer { buffer_id } = &clip.source {
                    self.audio_buffers.get(buffer_id).cloned()
                } else {
                    None
                };
                self.clipboard_clips.push((clip, buf));
            }
        }
        let count = self.clipboard_clips.len();
        self.set_status(&format!("{} clip(s) copied", count));
    }

    pub fn paste_clips(&mut self) {
        if self.clipboard_clips.is_empty() {
            self.set_status("Nothing to paste");
            return;
        }
        let ti = self.selected_track.unwrap_or(0);
        if ti >= self.project.tracks.len() { return; }

        self.push_undo("Paste clips");
        let pos = self.position_samples();

        // Find the earliest start_sample among clipboard clips to compute offsets
        let min_start = self.clipboard_clips.iter()
            .map(|(c, _)| c.start_sample)
            .min()
            .unwrap_or(0);

        self.selected_clips.clear();
        let clip_data: Vec<_> = self.clipboard_clips.clone();
        for (clip, buf) in &clip_data {
            let mut new_clip = clip.clone();
            new_clip.id = Uuid::new_v4();
            let offset = clip.start_sample.saturating_sub(min_start);
            new_clip.start_sample = pos + offset;

            if let Some(samples) = buf {
                let buffer_id = Uuid::new_v4();
                new_clip.source = ClipSource::AudioBuffer { buffer_id };
                self.waveform_cache.insert(buffer_id, samples);
                self.send_command(EngineCommand::LoadAudioBuffer {
                    id: buffer_id,
                    samples: samples.clone(),
                });
                self.audio_buffers.insert(buffer_id, samples.clone());
            }

            self.project.tracks[ti].clips.push(new_clip);
            let new_ci = self.project.tracks[ti].clips.len() - 1;
            self.selected_clips.insert((ti, new_ci));
        }
        self.sync_project();
        let count = clip_data.len();
        self.set_status(&format!("{} clip(s) pasted", count));
    }

    pub fn duplicate_selected_clips(&mut self) {
        if self.selected_clips.is_empty() {
            return;
        }
        self.push_undo("Duplicate clips");
        let mut new_selections: Vec<(usize, usize)> = Vec::new();

        let to_dup: Vec<(usize, usize)> = self.selected_clips.iter().copied().collect();
        for (ti, ci) in to_dup {
            if ti >= self.project.tracks.len() || ci >= self.project.tracks[ti].clips.len() {
                continue;
            }
            let mut new_clip = self.project.tracks[ti].clips[ci].clone();
            new_clip.id = Uuid::new_v4();
            new_clip.start_sample += new_clip.duration_samples;
            new_clip.name = format!("{} (copy)", new_clip.name);
            new_clip.muted = false;

            if let ClipSource::AudioBuffer { buffer_id } = &self.project.tracks[ti].clips[ci].source {
                if let Some(buf) = self.audio_buffers.get(buffer_id).cloned() {
                    let new_buf_id = Uuid::new_v4();
                    new_clip.source = ClipSource::AudioBuffer { buffer_id: new_buf_id };
                    self.waveform_cache.insert(new_buf_id, &buf);
                    self.send_command(EngineCommand::LoadAudioBuffer {
                        id: new_buf_id,
                        samples: buf.clone(),
                    });
                    self.audio_buffers.insert(new_buf_id, buf);
                }
            }

            self.project.tracks[ti].clips.push(new_clip);
            let new_ci = self.project.tracks[ti].clips.len() - 1;
            new_selections.push((ti, new_ci));
        }
        self.selected_clips.clear();
        for sel in new_selections {
            self.selected_clips.insert(sel);
        }
        self.sync_project();
        self.set_status("Clips duplicated");
    }

    pub fn zoom_to_fit(&mut self) {
        // Find the end of the last clip across all tracks
        let end_sample = self.project.tracks.iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.start_sample + c.duration_samples)
            .max()
            .unwrap_or(0);

        if end_sample == 0 { return; }

        let sr = self.sample_rate() as f64;
        let end_sec = end_sample as f64 / sr;
        // Assume ~1000px visible width, calculate zoom to fit
        let target_zoom = 800.0 / (end_sec as f32 * 100.0);
        self.zoom = target_zoom.clamp(0.1, 10.0);
        self.scroll_x = 0.0;
    }

    /// Zoom to selection if one exists, otherwise zoom to fit all content.
    pub fn zoom_to_selection_or_fit(&mut self) {
        if let (Some(sel_s), Some(sel_e)) = (self.selection_start, self.selection_end) {
            let s1 = sel_s.min(sel_e);
            let s2 = sel_s.max(sel_e);
            if s2 > s1 + 100 {
                let sr = self.sample_rate() as f64;
                let start_sec = s1 as f64 / sr;
                let end_sec = s2 as f64 / sr;
                let duration_sec = end_sec - start_sec;
                let pps_base = 100.0_f32;
                let target_zoom = 800.0 / (duration_sec as f32 * pps_base);
                self.zoom = target_zoom.clamp(0.1, 10.0);
                let pps = pps_base * self.zoom;
                self.scroll_x = (start_sec as f32 * pps - 20.0).max(0.0);
                self.set_status("Zoomed to selection");
                return;
            }
        }
        self.zoom_to_fit();
    }

    pub fn focus_playhead(&mut self) {
        let pos = self.position_samples();
        let sr = self.sample_rate() as f64;
        let pos_sec = pos as f64 / sr;
        let pps = 100.0 * self.zoom;
        let playhead_px = pos_sec as f32 * pps;
        // Center playhead in view (assume ~800px visible)
        self.scroll_x = (playhead_px - 400.0).max(0.0);
    }

    /// Move the selected track up in the arrangement order.
    pub fn move_selected_track_up(&mut self) {
        if let Some(idx) = self.selected_track {
            if idx > 0 && idx < self.project.tracks.len() {
                self.push_undo("Move track up");
                self.project.tracks.swap(idx, idx - 1);
                self.selected_track = Some(idx - 1);
                self.selected_clips.clear();
                self.sync_project();
                self.set_status("Track moved up");
            }
        }
    }

    /// Move the selected track down in the arrangement order.
    pub fn move_selected_track_down(&mut self) {
        if let Some(idx) = self.selected_track {
            if idx + 1 < self.project.tracks.len() {
                self.push_undo("Move track down");
                self.project.tracks.swap(idx, idx + 1);
                self.selected_track = Some(idx + 1);
                self.selected_clips.clear();
                self.sync_project();
                self.set_status("Track moved down");
            }
        }
    }

    /// Consolidate/glue selected clips on the same track into a single clip.
    /// Renders the clips (with gaps filled by silence) into one audio buffer.
    pub fn consolidate_selected_clips(&mut self) {
        if self.selected_clips.len() < 2 {
            self.set_status("Select 2 or more clips on the same track to consolidate");
            return;
        }

        // All selected clips must be on the same track
        let track_indices: HashSet<usize> = self.selected_clips.iter().map(|&(ti, _)| ti).collect();
        if track_indices.len() != 1 {
            self.set_status("Consolidate: all clips must be on the same track");
            return;
        }
        let ti = match track_indices.iter().next() {
            Some(&t) => t,
            None => return,
        };
        if ti >= self.project.tracks.len() {
            return;
        }

        let clip_indices: Vec<usize> = self.selected_clips.iter()
            .map(|&(_, ci)| ci)
            .filter(|&ci| ci < self.project.tracks[ti].clips.len())
            .collect();

        if clip_indices.len() < 2 {
            self.set_status("Need at least 2 valid clips to consolidate");
            return;
        }

        // Find the overall start and end positions
        let overall_start = clip_indices.iter()
            .map(|&ci| self.project.tracks[ti].clips[ci].start_sample)
            .min()
            .unwrap_or(0);
        let overall_end = clip_indices.iter()
            .map(|&ci| {
                let clip = &self.project.tracks[ti].clips[ci];
                clip.start_sample + clip.visual_duration_samples()
            })
            .max()
            .unwrap_or(0);

        if overall_end <= overall_start {
            return;
        }

        let total_len = (overall_end - overall_start) as usize;

        // Create a buffer filled with silence
        let mut consolidated = vec![0.0f32; total_len];

        // Mix each selected clip's audio into the buffer
        for &ci in &clip_indices {
            let clip = &self.project.tracks[ti].clips[ci];
            if clip.muted {
                continue;
            }
            if let ClipSource::AudioBuffer { buffer_id } = &clip.source {
                if let Some(buf) = self.audio_buffers.get(buffer_id) {
                    let clip_offset = (clip.start_sample - overall_start) as usize;
                    let rate = clip.playback_rate.max(0.01);
                    let loop_count = clip.effective_loop_count() as usize;
                    let single_visual = clip.single_loop_visual_duration() as usize;

                    for lp in 0..loop_count {
                        let loop_offset = lp * single_visual;
                        for i in 0..single_visual {
                            let dst = clip_offset + loop_offset + i;
                            if dst >= total_len {
                                break;
                            }
                            let src_pos = i as f64 * rate as f64 + clip.content_offset as f64;
                            let src_idx = src_pos.floor() as usize;
                            if src_idx >= buf.len() {
                                break;
                            }
                            let frac = src_pos - src_pos.floor();
                            let s0 = buf[src_idx];
                            let s1 = if src_idx + 1 < buf.len() { buf[src_idx + 1] } else { s0 };
                            consolidated[dst] += s0 + (s1 - s0) * frac as f32;
                        }
                    }
                }
            }
        }

        self.push_undo("Consolidate clips");

        // Create new buffer and clip
        let buffer_id = Uuid::new_v4();
        let duration = consolidated.len() as u64;

        self.waveform_cache.insert(buffer_id, &consolidated);
        self.send_command(jamhub_engine::EngineCommand::LoadAudioBuffer {
            id: buffer_id,
            samples: consolidated.clone(),
        });
        self.audio_buffers.insert(buffer_id, consolidated);

        // Remove old clips (in reverse order)
        let mut sorted_cis: Vec<usize> = clip_indices;
        sorted_cis.sort_unstable();
        sorted_cis.reverse();
        for ci in sorted_cis {
            if ci < self.project.tracks[ti].clips.len() {
                self.project.tracks[ti].clips.remove(ci);
            }
        }

        // Add consolidated clip
        let new_clip = Clip {
            id: Uuid::new_v4(),
            name: "Consolidated".to_string(),
            start_sample: overall_start,
            duration_samples: duration,
            source: ClipSource::AudioBuffer { buffer_id },
            muted: false, content_offset: 0, transpose_semitones: 0, reversed: false,
            fade_in_samples: 0,
            fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
            color: None,
            playback_rate: 1.0,
            preserve_pitch: false,
            loop_count: 1,
            gain_db: 0.0,
            take_index: 0,
        };
        self.project.tracks[ti].clips.push(new_clip);
        self.selected_clips.clear();
        self.sync_project();
        self.set_status("Clips consolidated");
    }

    pub fn open_import_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio Files", &["wav", "wave", "mp3", "ogg", "flac"])
            .pick_file()
        {
            self.import_audio_file(path);
        }
    }

    pub fn save_project(&mut self) {
        let dir = if let Some(ref path) = self.project_path {
            path.clone()
        } else {
            // Use save_file dialog so macOS shows "Save" instead of "Open"
            let safe_name = self.project.name.replace(['/', '\\', ':'], "_");
            let filename = format!("{}.twproj", safe_name);
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Save Project")
                .set_file_name(&filename)
                .add_filter("ThroughWaves Project", &["twproj"])
                .save_file()
            {
                // Use the chosen path (without extension) as project directory
                let project_dir = path.with_extension("");
                self.project_path = Some(project_dir.clone());
                project_dir
            } else {
                return;
            }
        };

        // Create backup of previous version before overwriting
        Self::backup_project(&dir);

        // Clean up unreferenced audio buffers before saving
        let _cleaned = self.cleanup_unused_audio();

        let sr = self.sample_rate();
        match jamhub_engine::save_project(&dir, &self.project, &self.audio_buffers, sr) {
            Ok(()) => {
                self.dirty = false;
                self.last_autosave = std::time::Instant::now();
                self.cleanup_autosave();
                add_to_recent_projects(&mut self.recent_projects, &dir);

                // Auto-create a version commit on save
                let msg = format!("Saved — {}", chrono::Local::now().format("%b %d %H:%M"));
                self.version_commit(&msg);

                self.set_status(&format!("Saved to {}", dir.display()));
            }
            Err(e) => self.set_status(&format!("Save failed: {e}")),
        }
    }

    pub fn load_project_dialog(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Open Project")
            .pick_folder()
        {
            self.load_project_from(&dir);
        }
    }

    /// Perform auto-save to a backup location (does not overwrite the main project file).
    fn perform_autosave(&mut self) {
        let sr = self.sample_rate();
        let dir = if let Some(ref path) = self.project_path {
            let mut autosave_path = path.as_os_str().to_owned();
            autosave_path.push(".autosave");
            PathBuf::from(autosave_path)
        } else {
            autosave_dir().join(&self.project.name)
        };

        match jamhub_engine::save_project(&dir, &self.project, &self.audio_buffers, sr) {
            Ok(()) => {
                self.last_autosave = std::time::Instant::now();
                self.set_status("Auto-saved");
            }
            Err(e) => {
                eprintln!("Auto-save failed: {e}");
            }
        }
    }

    /// Create a backup of the current project directory before saving.
    fn backup_project(dir: &std::path::Path) {
        if dir.exists() {
            let mut bak_path = dir.as_os_str().to_owned();
            bak_path.push(".bak");
            let bak = PathBuf::from(bak_path);
            if bak.exists() {
                let _ = fs::remove_dir_all(&bak);
            }
            if let Err(e) = Self::copy_dir_recursive(dir, &bak) {
                eprintln!("Backup failed: {e}");
            }
        }
    }

    fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let dest_path = dst.join(entry.file_name());
            if ty.is_dir() {
                Self::copy_dir_recursive(&entry.path(), &dest_path)?;
            } else {
                fs::copy(&entry.path(), &dest_path)?;
            }
        }
        Ok(())
    }

    /// Load a project from a directory (shared between dialog and recovery).
    fn load_project_from(&mut self, dir: &PathBuf) {
        match jamhub_engine::load_project(dir) {
            Ok((project, buffers)) => {
                for (id, samples) in &buffers {
                    self.waveform_cache.insert(*id, samples);
                    self.send_command(EngineCommand::LoadAudioBuffer {
                        id: *id,
                        samples: samples.clone(),
                    });
                }
                self.audio_buffers = buffers;
                self.project = project;
                self.project_path = Some(dir.clone());
                self.dirty = false;
                self.sync_project();
                add_to_recent_projects(&mut self.recent_projects, dir);
                self.set_status(&format!("Loaded: {}", dir.display()));
            }
            Err(e) => self.set_status(&format!("Load failed: {e}")),
        }
    }

    /// Clean up autosave file for the current project after a successful save.
    fn cleanup_autosave(&self) {
        if let Some(ref path) = self.project_path {
            let mut autosave_path = path.as_os_str().to_owned();
            autosave_path.push(".autosave");
            let autosave = PathBuf::from(autosave_path);
            if autosave.exists() {
                let _ = fs::remove_dir_all(&autosave);
            }
        }
    }

    /// Snap a sample position according to the current snap mode.
    pub fn snap_position(&self, sample: u64) -> u64 {
        let sr = self.sample_rate() as f64;
        let spb = self.project.tempo.samples_per_beat(sr);
        match self.snap_mode {
            SnapMode::Off => sample,
            SnapMode::HalfBeat => {
                let half = spb / 2.0;
                let n = (sample as f64 / half).round();
                (n * half) as u64
            }
            SnapMode::Triplet => {
                let third = spb / 3.0;
                let n = (sample as f64 / third).round();
                (n * third) as u64
            }
            SnapMode::Beat => {
                let n = (sample as f64 / spb).round();
                (n * spb) as u64
            }
            SnapMode::Sixteenth => {
                let sixteenth = spb / 4.0;
                let n = (sample as f64 / sixteenth).round();
                (n * sixteenth) as u64
            }
            SnapMode::ThirtySecond => {
                let thirty_second = spb / 8.0;
                let n = (sample as f64 / thirty_second).round();
                (n * thirty_second) as u64
            }
            SnapMode::Bar => {
                let spbar = spb * self.project.time_signature.numerator as f64;
                let n = (sample as f64 / spbar).round();
                (n * spbar) as u64
            }
            SnapMode::Marker => {
                if self.project.markers.is_empty() {
                    let n = (sample as f64 / spb).round();
                    (n * spb) as u64
                } else {
                    let mut best = self.project.markers[0].sample;
                    let mut best_dist = (sample as i64 - best as i64).unsigned_abs();
                    for marker in &self.project.markers {
                        let dist = (sample as i64 - marker.sample as i64).unsigned_abs();
                        if dist < best_dist {
                            best = marker.sample;
                            best_dist = dist;
                        }
                    }
                    best
                }
            }
        }
    }

    /// Magnetic snap: only snaps when within a pixel threshold.
    /// Returns (snapped_sample, did_snap).
    /// Detect BPM from an audio clip using basic onset/peak detection.
    /// Returns Some(bpm) on success, None if detection fails.
    pub fn detect_clip_tempo(&self, track_idx: usize, clip_idx: usize) -> Option<f64> {
        let clip = self.project.tracks.get(track_idx)?.clips.get(clip_idx)?;
        let buffer_id = match &clip.source {
            ClipSource::AudioBuffer { buffer_id } => *buffer_id,
            _ => return None,
        };
        let samples = self.audio_buffers.get(&buffer_id)?;
        if samples.len() < 1024 {
            return None;
        }

        let sr = self.sample_rate() as f64;

        // Compute energy envelope with a hop size of ~10ms
        let hop = (sr * 0.01) as usize;
        if hop == 0 {
            return None;
        }
        let num_frames = samples.len() / hop;
        if num_frames < 4 {
            return None;
        }

        let mut energy: Vec<f64> = Vec::with_capacity(num_frames);
        for i in 0..num_frames {
            let start = i * hop;
            let end = (start + hop).min(samples.len());
            let e: f64 = samples[start..end].iter().map(|&s| (s as f64) * (s as f64)).sum();
            energy.push(e);
        }

        // Compute onset detection function (spectral flux approximation: diff of energy)
        let mut onset: Vec<f64> = vec![0.0];
        for i in 1..energy.len() {
            let diff = (energy[i] - energy[i - 1]).max(0.0);
            onset.push(diff);
        }

        // Find peaks in onset function (local maxima above threshold)
        let mean_onset: f64 = onset.iter().sum::<f64>() / onset.len() as f64;
        let threshold = mean_onset * 1.5;
        let mut peaks: Vec<usize> = Vec::new();
        for i in 1..onset.len().saturating_sub(1) {
            if onset[i] > threshold && onset[i] > onset[i - 1] && onset[i] >= onset[i + 1] {
                peaks.push(i);
            }
        }

        if peaks.len() < 3 {
            return None;
        }

        // Calculate intervals between consecutive peaks
        let mut intervals: Vec<f64> = Vec::new();
        for w in peaks.windows(2) {
            let interval_sec = (w[1] - w[0]) as f64 * hop as f64 / sr;
            intervals.push(interval_sec);
        }

        // Filter out extreme intervals (keep only 60-200 BPM range)
        let valid: Vec<f64> = intervals
            .into_iter()
            .filter(|&i| i > 0.3 && i < 1.0) // 60-200 BPM
            .collect();

        if valid.is_empty() {
            return None;
        }

        let avg_interval = valid.iter().sum::<f64>() / valid.len() as f64;
        let bpm = 60.0 / avg_interval;

        // Sanity check
        if bpm >= 40.0 && bpm <= 240.0 {
            Some(bpm)
        } else {
            None
        }
    }

    /// Snap a sample position to nearby clip edges on the same or adjacent tracks.
    /// Returns (snapped_sample, did_snap) similar to magnetic_snap.
    /// `drag_track_idx` is the track of the clip being dragged.
    pub fn snap_to_clip_edges(&self, sample: u64, drag_track_idx: usize, pixels_per_second: f32, threshold_px: f32) -> (u64, bool) {
        let sr = self.sample_rate() as f64;
        let threshold_samples = (threshold_px as f64 / pixels_per_second as f64 * sr) as u64;

        let mut best_dist: u64 = u64::MAX;
        let mut best_pos: u64 = sample;

        // Check clips on same track and adjacent tracks
        let track_range_start = drag_track_idx.saturating_sub(1);
        let track_range_end = (drag_track_idx + 2).min(self.project.tracks.len());

        for ti in track_range_start..track_range_end {
            for clip in &self.project.tracks[ti].clips {
                let clip_start = clip.start_sample;
                let clip_end = clip.start_sample + clip.visual_duration_samples();

                // Check distance to clip start
                let dist_start = (sample as i64 - clip_start as i64).unsigned_abs();
                if dist_start < best_dist && dist_start <= threshold_samples {
                    best_dist = dist_start;
                    best_pos = clip_start;
                }

                // Check distance to clip end
                let dist_end = (sample as i64 - clip_end as i64).unsigned_abs();
                if dist_end < best_dist && dist_end <= threshold_samples {
                    best_dist = dist_end;
                    best_pos = clip_end;
                }
            }
        }

        if best_dist <= threshold_samples && best_dist < u64::MAX {
            (best_pos, true)
        } else {
            (sample, false)
        }
    }

    pub fn magnetic_snap(&self, sample: u64, pixels_per_second: f32, threshold_px: f32) -> (u64, bool) {
        if self.snap_mode == SnapMode::Off {
            return (sample, false);
        }
        let snapped = self.snap_position(sample);
        let sr = self.sample_rate() as f64;
        let dist_samples = (sample as i64 - snapped as i64).unsigned_abs();
        let dist_seconds = dist_samples as f64 / sr;
        let dist_px = dist_seconds as f32 * pixels_per_second;
        if dist_px <= threshold_px {
            (snapped, true)
        } else {
            (sample, false)
        }
    }
}

/// Find the nearest zero crossing in an audio buffer near a given position.
/// Searches within search_range samples in both directions from position.
/// Draw the ThroughWaves waveform logo icon at the given position.
/// `size` is the side length of the square icon area.
/// 3D effect with gradient, glass highlight, cylindrical bars, and specular dots.
pub fn draw_waveform_logo(painter: &egui::Painter, center: egui::Pos2, size: f32, bg_color: egui::Color32, bar_color: egui::Color32) {
    let half = size / 2.0;
    let r = size * 0.22;

    // Drop shadow
    let shadow_rect = egui::Rect::from_center_size(
        egui::pos2(center.x + size * 0.02, center.y + size * 0.03),
        egui::vec2(size, size),
    );
    painter.rect_filled(shadow_rect, r, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 40));

    // Background rounded rectangle with gradient feel
    let icon_rect = egui::Rect::from_center_size(center, egui::vec2(size, size));
    painter.rect_filled(icon_rect, r, bg_color);

    // Inner bevel: bright top edge
    let top_highlight = egui::Rect::from_min_max(
        icon_rect.left_top(),
        egui::pos2(icon_rect.right(), icon_rect.top() + size * 0.08),
    );
    painter.rect_filled(top_highlight, r, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 50));

    // Glass highlight ellipse (top half, faded)
    let hl_rect = egui::Rect::from_min_max(
        egui::pos2(icon_rect.left() + size * 0.1, icon_rect.top() + size * 0.05),
        egui::pos2(icon_rect.right() - size * 0.1, icon_rect.center().y - size * 0.05),
    );
    painter.rect_filled(hl_rect, size * 0.15, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 30));

    // 5 waveform bars with 3D cylindrical look
    let bar_heights: [f32; 5] = [0.30, 0.65, 0.50, 0.80, 0.22];
    let bar_w = size * 0.09;
    let spacing = size * 0.155;
    let start_x = center.x - 2.0 * spacing;

    for (i, &h_frac) in bar_heights.iter().enumerate() {
        let x = start_x + i as f32 * spacing;
        let bar_h = half * h_frac * 1.6;
        let top = center.y - bar_h / 2.0;
        let bot = center.y + bar_h / 2.0;

        // Main bar (dark)
        painter.line_segment(
            [egui::pos2(x, top), egui::pos2(x, bot)],
            egui::Stroke::new(bar_w, bar_color),
        );

        // Left highlight (specular, thinner, brighter)
        let hl_x = x - bar_w * 0.25;
        painter.line_segment(
            [egui::pos2(hl_x, top + bar_w * 0.3), egui::pos2(hl_x, bot - bar_w * 0.3)],
            egui::Stroke::new(bar_w * 0.15, egui::Color32::from_rgba_unmultiplied(255, 250, 220, 80)),
        );
    }

    // Specular sparkle dots
    if size > 20.0 {
        let sparkle_r = (size * 0.015).max(0.5);
        let sparkles = [
            (center.x - size * 0.22, center.y - size * 0.28, 120u8),
            (center.x + size * 0.18, center.y - size * 0.22, 70),
            (center.x - size * 0.08, center.y - size * 0.15, 50),
        ];
        for (sx, sy, alpha) in sparkles {
            painter.circle_filled(egui::pos2(sx, sy), sparkle_r, egui::Color32::from_rgba_unmultiplied(255, 252, 230, alpha));
        }
    }
}

/// Returns the adjusted position snapped to the nearest zero crossing, or
/// the original position if no crossing is found.
pub fn find_nearest_zero_crossing(samples: &[f32], position: usize, search_range: usize) -> usize {
    if samples.is_empty() || position >= samples.len() {
        return position;
    }

    let start = position.saturating_sub(search_range);
    let end = (position + search_range).min(samples.len().saturating_sub(1));

    let mut best_pos = position;
    let mut best_dist = search_range + 1;

    for i in start..end {
        if i + 1 < samples.len() {
            let s0 = samples[i];
            let s1 = samples[i + 1];
            // Detect sign change (zero crossing)
            if (s0 >= 0.0 && s1 < 0.0) || (s0 < 0.0 && s1 >= 0.0) {
                let cross_pos = if s0.abs() <= s1.abs() { i } else { i + 1 };
                let dist = (cross_pos as i64 - position as i64).unsigned_abs() as usize;
                if dist < best_dist {
                    best_dist = dist;
                    best_pos = cross_pos;
                }
            }
        }
    }

    best_pos
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Reset per-frame flags
        self.piano_roll_consumed_delete = false;

        // Update window title with project name, branch, and dirty indicator
        let dirty_mark = if self.dirty { " *" } else { "" };
        let branch_label = if self.project.current_branch == "main" {
            String::new()
        } else {
            format!(" [{}]", self.project.current_branch)
        };
        let title = format!("{}{branch_label}{dirty_mark} — ThroughWaves", self.project.name);

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // Smart repaint scheduling: full-speed when active, 10fps when idle
        let is_playing = self.transport_state() == TransportState::Playing;
        let has_plugin_editors = !self.plugin_windows.windows.is_empty() || !self.builtin_fx_open.is_empty();
        if is_playing || self.is_recording || has_plugin_editors {
            ctx.request_repaint();
        } else {
            // Idle: 10fps refresh for meters, status messages, etc.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Beat flash: detect beat changes and trigger visual flash
        if is_playing {
            let pos = self.position_samples();
            let sr = self.sample_rate() as f64;
            let beat = self.project.tempo.beat_at_sample(pos, sr);
            let beat_num = beat.floor() as u64;
            if beat_num != self.last_beat_num {
                self.last_beat_num = beat_num;
                self.beat_flash = 1.0;
            }
            // Decay the flash
            if self.beat_flash > 0.0 {
                self.beat_flash = (self.beat_flash - 0.08).max(0.0);
            }
        } else {
            self.beat_flash = 0.0;
        }

        // Auto-save check: use preferences interval (0 = disabled)
        let autosave_interval = self.preferences.autosave_interval_secs;
        if self.autosave_enabled && self.dirty && autosave_interval > 0 && self.last_autosave.elapsed().as_secs() >= autosave_interval {
            self.perform_autosave();
        }

        // Periodic layout persistence (every 30 seconds)
        if self.last_layout_save.elapsed().as_secs() >= 30 {
            save_layout(self);
            self.last_layout_save = std::time::Instant::now();
        }

        // Live waveform update during recording (every 100ms)

        // Process MIDI CC for learn/mapping and macro updates
        midi_mapping::process_midi_cc(self);

        if self.is_recording && self.live_rec_last_update.elapsed().as_millis() > 100 {
            self.live_rec_last_update = std::time::Instant::now();
            let live_ids = self.live_rec_buffer_ids.clone();
            if !live_ids.is_empty() {
                let (samples, rec_sr) = self.recorder.peek_buffer();
                if !samples.is_empty() {
                    let engine_sr = self.sample_rate();
                    let display_samples = if rec_sr != engine_sr {
                        jamhub_engine::resample(&samples, rec_sr, engine_sr)
                    } else {
                        samples
                    };

                    let duration = display_samples.len() as u64;
                    // Update waveform and clip duration for all armed tracks
                    for &(track_idx, live_id) in &live_ids {
                        self.waveform_cache.insert(live_id, &display_samples);
                        if track_idx < self.project.tracks.len() {
                            for clip in &mut self.project.tracks[track_idx].clips {
                                if let ClipSource::AudioBuffer { buffer_id } = &clip.source {
                                    if *buffer_id == live_id {
                                        clip.duration_samples = duration;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Count-in tick: track beats elapsed and transition to actual recording
        if let Some(beats_remaining) = self.count_in_beats_remaining {
            let sr = self.sample_rate() as f64;
            let samples_per_beat = self.project.tempo.samples_per_beat(sr) as u64;
            let total_beats = self.project.time_signature.numerator as u32;
            let pos = self.position_samples();
            let beats_elapsed = (pos / samples_per_beat) as u32;

            if beats_elapsed >= total_beats {
                // Count-in finished — stop engine, start actual recording
                self.send_command(EngineCommand::Stop);
                self.count_in_beats_remaining = None;

                // Restore position to where recording should start
                self.send_command(EngineCommand::SetPosition(self.recording_start_pos));

                let track_idx = self.selected_track.unwrap_or(0);
                self.start_actual_recording(track_idx);
            } else {
                let new_remaining = total_beats - beats_elapsed;
                if new_remaining != beats_remaining {
                    self.count_in_beats_remaining = Some(new_remaining);
                    self.set_status(&format!("Count-in: {}...", new_remaining));
                }
            }
        }

        // Punch-out: auto-stop recording when playhead passes punch end
        if self.is_recording && self.punch_recording && self.count_in_beats_remaining.is_none() {
            if let (Some(sel_s), Some(sel_e)) = (self.selection_start, self.selection_end) {
                let punch_end = sel_s.max(sel_e);
                let pos = self.position_samples();
                if pos >= punch_end {
                    self.toggle_recording(); // stop recording
                }
            }
        }

        // Handle dropped files
        let mut files_to_import: Vec<PathBuf> = Vec::new();
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
                if let Some(path) = &file.path {
                    if let Some(ext) = path.extension() {
                        let ext = ext.to_string_lossy().to_lowercase();
                        if matches!(ext.as_str(), "wav" | "wave" | "mp3" | "ogg" | "flac") {
                            files_to_import.push(path.clone());
                        }
                    }
                }
            }
        });
        for path in files_to_import {
            self.import_audio_file(path);
        }

        // Keyboard shortcuts — skip when a text field has focus
        let _text_has_focus = ctx.memory(|m| m.focused().is_some())
            && ctx.input(|i| !i.raw.events.is_empty());
        // More reliable: check if any text edit is active
        let any_text_edit = self.renaming_track.is_some()
            || self.renaming_marker.is_some()
            || self.renaming_clip.is_some()
            || self.tempo_change_input.is_some()
            || self.speed_input.is_some()
            || self.region_name_input.is_some()
            || self.insert_silence_input.is_some()
            || self.template_name_input.is_some()
            || self.fx_preset_name_input.is_some()
            || (self.session.chat_input.len() > 0 && self.session.show_panel);
        // NOTE: ctx.wants_keyboard_input() must be called OUTSIDE ctx.input() to avoid deadlock
        let wants_kb = ctx.wants_keyboard_input();

        let mut actions: Vec<String> = Vec::new();
        ctx.input(|i| {
            // Track Ctrl key state for magnetic snap override
            self.ctrl_held = i.modifiers.ctrl;

            // Always allow Cmd shortcuts (they don't conflict with typing)
            // But skip single-key shortcuts when typing in a text field
            let typing = any_text_edit || wants_kb;

            // --- Single-key shortcuts (blocked when typing in text fields) ---
            if !typing {
                if i.key_pressed(egui::Key::Space) { actions.push("toggle_play".into()); }
                if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) { actions.push("delete".into()); }
                if i.key_pressed(egui::Key::Home) { actions.push("rewind".into()); }
                if i.key_pressed(egui::Key::R) && i.modifiers.shift && !i.modifiers.command { actions.push("toggle_ripple".into()); }
                else if i.key_pressed(egui::Key::R) && !i.modifiers.command { actions.push("record".into()); }
                if i.key_pressed(egui::Key::M) && !i.modifiers.command { actions.push("metronome".into()); }
                if i.key_pressed(egui::Key::L) && !i.modifiers.command { actions.push("toggle_loop".into()); }
                if i.key_pressed(egui::Key::T) && !i.modifiers.command { actions.push("toggle_takes".into()); }
                if i.key_pressed(egui::Key::G) && !i.modifiers.command { actions.push("cycle_snap".into()); }
                if i.key_pressed(egui::Key::S) && !i.modifiers.command { actions.push("split".into()); }
                if i.key_pressed(egui::Key::I) && !i.modifiers.command { actions.push("input_monitor".into()); }
                if i.key_pressed(egui::Key::A) && !i.modifiers.command { actions.push("toggle_automation".into()); }
                if i.key_pressed(egui::Key::Z) && !i.modifiers.command { actions.push("zoom_fit".into()); }
                if i.key_pressed(egui::Key::C) && !i.modifiers.command { actions.push("toggle_count_in".into()); }
                if i.key_pressed(egui::Key::P) && !i.modifiers.command { actions.push("toggle_punch".into()); }
                if i.key_pressed(egui::Key::Q) && !i.modifiers.command { actions.push("spectrum".into()); }
                if i.key_pressed(egui::Key::X) && !i.modifiers.command { actions.push("toggle_mixer_panel".into()); }
                if i.key_pressed(egui::Key::F) && i.modifiers.shift && !i.modifiers.command { actions.push("flatten_comp".into()); }
                if i.key_pressed(egui::Key::Tab) && !i.modifiers.command { actions.push("cycle_view".into()); }
                if i.key_pressed(egui::Key::Slash) && i.modifiers.shift { actions.push("show_shortcuts".into()); }
                if i.key_pressed(egui::Key::Escape) {
                    // Close open windows first, then clear selection
                    let mut closed_something = false;
                    if self.show_piano_roll { self.show_piano_roll = false; closed_something = true; }
                    else if self.show_effects { self.show_effects = false; closed_something = true; }
                    else if self.show_about { self.show_about = false; closed_something = true; }
                    else if self.show_shortcuts { self.show_shortcuts = false; closed_something = true; }
                    else if self.show_analysis { self.show_analysis = false; closed_something = true; }
                    else if self.show_project_info { self.show_project_info = false; closed_something = true; }
                    else if self.show_audio_pool { self.show_audio_pool = false; closed_something = true; }
                    else if self.show_midi_mappings { self.show_midi_mappings = false; closed_something = true; }
                    else if self.editing_clip.is_some() { self.editing_clip = None; closed_something = true; }
                    else if self.confirm_delete_track.is_some() { self.confirm_delete_track = None; closed_something = true; }
                    else if self.show_preferences { self.show_preferences = false; closed_something = true; }
                    else if self.show_template_picker { self.show_template_picker = false; closed_something = true; }
                    else if self.show_welcome { self.show_welcome = false; closed_something = true; }
                    if !closed_something {
                        actions.push("clear_selection".into());
                        actions.push("deselect_clips".into());
                    }
                }
                if i.key_pressed(egui::Key::F) && !i.modifiers.command && !i.modifiers.shift { actions.push("focus_playhead".into()); }
                if i.key_pressed(egui::Key::H) && !i.modifiers.command { actions.push("toggle_follow".into()); }
                if i.key_pressed(egui::Key::OpenBracket) && !i.modifiers.command { actions.push("prev_marker".into()); }
                if i.key_pressed(egui::Key::CloseBracket) && !i.modifiers.command { actions.push("next_marker".into()); }
                if i.modifiers.alt && i.key_pressed(egui::Key::ArrowUp) && !i.modifiers.command { actions.push("move_track_up".into()); }
                else if i.modifiers.alt && i.key_pressed(egui::Key::ArrowDown) && !i.modifiers.command { actions.push("move_track_down".into()); }
                else if i.key_pressed(egui::Key::ArrowUp) && !i.modifiers.command { actions.push("track_up".into()); }
                else if i.key_pressed(egui::Key::ArrowDown) && !i.modifiers.command { actions.push("track_down".into()); }
                if i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft) { actions.push("nudge_left".into()); }
                if i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight) { actions.push("nudge_right".into()); }
                for (idx, key) in [
                    egui::Key::Num1, egui::Key::Num2, egui::Key::Num3,
                    egui::Key::Num4, egui::Key::Num5, egui::Key::Num6,
                    egui::Key::Num7, egui::Key::Num8, egui::Key::Num9,
                ].iter().enumerate() {
                    if i.key_pressed(*key) && !i.modifiers.command {
                        if i.modifiers.ctrl {
                            // Ctrl+1-9: save locator
                            actions.push(format!("save_locator_{}", idx));
                        } else {
                            // 1-9: recall locator (or select track if no locator saved)
                            actions.push(format!("recall_locator_{}", idx));
                        }
                    }
                }
            }

            // --- Cmd+ shortcuts (always active, even when typing) ---
            if i.modifiers.command && i.key_pressed(egui::Key::Z) {
                if i.modifiers.shift { actions.push("redo".into()); }
                else { actions.push("undo".into()); }
            }
            if i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::S) { actions.push("save".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::D) { actions.push("duplicate".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::A) { actions.push("select_all_clips_all_tracks".into()); }
            else if i.modifiers.command && i.key_pressed(egui::Key::A) { actions.push("select_all_clips".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::E) { actions.push("toggle_global_fx_bypass".into()); }
            else if i.modifiers.command && i.key_pressed(egui::Key::E) { actions.push("effects".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::I) { actions.push("project_info".into()); }
            else if i.modifiers.command && i.key_pressed(egui::Key::I) { actions.push("import".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::P) { actions.push("audio_pool".into()); }
            else if i.modifiers.command && i.key_pressed(egui::Key::P) { actions.push("piano_roll".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::C) { actions.push("version_quick_commit".into()); }
            else if i.modifiers.command && i.key_pressed(egui::Key::C) { actions.push("copy".into()); }
            else if i.modifiers.command && i.key_pressed(egui::Key::V) { actions.push("paste".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::M) { actions.push("add_marker".into()); }
            else if i.modifiers.command && i.key_pressed(egui::Key::M) { actions.push("toggle_mute_selected".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::S) { actions.push("toggle_solo_selected".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::F) { actions.push("fx_browser".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::J) { actions.push("consolidate".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::U) { actions.push("toggle_platform".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::Comma) { actions.push("preferences".into()); }
            // Zoom presets: Cmd+1/2/3/4
            if i.modifiers.command && i.key_pressed(egui::Key::Num1) { actions.push("zoom_fit_all".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::Num2) { actions.push("zoom_to_selection".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::Num3) { actions.push("zoom_one_bar".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::Num4) { actions.push("zoom_max".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::N) { actions.push("new_session".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::O) { actions.push("open_project".into()); }
        });

        for action in &actions {
            match action.as_str() {
                "toggle_play" => {
                    if self.transport_state() == TransportState::Playing {
                        // Stop recording if active
                        if self.is_recording {
                            self.toggle_recording();
                        }
                        self.send_command(EngineCommand::Stop);
                        // Return playhead to where playback started
                        self.send_command(EngineCommand::SetPosition(self.play_start_position));
                    } else {
                        // Save current position so we can return on stop
                        self.play_start_position = self.position_samples();
                        // If loop is enabled and playhead is outside the loop, jump to loop start
                        if self.loop_enabled && self.loop_end > self.loop_start {
                            let pos = self.position_samples();
                            if pos < self.loop_start || pos >= self.loop_end {
                                self.send_command(EngineCommand::SetPosition(self.loop_start));
                                self.play_start_position = self.loop_start;
                            }
                        }
                        self.send_command(EngineCommand::Play);
                    }
                }
                "undo" => self.undo(),
                "redo" => self.redo(),
                "save" => self.save_project(),
                "delete" => {
                    // If piano roll is open with selected notes, delete notes instead
                    if self.show_piano_roll && !self.piano_roll_state.selected_notes.is_empty() {
                        if let Some(ti) = self.selected_track {
                            piano_roll::delete_selected_public(self, ti);
                        }
                    } else if self.has_selected_clips() {
                        self.delete_selected_clips();
                    } else {
                        self.delete_selected_track();
                    }
                }
                "rewind" => {
                    self.send_command(EngineCommand::SetPosition(0));
                }
                "record" => {
                    self.toggle_recording();
                }
                "metronome" => {
                    self.metronome_enabled = !self.metronome_enabled;
                    self.send_command(EngineCommand::SetMetronome(self.metronome_enabled));
                }
                "toggle_count_in" => {
                    self.count_in_enabled = !self.count_in_enabled;
                    let state = if self.count_in_enabled { "ON" } else { "OFF" };
                    self.set_status(&format!("Count-in: {state}"));
                }
                "toggle_punch" => {
                    self.punch_recording = !self.punch_recording;
                    let state = if self.punch_recording { "ON" } else { "OFF" };
                    self.set_status(&format!("Punch In/Out: {state}"));
                }
                "duplicate_track" => {
                    self.duplicate_selected_track();
                }
                "duplicate" => {
                    // Cmd+D: duplicate selected clips, or track if none selected
                    if self.has_selected_clips() {
                        self.duplicate_selected_clips();
                    } else {
                        self.duplicate_selected_track();
                    }
                }
                "deselect_clips" => {
                    self.selected_clips.clear();
                }
                "select_all_clips" => {
                    // Cmd+A: select all clips on the selected track
                    if let Some(ti) = self.selected_track {
                        if ti < self.project.tracks.len() {
                            self.selected_clips.clear();
                            for ci in 0..self.project.tracks[ti].clips.len() {
                                self.selected_clips.insert((ti, ci));
                            }
                            let count = self.project.tracks[ti].clips.len();
                            self.set_status(&format!("Selected {} clip(s)", count));
                        }
                    }
                }
                "select_all_clips_all_tracks" => {
                    // Cmd+Shift+A: select all clips on ALL tracks
                    self.selected_clips.clear();
                    let mut total = 0;
                    for ti in 0..self.project.tracks.len() {
                        for ci in 0..self.project.tracks[ti].clips.len() {
                            self.selected_clips.insert((ti, ci));
                            total += 1;
                        }
                    }
                    self.set_status(&format!("Selected all {} clip(s) on all tracks", total));
                }
                "insert_silence" => {
                    self.insert_silence_input = Some(InsertSilenceInput::default());
                }
                "remove_time" => {
                    self.remove_time_selection();
                }
                "crop_to_selection" => {
                    self.crop_to_selection();
                }
                "track_up" => {
                    if let Some(idx) = self.selected_track {
                        if idx > 0 {
                            self.selected_track = Some(idx - 1);
                            self.selected_clips.clear();
                        }
                    }
                }
                "track_down" => {
                    if let Some(idx) = self.selected_track {
                        if idx + 1 < self.project.tracks.len() {
                            self.selected_track = Some(idx + 1);
                            self.selected_clips.clear();
                        }
                    }
                }
                "toggle_loop" => {
                    self.loop_enabled = !self.loop_enabled;
                    if self.loop_enabled {
                        self.set_status("Loop ON");
                    } else {
                        self.set_status("Loop OFF");
                    }
                }
                "toggle_global_fx_bypass" => {
                    self.global_fx_bypass = !self.global_fx_bypass;
                    if self.global_fx_bypass {
                        // Save all current enabled states and disable everything
                        self.pre_bypass_states.clear();
                        for track in &self.project.tracks {
                            for slot in &track.effects {
                                self.pre_bypass_states.insert((track.id, slot.id), slot.enabled);
                            }
                            // Also save master effects
                        }
                        for slot in &self.project.master_effects {
                            self.pre_bypass_states.insert((Uuid::nil(), slot.id), slot.enabled);
                        }
                        // Disable all effects
                        for track in &mut self.project.tracks {
                            for slot in &mut track.effects {
                                slot.enabled = false;
                            }
                        }
                        for slot in &mut self.project.master_effects {
                            slot.enabled = false;
                        }
                        self.set_status("Global FX Bypass: ON");
                    } else {
                        // Restore original enabled states
                        for track in &mut self.project.tracks {
                            for slot in &mut track.effects {
                                if let Some(&original) = self.pre_bypass_states.get(&(track.id, slot.id)) {
                                    slot.enabled = original;
                                }
                            }
                        }
                        for slot in &mut self.project.master_effects {
                            if let Some(&original) = self.pre_bypass_states.get(&(Uuid::nil(), slot.id)) {
                                slot.enabled = original;
                            }
                        }
                        self.pre_bypass_states.clear();
                        self.set_status("Global FX Bypass: OFF");
                    }
                    self.sync_project();
                }
                "effects" => {
                    self.show_effects = !self.show_effects;
                }
                "import" => {
                    self.open_import_dialog();
                }
                "project_info" => {
                    self.project_info_name_buf = self.project.name.clone();
                    self.project_info_notes_buf = self.project.notes.clone();
                    self.show_project_info = true;
                }
                "piano_roll" => {
                    self.show_piano_roll = !self.show_piano_roll;
                }
                "toggle_takes" => {
                    if let Some(ti) = self.selected_track {
                        if ti < self.project.tracks.len() {
                            self.project.tracks[ti].lanes_expanded =
                                !self.project.tracks[ti].lanes_expanded;
                            self.project.tracks[ti].custom_height = 0.0;
                        }
                    }
                }
                "flatten_comp" => {
                    if let Some(ti) = self.selected_track {
                        self.flatten_comp(ti);
                    }
                }
                "cycle_snap" => {
                    self.snap_mode = self.snap_mode.next();
                    self.set_status(&format!("Snap: {}", self.snap_mode.label()));
                }
                "split" => {
                    // If no clips are selected, split ALL tracks at playhead (Reaper-style)
                    if self.selected_clips.is_empty() {
                        self.split_all_tracks_at_playhead();
                    } else {
                        self.split_clip_at_playhead();
                    }
                }
                "input_monitor" => {
                    self.toggle_input_monitor();
                }
                "bounce" => {
                    self.bounce_selected_track();
                }
                "add_marker" => {
                    let pos = self.position_samples();
                    let marker_num = self.project.markers.len() + 1;
                    self.project.markers.push(jamhub_model::Marker {
                        id: Uuid::new_v4(),
                        name: format!("Marker {marker_num}"),
                        sample: pos,
                        color: [255, 200, 50],
                    });
                    self.set_status(&format!("Marker {} added", marker_num));
                }
                "prev_marker" => {
                    let pos = self.position_samples();
                    let mut sorted: Vec<&jamhub_model::Marker> = self.project.markers.iter().collect();
                    sorted.sort_by_key(|m| m.sample);
                    // Find the last marker before current position (with small threshold to avoid sticking)
                    let threshold = (self.sample_rate() as f64 * 0.05) as u64;
                    if let Some(m) = sorted.iter().rev().find(|m| m.sample + threshold < pos) {
                        self.send_command(EngineCommand::SetPosition(m.sample));
                        self.set_status(&format!("Jumped to: {}", m.name));
                    } else {
                        // Wrap to last marker
                        if let Some(m) = sorted.last() {
                            self.send_command(EngineCommand::SetPosition(m.sample));
                            self.set_status(&format!("Jumped to: {}", m.name));
                        }
                    }
                }
                "next_marker" => {
                    let pos = self.position_samples();
                    let mut sorted: Vec<&jamhub_model::Marker> = self.project.markers.iter().collect();
                    sorted.sort_by_key(|m| m.sample);
                    let threshold = (self.sample_rate() as f64 * 0.05) as u64;
                    if let Some(m) = sorted.iter().find(|m| m.sample > pos + threshold) {
                        self.send_command(EngineCommand::SetPosition(m.sample));
                        self.set_status(&format!("Jumped to: {}", m.name));
                    } else {
                        // Wrap to first marker
                        if let Some(m) = sorted.first() {
                            self.send_command(EngineCommand::SetPosition(m.sample));
                            self.set_status(&format!("Jumped to: {}", m.name));
                        }
                    }
                }
                "fx_browser" => {
                    self.fx_browser.show = !self.fx_browser.show;
                }
                "media_browser" => {
                    self.media_browser.show = !self.media_browser.show;
                }
                "spectrum" => {
                    self.spectrum_analyzer.show = !self.spectrum_analyzer.show;
                    if self.spectrum_analyzer.show {
                        self.set_status("Spectrum analyzer ON");
                    } else {
                        self.set_status("Spectrum analyzer OFF");
                    }
                }
                "toggle_mixer_panel" => {
                    self.show_mixer_panel = !self.show_mixer_panel;
                    if self.show_mixer_panel {
                        self.set_status("Mixer panel docked at bottom");
                    } else {
                        self.set_status("Mixer panel hidden");
                    }
                }
                "cycle_view" => {
                    self.view = match self.view {
                        View::Arrange => View::Mixer,
                        View::Mixer => View::Session,
                        View::Session => View::Arrange,
                    };
                    let label = match self.view {
                        View::Arrange => "Arrange",
                        View::Mixer => "Mixer",
                        View::Session => "Session",
                    };
                    self.set_status(&format!("View: {label}"));
                }
                "copy" => {
                    self.copy_selected_clips();
                }
                "paste" => {
                    self.paste_clips();
                }
                "zoom_fit" => {
                    self.zoom_to_selection_or_fit();
                }
                "zoom_fit_all" => {
                    self.zoom_to_fit();
                    self.set_status("Zoom: fit all content [Cmd+1]");
                }
                "zoom_to_selection" => {
                    self.zoom_to_selection_or_fit();
                    self.set_status("Zoom: selection [Cmd+2]");
                }
                "zoom_one_bar" => {
                    let sr = self.sample_rate() as f64;
                    let beats_per_bar = self.project.time_signature.numerator as f64;
                    let bar_duration_sec = beats_per_bar * 60.0 / self.project.tempo.bpm as f64;
                    let pps_base = 100.0_f32;
                    let target_zoom = 800.0 / (bar_duration_sec as f32 * pps_base);
                    self.zoom = target_zoom.clamp(0.1, 20.0);
                    let pos = self.position_samples();
                    let pos_sec = pos as f64 / sr;
                    let pps = pps_base * self.zoom;
                    self.scroll_x = (pos_sec as f32 * pps - 400.0).max(0.0);
                    self.set_status("Zoom: 1 bar per screen [Cmd+3]");
                }
                "zoom_max" => {
                    self.zoom = 20.0;
                    let sr = self.sample_rate() as f64;
                    let pos = self.position_samples();
                    let pos_sec = pos as f64 / sr;
                    let pps = 100.0 * self.zoom;
                    self.scroll_x = (pos_sec as f32 * pps - 400.0).max(0.0);
                    self.set_status("Zoom: maximum (sample level) [Cmd+4]");
                }
                "toggle_mute_selected" => {
                    if let Some(ti) = self.selected_track {
                        if ti < self.project.tracks.len() {
                            self.push_undo("Toggle mute");
                            self.project.tracks[ti].muted = !self.project.tracks[ti].muted;
                            let state = if self.project.tracks[ti].muted { "muted" } else { "unmuted" };
                            self.set_status(&format!("{} {}", self.project.tracks[ti].name, state));
                            self.sync_project();
                        }
                    }
                }
                "toggle_solo_selected" => {
                    if let Some(ti) = self.selected_track {
                        if ti < self.project.tracks.len() {
                            self.push_undo("Toggle solo");
                            self.project.tracks[ti].solo = !self.project.tracks[ti].solo;
                            let state = if self.project.tracks[ti].solo { "soloed" } else { "unsoloed" };
                            self.set_status(&format!("{} {}", self.project.tracks[ti].name, state));
                            self.sync_project();
                        }
                    }
                }
                "focus_playhead" => {
                    self.focus_playhead();
                }
                "toggle_follow" => {
                    self.follow_playhead = !self.follow_playhead;
                    let state = if self.follow_playhead { "ON" } else { "OFF" };
                    self.set_status(&format!("Follow playhead: {state}"));
                }
                "toggle_automation" => {
                    self.show_automation = !self.show_automation;
                    if self.show_automation {
                        self.set_status("Automation visible — click timeline to add points");
                    } else {
                        self.set_status("Automation hidden");
                    }
                }
                "clear_selection" => {
                    self.selection_start = None;
                    self.selection_end = None;
                    self.loop_enabled = false;
                    self.loop_start = 0;
                    self.loop_end = 0;
                    self.send_command(EngineCommand::SetLoop {
                        enabled: false,
                        start: 0,
                        end: 0,
                    });
                    self.set_status("Selection cleared");
                }
                "nudge_left" => {
                    if !self.selected_clips.is_empty() {
                        self.push_undo("Nudge clips");
                        let sr = self.sample_rate() as f64;
                        let nudge = match self.snap_mode {
                            SnapMode::Off => 1u64,
                            SnapMode::ThirtySecond => {
                                (self.project.tempo.samples_per_beat(sr) / 8.0) as u64
                            }
                            SnapMode::Sixteenth => {
                                (self.project.tempo.samples_per_beat(sr) / 4.0) as u64
                            }
                            SnapMode::Triplet => {
                                (self.project.tempo.samples_per_beat(sr) / 3.0) as u64
                            }
                            SnapMode::HalfBeat => {
                                (self.project.tempo.samples_per_beat(sr) / 2.0) as u64
                            }
                            SnapMode::Beat | SnapMode::Marker => {
                                self.project.tempo.samples_per_beat(sr) as u64
                            }
                            SnapMode::Bar => {
                                (self.project.tempo.samples_per_beat(sr)
                                    * self.project.time_signature.numerator as f64)
                                    as u64
                            }
                        };
                        let clips_snapshot: Vec<_> = self.selected_clips.iter().copied().collect();
                        for (ti, ci) in clips_snapshot {
                            if ti < self.project.tracks.len()
                                && ci < self.project.tracks[ti].clips.len()
                            {
                                let clip = &mut self.project.tracks[ti].clips[ci];
                                clip.start_sample = clip.start_sample.saturating_sub(nudge);
                            }
                        }
                        self.sync_project();
                    }
                }
                "nudge_right" => {
                    if !self.selected_clips.is_empty() {
                        self.push_undo("Nudge clips");
                        let sr = self.sample_rate() as f64;
                        let nudge = match self.snap_mode {
                            SnapMode::Off => 1u64,
                            SnapMode::ThirtySecond => {
                                (self.project.tempo.samples_per_beat(sr) / 8.0) as u64
                            }
                            SnapMode::Sixteenth => {
                                (self.project.tempo.samples_per_beat(sr) / 4.0) as u64
                            }
                            SnapMode::Triplet => {
                                (self.project.tempo.samples_per_beat(sr) / 3.0) as u64
                            }
                            SnapMode::HalfBeat => {
                                (self.project.tempo.samples_per_beat(sr) / 2.0) as u64
                            }
                            SnapMode::Beat | SnapMode::Marker => {
                                self.project.tempo.samples_per_beat(sr) as u64
                            }
                            SnapMode::Bar => {
                                (self.project.tempo.samples_per_beat(sr)
                                    * self.project.time_signature.numerator as f64)
                                    as u64
                            }
                        };
                        let clips_snapshot: Vec<_> = self.selected_clips.iter().copied().collect();
                        // In ripple mode, also shift subsequent clips on the same track
                        if self.ripple_mode {
                            let mut tracks_affected: HashSet<usize> = HashSet::new();
                            let mut moved_clip_ids: HashSet<Uuid> = HashSet::new();
                            for &(ti, ci) in &clips_snapshot {
                                if ti < self.project.tracks.len()
                                    && ci < self.project.tracks[ti].clips.len()
                                {
                                    tracks_affected.insert(ti);
                                    moved_clip_ids.insert(self.project.tracks[ti].clips[ci].id);
                                    self.project.tracks[ti].clips[ci].start_sample += nudge;
                                }
                            }
                            // Shift all subsequent unselected clips
                            for &ti in &tracks_affected {
                                let max_end = clips_snapshot.iter()
                                    .filter(|&&(t, _)| t == ti)
                                    .filter_map(|&(_, ci)| {
                                        if ci < self.project.tracks[ti].clips.len() {
                                            Some(self.project.tracks[ti].clips[ci].start_sample)
                                        } else { None }
                                    })
                                    .min()
                                    .unwrap_or(0);
                                for clip in &mut self.project.tracks[ti].clips {
                                    if !moved_clip_ids.contains(&clip.id) && clip.start_sample >= max_end {
                                        clip.start_sample += nudge;
                                    }
                                }
                            }
                        } else {
                            for (ti, ci) in clips_snapshot {
                                if ti < self.project.tracks.len()
                                    && ci < self.project.tracks[ti].clips.len()
                                {
                                    self.project.tracks[ti].clips[ci].start_sample += nudge;
                                }
                            }
                        }
                        self.sync_project();
                    }
                }
                a if a.starts_with("select_track_") => {
                    if let Ok(idx) = a[13..].parse::<usize>() {
                        if idx < self.project.tracks.len() {
                            self.selected_track = Some(idx);
                            self.selected_clips.clear();
                        }
                    }
                }
                a if a.starts_with("save_locator_") => {
                    if let Ok(idx) = a[13..].parse::<usize>() {
                        if idx < 9 {
                            let pos = self.position_samples();
                            self.locators[idx] = Some(pos);
                            self.set_status(&format!("Locator {} saved at playhead", idx + 1));
                        }
                    }
                }
                a if a.starts_with("recall_locator_") => {
                    if let Ok(idx) = a[15..].parse::<usize>() {
                        if idx < 9 {
                            if let Some(pos) = self.locators[idx] {
                                self.send_command(EngineCommand::SetPosition(pos));
                                self.set_status(&format!("Jumped to locator {}", idx + 1));
                            } else {
                                // No locator saved — select track instead
                                if idx < self.project.tracks.len() {
                                    self.selected_track = Some(idx);
                                    self.selected_clips.clear();
                                }
                            }
                        }
                    }
                }
                "show_shortcuts" => {
                    self.show_shortcuts = !self.show_shortcuts;
                }
                "audio_pool" => {
                    self.show_audio_pool = !self.show_audio_pool;
                }
                "freeze_track" => {
                    if let Some(ti) = self.selected_track {
                        if ti < self.project.tracks.len() {
                            if self.project.tracks[ti].frozen {
                                self.unfreeze_selected_track();
                            } else {
                                self.freeze_selected_track();
                            }
                        }
                    }
                }
                "bounce_selection" => {
                    self.bounce_selection_range();
                }
                "preferences" => {
                    self.show_preferences = !self.show_preferences;
                }
                "toggle_platform" => {
                    self.platform.show_panel = !self.platform.show_panel;
                }
                "new_session" => {
                    self.show_template_picker = true;
                }
                "open_project" => {
                    self.load_project_dialog();
                }
                "toggle_ripple" => {
                    self.ripple_mode = !self.ripple_mode;
                    let state = if self.ripple_mode { "ON" } else { "OFF" };
                    self.set_status(&format!("Ripple editing: {state}"));
                }
                "move_track_up" => {
                    self.move_selected_track_up();
                }
                "move_track_down" => {
                    self.move_selected_track_down();
                }
                "consolidate" => {
                    self.consolidate_selected_clips();
                }
                "version_panel" => {
                    // removed — versioning is now via Remote Push/Pull
                }
                "version_quick_commit" => {
                    self.version_quick_commit();
                }
                _ => {}
            }
        }

        // CPU usage estimate
        let frame_start = std::time::Instant::now();

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Session...         Cmd+N").clicked() {
                        self.show_template_picker = true;
                        ui.close_menu();
                    }
                    if ui.button("Open Project...        Cmd+O").clicked() {
                        ui.close_menu();
                        self.load_project_dialog();
                    }
                    if ui.button("Save Project           Cmd+S").clicked() {
                        ui.close_menu();
                        self.save_project();
                    }
                    ui.separator();
                    // Recent Projects submenu
                    let has_recent = !self.recent_projects.is_empty();
                    ui.add_enabled_ui(has_recent, |ui| {
                        ui.menu_button("Recent Projects", |ui| {
                            let mut load_path: Option<PathBuf> = None;
                            for rp in &self.recent_projects {
                                let label = rp.path.file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| rp.path.display().to_string());
                                if ui.button(&label).on_hover_text(rp.path.display().to_string()).clicked() {
                                    load_path = Some(rp.path.clone());
                                    ui.close_menu();
                                }
                            }
                            if let Some(path) = load_path {
                                self.load_project_from(&path);
                            }
                        });
                    });
                    ui.separator();
                    if ui.button("Import Audio...        Cmd+I").clicked() {
                        ui.close_menu();
                        self.open_import_dialog();
                    }
                    ui.separator();
                    ui.menu_button("Export Format", |ui| {
                        for fmt in ExportFormat::ALL {
                            if ui.selectable_label(self.export_format == fmt, fmt.label()).clicked() {
                                self.export_format = fmt;
                            }
                        }
                        ui.separator();
                        ui.label(egui::RichText::new("Bit Depth:").small().color(egui::Color32::GRAY));
                        for &bd in &[16u16, 24, 32] {
                            let label = if bd == 32 { "32-bit float".to_string() } else { format!("{bd}-bit") };
                            if ui.selectable_label(self.export_bit_depth == bd, label).clicked() {
                                self.export_bit_depth = bd;
                            }
                        }
                        ui.separator();
                        ui.label(egui::RichText::new("Sample Rate:").small().color(egui::Color32::GRAY));
                        let project_sr = self.sample_rate();
                        if ui.selectable_label(self.export_sample_rate == 0, format!("Project ({project_sr} Hz)")).clicked() {
                            self.export_sample_rate = 0;
                        }
                        for &sr in &[44100u32, 48000, 96000] {
                            if ui.selectable_label(self.export_sample_rate == sr, format!("{sr} Hz")).clicked() {
                                self.export_sample_rate = sr;
                            }
                        }
                        ui.separator();
                        ui.checkbox(&mut self.export_normalize, "Normalize");
                    });
                    if ui.button(format!("Export Mixdown ({})...", self.export_format.label())).clicked() {
                        ui.close_menu();
                        self.export_mixdown();
                    }
                    if ui.button(format!("Export Stems ({})...", self.export_format.label())).clicked() {
                        ui.close_menu();
                        self.export_stems();
                    }
                    ui.separator();
                    if ui.button("Audio Settings...").clicked() {
                        self.audio_settings.show = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    let undo_label = self
                        .undo_manager
                        .undo_label()
                        .map(|l| format!("Undo {l}              Cmd+Z"))
                        .unwrap_or_else(|| "Undo                   Cmd+Z".into());
                    if ui
                        .add_enabled(self.undo_manager.can_undo(), egui::Button::new(undo_label))
                        .clicked()
                    {
                        self.undo();
                        ui.close_menu();
                    }
                    let redo_label = self
                        .undo_manager
                        .redo_label()
                        .map(|l| format!("Redo {l}        Cmd+Shift+Z"))
                        .unwrap_or_else(|| "Redo             Cmd+Shift+Z".into());
                    if ui
                        .add_enabled(self.undo_manager.can_redo(), egui::Button::new(redo_label))
                        .clicked()
                    {
                        self.redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Delete                 Del").clicked() {
                        if self.has_selected_clips() {
                            self.delete_selected_clips();
                        } else {
                            self.delete_selected_track();
                        }
                        ui.close_menu();
                    }
                    if ui.button("Undo History...").clicked() {
                        self.show_undo_history = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Select All on Track    Cmd+A").clicked() {
                        if let Some(ti) = self.selected_track {
                            if ti < self.project.tracks.len() {
                                self.selected_clips.clear();
                                for ci in 0..self.project.tracks[ti].clips.len() {
                                    self.selected_clips.insert((ti, ci));
                                }
                                let count = self.project.tracks[ti].clips.len();
                                self.set_status(&format!("Selected {} clip(s)", count));
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Select All (All Tracks) Cmd+Shift+A").clicked() {
                        self.selected_clips.clear();
                        let mut total = 0;
                        for ti in 0..self.project.tracks.len() {
                            for ci in 0..self.project.tracks[ti].clips.len() {
                                self.selected_clips.insert((ti, ci));
                                total += 1;
                            }
                        }
                        self.set_status(&format!("Selected all {} clip(s)", total));
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("MIDI Mappings...").clicked() {
                        self.show_midi_mappings = !self.show_midi_mappings;
                        ui.close_menu();
                    }
                    if ui.button("Macro Controls...").clicked() {
                        self.show_macros = !self.show_macros;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Project Info...  Cmd+Shift+I").clicked() {
                        self.project_info_name_buf = self.project.name.clone();
                        self.project_info_notes_buf = self.project.notes.clone();
                        self.show_project_info = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Split at Playhead      S").clicked() {
                        if self.selected_clips.is_empty() {
                            self.split_all_tracks_at_playhead();
                        } else {
                            self.split_clip_at_playhead();
                        }
                        ui.close_menu();
                    }
                    if ui.button("Duplicate Track        Cmd+D").clicked() {
                        self.duplicate_selected_track();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Insert Silence at Playhead...").clicked() {
                        self.insert_silence_input = Some(InsertSilenceInput::default());
                        ui.close_menu();
                    }
                    if ui.add_enabled(
                        self.selection_start.is_some() && self.selection_end.is_some(),
                        egui::Button::new("Remove Time at Selection"),
                    ).clicked() {
                        self.remove_time_selection();
                        ui.close_menu();
                    }
                    if ui.add_enabled(
                        self.selection_start.is_some() && self.selection_end.is_some(),
                        egui::Button::new("Crop to Selection"),
                    ).clicked() {
                        self.crop_to_selection();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Audio Pool...          Cmd+Shift+P").clicked() {
                        self.show_audio_pool = !self.show_audio_pool;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Preferences...         Cmd+,").clicked() {
                        self.show_preferences = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Track", |ui| {
                    if ui.button("Add Audio Track").clicked() {
                        self.push_undo("Add track");
                        let n = self.project.tracks.len() + 1;
                        self.project
                            .add_track(&format!("Track {n}"), TrackKind::Audio);
                        self.sync_project();
                        ui.close_menu();
                    }
                    if ui.button("Add MIDI Track").clicked() {
                        self.push_undo("Add track");
                        let n = self.project.tracks.len() + 1;
                        self.project
                            .add_track(&format!("MIDI {n}"), TrackKind::Midi);
                        self.sync_project();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Add from Template...").clicked() {
                        self.show_track_template_picker = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Delete Selected Track").clicked() {
                        self.delete_selected_track();
                        ui.close_menu();
                    }
                    ui.separator();
                    let has_sel = self.selected_track.is_some() && !self.project.tracks.is_empty();
                    if ui.add_enabled(has_sel, egui::Button::new("Effects...")).clicked() {
                        self.show_effects = true;
                        ui.close_menu();
                    }
                    let is_midi = self.selected_track
                        .and_then(|i| self.project.tracks.get(i))
                        .map_or(false, |t| t.kind == jamhub_model::TrackKind::Midi);
                    if ui.add_enabled(is_midi, egui::Button::new("Piano Roll...    Cmd+P")).clicked() {
                        self.show_piano_roll = true;
                        ui.close_menu();
                    }
                    if ui.button("MIDI Input...").clicked() {
                        self.midi_panel.show = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Session", |ui| {
                    let connected = self.session.is_connected();
                    let label = if connected {
                        "Session Panel (connected)"
                    } else {
                        "Session Panel"
                    };
                    if ui.button(label).clicked() {
                        self.session.show_panel = !self.session.show_panel;
                        ui.close_menu();
                    }
                    ui.separator();
                    let jam_label = if self.jam.connected {
                        "Live Jam Session (connected)"
                    } else {
                        "Live Jam Session..."
                    };
                    if ui.button(jam_label).clicked() {
                        self.jam.show = !self.jam.show;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Remote", |ui| {
                    let label = if self.platform.logged_in {
                        if self.platform.remote_project_id.is_some() {
                            "Platform (connected)"
                        } else {
                            "Platform (logged in)"
                        }
                    } else {
                        "Platform"
                    };
                    if ui.button(format!("{label}    Cmd+Shift+U")).clicked() {
                        self.platform.show_panel = !self.platform.show_panel;
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui
                        .selectable_label(self.view == View::Arrange, "Arrange")
                        .clicked()
                    {
                        self.view = View::Arrange;
                        ui.close_menu();
                    }
                    if ui
                        .selectable_label(self.show_mixer_panel, "Mixer Panel     X")
                        .on_hover_text("Dock mixer at bottom of arrange view")
                        .clicked()
                    {
                        self.show_mixer_panel = !self.show_mixer_panel;
                        ui.close_menu();
                    }
                    if ui
                        .selectable_label(self.view == View::Session, "Session        Tab")
                        .clicked()
                    {
                        self.view = View::Session;
                        ui.close_menu();
                    }
                    ui.separator();
                    let is_midi_track = self.selected_track
                        .and_then(|i| self.project.tracks.get(i))
                        .map_or(false, |t| t.kind == jamhub_model::TrackKind::Midi);
                    if ui.add_enabled(is_midi_track, egui::Button::new("Piano Roll       Cmd+P")).clicked() {
                        self.show_piano_roll = !self.show_piano_roll;
                        ui.close_menu();
                    }
                    let has_track = self.selected_track.is_some() && !self.project.tracks.is_empty();
                    if ui.add_enabled(has_track, egui::Button::new("Effects          Cmd+E")).clicked() {
                        self.show_effects = !self.show_effects;
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_track, egui::Button::new("Spectrum Analyzer    Q")).clicked() {
                        self.spectrum_analyzer.show = !self.spectrum_analyzer.show;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("Snap Mode:").small().color(egui::Color32::GRAY));
                    for mode in SnapMode::all() {
                        if ui.selectable_label(self.snap_mode == *mode, mode.label()).clicked() {
                            self.snap_mode = *mode;
                        }
                    }
                });
                ui.menu_button("Tools", |ui| {
                    if ui.button("Reference Track...").clicked() {
                        self.show_analysis = true;
                        ui.close_menu();
                    }
                    if ui.button("Correlation Meter").clicked() {
                        self.show_analysis = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Audio to MIDI").on_hover_text("Right-click an audio clip to convert it to MIDI").clicked() {
                        self.set_status("Right-click an audio clip and choose 'Convert to MIDI'");
                        ui.close_menu();
                    }
                    if ui.button("Detect Chords").on_hover_text("Right-click an audio clip to detect chords").clicked() {
                        self.set_status("Right-click an audio clip and choose 'Detect Chords'");
                        ui.close_menu();
                    }
                    ui.separator();
                    let lm_label = if self.loudness_match_enabled {
                        "Loudness Match ON"
                    } else {
                        "Loudness Match OFF"
                    };
                    if ui.button(lm_label).on_hover_text("Auto-compensate volume when bypassing effects").clicked() {
                        self.loudness_match_enabled = !self.loudness_match_enabled;
                        if !self.loudness_match_enabled {
                            self.loudness_compensation_db = 0.0;
                        }
                        ui.close_menu();
                    }
                });
                ui.menu_button("AI", |ui| {
                    if ui.button("Stem Separation...").clicked() {
                        self.stem_sep.show = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About ThroughWaves").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                    if ui.button("Keyboard Shortcuts    ?").clicked() {
                        self.show_shortcuts = true;
                        ui.close_menu();
                    }
                });
            });
        });

        // Separator line between menu and transport
        egui::TopBottomPanel::top("menu_transport_sep")
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(18, 18, 22)).inner_margin(0.0))
            .exact_height(1.0)
            .show(ctx, |_ui| {});

        // Transport bar — visually prominent with distinct background
        egui::TopBottomPanel::top("transport")
            .frame(
                egui::Frame::default()
                    .fill(egui::Color32::from_rgb(20, 22, 28))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 36, 44)))
            )
            .show(ctx, |ui| {
                transport_bar::show(self, ui);
            });

        // Separator line between transport and content
        egui::TopBottomPanel::top("transport_content_sep")
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(18, 18, 22)).inner_margin(0.0))
            .exact_height(1.0)
            .show(ctx, |_ui| {});

        // Macro controls panel (below transport)
        midi_mapping::show_macro_panel(self, ctx);

        // Status bar — premium, spacious, glass-effect with info pills
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::default()
                .fill(egui::Color32::from_rgb(17, 17, 21))
                .inner_margin(egui::Margin::symmetric(12, 5))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 33, 42)))
            )
            .exact_height(28.0)
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // ThroughWaves logo + wordmark at far left
                let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                draw_waveform_logo(
                    ui.painter(), icon_rect.center(), 18.0,
                    egui::Color32::from_rgb(235, 180, 60),
                    egui::Color32::from_rgb(20, 18, 14),
                );
                ui.label(
                    egui::RichText::new("ThroughWaves")
                        .size(10.0)
                        .strong()
                        .color(egui::Color32::from_rgb(240, 192, 64)),
                );
                ui.add_space(6.0);

                // Thin separator
                let (sep_rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(sep_rect, 0.0, egui::Color32::from_rgb(40, 40, 50));
                ui.add_space(6.0);

                // Bounce progress indicator
                if let Some(progress) = self.bounce_progress {
                    let pct = (progress * 100.0) as u32;
                    ui.label(egui::RichText::new(format!("Bouncing... {}%", pct))
                        .size(10.5).color(egui::Color32::from_rgb(100, 180, 255)));
                    let bar_width = 80.0;
                    let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(bar_width, 4.0), egui::Sense::hover());
                    ui.painter().rect_filled(bar_rect, 2.0, egui::Color32::from_rgb(36, 36, 46));
                    let filled = egui::Rect::from_min_size(bar_rect.min, egui::vec2(bar_width * progress, 4.0));
                    ui.painter().rect_filled(filled, 2.0, egui::Color32::from_rgb(80, 160, 255));
                }

                // Status message with fade-out animation (dims after 4 seconds)
                if let Some((msg, time)) = &self.status_message {
                    let elapsed = time.elapsed().as_secs_f32();
                    if elapsed < 7.0 {
                        let alpha = if elapsed < 4.0 {
                            1.0
                        } else {
                            1.0 - ((elapsed - 4.0) / 3.0).min(1.0)
                        };
                        let a = (190.0 * alpha) as u8;
                        ui.label(egui::RichText::new(msg).size(10.5).color(egui::Color32::from_rgba_premultiplied(180, 180, 195, a)));
                        if elapsed >= 4.0 && elapsed < 7.0 {
                            ui.ctx().request_repaint(); // animate fade
                        }
                    }
                }

                // Mode indicators inline (left side)
                if self.ripple_mode {
                    status_pill(ui, "RIPPLE", egui::Color32::from_rgb(255, 140, 60), true);
                }
                if self.show_automation {
                    status_pill(ui, "AUTO", egui::Color32::from_rgb(200, 170, 60), false);
                }
                if self.global_fx_bypass {
                    status_pill(ui, "FX OFF", egui::Color32::from_rgb(255, 80, 80), true);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;

                    // Memory pill — purple dot
                    let total_samples: usize = self.audio_buffers.values().map(|b| b.len()).sum();
                    let mem_mb = (total_samples * 4) as f64 / (1024.0 * 1024.0);
                    status_pill(ui, &format!("{mem_mb:.1}MB"), egui::Color32::from_rgb(160, 130, 200), false);

                    // CPU pill — color-coded dot (green/yellow/red)
                    let cpu_pct = self.cpu_usage * 100.0;
                    let cpu_color = if cpu_pct > 80.0 {
                        egui::Color32::from_rgb(255, 80, 80)
                    } else if cpu_pct > 50.0 {
                        egui::Color32::from_rgb(240, 192, 64)
                    } else {
                        egui::Color32::from_rgb(80, 200, 100)
                    };
                    status_pill(ui, &format!("CPU {cpu_pct:.0}%"), cpu_color, false);

                    // Snap pill
                    let snap_label = format!("Snap: {}", self.snap_mode.label());
                    let snap_color = if self.snap_mode != SnapMode::Off {
                        egui::Color32::from_rgb(100, 170, 255)
                    } else {
                        egui::Color32::from_rgb(90, 88, 98)
                    };
                    status_pill(ui, &snap_label, snap_color, self.snap_mode != SnapMode::Off);

                    // Tracks pill — green dot
                    status_pill(ui, &format!("{} tracks", self.project.tracks.len()), egui::Color32::from_rgb(80, 200, 130), false);

                    // Sample rate pill — blue dot
                    let sr = self.sample_rate();
                    status_pill(ui, &format!("{:.1}kHz", sr as f64 / 1000.0), egui::Color32::from_rgb(100, 160, 240), false);

                    // Grid pill
                    status_pill(ui, &format!("Grid: {}", self.grid_division.label()), egui::Color32::from_rgb(140, 130, 160), false);
                });
            });
        });

        // Update CPU usage estimate from frame timing
        {
            let elapsed = frame_start.elapsed().as_secs_f64();
            self.render_time_accum += elapsed;
            self.render_frame_count += 1;
            if self.render_frame_count >= 30 {
                let sr = self.sample_rate() as f64;
                let buffer_duration = 256.0 / sr;
                let avg_frame_time = self.render_time_accum / self.render_frame_count as f64;
                self.cpu_usage = (avg_frame_time / buffer_duration).min(1.0) as f32;
                self.render_time_accum = 0.0;
                self.render_frame_count = 0;
            }
        }

        if let Some(ref err) = self.engine_error {
            egui::TopBottomPanel::top("error").show(ctx, |ui| {
                ui.colored_label(egui::Color32::RED, format!("Engine error: {err}"));
            });
        }

        // Process network messages
        let net_messages = self.session.poll();
        for msg in net_messages {
            match msg {
                jamhub_network::message::SessionMessage::TrackAdded { track, .. } => {
                    self.project.tracks.push(track);
                    self.sync_project();
                }
                jamhub_network::message::SessionMessage::TrackUpdated {
                    track_id,
                    volume,
                    pan,
                    muted,
                    solo,
                    ..
                } => {
                    if let Some(track) =
                        self.project.tracks.iter_mut().find(|t| t.id == track_id)
                    {
                        if let Some(v) = volume {
                            track.volume = v;
                        }
                        if let Some(p) = pan {
                            track.pan = p;
                        }
                        if let Some(m) = muted {
                            track.muted = m;
                        }
                        if let Some(s) = solo {
                            track.solo = s;
                        }
                    }
                    self.sync_project();
                }
                jamhub_network::message::SessionMessage::TempoChange { tempo, .. } => {
                    self.project.tempo = tempo;
                    self.sync_project();
                }
                jamhub_network::message::SessionMessage::Welcome {
                    tracks,
                    tempo,
                    time_signature,
                    ..
                } => {
                    self.project.tracks = tracks;
                    self.project.tempo = tempo;
                    self.project.time_signature = time_signature;
                    self.sync_project();
                }
                _ => {}
            }
        }

        // Session panel (right side)
        session_panel::show(self, ctx);

        // Live jam session panel
        jam_session::show(self, ctx);

        // Platform integration panel
        platform_panel::show(self, ctx);

        // Floating panels
        effects_panel::show(self, ctx);
        piano_roll::show(self, ctx);
        fx_browser::show(self, ctx);
        // media_browser removed
        audio_settings::show(self, ctx);
        midi_panel::show(self, ctx);
        undo_panel::show(self, ctx);
        project_info::show(self, ctx);
        about::show(self, ctx);
        shortcuts_panel::show(self, ctx);
        spectrum::show(self, ctx);
        self.show_audio_pool_window(ctx);
        midi_mapping::show_mapping_manager(self, ctx);
        stem_separator::show(self, ctx);
        analysis_tools::show(self, ctx);
        // version_control panel removed — versioning via Remote Push/Pull

        // Template & preset dialogs
        templates::show_template_name_dialog(self, ctx);
        templates::show_fx_preset_name_dialog(self, ctx);
        templates::show_custom_color_dialog(self, ctx);
        self.show_track_template_picker_window(ctx);
        self.show_color_palette_popup(ctx);

        // ── Confirm Track Delete Dialog ──
        if let Some((tidx, ref tname)) = self.confirm_delete_track.clone() {
            let mut open = true;
            egui::Window::new("Delete Track")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.label(format!("Are you sure you want to delete \"{}\"?", tname));
                    ui.label(egui::RichText::new("This action can be undone with Cmd+Z.").size(11.0).color(egui::Color32::GRAY));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(
                            egui::RichText::new("Delete").color(egui::Color32::WHITE))
                            .fill(egui::Color32::from_rgb(200, 50, 50))
                            .min_size(egui::vec2(80.0, 28.0))
                        ).clicked() {
                            self.do_delete_track(tidx);
                            self.confirm_delete_track = None;
                        }
                        if ui.add(egui::Button::new("Cancel")
                            .min_size(egui::vec2(80.0, 28.0))
                        ).clicked() {
                            self.confirm_delete_track = None;
                        }
                    });
                });
            if !open {
                self.confirm_delete_track = None;
            }
        }

        // Cleanup closed plugin editor windows
        self.plugin_windows.cleanup_closed();

        // ── Template Picker Dialog ──────────────────────────────────────
        if self.show_template_picker {
            let mut tp_open = true;
            egui::Window::new("New Project \u{2014} Choose Template")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut tp_open)
                .min_width(400.0)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.label("Select a template for your new project:");
                    ui.add_space(8.0);
                    let mut chosen: Option<ProjectTemplate> = None;
                    for tpl in ProjectTemplate::ALL {
                        ui.horizontal(|ui| {
                            if ui.button(egui::RichText::new(tpl.label()).strong()).clicked() {
                                chosen = Some(tpl);
                            }
                            ui.label(egui::RichText::new(tpl.description()).weak());
                        });
                        ui.add_space(2.0);
                    }
                    if let Some(tpl) = chosen {
                        self.project = Project::default();
                        self.project.created_at = chrono::Local::now().to_rfc3339();
                        tpl.apply(&mut self.project);
                        self.audio_buffers.clear();
                        self.waveform_cache.clear();
                        self.undo_manager.clear();
                        self.project_path = None;
                        self.dirty = false;
                        self.selected_track = Some(0);
                        self.selected_clips.clear();
                        self.sync_project();
                        self.show_template_picker = false;
                        self.set_status(&format!("New project from template: {}", tpl.label()));
                    }
                });
            if !tp_open {
                self.show_template_picker = false;
            }
        }

        // ── User Preferences Window ──────────────────────────────────────
        if self.show_preferences {
            let mut pref_open = true;
            egui::Window::new("Preferences")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut pref_open)
                .min_width(380.0)
                .show(ctx, |ui| {
                    egui::Grid::new("prefs_grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Audio Buffer Size:");
                            egui::ComboBox::from_id_salt("pref_buffer")
                                .selected_text(format!("{}", self.preferences.audio_buffer_size))
                                .show_ui(ui, |ui| {
                                    for &sz in &[128u32, 256, 512, 1024] {
                                        ui.selectable_value(
                                            &mut self.preferences.audio_buffer_size,
                                            sz,
                                            format!("{sz} samples"),
                                        );
                                    }
                                });
                            ui.end_row();

                            ui.label("Default Template:");
                            egui::ComboBox::from_id_salt("pref_template")
                                .selected_text(self.preferences.default_template.label())
                                .show_ui(ui, |ui| {
                                    for tpl in ProjectTemplate::ALL {
                                        ui.selectable_value(
                                            &mut self.preferences.default_template,
                                            tpl,
                                            tpl.label(),
                                        );
                                    }
                                });
                            ui.end_row();

                            ui.label("Auto-save Interval:");
                            let autosave_label = if self.preferences.autosave_interval_secs == 0 {
                                "Disabled".to_string()
                            } else {
                                format!("{} min", self.preferences.autosave_interval_secs / 60)
                            };
                            egui::ComboBox::from_id_salt("pref_autosave")
                                .selected_text(autosave_label)
                                .show_ui(ui, |ui| {
                                    for &(secs, label) in &[
                                        (60u64, "1 minute"),
                                        (120, "2 minutes"),
                                        (300, "5 minutes"),
                                        (600, "10 minutes"),
                                        (0, "Disabled"),
                                    ] {
                                        ui.selectable_value(
                                            &mut self.preferences.autosave_interval_secs,
                                            secs,
                                            label,
                                        );
                                    }
                                });
                            ui.end_row();

                            ui.label("UI Scale:");
                            egui::ComboBox::from_id_salt("pref_scale")
                                .selected_text(format!("{}x", self.preferences.ui_scale))
                                .show_ui(ui, |ui| {
                                    for &s in &[0.8f32, 1.0, 1.2, 1.5] {
                                        ui.selectable_value(
                                            &mut self.preferences.ui_scale,
                                            s,
                                            format!("{s}x"),
                                        );
                                    }
                                });
                            ui.end_row();

                            ui.label("Theme:");
                            egui::ComboBox::from_id_salt("pref_theme")
                                .selected_text(self.preferences.theme.label())
                                .show_ui(ui, |ui| {
                                    for t in ThemeChoice::ALL {
                                        ui.selectable_value(
                                            &mut self.preferences.theme,
                                            t,
                                            t.label(),
                                        );
                                    }
                                });
                            ui.end_row();
                        });

                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            save_preferences(&self.preferences);
                            ctx.set_pixels_per_point(self.preferences.ui_scale);
                            apply_theme(ctx, self.preferences.theme);
                            self.set_status("Preferences saved");
                            self.show_preferences = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.preferences = load_preferences();
                            self.show_preferences = false;
                        }
                    });
                });
            if !pref_open {
                self.preferences = load_preferences();
                self.show_preferences = false;
            }
        }

        // ── Welcome Screen ───────────────────────────────────────────────
        if self.show_welcome {
            let mut wel_open = true;
            egui::Window::new("Welcome to ThroughWaves")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut wel_open)
                .min_width(420.0)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        // Waveform logo
                        let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(52.0, 52.0), egui::Sense::hover());
                        draw_waveform_logo(
                            ui.painter(), icon_rect.center(), 52.0,
                            egui::Color32::from_rgb(235, 180, 60),
                            egui::Color32::from_rgb(20, 18, 14),
                        );
                        ui.add_space(6.0);
                        ui.heading(egui::RichText::new("ThroughWaves").size(28.0).strong());
                        ui.label(egui::RichText::new("Professional DAW — Create, Mix, Collaborate").size(13.0).weak());
                        ui.add_space(16.0);
                    });

                    ui.horizontal(|ui| {
                        let btn_size = egui::vec2(160.0, 36.0);
                        if ui.add_sized(btn_size, egui::Button::new("New Project...")).clicked() {
                            self.show_welcome = false;
                            self.show_template_picker = true;
                        }
                        if ui.add_sized(btn_size, egui::Button::new("Open Project...")).clicked() {
                            self.show_welcome = false;
                            self.load_project_dialog();
                        }
                    });

                    if !self.recent_projects.is_empty() {
                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Recent Projects").strong());
                        ui.add_space(4.0);
                        let mut load_path: Option<PathBuf> = None;
                        for rp in &self.recent_projects {
                            let label = rp.path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| rp.path.display().to_string());
                            if ui.button(&label)
                                .on_hover_text(rp.path.display().to_string())
                                .clicked()
                            {
                                load_path = Some(rp.path.clone());
                            }
                        }
                        if let Some(path) = load_path {
                            self.load_project_from(&path);
                            self.show_welcome = false;
                        }
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(4.0);
                    let mut dont_show = self.preferences.dont_show_welcome;
                    if ui.checkbox(&mut dont_show, "Don't show this again").changed() {
                        self.preferences.dont_show_welcome = dont_show;
                        save_preferences(&self.preferences);
                    }
                });
            if !wel_open {
                self.show_welcome = false;
            }
        }

        // Autosave recovery dialog
        if self.show_autosave_recovery {
            let mut open = true;
            egui::Window::new("Recover Auto-saved Project?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    if let Some(ref path) = self.autosave_recovery_path.clone() {
                        ui.label(format!("An auto-saved project was found at:"));
                        ui.label(egui::RichText::new(path.display().to_string()).monospace().small());
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Recover").clicked() {
                                self.load_project_from(path);
                                self.show_autosave_recovery = false;
                                self.dirty = true; // Mark dirty since this is recovered, not saved
                            }
                            if ui.button("Discard").clicked() {
                                // Remove the autosave
                                if path.exists() {
                                    let _ = fs::remove_dir_all(path);
                                }
                                self.show_autosave_recovery = false;
                                self.autosave_recovery_path = None;
                            }
                        });
                    }
                });
            if !open {
                self.show_autosave_recovery = false;
            }
        }

        // ── Clip Properties Window ───────────────────────────────────────
        if let Some((ti, ci)) = self.editing_clip {
            let valid = ti < self.project.tracks.len()
                && ci < self.project.tracks[ti].clips.len();
            if valid {
                let mut clip_open = true;
                let clip_name = self.project.tracks[ti].clips[ci].name.clone();
                egui::Window::new(format!("Clip Properties \u{2014} {}", clip_name))
                    .id(egui::Id::new("clip_properties_panel"))
                    .collapsible(false)
                    .resizable(true)
                    .open(&mut clip_open)
                    .default_width(320.0)
                    .show(ctx, |ui| {
                        let sr = self.sample_rate() as f64;
                        let clip = &mut self.project.tracks[ti].clips[ci];

                        egui::Grid::new("clip_props_grid")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                ui.label("Name:");
                                let mut name_buf = clip.name.clone();
                                if ui.text_edit_singleline(&mut name_buf).changed() {
                                    clip.name = name_buf;
                                }
                                ui.end_row();

                                ui.label("Start:");
                                let mut start_sec = clip.start_sample as f64 / sr;
                                if ui.add(egui::DragValue::new(&mut start_sec)
                                    .speed(0.01).suffix(" s").range(0.0..=f64::MAX)).changed()
                                {
                                    clip.start_sample = (start_sec * sr) as u64;
                                }
                                ui.end_row();

                                ui.label("Duration:");
                                let mut dur_sec = clip.duration_samples as f64 / sr;
                                if ui.add(egui::DragValue::new(&mut dur_sec)
                                    .speed(0.01).suffix(" s").range(0.001..=f64::MAX)).changed()
                                {
                                    clip.duration_samples = (dur_sec * sr).max(1.0) as u64;
                                }
                                ui.end_row();

                                ui.label("Clip Gain:");
                                ui.add(egui::Slider::new(&mut clip.gain_db, -60.0..=60.0)
                                    .suffix(" dB").step_by(0.1).logarithmic(true));
                                ui.end_row();

                                ui.end_row();

                                ui.label("Speed:");
                                ui.horizontal(|ui| {
                                    ui.add(egui::Slider::new(&mut clip.playback_rate, 0.1..=4.0)
                                        .step_by(0.01).suffix("x"));
                                    if ui.small_button("1x").on_hover_text("Reset to normal speed").clicked() {
                                        clip.playback_rate = 1.0;
                                    }
                                });
                                ui.end_row();

                                ui.label("Transpose:");
                                ui.horizontal(|ui| {
                                    ui.add(egui::Slider::new(&mut clip.transpose_semitones, -24..=24)
                                        .suffix(" st"));
                                    if ui.small_button("0").on_hover_text("Reset transpose").clicked() {
                                        clip.transpose_semitones = 0;
                                    }
                                });
                                ui.end_row();

                                ui.label("Fade In:");
                                let mut fi_sec = clip.fade_in_samples as f64 / sr;
                                if ui.add(egui::DragValue::new(&mut fi_sec)
                                    .speed(0.001).suffix(" s").range(0.0..=f64::MAX)).changed()
                                {
                                    clip.fade_in_samples = (fi_sec * sr) as u64;
                                }
                                ui.end_row();

                                ui.label("Fade Out:");
                                let mut fo_sec = clip.fade_out_samples as f64 / sr;
                                if ui.add(egui::DragValue::new(&mut fo_sec)
                                    .speed(0.001).suffix(" s").range(0.0..=f64::MAX)).changed()
                                {
                                    clip.fade_out_samples = (fo_sec * sr) as u64;
                                }
                                ui.end_row();

                                ui.label("Loop:");
                                ui.add(egui::Slider::new(&mut clip.loop_count, 1..=32).suffix("x"));
                                ui.end_row();

                                ui.label("Reversed:");
                                ui.checkbox(&mut clip.reversed, "Reverse playback");
                                ui.end_row();

                                ui.label("Preserve Pitch:");
                                ui.checkbox(&mut clip.preserve_pitch, "Keep pitch when speed changes");
                                ui.end_row();

                                // Info label about independent controls
                                ui.label("");
                                ui.label(egui::RichText::new(
                                    "Speed changes duration. Transpose changes pitch.\nBoth are independent — like Ableton Warp."
                                ).size(10.0).color(egui::Color32::from_rgb(100, 100, 120)));
                                ui.end_row();
                            });

                        // Track volume (outside clip borrow)
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Track Volume:");
                            // Edit in dB space for symmetric control
                            let vol_lin = self.project.tracks[ti].volume;
                            let mut vol_db = if vol_lin > 0.0001 { 20.0 * vol_lin.log10() } else { -40.0 };
                            if ui.add(egui::Slider::new(&mut vol_db, -40.0..=40.0)
                                .suffix(" dB").step_by(0.1)
                            ).changed() {
                                self.project.tracks[ti].volume = 10.0_f32.powf(vol_db / 20.0);
                            }
                        });

                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Reset Gain").clicked() {
                                self.project.tracks[ti].clips[ci].gain_db = 0.0;
                            }
                            if ui.button("Reset Transpose").clicked() {
                                self.project.tracks[ti].clips[ci].transpose_semitones = 0;
                            }
                            if ui.button("Reset Speed").clicked() {
                                self.project.tracks[ti].clips[ci].playback_rate = 1.0;
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Maximize Volume")
                                .on_hover_text("Set clip gain to bring peak to 0 dB without clipping.\nAlso sets track volume to unity (100%).")
                                .clicked()
                            {
                                if let ClipSource::AudioBuffer { buffer_id } = &self.project.tracks[ti].clips[ci].source {
                                    if let Some(buf) = self.audio_buffers.get(buffer_id) {
                                        let peak = buf.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
                                        if peak > 0.0001 {
                                            let gain_needed = 20.0 * (1.0 / peak).log10();
                                            // No cap — apply exactly what's needed to reach 0 dB peak
                                            self.project.tracks[ti].clips[ci].gain_db = gain_needed;
                                            // Reset track volume to unity so the knob reflects the maximized state
                                            self.project.tracks[ti].volume = 1.0;
                                            self.set_status(&format!("Volume maximized: clip gain +{:.1} dB, track at 0 dB", gain_needed));
                                        } else {
                                            self.set_status("Clip is silent — cannot maximize");
                                        }
                                    }
                                } else {
                                    self.set_status("Maximize only works on audio clips");
                                }
                            }
                        });
                    });
                if !clip_open {
                    self.editing_clip = None;
                }
                self.sync_project();
            } else {
                self.editing_clip = None;
            }
        }

        // Main content
        // Docked mixer panel at bottom (Reaper-style) — only in Arrange view
        if self.show_mixer_panel && self.view == View::Arrange {
            egui::TopBottomPanel::bottom("mixer_dock")
                .resizable(true)
                .default_height(200.0)
                .min_height(120.0)
                .max_height(400.0)
                .frame(egui::Frame::default()
                    .fill(egui::Color32::from_rgb(18, 19, 24))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(35, 36, 44)))
                    .inner_margin(egui::Margin::same(0)))
                .show(ctx, |ui| {
                    mixer_view::show(self, ui);
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(17, 17, 20)))
            .show(ctx, |ui| match self.view {
            View::Arrange => timeline::show(self, ui),
            View::Mixer => mixer_view::show(self, ui),
            View::Session => session_view::show(self, ui, ctx),
        });
    }
}
