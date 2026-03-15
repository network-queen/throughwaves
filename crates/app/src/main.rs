mod mixer_view;
mod session_panel;
mod timeline;
mod transport_bar;
mod undo;

use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui;
use jamhub_engine::{load_wav, EngineCommand, EngineHandle, Recorder, WaveformCache};
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
    pub status_message: Option<(String, std::time::Instant)>,
    pub selected_track: Option<usize>,
    pub waveform_cache: WaveformCache,
    undo_manager: UndoManager,
    /// Keeps a copy of audio buffers for save/load
    audio_buffers: HashMap<Uuid, Vec<f32>>,
    pub project_path: Option<PathBuf>,
    pub session: SessionPanel,
}

#[derive(PartialEq)]
pub enum View {
    Arrange,
    Mixer,
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
            status_message: None,
            selected_track: Some(0),
            waveform_cache: WaveformCache::new(),
            undo_manager: UndoManager::new(),
            audio_buffers: HashMap::new(),
            project_path: None,
            session: SessionPanel::default(),
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

        match load_wav(&path) {
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
                };

                // Build waveform peaks for display
                self.waveform_cache.insert(buffer_id, &data.samples);

                // Store buffer copy for save/load
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
        if self.recorder.is_recording() {
            let samples = self.recorder.stop();
            if samples.is_empty() {
                self.set_status("Recording was empty");
                return;
            }

            let track_idx = self.selected_track.unwrap_or(0);
            if track_idx >= self.project.tracks.len() {
                return;
            }

            self.push_undo("Record audio");

            let buffer_id = Uuid::new_v4();
            let duration = samples.len() as u64;
            let rec_start = self.position_samples().saturating_sub(duration);

            let clip = Clip {
                id: Uuid::new_v4(),
                name: format!(
                    "Recording {}",
                    self.project.tracks[track_idx].clips.len() + 1
                ),
                start_sample: rec_start,
                duration_samples: duration,
                source: ClipSource::AudioBuffer { buffer_id },
            };

            self.waveform_cache.insert(buffer_id, &samples);
            self.audio_buffers.insert(buffer_id, samples.clone());

            self.project.tracks[track_idx].clips.push(clip);
            self.send_command(EngineCommand::LoadAudioBuffer {
                id: buffer_id,
                samples,
            });
            self.sync_project();
            self.send_command(EngineCommand::Stop);
            self.set_status("Recording saved");
        } else {
            let track_idx = self.selected_track.unwrap_or(0);
            if track_idx < self.project.tracks.len() {
                self.project.tracks[track_idx].armed = true;
            }

            match self.recorder.start() {
                Ok(()) => {
                    self.send_command(EngineCommand::Play);
                    self.set_status("Recording...");
                }
                Err(e) => {
                    self.set_status(&format!("Record failed: {e}"));
                }
            }
        }
    }

    pub fn open_import_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio Files", &["wav", "wave"])
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
                    // Load all audio buffers into engine and waveform cache
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
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.transport_state() == TransportState::Playing {
            ctx.request_repaint();
        }

        // Handle dropped files
        let mut files_to_import: Vec<PathBuf> = Vec::new();
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
                if let Some(path) = &file.path {
                    if let Some(ext) = path.extension() {
                        let ext = ext.to_string_lossy().to_lowercase();
                        if ext == "wav" || ext == "wave" {
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
        let mut do_undo = false;
        let mut do_redo = false;
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Space) {
                if self.transport_state() == TransportState::Playing {
                    self.send_command(EngineCommand::Stop);
                } else {
                    self.send_command(EngineCommand::Play);
                }
            }
            if i.modifiers.command && i.key_pressed(egui::Key::Z) {
                if i.modifiers.shift {
                    do_redo = true;
                } else {
                    do_undo = true;
                }
            }
            if i.modifiers.command && i.key_pressed(egui::Key::S) {
                // save handled below
            }
        });
        if do_undo {
            self.undo();
        }
        if do_redo {
            self.redo();
        }

        // Cmd+S save
        let mut do_save = false;
        ctx.input(|i| {
            if i.modifiers.command && i.key_pressed(egui::Key::S) {
                do_save = true;
            }
        });
        if do_save {
            self.save_project();
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
                    if ui.button("Open Project...").clicked() {
                        ui.close_menu();
                        self.load_project_dialog();
                    }
                    if ui.button("Save Project").clicked() {
                        ui.close_menu();
                        self.save_project();
                    }
                    ui.separator();
                    if ui.button("Import Audio...").clicked() {
                        ui.close_menu();
                        self.open_import_dialog();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    let undo_label = self
                        .undo_manager
                        .undo_label()
                        .map(|l| format!("Undo {l}"))
                        .unwrap_or_else(|| "Undo".into());
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
                        .map(|l| format!("Redo {l}"))
                        .unwrap_or_else(|| "Redo".into());
                    if ui
                        .add_enabled(self.undo_manager.can_redo(), egui::Button::new(redo_label))
                        .clicked()
                    {
                        self.redo();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Session", |ui| {
                    let connected = self.session.is_connected();
                    let label = if connected { "Session Panel (connected)" } else { "Session Panel" };
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
                    if time.elapsed().as_secs() < 5 {
                        ui.label(msg);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!(
                        "{}Hz | {} tracks",
                        self.sample_rate(),
                        self.project.tracks.len()
                    ));
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
                    track_id, volume, pan, muted, solo, ..
                } => {
                    if let Some(track) = self.project.tracks.iter_mut().find(|t| t.id == track_id) {
                        if let Some(v) = volume { track.volume = v; }
                        if let Some(p) = pan { track.pan = p; }
                        if let Some(m) = muted { track.muted = m; }
                        if let Some(s) = solo { track.solo = s; }
                    }
                    self.sync_project();
                }
                jamhub_network::message::SessionMessage::TempoChange { tempo, .. } => {
                    self.project.tempo = tempo;
                    self.sync_project();
                }
                jamhub_network::message::SessionMessage::Welcome { tracks, tempo, time_signature, .. } => {
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

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.view {
                View::Arrange => timeline::show(self, ui),
                View::Mixer => mixer_view::show(self, ui),
            }
        });
    }
}
