use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui;
use jamhub_engine::{load_audio, EngineCommand};
use jamhub_model::{Clip, ClipSource, TrackKind};
use uuid::Uuid;

use crate::DawApp;

// ── Audio extensions ────────────────────────────────────────────────────────
const AUDIO_EXTENSIONS: &[&str] = &["wav", "wave", "mp3", "flac", "aiff", "aif", "ogg"];

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

// ── Sort mode ───────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Name,
    Date,
    Size,
    Duration,
}

impl SortMode {
    pub fn label(&self) -> &'static str {
        match self {
            SortMode::Name => "Name",
            SortMode::Date => "Date",
            SortMode::Size => "Size",
            SortMode::Duration => "Duration",
        }
    }
    pub fn all() -> &'static [SortMode] {
        &[SortMode::Name, SortMode::Date, SortMode::Size, SortMode::Duration]
    }
}

// ── Cached file info ────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct AudioFileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size_bytes: u64,
    pub modified: Option<std::time::SystemTime>,
    pub format: String,
    // Loaded lazily on first select
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub duration_secs: Option<f64>,
    pub waveform_peaks: Option<Vec<f32>>,
}

impl AudioFileInfo {
    fn from_path(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let name = path.file_name()?.to_string_lossy().to_string();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_default();
        Some(Self {
            path: path.to_path_buf(),
            name,
            size_bytes: meta.len(),
            modified: meta.modified().ok(),
            format: ext,
            sample_rate: None,
            channels: None,
            duration_secs: None,
            waveform_peaks: None,
        })
    }

    fn format_size(&self) -> String {
        let b = self.size_bytes;
        if b < 1024 {
            format!("{b} B")
        } else if b < 1024 * 1024 {
            format!("{:.1} KB", b as f64 / 1024.0)
        } else {
            format!("{:.1} MB", b as f64 / (1024.0 * 1024.0))
        }
    }

    fn format_duration(&self) -> String {
        match self.duration_secs {
            Some(d) => {
                let mins = (d / 60.0).floor() as u32;
                let secs = d % 60.0;
                format!("{mins}:{secs:05.2}")
            }
            None => "--:--".to_string(),
        }
    }
}

// ── Favorites persistence ───────────────────────────────────────────────────
fn favorites_path() -> PathBuf {
    let config = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = config.join("jamhub");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("browser_favorites.json")
}

fn load_favorites() -> Vec<PathBuf> {
    let path = favorites_path();
    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_favorites(favorites: &[PathBuf]) {
    let path = favorites_path();
    if let Ok(data) = serde_json::to_string_pretty(favorites) {
        let _ = std::fs::write(&path, data);
    }
}

// ── BPM detection from filename ─────────────────────────────────────────────
fn detect_bpm_from_filename(name: &str) -> Option<f64> {
    // Match patterns like "120bpm", "120_bpm", "120 bpm", "bpm120", "bpm_120"
    let lower = name.to_lowercase();
    // Try "NNNbpm"
    if let Some(idx) = lower.find("bpm") {
        // Check digits before "bpm"
        let before = &lower[..idx];
        let num_str: String = before.chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<String>().chars().rev().collect();
        if let Ok(bpm) = num_str.parse::<f64>() {
            if (30.0..=300.0).contains(&bpm) {
                return Some(bpm);
            }
        }
        // Check digits after "bpm"
        let after = &lower[idx + 3..];
        let num_str: String = after.chars().skip_while(|c| *c == '_' || *c == ' ').take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if let Ok(bpm) = num_str.parse::<f64>() {
            if (30.0..=300.0).contains(&bpm) {
                return Some(bpm);
            }
        }
    }
    None
}

// ── Preview state ───────────────────────────────────────────────────────────
pub struct PreviewState {
    pub playing: bool,
    pub file_path: Option<PathBuf>,
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub position: usize, // current sample position
    pub volume: f32,
    pub waveform_peaks: Vec<f32>,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            playing: false,
            file_path: None,
            samples: Vec::new(),
            sample_rate: 44100,
            position: 0,
            volume: 0.8,
            waveform_peaks: Vec::new(),
        }
    }
}

impl PreviewState {
    pub fn load_file(&mut self, path: &Path) {
        self.stop();
        match load_audio(path) {
            Ok(data) => {
                self.waveform_peaks = compute_peaks(&data.samples, 200);
                self.samples = data.samples;
                self.sample_rate = data.sample_rate;
                self.position = 0;
                self.file_path = Some(path.to_path_buf());
                self.playing = true;
            }
            Err(_) => {
                self.file_path = None;
            }
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.position = 0;
    }

    pub fn toggle(&mut self) {
        if self.samples.is_empty() {
            return;
        }
        if self.playing {
            self.playing = false;
        } else {
            if self.position >= self.samples.len() {
                self.position = 0;
            }
            self.playing = true;
        }
    }

    pub fn progress(&self) -> f32 {
        if self.samples.is_empty() {
            0.0
        } else {
            self.position as f32 / self.samples.len() as f32
        }
    }
}

fn compute_peaks(samples: &[f32], num_bins: usize) -> Vec<f32> {
    if samples.is_empty() || num_bins == 0 {
        return vec![0.0; num_bins];
    }
    let bin_size = (samples.len() / num_bins).max(1);
    (0..num_bins)
        .map(|i| {
            let start = i * bin_size;
            let end = (start + bin_size).min(samples.len());
            samples[start..end]
                .iter()
                .map(|s| s.abs())
                .fold(0.0f32, f32::max)
        })
        .collect()
}

// ── Drag state ──────────────────────────────────────────────────────────────
pub struct MediaDragState {
    pub file_path: PathBuf,
    pub file_name: String,
    pub duration_samples: u64,
    pub sample_rate: u32,
}

// ── Main browser state ──────────────────────────────────────────────────────
pub struct MediaBrowser {
    pub show: bool,
    // Folder tree
    pub current_folder: Option<PathBuf>,
    pub favorites: Vec<PathBuf>,
    pub expanded_folders: std::collections::HashSet<PathBuf>,
    // File list
    pub files: Vec<AudioFileInfo>,
    pub selected_file: Option<usize>,
    pub filter: String,
    pub sort_mode: SortMode,
    pub sort_ascending: bool,
    // Preview
    pub preview: PreviewState,
    // Drag state
    pub dragging: Option<MediaDragState>,
    // Info cache (path -> loaded metadata)
    pub info_cache: HashMap<PathBuf, (Option<u32>, Option<u16>, Option<f64>, Option<Vec<f32>>)>,
    // Last folder scan time
    last_scan: Option<Instant>,
}

impl Default for MediaBrowser {
    fn default() -> Self {
        Self {
            show: false,
            current_folder: None,
            favorites: load_favorites(),
            expanded_folders: std::collections::HashSet::new(),
            files: Vec::new(),
            selected_file: None,
            filter: String::new(),
            sort_mode: SortMode::Name,
            sort_ascending: true,
            preview: PreviewState::default(),
            dragging: None,
            info_cache: HashMap::new(),
            last_scan: None,
        }
    }
}

impl MediaBrowser {
    pub fn save_favorites(&self) {
        save_favorites(&self.favorites);
    }

    pub fn toggle_favorite(&mut self, path: &Path) {
        if let Some(idx) = self.favorites.iter().position(|f| f == path) {
            self.favorites.remove(idx);
        } else {
            self.favorites.push(path.to_path_buf());
        }
        self.save_favorites();
    }

    pub fn is_favorite(&self, path: &Path) -> bool {
        self.favorites.iter().any(|f| f == path)
    }

    pub fn scan_folder(&mut self, folder: &Path) {
        self.files.clear();
        self.selected_file = None;
        self.current_folder = Some(folder.to_path_buf());
        self.last_scan = Some(Instant::now());

        if let Ok(entries) = std::fs::read_dir(folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && is_audio_file(&path) {
                    if let Some(mut info) = AudioFileInfo::from_path(&path) {
                        // Apply cached metadata if available
                        if let Some(cached) = self.info_cache.get(&path) {
                            info.sample_rate = cached.0;
                            info.channels = cached.1;
                            info.duration_secs = cached.2;
                            info.waveform_peaks = cached.3.clone();
                        }
                        self.files.push(info);
                    }
                }
            }
        }
        self.apply_sort();
    }

    pub fn apply_sort(&mut self) {
        let asc = self.sort_ascending;
        match self.sort_mode {
            SortMode::Name => self.files.sort_by(|a, b| {
                let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                if asc { cmp } else { cmp.reverse() }
            }),
            SortMode::Date => self.files.sort_by(|a, b| {
                let cmp = a.modified.cmp(&b.modified);
                if asc { cmp } else { cmp.reverse() }
            }),
            SortMode::Size => self.files.sort_by(|a, b| {
                let cmp = a.size_bytes.cmp(&b.size_bytes);
                if asc { cmp } else { cmp.reverse() }
            }),
            SortMode::Duration => self.files.sort_by(|a, b| {
                let cmp = a.duration_secs.partial_cmp(&b.duration_secs).unwrap_or(std::cmp::Ordering::Equal);
                if asc { cmp } else { cmp.reverse() }
            }),
        }
    }

    /// Load metadata for a single file (sample rate, duration, waveform).
    pub fn load_file_info(&mut self, idx: usize) {
        if idx >= self.files.len() {
            return;
        }
        let path = self.files[idx].path.clone();
        if self.files[idx].sample_rate.is_some() {
            return; // already loaded
        }
        if let Ok(data) = load_audio(&path) {
            let sr = data.sample_rate;
            let ch = data.channels;
            let dur = data.duration_samples as f64 / sr as f64;
            let peaks = compute_peaks(&data.samples, 200);
            self.files[idx].sample_rate = Some(sr);
            self.files[idx].channels = Some(ch);
            self.files[idx].duration_secs = Some(dur);
            self.files[idx].waveform_peaks = Some(peaks.clone());
            self.info_cache.insert(path, (Some(sr), Some(ch), Some(dur), Some(peaks)));
        }
    }
}

// ── Default directories ─────────────────────────────────────────────────────
fn default_directories(project_path: &Option<PathBuf>) -> Vec<(String, PathBuf)> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let music = home.join("Music");
        if music.exists() {
            dirs.push(("Music".to_string(), music));
        }
        let desktop = home.join("Desktop");
        if desktop.exists() {
            dirs.push(("Desktop".to_string(), desktop));
        }
        let downloads = home.join("Downloads");
        if downloads.exists() {
            dirs.push(("Downloads".to_string(), downloads));
        }
    }
    if let Some(ref proj) = project_path {
        if let Some(parent) = proj.parent() {
            dirs.push(("Project".to_string(), parent.to_path_buf()));
        }
    }
    dirs
}

fn get_subdirs(path: &Path) -> Vec<(String, PathBuf)> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Some(name) = p.file_name() {
                    let name_str = name.to_string_lossy().to_string();
                    // Skip hidden directories
                    if !name_str.starts_with('.') {
                        result.push((name_str, p));
                    }
                }
            }
        }
    }
    result.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    result
}

// ── UI ──────────────────────────────────────────────────────────────────────
pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.media_browser.show {
        return;
    }

    // Advance preview playback position (simple time-based advancement)
    if app.media_browser.preview.playing {
        // Advance by roughly one frame worth of samples (assuming ~60fps)
        let advance = (app.media_browser.preview.sample_rate as f32 / 60.0) as usize;
        app.media_browser.preview.position += advance;
        if app.media_browser.preview.position >= app.media_browser.preview.samples.len() {
            app.media_browser.preview.playing = false;
            app.media_browser.preview.position = 0;
        }
        ctx.request_repaint();
    }

    let mut open = true;
    egui::Window::new("Media Browser")
        .open(&mut open)
        .default_size([680.0, 500.0])
        .min_width(480.0)
        .min_height(350.0)
        .show(ctx, |ui| {
            // ── Top bar: search + sort ──────────────────────────────────
            ui.horizontal(|ui| {
                ui.heading("Media Browser");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Sort controls
                    let arrow = if app.media_browser.sort_ascending { " ^" } else { " v" };
                    if ui
                        .small_button(format!("{}{arrow}", app.media_browser.sort_mode.label()))
                        .on_hover_text("Click to reverse, right-click to change mode")
                        .clicked()
                    {
                        app.media_browser.sort_ascending = !app.media_browser.sort_ascending;
                        app.media_browser.apply_sort();
                    }
                    ui.label(
                        egui::RichText::new("Sort:")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                });
            });
            // Sort mode context menu on the sort button area
            ui.horizontal(|ui| {
                ui.label("Filter:");
                let resp = ui.text_edit_singleline(&mut app.media_browser.filter);
                if resp.changed() {
                    // Filter is applied during rendering below
                }
                // Sort mode picker
                ui.separator();
                for mode in SortMode::all() {
                    if ui
                        .selectable_label(app.media_browser.sort_mode == *mode, mode.label())
                        .clicked()
                    {
                        app.media_browser.sort_mode = *mode;
                        app.media_browser.apply_sort();
                    }
                }
            });

            ui.separator();

            // ── Two-panel layout ────────────────────────────────────────
            let available = ui.available_size();
            let left_width = 200.0_f32.min(available.x * 0.35);

            ui.horizontal(|ui| {
                // ── Left panel: folder tree ─────────────────────────────
                ui.vertical(|ui| {
                    ui.set_min_width(left_width);
                    ui.set_max_width(left_width);
                    ui.set_min_height(available.y - 120.0);

                    egui::ScrollArea::vertical()
                        .id_salt("media_folder_tree")
                        .show(ui, |ui| {
                            // Favorites section
                            if !app.media_browser.favorites.is_empty() {
                                ui.label(
                                    egui::RichText::new("Favorites")
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(235, 180, 60)),
                                );
                                let favorites_snapshot: Vec<PathBuf> =
                                    app.media_browser.favorites.clone();
                                for fav in &favorites_snapshot {
                                    let label = fav
                                        .file_name()
                                        .map(|f| f.to_string_lossy().to_string())
                                        .unwrap_or_else(|| fav.to_string_lossy().to_string());
                                    let is_current =
                                        app.media_browser.current_folder.as_ref() == Some(fav);
                                    let resp = ui.selectable_label(
                                        is_current,
                                        format!("  \u{2605} {label}"),
                                    );
                                    if resp.clicked() {
                                        let fav_clone = fav.clone();
                                        app.media_browser.scan_folder(&fav_clone);
                                    }
                                    resp.context_menu(|ui| {
                                        if ui.button("Remove from favorites").clicked() {
                                            let fav_clone = fav.clone();
                                            app.media_browser.toggle_favorite(&fav_clone);
                                            ui.close_menu();
                                        }
                                    });
                                }
                                ui.add_space(6.0);
                                ui.separator();
                            }

                            // Default directories
                            ui.label(
                                egui::RichText::new("Locations")
                                    .size(10.0)
                                    .color(egui::Color32::GRAY),
                            );
                            let project_path = app.project_path.clone();
                            let default_dirs = default_directories(&project_path);

                            for (name, dir_path) in &default_dirs {
                                let is_current =
                                    app.media_browser.current_folder.as_ref() == Some(dir_path);
                                let is_expanded =
                                    app.media_browser.expanded_folders.contains(dir_path);

                                // Folder row with expand arrow
                                let arrow = if is_expanded { "\u{25BE}" } else { "\u{25B8}" };
                                let resp = ui.selectable_label(
                                    is_current,
                                    format!("{arrow} {name}"),
                                );
                                if resp.clicked() {
                                    let dir_clone = dir_path.clone();
                                    app.media_browser.scan_folder(&dir_clone);
                                    if is_expanded {
                                        app.media_browser.expanded_folders.remove(dir_path);
                                    } else {
                                        app.media_browser
                                            .expanded_folders
                                            .insert(dir_path.clone());
                                    }
                                }
                                resp.context_menu(|ui| {
                                    let is_fav = app.media_browser.is_favorite(dir_path);
                                    let label = if is_fav {
                                        "Remove from favorites"
                                    } else {
                                        "Add to favorites"
                                    };
                                    if ui.button(label).clicked() {
                                        let dp = dir_path.clone();
                                        app.media_browser.toggle_favorite(&dp);
                                        ui.close_menu();
                                    }
                                });

                                // Show subdirectories if expanded
                                if is_expanded {
                                    let subdirs = get_subdirs(dir_path);
                                    for (sub_name, sub_path) in &subdirs {
                                        let is_sub_current = app
                                            .media_browser
                                            .current_folder
                                            .as_ref()
                                            == Some(sub_path);
                                        let is_sub_expanded =
                                            app.media_browser.expanded_folders.contains(sub_path);
                                        let sub_arrow =
                                            if is_sub_expanded { "\u{25BE}" } else { "\u{25B8}" };
                                        let resp = ui.selectable_label(
                                            is_sub_current,
                                            format!("    {sub_arrow} {sub_name}"),
                                        );
                                        if resp.clicked() {
                                            let sp = sub_path.clone();
                                            app.media_browser.scan_folder(&sp);
                                            if is_sub_expanded {
                                                app.media_browser
                                                    .expanded_folders
                                                    .remove(sub_path);
                                            } else {
                                                app.media_browser
                                                    .expanded_folders
                                                    .insert(sub_path.clone());
                                            }
                                        }
                                        resp.context_menu(|ui| {
                                            let is_fav =
                                                app.media_browser.is_favorite(sub_path);
                                            let label = if is_fav {
                                                "Remove from favorites"
                                            } else {
                                                "Add to favorites"
                                            };
                                            if ui.button(label).clicked() {
                                                let sp = sub_path.clone();
                                                app.media_browser.toggle_favorite(&sp);
                                                ui.close_menu();
                                            }
                                        });

                                        // Third level subdirs
                                        if is_sub_expanded {
                                            let sub_subdirs = get_subdirs(sub_path);
                                            for (ss_name, ss_path) in &sub_subdirs {
                                                let is_ss = app
                                                    .media_browser
                                                    .current_folder
                                                    .as_ref()
                                                    == Some(ss_path);
                                                let resp = ui.selectable_label(
                                                    is_ss,
                                                    format!("        {ss_name}"),
                                                );
                                                if resp.clicked() {
                                                    let ssp = ss_path.clone();
                                                    app.media_browser.scan_folder(&ssp);
                                                }
                                                resp.context_menu(|ui| {
                                                    let is_fav = app
                                                        .media_browser
                                                        .is_favorite(ss_path);
                                                    let label = if is_fav {
                                                        "Remove from favorites"
                                                    } else {
                                                        "Add to favorites"
                                                    };
                                                    if ui.button(label).clicked() {
                                                        let ssp = ss_path.clone();
                                                        app.media_browser
                                                            .toggle_favorite(&ssp);
                                                        ui.close_menu();
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        });
                });

                ui.separator();

                // ── Right panel: file list + preview ────────────────────
                ui.vertical(|ui| {
                    let filter_lower = app.media_browser.filter.to_lowercase();

                    // File count
                    let visible_count = if filter_lower.is_empty() {
                        app.media_browser.files.len()
                    } else {
                        app.media_browser
                            .files
                            .iter()
                            .filter(|f| f.name.to_lowercase().contains(&filter_lower))
                            .count()
                    };
                    ui.horizontal(|ui| {
                        if let Some(ref folder) = app.media_browser.current_folder {
                            ui.label(
                                egui::RichText::new(
                                    folder
                                        .file_name()
                                        .map(|f| f.to_string_lossy().to_string())
                                        .unwrap_or_default(),
                                )
                                .strong(),
                            );
                        }
                        ui.label(
                            egui::RichText::new(format!("{visible_count} files"))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    });

                    ui.separator();

                    // Column headers
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Name")
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new("Size")
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                                ui.label(
                                    egui::RichText::new("Dur")
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                                ui.label(
                                    egui::RichText::new("SR")
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                            },
                        );
                    });

                    // File list
                    let list_height = (available.y - 250.0).max(100.0);
                    egui::ScrollArea::vertical()
                        .id_salt("media_file_list")
                        .max_height(list_height)
                        .show(ui, |ui| {
                            let mut click_idx: Option<usize> = None;
                            let mut drag_start: Option<usize> = None;

                            let files_snapshot: Vec<(usize, String, String, String, String, String)> = app
                                .media_browser
                                .files
                                .iter()
                                .enumerate()
                                .filter(|(_, f)| {
                                    filter_lower.is_empty()
                                        || f.name.to_lowercase().contains(&filter_lower)
                                })
                                .map(|(i, f)| {
                                    let sr = f
                                        .sample_rate
                                        .map(|s| format!("{s}"))
                                        .unwrap_or_else(|| "---".into());
                                    let dur = f.format_duration();
                                    let size = f.format_size();
                                    let fmt = f.format.clone();
                                    (i, f.name.clone(), sr, dur, size, fmt)
                                })
                                .collect();

                            for (idx, name, sr, dur, size, fmt) in &files_snapshot {
                                let is_selected =
                                    app.media_browser.selected_file == Some(*idx);

                                let resp = ui.horizontal(|ui| {
                                    // Format badge
                                    ui.label(
                                        egui::RichText::new(fmt)
                                            .size(9.0)
                                            .color(egui::Color32::from_rgb(120, 140, 160)),
                                    );

                                    let text = if is_selected {
                                        egui::RichText::new(name)
                                            .color(egui::Color32::from_rgb(80, 200, 190))
                                    } else {
                                        egui::RichText::new(name)
                                    };
                                    let label_resp =
                                        ui.selectable_label(is_selected, text);

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(size)
                                                    .size(10.0)
                                                    .color(egui::Color32::GRAY),
                                            );
                                            ui.label(
                                                egui::RichText::new(dur)
                                                    .size(10.0)
                                                    .color(egui::Color32::GRAY),
                                            );
                                            ui.label(
                                                egui::RichText::new(sr)
                                                    .size(10.0)
                                                    .color(egui::Color32::GRAY),
                                            );
                                        },
                                    );

                                    label_resp
                                });

                                let inner_resp = resp.inner;
                                if inner_resp.clicked() {
                                    click_idx = Some(*idx);
                                }
                                // Drag source
                                if inner_resp.dragged() {
                                    drag_start = Some(*idx);
                                }
                            }

                            // Handle click: select + preview
                            if let Some(idx) = click_idx {
                                let was_selected = app.media_browser.selected_file == Some(idx);
                                app.media_browser.selected_file = Some(idx);

                                // Load metadata if not loaded
                                app.media_browser.load_file_info(idx);

                                // Auto-preview: load + play the file
                                if !was_selected {
                                    let path = app.media_browser.files[idx].path.clone();
                                    app.media_browser.preview.load_file(&path);
                                }
                            }

                            // Handle drag initiation
                            if let Some(idx) = drag_start {
                                if idx < app.media_browser.files.len() {
                                    let file = &app.media_browser.files[idx];
                                    let sr = file.sample_rate.unwrap_or(44100);
                                    let dur_samples = file
                                        .duration_secs
                                        .map(|d| (d * sr as f64) as u64)
                                        .unwrap_or(44100);
                                    app.media_browser.dragging = Some(MediaDragState {
                                        file_path: file.path.clone(),
                                        file_name: file.name.clone(),
                                        duration_samples: dur_samples,
                                        sample_rate: sr,
                                    });
                                }
                            }
                        });

                    ui.separator();

                    // ── Preview section ──────────────────────────────────
                    show_preview(app, ui);
                });
            });
        });

    if !open {
        app.media_browser.show = false;
        app.media_browser.preview.stop();
    }
}

fn show_preview(app: &mut DawApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Preview")
                .size(11.0)
                .color(egui::Color32::GRAY),
        );

        if let Some(ref path) = app.media_browser.preview.file_path {
            let name = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            ui.label(egui::RichText::new(&name).small());

            // BPM indicator
            if let Some(bpm) = detect_bpm_from_filename(&name) {
                ui.label(
                    egui::RichText::new(format!("{bpm:.0} BPM"))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(235, 180, 60)),
                );
            }
        }
    });

    // Waveform display
    let waveform_rect = ui.allocate_space(egui::vec2(ui.available_width(), 48.0));
    let rect = waveform_rect.1;
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 3.0, egui::Color32::from_rgb(20, 20, 24));

    if !app.media_browser.preview.waveform_peaks.is_empty() {
        let peaks = &app.media_browser.preview.waveform_peaks;
        let w = rect.width();
        let h = rect.height();
        let mid_y = rect.center().y;
        let num = peaks.len();

        for (i, &peak) in peaks.iter().enumerate() {
            let x = rect.left() + (i as f32 / num as f32) * w;
            let bar_h = peak * h * 0.9;

            // Color based on playback position
            let progress = app.media_browser.preview.progress();
            let frac = i as f32 / num as f32;
            let color = if frac < progress {
                egui::Color32::from_rgb(80, 200, 190)
            } else {
                egui::Color32::from_rgb(60, 65, 80)
            };

            painter.line_segment(
                [
                    egui::pos2(x, mid_y - bar_h / 2.0),
                    egui::pos2(x, mid_y + bar_h / 2.0),
                ],
                egui::Stroke::new(1.5, color),
            );
        }

        // Playhead line
        if app.media_browser.preview.playing {
            let px = rect.left() + progress_val(&app.media_browser.preview) * w;
            painter.line_segment(
                [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            );
        }
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Select a file to preview",
            egui::FontId::proportional(11.0),
            egui::Color32::from_rgb(80, 80, 90),
        );
    }

    // Transport controls
    ui.horizontal(|ui| {
        let play_label = if app.media_browser.preview.playing {
            "\u{23F9} Stop"
        } else {
            "\u{25B6} Play"
        };
        if ui.small_button(play_label).clicked() {
            app.media_browser.preview.toggle();
        }

        // Volume slider
        ui.label(
            egui::RichText::new("Vol:")
                .size(10.0)
                .color(egui::Color32::GRAY),
        );
        ui.add(
            egui::Slider::new(&mut app.media_browser.preview.volume, 0.0..=1.0)
                .show_value(false)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
        );

        // Duration / position display
        if !app.media_browser.preview.samples.is_empty() {
            let sr = app.media_browser.preview.sample_rate as f64;
            let pos = app.media_browser.preview.position as f64 / sr;
            let total = app.media_browser.preview.samples.len() as f64 / sr;
            ui.label(
                egui::RichText::new(format!(
                    "{:.1}s / {:.1}s",
                    pos, total
                ))
                .size(10.0)
                .color(egui::Color32::GRAY),
            );
        }

        // Import button: drag file to timeline or click to import at playhead
        if app.media_browser.selected_file.is_some() {
            if ui
                .small_button("+ Import to Track")
                .on_hover_text("Import selected file at playhead position on selected track")
                .clicked()
            {
                if let Some(idx) = app.media_browser.selected_file {
                    if idx < app.media_browser.files.len() {
                        let path = app.media_browser.files[idx].path.clone();
                        app.import_audio_file(path);
                    }
                }
            }
        }
    });
}

fn progress_val(preview: &PreviewState) -> f32 {
    preview.progress()
}

// ── Drag-to-timeline handling ───────────────────────────────────────────────
/// Call this from the timeline drawing code to handle media browser drag-and-drop.
/// Returns Some((track_idx, start_sample, path)) if a drop occurred.
pub fn handle_drag_drop(
    app: &mut DawApp,
    ui: &egui::Ui,
    timeline_rect: egui::Rect,
    track_height: f32,
    num_tracks: usize,
    samples_per_pixel: f64,
    scroll_x: f32,
) -> Option<(usize, u64, PathBuf)> {
    let dragging = app.media_browser.dragging.as_ref()?;

    // Show ghost rectangle while dragging
    if let Some(pos) = ui.ctx().pointer_latest_pos() {
        if timeline_rect.contains(pos) {
            let relative_y = pos.y - timeline_rect.top();
            let track_idx = (relative_y / track_height) as usize;
            let relative_x = pos.x - timeline_rect.left() + scroll_x;
            let start_sample = (relative_x as f64 * samples_per_pixel) as u64;

            // Draw ghost clip
            let ghost_x = (start_sample as f64 / samples_per_pixel) as f32 - scroll_x + timeline_rect.left();
            let ghost_y = timeline_rect.top() + track_idx as f32 * track_height;
            let ghost_w = (dragging.duration_samples as f64 / samples_per_pixel) as f32;
            let ghost_rect =
                egui::Rect::from_min_size(egui::pos2(ghost_x, ghost_y), egui::vec2(ghost_w.min(400.0), track_height));

            let painter = ui.painter();
            painter.rect_filled(
                ghost_rect,
                3.0,
                egui::Color32::from_rgba_premultiplied(80, 200, 190, 60),
            );
            painter.rect_stroke(
                ghost_rect,
                3.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 200, 190)),
                egui::StrokeKind::Inside,
            );
            painter.text(
                ghost_rect.center(),
                egui::Align2::CENTER_CENTER,
                &dragging.file_name,
                egui::FontId::proportional(10.0),
                egui::Color32::from_rgb(80, 200, 190),
            );

            // Check for drop (mouse released)
            if ui.input(|i| i.pointer.any_released()) {
                let path = dragging.file_path.clone();
                let target_track = if track_idx < num_tracks {
                    track_idx
                } else {
                    // Drop below last track -> create new track
                    num_tracks // signals "create new track"
                };
                app.media_browser.dragging = None;
                return Some((target_track, start_sample, path));
            }
        }
    }

    // If pointer released outside timeline, cancel drag
    if ui.input(|i| i.pointer.any_released()) {
        app.media_browser.dragging = None;
    }

    None
}

/// Import a file at a specific position on a specific track.
/// If track_idx >= project.tracks.len(), creates a new audio track.
pub fn import_at_position(app: &mut DawApp, track_idx: usize, start_sample: u64, path: &Path) {
    // Create new track if needed
    if track_idx >= app.project.tracks.len() {
        let n = app.project.tracks.len() + 1;
        app.project
            .add_track(&format!("Track {n}"), TrackKind::Audio);
    }

    let ti = track_idx.min(app.project.tracks.len() - 1);

    match load_audio(path) {
        Ok(data) => {
            app.push_undo("Import audio from browser");

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
                muted: false,
                fade_in_samples: 0,
                fade_out_samples: 0,
                color: None,
                playback_rate: 1.0,
                preserve_pitch: false,
                loop_count: 1,
                gain_db: 0.0,
                take_index: 0,
                content_offset: 0,
            };

            app.waveform_cache.insert(buffer_id, &data.samples);
            app.audio_buffers.insert(buffer_id, data.samples.clone());

            app.project.tracks[ti].clips.push(clip);

            app.send_command(EngineCommand::LoadAudioBuffer {
                id: buffer_id,
                samples: data.samples,
            });
            app.sync_project();
            app.set_status(&format!("Imported: {file_name}"));
        }
        Err(e) => {
            app.set_status(&format!("Import failed: {e}"));
        }
    }
}
