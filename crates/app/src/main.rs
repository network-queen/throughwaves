mod about;
mod audio_settings;
mod effects_panel;
mod fx_browser;
mod media_browser;
mod midi_mapping;
mod midi_panel;
mod mixer_view;
mod piano_roll;
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

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::fs;

use eframe::egui;
use jamhub_engine::{load_audio, EngineCommand, EngineHandle, ExportFormat, ExportOptions, InputMonitor, LevelMeters, Recorder, WaveformCache};
use jamhub_model::{Clip, ClipSource, Project, TrackKind, TransportState};
use uuid::Uuid;

use session_panel::SessionPanel;
use undo::UndoManager;

fn setup_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    // Warm modern dark theme — Ableton Live 12 clean meets Bitwig warmth
    let bg = egui::Color32::from_rgb(20, 20, 24);          // deepest background
    let panel_bg = egui::Color32::from_rgb(28, 29, 34);    // panel surfaces
    let widget_bg = egui::Color32::from_rgb(36, 37, 44);   // widget backgrounds
    let widget_hover = egui::Color32::from_rgb(48, 50, 60); // hover state
    let widget_active = egui::Color32::from_rgb(58, 60, 72); // active/pressed
    let accent = egui::Color32::from_rgb(235, 180, 60);    // warm amber/gold
    let selection = egui::Color32::from_rgb(80, 200, 190);  // soft teal
    let text = egui::Color32::from_rgb(230, 228, 224);     // warm white
    let text_dim = egui::Color32::from_rgb(145, 142, 138); // secondary text

    visuals.panel_fill = panel_bg;
    visuals.window_fill = egui::Color32::from_rgb(26, 27, 32);
    visuals.extreme_bg_color = bg;
    visuals.faint_bg_color = egui::Color32::from_rgb(32, 33, 38);

    // Widget styles — softer corners, warmer feel
    visuals.widgets.noninteractive.bg_fill = panel_bg;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text_dim);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.inactive.bg_fill = widget_bg;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(5);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;

    visuals.widgets.hovered.bg_fill = widget_hover;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(5);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent.gamma_multiply(0.5));
    visuals.widgets.hovered.expansion = 1.0; // subtle grow on hover

    visuals.widgets.active.bg_fill = widget_active;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(5);

    visuals.widgets.open.bg_fill = widget_hover;
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

    visuals.widgets.open.corner_radius = egui::CornerRadius::same(5);
    visuals.selection.bg_fill = selection.gamma_multiply(0.25);
    visuals.selection.stroke = egui::Stroke::new(1.5, selection);

    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 16,
        spread: 0,
        color: egui::Color32::from_black_alpha(100),
    };
    visuals.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(44, 45, 52));

    ctx.set_visuals(visuals);

    // Typography & spacing — generous, readable, modern
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(7.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.window_margin = egui::Margin::same(12);

    // Larger default font sizes
    use egui::FontId;
    use egui::TextStyle;
    style.text_styles.insert(TextStyle::Body, FontId::proportional(13.5));
    style.text_styles.insert(TextStyle::Heading, FontId::proportional(19.0));
    style.text_styles.insert(TextStyle::Button, FontId::proportional(13.5));
    style.text_styles.insert(TextStyle::Small, FontId::proportional(11.0));
    style.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));

    ctx.set_style(style);
}

fn apply_theme(ctx: &egui::Context, theme: ThemeChoice) {
    let mut visuals = egui::Visuals::dark();

    let (bg, panel_bg, widget_bg, widget_hover, widget_active, accent, text, text_dim, win_fill, win_stroke_col) = match theme {
        ThemeChoice::Dark => (
            egui::Color32::from_rgb(20, 20, 24),
            egui::Color32::from_rgb(28, 29, 34),
            egui::Color32::from_rgb(36, 37, 44),
            egui::Color32::from_rgb(48, 50, 60),
            egui::Color32::from_rgb(58, 60, 72),
            egui::Color32::from_rgb(235, 180, 60),
            egui::Color32::from_rgb(230, 228, 224),
            egui::Color32::from_rgb(145, 142, 138),
            egui::Color32::from_rgb(26, 27, 32),
            egui::Color32::from_rgb(44, 45, 52),
        ),
        ThemeChoice::Darker => (
            egui::Color32::from_rgb(14, 14, 18),
            egui::Color32::from_rgb(20, 21, 26),
            egui::Color32::from_rgb(30, 31, 38),
            egui::Color32::from_rgb(40, 42, 52),
            egui::Color32::from_rgb(50, 52, 64),
            egui::Color32::from_rgb(220, 165, 50),
            egui::Color32::from_rgb(220, 218, 214),
            egui::Color32::from_rgb(130, 128, 122),
            egui::Color32::from_rgb(18, 18, 24),
            egui::Color32::from_rgb(36, 37, 44),
        ),
        ThemeChoice::Midnight => (
            egui::Color32::from_rgb(8, 8, 14),
            egui::Color32::from_rgb(12, 13, 20),
            egui::Color32::from_rgb(22, 23, 34),
            egui::Color32::from_rgb(34, 36, 48),
            egui::Color32::from_rgb(44, 46, 60),
            egui::Color32::from_rgb(210, 160, 50),
            egui::Color32::from_rgb(200, 198, 195),
            egui::Color32::from_rgb(115, 112, 108),
            egui::Color32::from_rgb(14, 14, 24),
            egui::Color32::from_rgb(28, 30, 40),
        ),
    };

    visuals.panel_fill = panel_bg;
    visuals.window_fill = win_fill;
    visuals.extreme_bg_color = bg;
    visuals.faint_bg_color = egui::Color32::from_rgb(
        (panel_bg.r() as u16 + bg.r() as u16 / 2) as u8,
        (panel_bg.g() as u16 + bg.g() as u16 / 2) as u8,
        (panel_bg.b() as u16 + bg.b() as u16 / 2) as u8,
    );

    visuals.widgets.noninteractive.bg_fill = panel_bg;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text_dim);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.inactive.bg_fill = widget_bg;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(5);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;

    visuals.widgets.hovered.bg_fill = widget_hover;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(5);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent.gamma_multiply(0.5));

    visuals.widgets.active.bg_fill = widget_active;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(5);

    visuals.widgets.open.bg_fill = widget_hover;
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

    let teal = egui::Color32::from_rgb(80, 200, 190);
    visuals.selection.bg_fill = teal.gamma_multiply(0.25);
    visuals.selection.stroke = egui::Stroke::new(1.5, teal);

    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 16,
        spread: 0,
        color: egui::Color32::from_black_alpha(100),
    };
    visuals.window_stroke = egui::Stroke::new(1.0, win_stroke_col);

    ctx.set_visuals(visuals);
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("JamHub — Collaborative DAW"),
        ..Default::default()
    };

    eframe::run_native(
        "JamHub",
        options,
        Box::new(|cc| {
            setup_theme(&cc.egui_ctx);
            let app = DawApp::new();
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
    // Live recording waveform
    live_rec_buffer_id: Option<uuid::Uuid>,
    live_rec_last_update: std::time::Instant,
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
    pub builtin_fx_open: std::collections::HashSet<Uuid>,
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

fn config_dir() -> PathBuf {
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
                project.add_track("Track 2", TrackKind::Audio);
            }
            ProjectTemplate::SingerSongwriter => {
                project.add_track("Vocals", TrackKind::Audio);
                project.add_track("Guitar", TrackKind::Audio);
                project.add_track("Mix Bus", TrackKind::Bus);
            }
            ProjectTemplate::Band => {
                project.add_track("Drums", TrackKind::Audio);
                project.add_track("Bass", TrackKind::Audio);
                project.add_track("Guitar", TrackKind::Audio);
                project.add_track("Vocal", TrackKind::Audio);
                project.add_track("Mix Bus", TrackKind::Bus);
            }
            ProjectTemplate::Electronic => {
                project.add_track("Synth 1", TrackKind::Midi);
                project.add_track("Synth 2", TrackKind::Midi);
                project.add_track("Drums", TrackKind::Midi);
                project.add_track("Bass", TrackKind::Midi);
                project.add_track("Audio 1", TrackKind::Audio);
                project.add_track("Audio 2", TrackKind::Audio);
                project.add_track("Master Bus", TrackKind::Bus);
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
        project.add_track("Track 2", TrackKind::Audio);

        let sample_rate = engine.as_ref()
            .map(|e| e.state.read().sample_rate)
            .unwrap_or(44100);

        if let Some(ref eng) = engine {
            eng.send(EngineCommand::UpdateProject(project.clone()));
        }

        Self {
            project,
            engine_error: if engine.is_none() {
                Some("Failed to initialize audio engine".into())
            } else {
                None
            },
            engine,
            view: View::Arrange,
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
            live_rec_buffer_id: None,
            live_rec_last_update: std::time::Instant::now(),
            show_undo_history: false,
            clipboard_clips: Vec::new(),
            dirty: false,
            color_picker_track: None,
            midi_panel: midi_panel::MidiPanel::default(),
            plugin_windows: plugin_window::PluginWindowManager::default(),
            builtin_fx_open: std::collections::HashSet::new(),
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
        }
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
                    muted: false, content_offset: 0,
                    fade_in_samples: 0,
                    fade_out_samples: 0,
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

    pub fn toggle_recording(&mut self) {
        if self.is_recording {
            // === STOP RECORDING ===
            self.is_recording = false;

            // If we were in count-in phase, just cancel without saving
            if self.count_in_beats_remaining.is_some() {
                self.count_in_beats_remaining = None;
                self.send_command(EngineCommand::Stop);
                self.send_command(EngineCommand::SetMetronome(self.metronome_enabled));
                let track_idx = self.selected_track.unwrap_or(0);
                if track_idx < self.project.tracks.len() {
                    self.project.tracks[track_idx].muted = false;
                    self.project.tracks[track_idx].armed = false;
                    self.sync_project();
                }
                self.set_status("Count-in cancelled");
                return;
            }

            // 1. Stop the recorder FIRST to get captured audio
            let result = self.recorder.stop();

            // 2. Stop the engine AFTER getting recording data
            self.send_command(EngineCommand::Stop);

            // 3. Remove the live recording placeholder clip
            let track_idx = self.selected_track.unwrap_or(0);
            if let Some(live_id) = self.live_rec_buffer_id.take() {
                if track_idx < self.project.tracks.len() {
                    self.project.tracks[track_idx]
                        .clips
                        .retain(|c| {
                            if let ClipSource::AudioBuffer { buffer_id } = &c.source {
                                *buffer_id != live_id
                            } else {
                                true
                            }
                        });
                }
            }

            // Unmute the track we muted during recording, disarm it
            if track_idx < self.project.tracks.len() {
                self.project.tracks[track_idx].muted = false;
                self.project.tracks[track_idx].armed = false;
            }

            if result.samples.is_empty() {
                self.sync_project();
                self.set_status("Recording was empty");
                return;
            }

            if track_idx >= self.project.tracks.len() {
                return;
            }

            self.push_undo("Record audio");

            // 3. Resample to engine sample rate if needed
            let engine_sr = self.sample_rate();
            let samples = if result.sample_rate != engine_sr {
                println!(
                    "Resampling recording from {}Hz to {}Hz",
                    result.sample_rate, engine_sr
                );
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
                    // The recording started at pre-roll, but recording_start_pos
                    // was set to punch_start. Calculate offset into buffer where
                    // punch region audio begins (may be 0 if recorder started at punch start).
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
            let buffer_id = Uuid::new_v4();
            let rec_start = self.recording_start_pos;
            let duration = samples.len() as u64;

            println!(
                "Recording clip: start={}, duration={} ({:.2}s), buffer_len={}, engine_sr={}",
                rec_start,
                duration,
                duration as f64 / engine_sr as f64,
                samples.len(),
                engine_sr,
            );

            // Auto-mute older overlapping clips (takes behavior)
            for existing_clip in &mut self.project.tracks[track_idx].clips {
                let existing_end = existing_clip.start_sample + existing_clip.duration_samples;
                let new_end = rec_start + duration;
                // If clips overlap, mute the old one
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
                source: ClipSource::AudioBuffer { buffer_id },
                muted: false, content_offset: 0,
                fade_in_samples: 0,
                fade_out_samples: 0,
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

            // 5. Build waveform for display
            self.waveform_cache.insert(buffer_id, &samples);
            self.audio_buffers.insert(buffer_id, samples.clone());

            // 6. CRITICAL ORDER: Load buffer into engine FIRST, then update project.
            //    The engine processes commands in order from a single channel.
            //    If project arrives first, mixer would try to read a buffer that
            //    doesn't exist yet.
            self.send_command(EngineCommand::LoadAudioBuffer {
                id: buffer_id,
                samples,
            });

            // 7. Add clip to project and sync AFTER buffer is queued
            self.project.tracks[track_idx].clips.push(clip);
            self.sync_project();

            // 8. Scroll view to show the recorded clip
            self.scroll_x = 0.0; // Reset to start since clip starts at rec_start
            let _clip_end_sec = (rec_start + duration) as f64 / engine_sr as f64;
            // Ensure zoom shows the whole clip — adjust if clip doesn't fit in view
            let min_zoom = 0.3;
            if self.zoom < min_zoom {
                self.zoom = min_zoom;
            }

            // Rewind playhead to start of clip for immediate playback
            self.send_command(EngineCommand::SetPosition(rec_start));

            self.set_status(&format!(
                "Take {} saved ({:.1}s) — press Space to play",
                take_num,
                duration as f64 / engine_sr as f64
            ));
        } else {
            // === START RECORDING ===
            let track_idx = self.selected_track.unwrap_or(0);
            if track_idx < self.project.tracks.len() {
                self.project.tracks[track_idx].armed = true;
                // Mute this track while recording so old takes don't
                // play back through speakers (prevents feedback/confusion)
                self.project.tracks[track_idx].muted = true;
                self.sync_project();
            }

            // Pre-roll: if punch recording with a selection, start 1 bar before selection
            let mut start_pos = self.position_samples();
            if self.punch_recording {
                if let (Some(sel_s), Some(sel_e)) = (self.selection_start, self.selection_end) {
                    let punch_start = sel_s.min(sel_e);
                    let sr = self.sample_rate() as f64;
                    let beats_per_bar = self.project.time_signature.numerator as u64;
                    let samples_per_bar = self.project.tempo.samples_per_beat(sr) as u64
                        * beats_per_bar;
                    // Pre-roll: 1 bar before punch start
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
                self.live_rec_buffer_id = Some(live_id);
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
                    muted: false, content_offset: 0,
                    fade_in_samples: 0,
                    fade_out_samples: 0,
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
                self.set_status(&format!("Record failed: {e}"));
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

    /// Backward-compatible: check if any clips are selected
    pub fn has_selected_clips(&self) -> bool {
        !self.selected_clips.is_empty()
    }

    pub fn delete_selected_track(&mut self) {
        if let Some(track_idx) = self.selected_track {
            if track_idx < self.project.tracks.len() && self.project.tracks.len() > 1 {
                self.push_undo("Delete track");
                self.project.tracks.remove(track_idx);
                self.selected_track = Some(track_idx.min(self.project.tracks.len() - 1));
                self.selected_clips.clear();
                self.sync_project();
                self.set_status("Track deleted");
            }
        }
    }

    pub fn duplicate_selected_track(&mut self) {
        if let Some(track_idx) = self.selected_track {
            if track_idx < self.project.tracks.len() {
                self.push_undo("Duplicate track");
                let mut new_track = self.project.tracks[track_idx].clone();
                new_track.id = Uuid::new_v4();
                new_track.name = format!("{} (copy)", new_track.name);
                self.project.tracks.insert(track_idx + 1, new_track);
                self.selected_track = Some(track_idx + 1);
                self.sync_project();
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
                fade_out_samples: 0,
                color: clip_color,
                playback_rate: clip_rate,
                preserve_pitch: clip_preserve_pitch,
                loop_count: 1,
                gain_db: 0.0,
                take_index: 0,
                content_offset: 0,
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
        let track_idx = self.selected_track.unwrap_or(0);
        if track_idx >= self.project.tracks.len() {
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
                    muted: false, content_offset: 0,
                    fade_in_samples: 0,
                    fade_out_samples: 0,
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
                    muted: false, fade_in_samples: 0, fade_out_samples: 0,
                    color: None, playback_rate: 1.0, preserve_pitch: false, loop_count: 1, gain_db: 0.0, take_index: 0, content_offset: 0,
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
                    muted: false, fade_in_samples: 0, fade_out_samples: 0,
                    color: None, playback_rate: 1.0, preserve_pitch: false, loop_count: 1, gain_db: 0.0, take_index: 0, content_offset: 0,
                });
                self.sync_project();
                self.set_status(&format!("Selection bounced ({:.1}s)", duration as f64 / sr as f64));
            }
            Err(e) => self.set_status(&format!("Bounce selection failed: {e}")),
        }
        self.bounce_progress = None;
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
            match jamhub_engine::export_with_options(&path, &self.project, &self.audio_buffers, sr, &options) {
                Ok(()) => self.set_status(&format!("Exported {} {}-bit: {}", fmt.label(), self.export_bit_depth, path.display())),
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
            let result = jamhub_engine::export_stems(
                &dir,
                &self.project,
                &self.audio_buffers,
                sr,
                &options,
                |current, total| {
                    // Progress callback — in a real async implementation this would
                    // update the UI; here the export runs synchronously so we log.
                    eprintln!("Exporting stem {current}/{total}...");
                },
            );
            match result {
                Ok(res) => {
                    let count = res.stems.len();
                    self.set_status(&format!("Exported {count} stems to {}", dir.display()));
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
        let ti = *track_indices.iter().next().unwrap();
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
            muted: false, content_offset: 0,
            fade_in_samples: 0,
            fade_out_samples: 0,
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
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Save Project")
                .pick_folder()
            {
                let project_dir = path.join(&self.project.name);
                self.project_path = Some(project_dir.clone());
                project_dir
            } else {
                return;
            }
        };

        // Create backup of previous version before overwriting
        Self::backup_project(&dir);

        let sr = self.sample_rate();
        match jamhub_engine::save_project(&dir, &self.project, &self.audio_buffers, sr) {
            Ok(()) => {
                self.dirty = false;
                self.last_autosave = std::time::Instant::now();
                self.cleanup_autosave();
                add_to_recent_projects(&mut self.recent_projects, &dir);
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

        // Update window title with project name and dirty indicator
        let dirty_mark = if self.dirty { " *" } else { "" };
        let title = format!("{}{dirty_mark} — JamHub", self.project.name);

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        if self.transport_state() == TransportState::Playing || self.is_recording {
            ctx.request_repaint();
        }

        // Auto-save check: use preferences interval (0 = disabled)
        let autosave_interval = self.preferences.autosave_interval_secs;
        if self.autosave_enabled && self.dirty && autosave_interval > 0 && self.last_autosave.elapsed().as_secs() >= autosave_interval {
            self.perform_autosave();
        }

        // Live waveform update during recording (every 100ms)

        // Process MIDI CC for learn/mapping and macro updates
        midi_mapping::process_midi_cc(self);

        if self.is_recording && self.live_rec_last_update.elapsed().as_millis() > 100 {
            self.live_rec_last_update = std::time::Instant::now();
            if let Some(live_id) = self.live_rec_buffer_id {
                let (samples, rec_sr) = self.recorder.peek_buffer();
                if !samples.is_empty() {
                    // Resample if needed
                    let engine_sr = self.sample_rate();
                    let display_samples = if rec_sr != engine_sr {
                        jamhub_engine::resample(&samples, rec_sr, engine_sr)
                    } else {
                        samples
                    };

                    let duration = display_samples.len() as u64;
                    self.waveform_cache.insert(live_id, &display_samples);

                    // Update the live clip's duration
                    let track_idx = self.selected_track.unwrap_or(0);
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
        let text_has_focus = ctx.memory(|m| m.focused().is_some())
            && ctx.input(|i| !i.raw.events.is_empty());
        // More reliable: check if any text edit is active
        let any_text_edit = self.renaming_track.is_some()
            || self.session.chat_input.len() > 0 && self.session.show_panel;
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
                if i.key_pressed(egui::Key::B) && !i.modifiers.command { actions.push("media_browser".into()); }
                if i.key_pressed(egui::Key::Q) && !i.modifiers.command { actions.push("spectrum".into()); }
                if i.key_pressed(egui::Key::F) && i.modifiers.shift && !i.modifiers.command { actions.push("flatten_comp".into()); }
                if i.key_pressed(egui::Key::Tab) && !i.modifiers.command { actions.push("cycle_view".into()); }
                if i.key_pressed(egui::Key::Slash) && i.modifiers.shift { actions.push("show_shortcuts".into()); }
                if i.key_pressed(egui::Key::Escape) { actions.push("clear_selection".into()); actions.push("deselect_clips".into()); }
                if i.key_pressed(egui::Key::F) && !i.modifiers.command { actions.push("focus_playhead".into()); }
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
                        if i.modifiers.shift {
                            // Shift+1-9: save current playhead to locator slot
                            actions.push(format!("save_locator_{}", idx));
                        } else if i.modifiers.ctrl {
                            // Ctrl+1-9: select track
                            actions.push(format!("select_track_{}", idx));
                        } else {
                            // 1-9: recall locator (jump to saved position)
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
            if i.modifiers.command && i.key_pressed(egui::Key::S) { actions.push("save".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::D) { actions.push("duplicate".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::A) { actions.push("select_all_clips".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::E) { actions.push("effects".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::I) { actions.push("project_info".into()); }
            else if i.modifiers.command && i.key_pressed(egui::Key::I) { actions.push("import".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::P) { actions.push("audio_pool".into()); }
            else if i.modifiers.command && i.key_pressed(egui::Key::P) { actions.push("piano_roll".into()); }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::B) { actions.push("bounce_selection".into()); }
            else if i.modifiers.command && i.key_pressed(egui::Key::B) { actions.push("bounce".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::C) { actions.push("copy".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::V) { actions.push("paste".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::M) { actions.push("add_marker".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::F) { actions.push("fx_browser".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::J) { actions.push("consolidate".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::Comma) { actions.push("preferences".into()); }
            if i.modifiers.command && i.key_pressed(egui::Key::N) { actions.push("new_session".into()); }
        });

        for action in &actions {
            match action.as_str() {
                "toggle_play" => {
                    if self.transport_state() == TransportState::Playing {
                        self.send_command(EngineCommand::Stop);
                    } else {
                        // If loop is enabled and playhead is outside the loop, jump to loop start
                        if self.loop_enabled && self.loop_end > self.loop_start {
                            let pos = self.position_samples();
                            if pos < self.loop_start || pos >= self.loop_end {
                                self.send_command(EngineCommand::SetPosition(self.loop_start));
                            }
                        }
                        self.send_command(EngineCommand::Play);
                    }
                }
                "undo" => self.undo(),
                "redo" => self.redo(),
                "save" => self.save_project(),
                "delete" => {
                    if self.has_selected_clips() {
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
                    self.split_clip_at_playhead();
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
                                self.set_status(&format!("Locator {} not set (Shift+{} to save)", idx + 1, idx + 1));
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
                "new_session" => {
                    self.show_template_picker = true;
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
                _ => {}
            }
        }

        // CPU usage estimate
        let frame_start = std::time::Instant::now();

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Session...").clicked() {
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
                    if ui.button("Import Audio...").clicked() {
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
                    ui.separator();
                    if ui.button("MIDI Mappings...").clicked() {
                        self.show_midi_mappings = !self.show_midi_mappings;
                        ui.close_menu();
                    }
                    if ui.button("Macro Controls...").clicked() {
                        self.show_macros = !self.show_macros;
                        ui.close_menu();
                    }
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
                    if ui.button("Split Clip at Playhead  S").clicked() {
                        self.split_clip_at_playhead();
                        ui.close_menu();
                    }
                    if ui.button("Duplicate Track        Cmd+D").clicked() {
                        self.duplicate_selected_track();
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
                    if ui.button("Add Bus Track").clicked() {
                        self.push_undo("Add bus track");
                        let bus_count = self.project.tracks.iter()
                            .filter(|t| t.kind == TrackKind::Bus)
                            .count() + 1;
                        self.project
                            .add_track(&format!("Bus {bus_count}"), TrackKind::Bus);
                        self.sync_project();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Delete Selected Track").clicked() {
                        self.delete_selected_track();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Effects...").clicked() {
                        self.show_effects = true;
                        ui.close_menu();
                    }
                    if ui.button("Piano Roll...    Cmd+P").clicked() {
                        self.show_piano_roll = true;
                        ui.close_menu();
                    }
                    if ui.button("MIDI Input...").clicked() {
                        self.midi_panel.show = true;
                        ui.close_menu();
                    }
                    if ui.button("Bounce Track     Cmd+B").clicked() {
                        self.bounce_selected_track();
                        ui.close_menu();
                    }
                    if ui.button("Bounce Selection Range").clicked() {
                        self.bounce_selection_range();
                        ui.close_menu();
                    }
                    ui.separator();
                    if let Some(ti) = self.selected_track {
                        if ti < self.project.tracks.len() {
                            if self.project.tracks[ti].frozen {
                                if ui.button("Unfreeze Track").clicked() {
                                    self.unfreeze_selected_track();
                                    ui.close_menu();
                                }
                            } else {
                                if ui.button("Freeze Track").clicked() {
                                    self.freeze_selected_track();
                                    ui.close_menu();
                                }
                            }
                        }
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
                        .selectable_label(self.view == View::Mixer, "Mixer")
                        .clicked()
                    {
                        self.view = View::Mixer;
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
                    if ui.button("Piano Roll       Cmd+P").clicked() {
                        self.show_piano_roll = !self.show_piano_roll;
                        ui.close_menu();
                    }
                    if ui.button("Effects          Cmd+E").clicked() {
                        self.show_effects = !self.show_effects;
                        ui.close_menu();
                    }
                    if ui.button("Spectrum Analyzer    Q").clicked() {
                        self.spectrum_analyzer.show = !self.spectrum_analyzer.show;
                        ui.close_menu();
                    }
                    if ui.button("Media Browser        B").clicked() {
                        self.media_browser.show = !self.media_browser.show;
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
                ui.menu_button("Help", |ui| {
                    if ui.button("About JamHub").clicked() {
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
                    .fill(egui::Color32::from_rgb(22, 24, 30))
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 38, 46)))
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

        // Status bar
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(22, 22, 26)).inner_margin(egui::Margin::symmetric(8, 2)))
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Bounce progress indicator
                if let Some(progress) = self.bounce_progress {
                    let pct = (progress * 100.0) as u32;
                    ui.label(egui::RichText::new(format!("Bouncing... {}%", pct))
                        .size(11.0).color(egui::Color32::from_rgb(100, 180, 255)));
                    let bar_width = 80.0;
                    let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(bar_width, 6.0), egui::Sense::hover());
                    ui.painter().rect_filled(bar_rect, 3.0, egui::Color32::from_rgb(40, 40, 50));
                    let filled = egui::Rect::from_min_size(bar_rect.min, egui::vec2(bar_width * progress, 6.0));
                    ui.painter().rect_filled(filled, 3.0, egui::Color32::from_rgb(80, 160, 255));
                }

                // Status message with subtle styling
                if let Some((msg, time)) = &self.status_message {
                    if time.elapsed().as_secs() < 6 {
                        ui.label(egui::RichText::new(msg).size(11.0).color(egui::Color32::from_rgb(180, 180, 190)));
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let dim = egui::Color32::from_rgb(80, 80, 90);
                    let sep = egui::Color32::from_rgb(45, 45, 52);

                    let sr = self.sample_rate();
                    ui.label(egui::RichText::new(format!("{:.1}kHz", sr as f64 / 1000.0)).size(10.0).color(dim));
                    ui.label(egui::RichText::new("·").size(10.0).color(sep));

                    ui.label(egui::RichText::new(format!("{} tracks", self.project.tracks.len())).size(10.0).color(dim));
                    ui.label(egui::RichText::new("·").size(10.0).color(sep));

                    // CPU usage indicator
                    let cpu_pct = self.cpu_usage * 100.0;
                    let cpu_color = if cpu_pct > 80.0 {
                        egui::Color32::from_rgb(255, 80, 80)
                    } else if cpu_pct > 50.0 {
                        egui::Color32::from_rgb(220, 180, 60)
                    } else {
                        egui::Color32::from_rgb(80, 180, 100)
                    };
                    ui.label(egui::RichText::new(format!("CPU {cpu_pct:.0}%")).size(10.0).color(cpu_color));
                    ui.label(egui::RichText::new("·").size(10.0).color(sep));

                    // Memory usage — count audio buffers and total size
                    let buf_count = self.audio_buffers.len();
                    let total_samples: usize = self.audio_buffers.values().map(|b| b.len()).sum();
                    let mem_mb = (total_samples * 4) as f64 / (1024.0 * 1024.0);
                    ui.label(egui::RichText::new(format!("{buf_count} bufs {mem_mb:.1}MB")).size(10.0).color(dim));
                    ui.label(egui::RichText::new("·").size(10.0).color(sep));

                    // Snap mode — always visible, highlighted when active, with key hint
                    let snap_icon = if self.snap_mode != SnapMode::Off { "[G] Snap: " } else { "[G] Snap: " };
                    let snap_text = format!("{}{}", snap_icon, self.snap_mode.label());
                    let snap_color = if self.snap_mode != SnapMode::Off {
                        egui::Color32::from_rgb(100, 170, 255)
                    } else {
                        dim
                    };
                    ui.label(egui::RichText::new(snap_text).size(10.0).strong().color(snap_color));
                    ui.label(egui::RichText::new("·").size(10.0).color(sep));

                    // Grid division indicator
                    let grid_text = format!("Grid: {}", self.grid_division.label());
                    ui.label(egui::RichText::new(grid_text).size(10.0).color(dim));
                    ui.label(egui::RichText::new("·").size(10.0).color(sep));

                    if self.ripple_mode {
                        ui.label(egui::RichText::new("RIPPLE").size(10.0).strong().color(egui::Color32::from_rgb(255, 140, 60)));
                        ui.label(egui::RichText::new("·").size(10.0).color(sep));
                    }

                    if self.show_automation {
                        ui.label(egui::RichText::new("AUTO").size(10.0).color(egui::Color32::from_rgb(200, 170, 60)));
                        ui.label(egui::RichText::new("·").size(10.0).color(sep));
                    }
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

        // Floating panels
        effects_panel::show(self, ctx);
        piano_roll::show(self, ctx);
        fx_browser::show(self, ctx);
        media_browser::show(self, ctx);
        audio_settings::show(self, ctx);
        midi_panel::show(self, ctx);
        undo_panel::show(self, ctx);
        project_info::show(self, ctx);
        about::show(self, ctx);
        shortcuts_panel::show(self, ctx);
        spectrum::show(self, ctx);
        self.show_audio_pool_window(ctx);
        midi_mapping::show_mapping_manager(self, ctx);

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
                        self.project_path = None;
                        self.dirty = false;
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
            egui::Window::new("Welcome to JamHub")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut wel_open)
                .min_width(420.0)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        ui.heading(egui::RichText::new("JamHub").size(28.0).strong());
                        ui.label(egui::RichText::new("Collaborative DAW").size(14.0).weak());
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

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            View::Arrange => timeline::show(self, ui),
            View::Mixer => mixer_view::show(self, ui),
            View::Session => session_view::show(self, ui, ctx),
        });
    }
}
