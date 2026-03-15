mod about;
mod audio_settings;
mod effects_panel;
mod fx_browser;
mod mixer_view;
mod piano_roll;
mod session_panel;
mod timeline;
mod transport_bar;
mod undo;

use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui;
use jamhub_engine::{load_audio, EngineCommand, EngineHandle, InputMonitor, LevelMeters, Recorder, WaveformCache};
use jamhub_model::{Clip, ClipSource, Project, TrackKind, TransportState};
use uuid::Uuid;

use session_panel::SessionPanel;
use undo::UndoManager;

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
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(DawApp::new()))
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
    pub selected_clip: Option<(usize, usize)>, // (track_idx, clip_idx)
    pub waveform_cache: WaveformCache,
    undo_manager: UndoManager,
    audio_buffers: HashMap<Uuid, Vec<f32>>,
    pub project_path: Option<PathBuf>,
    pub session: SessionPanel,
    pub metronome_enabled: bool,
    pub snap_mode: SnapMode,
    // Clip dragging state
    pub dragging_clip: Option<ClipDragState>,
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
    pub audio_settings: audio_settings::AudioSettings,
    // Time selection range (for export selection, loop-to-selection, delete range)
    pub selection_start: Option<u64>,
    pub selection_end: Option<u64>,
    pub selecting: bool,
    // Automation editing
    pub show_automation: bool,
    pub automation_param: jamhub_model::AutomationParam,
    // Clip trim state
    pub trimming_clip: Option<ClipTrimState>,
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

#[derive(PartialEq)]
pub enum View {
    Arrange,
    Mixer,
}

#[derive(PartialEq, Clone, Copy)]
pub enum SnapMode {
    Off,        // Free positioning, sample-accurate
    Beat,       // Snap to beats
    Bar,        // Snap to bars
    HalfBeat,   // Snap to half beats (8th notes in 4/4)
}

impl SnapMode {
    pub fn label(&self) -> &str {
        match self {
            SnapMode::Off => "Free",
            SnapMode::Beat => "Beat",
            SnapMode::Bar => "Bar",
            SnapMode::HalfBeat => "1/2 Beat",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            SnapMode::Off => SnapMode::HalfBeat,
            SnapMode::HalfBeat => SnapMode::Beat,
            SnapMode::Beat => SnapMode::Bar,
            SnapMode::Bar => SnapMode::Off,
        }
    }
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
        project.add_track("Track 1", TrackKind::Audio);
        project.add_track("Track 2", TrackKind::Audio);

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
            selected_clip: None,
            waveform_cache: WaveformCache::new(),
            undo_manager: UndoManager::new(),
            audio_buffers: HashMap::new(),
            project_path: None,
            session: SessionPanel::default(),
            metronome_enabled: false,
            snap_mode: SnapMode::Beat,
            dragging_clip: None,
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
            fx_browser: fx_browser::FxBrowser::default(),
            audio_settings: audio_settings::AudioSettings::default(),
            selection_start: None,
            selection_end: None,
            selecting: false,
            show_automation: false,
            automation_param: jamhub_model::AutomationParam::Volume,
            trimming_clip: None,
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

    pub fn levels(&self) -> Option<&LevelMeters> {
        self.engine.as_ref().map(|e| &e.levels)
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

    pub fn push_undo(&mut self, label: &str) {
        self.undo_manager.push(label, &self.project);
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
                    muted: false,
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

            // 1. Stop the recorder FIRST to get captured audio
            let result = self.recorder.stop();

            // 2. Stop the engine AFTER getting recording data
            self.send_command(EngineCommand::Stop);

            // Unmute the track we muted during recording, disarm it
            let track_idx = self.selected_track.unwrap_or(0);
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

            // 4. Duration is simply the buffer length — this is the actual audio data
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
                muted: false,
            };

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

            // Store the current playhead position BEFORE starting
            self.recording_start_pos = self.position_samples();

            match self.recorder.start() {
                Ok(()) => {
                    self.is_recording = true;
                    self.send_command(EngineCommand::Play);
                    self.set_status("Recording... (track muted for monitoring)");
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
    }

    pub fn delete_selected_clip(&mut self) {
        if let Some((track_idx, clip_idx)) = self.selected_clip {
            if track_idx < self.project.tracks.len()
                && clip_idx < self.project.tracks[track_idx].clips.len()
            {
                self.push_undo("Delete clip");
                self.project.tracks[track_idx].clips.remove(clip_idx);
                self.selected_clip = None;
                self.sync_project();
                self.set_status("Clip deleted");
            }
        }
    }

    pub fn delete_selected_track(&mut self) {
        if let Some(track_idx) = self.selected_track {
            if track_idx < self.project.tracks.len() && self.project.tracks.len() > 1 {
                self.push_undo("Delete track");
                self.project.tracks.remove(track_idx);
                self.selected_track = Some(track_idx.min(self.project.tracks.len() - 1));
                self.selected_clip = None;
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

        // Process in reverse order so indices stay valid
        let mut new_clips: Vec<Clip> = Vec::new();

        for &ci in to_split.iter().rev() {
            let clip_start = self.project.tracks[track_idx].clips[ci].start_sample;
            let clip_duration = self.project.tracks[track_idx].clips[ci].duration_samples;
            let clip_name = self.project.tracks[track_idx].clips[ci].name.clone();
            let clip_source = self.project.tracks[track_idx].clips[ci].source.clone();
            let clip_muted = self.project.tracks[track_idx].clips[ci].muted;
            let split_offset = pos - clip_start;

            let mut right_clip = Clip {
                id: Uuid::new_v4(),
                name: clip_name.clone(),
                start_sample: pos,
                duration_samples: clip_duration - split_offset,
                source: clip_source.clone(),
                muted: clip_muted,
            };

            if let ClipSource::AudioBuffer { buffer_id } = &clip_source {
                let buf_data = self.audio_buffers.get(buffer_id).cloned();
                if let Some(buf) = buf_data {
                    let split_at = (split_offset as usize).min(buf.len());
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
            new_clips.push(right_clip);
        }

        // Add all right halves — preserve everything, change nothing about selection
        for clip in new_clips {
            self.project.tracks[track_idx].clips.push(clip);
        }

        // Don't change selection — user's current state stays as-is
        self.sync_project();
        self.set_status(&format!("Split {} clip(s) at playhead", to_split.len()));
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
                    muted: false,
                });
                self.project.tracks[track_idx].effects.clear();
                self.sync_project();
                self.set_status("Track bounced — effects baked in");
            }
            Err(e) => self.set_status(&format!("Bounce failed: {e}")),
        }
    }

    pub fn toggle_input_monitor(&mut self) {
        match self.input_monitor.toggle() {
            Ok(true) => self.set_status("Input monitoring ON — you can hear your mic"),
            Ok(false) => self.set_status("Input monitoring OFF"),
            Err(e) => self.set_status(&format!("Monitor failed: {e}")),
        }
    }

    pub fn export_mixdown(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Export Mixdown")
            .add_filter("WAV Audio", &["wav"])
            .set_file_name("mixdown.wav")
            .save_file()
        {
            let sr = self.sample_rate();
            match jamhub_engine::export_wav(&path, &self.project, &self.audio_buffers, sr, 2) {
                Ok(()) => self.set_status(&format!("Exported: {}", path.display())),
                Err(e) => self.set_status(&format!("Export failed: {e}")),
            }
        }
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

        let sr = self.sample_rate();
        match jamhub_engine::save_project(&dir, &self.project, &self.audio_buffers, sr) {
            Ok(()) => self.set_status(&format!("Saved to {}", dir.display())),
            Err(e) => self.set_status(&format!("Save failed: {e}")),
        }
    }

    pub fn load_project_dialog(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Open Project")
            .pick_folder()
        {
            match jamhub_engine::load_project(&dir) {
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
                    self.sync_project();
                    self.set_status(&format!("Loaded: {}", dir.display()));
                }
                Err(e) => self.set_status(&format!("Load failed: {e}")),
            }
        }
    }

    /// Snap a sample position to the nearest beat.
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
            SnapMode::Beat => {
                let n = (sample as f64 / spb).round();
                (n * spb) as u64
            }
            SnapMode::Bar => {
                let spbar = spb * self.project.time_signature.numerator as f64;
                let n = (sample as f64 / spbar).round();
                (n * spbar) as u64
            }
        }
    }
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.transport_state() == TransportState::Playing || self.is_recording {
            ctx.request_repaint();
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

        // Keyboard shortcuts
        let mut actions: Vec<String> = Vec::new();
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Space) {
                actions.push("toggle_play".into());
            }
            if i.modifiers.command && i.key_pressed(egui::Key::Z) {
                if i.modifiers.shift {
                    actions.push("redo".into());
                } else {
                    actions.push("undo".into());
                }
            }
            if i.modifiers.command && i.key_pressed(egui::Key::S) {
                actions.push("save".into());
            }
            if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                actions.push("delete".into());
            }
            if i.key_pressed(egui::Key::Home) {
                actions.push("rewind".into());
            }
            if i.key_pressed(egui::Key::R) && !i.modifiers.command {
                actions.push("record".into());
            }
            if i.key_pressed(egui::Key::M) && !i.modifiers.command {
                actions.push("metronome".into());
            }
            if i.modifiers.command && i.key_pressed(egui::Key::D) {
                actions.push("duplicate_track".into());
            }
            // Track switching: Up/Down arrows
            if i.key_pressed(egui::Key::ArrowUp) && !i.modifiers.command {
                actions.push("track_up".into());
            }
            if i.key_pressed(egui::Key::ArrowDown) && !i.modifiers.command {
                actions.push("track_down".into());
            }
            // Number keys 1-9 to select track
            for (idx, key) in [
                egui::Key::Num1, egui::Key::Num2, egui::Key::Num3,
                egui::Key::Num4, egui::Key::Num5, egui::Key::Num6,
                egui::Key::Num7, egui::Key::Num8, egui::Key::Num9,
            ].iter().enumerate() {
                if i.key_pressed(*key) && !i.modifiers.command {
                    actions.push(format!("select_track_{}", idx));
                }
            }
            // L key for loop toggle
            if i.key_pressed(egui::Key::L) && !i.modifiers.command {
                actions.push("toggle_loop".into());
            }
            // Cmd+E for effects
            if i.modifiers.command && i.key_pressed(egui::Key::E) {
                actions.push("effects".into());
            }
            // Cmd+I for import
            if i.modifiers.command && i.key_pressed(egui::Key::I) {
                actions.push("import".into());
            }
            // Cmd+P for piano roll
            if i.modifiers.command && i.key_pressed(egui::Key::P) {
                actions.push("piano_roll".into());
            }
            // T for toggle take lanes on selected track
            if i.key_pressed(egui::Key::T) && !i.modifiers.command {
                actions.push("toggle_takes".into());
            }
            // G for snap mode cycle
            if i.key_pressed(egui::Key::G) && !i.modifiers.command {
                actions.push("cycle_snap".into());
            }
            // S for split clip
            if i.key_pressed(egui::Key::S) && !i.modifiers.command {
                actions.push("split".into());
            }
            // I (no Cmd) for input monitor
            if i.key_pressed(egui::Key::I) && !i.modifiers.command {
                actions.push("input_monitor".into());
            }
            // Cmd+B for bounce
            if i.modifiers.command && i.key_pressed(egui::Key::B) {
                actions.push("bounce".into());
            }
            // Cmd+M for add marker
            if i.modifiers.command && i.key_pressed(egui::Key::M) {
                actions.push("add_marker".into());
            }
            // F for FX browser
            if i.key_pressed(egui::Key::F) && !i.modifiers.command {
                actions.push("fx_browser".into());
            }
            // A for automation toggle
            if i.key_pressed(egui::Key::A) && !i.modifiers.command {
                actions.push("toggle_automation".into());
            }
            // Left/Right arrows to nudge selected clip
            if i.key_pressed(egui::Key::ArrowLeft) && !i.modifiers.command && i.modifiers.alt {
                actions.push("nudge_left".into());
            }
            if i.key_pressed(egui::Key::ArrowRight) && !i.modifiers.command && i.modifiers.alt {
                actions.push("nudge_right".into());
            }
        });

        for action in &actions {
            match action.as_str() {
                "toggle_play" => {
                    if self.transport_state() == TransportState::Playing {
                        self.send_command(EngineCommand::Stop);
                    } else {
                        self.send_command(EngineCommand::Play);
                    }
                }
                "undo" => self.undo(),
                "redo" => self.redo(),
                "save" => self.save_project(),
                "delete" => {
                    if self.selected_clip.is_some() {
                        self.delete_selected_clip();
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
                "duplicate_track" => {
                    self.duplicate_selected_track();
                }
                "track_up" => {
                    if let Some(idx) = self.selected_track {
                        if idx > 0 {
                            self.selected_track = Some(idx - 1);
                            self.selected_clip = None;
                        }
                    }
                }
                "track_down" => {
                    if let Some(idx) = self.selected_track {
                        if idx + 1 < self.project.tracks.len() {
                            self.selected_track = Some(idx + 1);
                            self.selected_clip = None;
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
                "fx_browser" => {
                    self.fx_browser.show = !self.fx_browser.show;
                }
                "toggle_automation" => {
                    self.show_automation = !self.show_automation;
                    if self.show_automation {
                        self.set_status("Automation visible — click timeline to add points");
                    } else {
                        self.set_status("Automation hidden");
                    }
                }
                "nudge_left" => {
                    if let Some((ti, ci)) = self.selected_clip {
                        if ti < self.project.tracks.len()
                            && ci < self.project.tracks[ti].clips.len()
                        {
                            self.push_undo("Nudge clip");
                            let sr = self.sample_rate() as f64;
                            let nudge = match self.snap_mode {
                                SnapMode::Off => 1u64, // 1 sample
                                SnapMode::HalfBeat => {
                                    (self.project.tempo.samples_per_beat(sr) / 2.0) as u64
                                }
                                SnapMode::Beat => {
                                    self.project.tempo.samples_per_beat(sr) as u64
                                }
                                SnapMode::Bar => {
                                    (self.project.tempo.samples_per_beat(sr)
                                        * self.project.time_signature.numerator as f64)
                                        as u64
                                }
                            };
                            let clip = &mut self.project.tracks[ti].clips[ci];
                            clip.start_sample = clip.start_sample.saturating_sub(nudge);
                            self.sync_project();
                        }
                    }
                }
                "nudge_right" => {
                    if let Some((ti, ci)) = self.selected_clip {
                        if ti < self.project.tracks.len()
                            && ci < self.project.tracks[ti].clips.len()
                        {
                            self.push_undo("Nudge clip");
                            let sr = self.sample_rate() as f64;
                            let nudge = match self.snap_mode {
                                SnapMode::Off => 1u64,
                                SnapMode::HalfBeat => {
                                    (self.project.tempo.samples_per_beat(sr) / 2.0) as u64
                                }
                                SnapMode::Beat => {
                                    self.project.tempo.samples_per_beat(sr) as u64
                                }
                                SnapMode::Bar => {
                                    (self.project.tempo.samples_per_beat(sr)
                                        * self.project.time_signature.numerator as f64)
                                        as u64
                                }
                            };
                            self.project.tracks[ti].clips[ci].start_sample += nudge;
                            self.sync_project();
                        }
                    }
                }
                a if a.starts_with("select_track_") => {
                    if let Ok(idx) = a[13..].parse::<usize>() {
                        if idx < self.project.tracks.len() {
                            self.selected_track = Some(idx);
                            self.selected_clip = None;
                        }
                    }
                }
                _ => {}
            }
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Session").clicked() {
                        self.project = Project::default();
                        self.project.add_track("Track 1", TrackKind::Audio);
                        self.audio_buffers.clear();
                        self.project_path = None;
                        self.sync_project();
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
                    if ui.button("Import Audio...").clicked() {
                        ui.close_menu();
                        self.open_import_dialog();
                    }
                    ui.separator();
                    if ui.button("Export Mixdown...").clicked() {
                        ui.close_menu();
                        self.export_mixdown();
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
                        if self.selected_clip.is_some() {
                            self.delete_selected_clip();
                        } else {
                            self.delete_selected_track();
                        }
                        ui.close_menu();
                    }
                    if ui.button("Split Clip at Playhead  S").clicked() {
                        self.split_clip_at_playhead();
                        ui.close_menu();
                    }
                    if ui.button("Duplicate Track        Cmd+D").clicked() {
                        self.duplicate_selected_track();
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
                    if ui.button("Bounce Track     Cmd+B").clicked() {
                        self.bounce_selected_track();
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
                    ui.separator();
                    if ui.button("Piano Roll       Cmd+P").clicked() {
                        self.show_piano_roll = !self.show_piano_roll;
                        ui.close_menu();
                    }
                    if ui.button("Effects          Cmd+E").clicked() {
                        self.show_effects = !self.show_effects;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("Snap Mode:").small().color(egui::Color32::GRAY));
                    for mode in [SnapMode::Off, SnapMode::HalfBeat, SnapMode::Beat, SnapMode::Bar] {
                        if ui.selectable_label(self.snap_mode == mode, mode.label()).clicked() {
                            self.snap_mode = mode;
                        }
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About JamHub").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                    if ui.button("Keyboard Shortcuts").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });

        // Transport bar
        egui::TopBottomPanel::top("transport").show(ctx, |ui| {
            transport_bar::show(self, ui);
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some((msg, time)) = &self.status_message {
                    if time.elapsed().as_secs() < 6 {
                        ui.label(egui::RichText::new(msg).small());
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let sr = self.sample_rate();
                    ui.label(egui::RichText::new(format!("{:.1}kHz", sr as f64 / 1000.0))
                        .small().color(egui::Color32::from_rgb(90, 90, 100)));
                    ui.label(egui::RichText::new("|").small().color(egui::Color32::from_rgb(50, 50, 60)));

                    if self.snap_mode != SnapMode::Off {
                        ui.label(egui::RichText::new(format!("SNAP: {}", self.snap_mode.label()))
                            .small().color(egui::Color32::from_rgb(100, 160, 220)));
                        ui.label(egui::RichText::new("|").small().color(egui::Color32::from_rgb(50, 50, 60)));
                    }

                    ui.label(egui::RichText::new("? Help > Shortcuts").small().color(egui::Color32::from_rgb(80, 80, 90)));
                });
            });
        });

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
        audio_settings::show(self, ctx);
        about::show(self, ctx);

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            View::Arrange => timeline::show(self, ui),
            View::Mixer => mixer_view::show(self, ui),
        });
    }
}
