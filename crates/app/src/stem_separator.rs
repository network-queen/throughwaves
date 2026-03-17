use eframe::egui;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::DawApp;

const SERVICE_URL: &str = "http://localhost:8000";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const DOCKER_IMAGE: &str = "jamhub/stem-separator:latest";
const CONTAINER_NAME: &str = "jamhub-stem-separator";

/// Docker environment state.
#[derive(Debug, Clone, PartialEq)]
pub enum DockerState {
    Unknown,
    NotInstalled,
    ImageMissing,
    Pulling { progress: String },
    ContainerStopped,
    Running,
}

fn check_docker_installed() -> bool {
    std::process::Command::new("docker").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn check_docker_image_exists() -> bool {
    std::process::Command::new("docker").args(["image", "inspect", DOCKER_IMAGE])
        .output().map(|o| o.status.success()).unwrap_or(false)
}

fn check_container_running() -> bool {
    std::process::Command::new("docker").args(["inspect", "-f", "{{.State.Running}}", CONTAINER_NAME])
        .output().map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true").unwrap_or(false)
}

fn start_docker_container() -> Result<(), String> {
    // Remove old container if exists
    let _ = std::process::Command::new("docker").args(["rm", "-f", CONTAINER_NAME]).output();
    let output = std::process::Command::new("docker")
        .args(["run", "-d", "--name", CONTAINER_NAME, "-p", "8000:8000", DOCKER_IMAGE])
        .output().map_err(|e| format!("Failed to start container: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn stop_docker_container() {
    let _ = std::process::Command::new("docker").args(["stop", CONTAINER_NAME]).output();
}

/// Which input mode the user has selected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    File,
    Url,
}

/// Status of the separation job.
#[derive(Debug, Clone, PartialEq)]
pub enum JobState {
    /// No job running, ready for input.
    Idle,
    /// Submitted, waiting / processing.
    Running {
        job_id: String,
        status: String,
        progress: f32,
        message: String,
    },
    /// Separation complete -- stems available.
    Complete {
        job_id: String,
        stems: Vec<String>,
    },
    /// Something went wrong.
    Failed {
        error: String,
    },
}

#[allow(dead_code)]
enum BgResult {
    HealthCheck(bool),
    JobStarted { job_id: String },
    JobStatus {
        status: String,
        progress: f32,
        message: String,
        stems: Option<Vec<String>>,
        error: Option<String>,
    },
    StemDownloaded {
        stem: String,
        path: PathBuf,
    },
    Error(String),
}

/// Persistent state for the stem separator panel.
pub struct StemSeparatorPanel {
    pub show: bool,
    pub input_mode: InputMode,
    pub url_input: String,
    pub selected_file: Option<PathBuf>,
    pub job_state: JobState,
    pub service_available: Option<bool>,
    pub docker_state: DockerState,
    pub docker_pulling: bool,
    last_health_check: Instant,
    last_poll: Instant,
    /// Background thread result channel.
    bg_result: Arc<Mutex<Option<BgResult>>>,
    /// Pending stem download to import (stem_name, file_path)
    pub _pending_stem: Option<(String, PathBuf)>,
}

impl Default for StemSeparatorPanel {
    fn default() -> Self {
        Self {
            show: false,
            input_mode: InputMode::Url,
            url_input: String::new(),
            selected_file: None,
            job_state: JobState::Idle,
            service_available: None,
            docker_state: DockerState::Unknown,
            docker_pulling: false,
            last_health_check: Instant::now() - Duration::from_secs(60),
            last_poll: Instant::now(),
            bg_result: Arc::new(Mutex::new(None)),
            _pending_stem: None,
        }
    }
}

impl StemSeparatorPanel {
    /// Check if the Python service is running (non-blocking).
    fn check_health(&mut self) {
        let result = Arc::clone(&self.bg_result);
        std::thread::spawn(move || {
            let agent = ureq::Agent::new_with_defaults();
            let available = match agent.get(&format!("{SERVICE_URL}/health")).call() {
                Ok(resp) => resp.status().as_u16() == 200,
                Err(_) => false,
            };
            if let Ok(mut r) = result.lock() {
                *r = Some(BgResult::HealthCheck(available));
            }
        });
    }

    /// Submit a separation job with a URL.
    fn submit_url(&mut self, url: String) {
        let result = Arc::clone(&self.bg_result);
        std::thread::spawn(move || {
            let agent = ureq::Agent::new_with_defaults();
            let body = serde_json::json!({"url": url});
            match agent
                .post(&format!("{SERVICE_URL}/separate/url"))
                .header("Content-Type", "application/json")
                .send(body.to_string().as_bytes())
            {
                Ok(resp) => {
                    if let Ok(text) = resp.into_body().read_to_string() {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(job_id) = json["job_id"].as_str() {
                                if let Ok(mut r) = result.lock() {
                                    *r = Some(BgResult::JobStarted {
                                        job_id: job_id.to_string(),
                                    });
                                }
                                return;
                            }
                        }
                    }
                    if let Ok(mut r) = result.lock() {
                        *r = Some(BgResult::Error("Invalid response from service".into()));
                    }
                }
                Err(e) => {
                    if let Ok(mut r) = result.lock() {
                        *r = Some(BgResult::Error(format!("Request failed: {e}")));
                    }
                }
            }
        });
    }

    /// Submit a separation job with a file upload.
    fn submit_file(&mut self, path: PathBuf) {
        let result = Arc::clone(&self.bg_result);
        std::thread::spawn(move || {
            let file_data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(e) => {
                    if let Ok(mut r) = result.lock() {
                        *r = Some(BgResult::Error(format!("Cannot read file: {e}")));
                    }
                    return;
                }
            };

            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "audio.wav".to_string());

            // Build multipart form body manually
            let boundary = format!("----JamHub{}", uuid::Uuid::new_v4().simple());
            let mut body = Vec::new();

            // File part
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
            body.extend_from_slice(&file_data);
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

            let content_type = format!("multipart/form-data; boundary={boundary}");

            let agent = ureq::Agent::new_with_defaults();
            match agent
                .post(&format!("{SERVICE_URL}/separate"))
                .header("Content-Type", &content_type)
                .send(&body[..])
            {
                Ok(resp) => {
                    if let Ok(text) = resp.into_body().read_to_string() {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(job_id) = json["job_id"].as_str() {
                                if let Ok(mut r) = result.lock() {
                                    *r = Some(BgResult::JobStarted {
                                        job_id: job_id.to_string(),
                                    });
                                }
                                return;
                            }
                        }
                    }
                    if let Ok(mut r) = result.lock() {
                        *r = Some(BgResult::Error("Invalid response from service".into()));
                    }
                }
                Err(e) => {
                    if let Ok(mut r) = result.lock() {
                        *r = Some(BgResult::Error(format!("Upload failed: {e}")));
                    }
                }
            }
        });
    }

    /// Poll the job status.
    fn poll_status(&mut self, job_id: &str) {
        let result = Arc::clone(&self.bg_result);
        let url = format!("{SERVICE_URL}/status/{job_id}");
        std::thread::spawn(move || {
            let agent = ureq::Agent::new_with_defaults();
            match agent.get(&url).call() {
                Ok(resp) => {
                    if let Ok(text) = resp.into_body().read_to_string() {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            let status = json["status"].as_str().unwrap_or("unknown").to_string();
                            let progress = json["progress"].as_f64().unwrap_or(0.0) as f32;
                            let message = json["message"].as_str().unwrap_or("").to_string();
                            let stems = json["stems"].as_array().map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            });
                            let error = json["error"].as_str().map(String::from);
                            if let Ok(mut r) = result.lock() {
                                *r = Some(BgResult::JobStatus {
                                    status,
                                    progress,
                                    message,
                                    stems,
                                    error,
                                });
                            }
                        }
                    }
                }
                Err(e) => {
                    if let Ok(mut r) = result.lock() {
                        *r = Some(BgResult::Error(format!("Poll failed: {e}")));
                    }
                }
            }
        });
    }

    /// Process any pending background result.
    fn process_bg_result(&mut self) {
        let result = {
            let mut guard = self.bg_result.lock().unwrap();
            guard.take()
        };

        if let Some(bg) = result {
            match bg {
                BgResult::HealthCheck(available) => {
                    self.service_available = Some(available);
                }
                BgResult::JobStarted { job_id } => {
                    self.job_state = JobState::Running {
                        job_id,
                        status: "pending".into(),
                        progress: 0.0,
                        message: "Submitted...".into(),
                    };
                }
                BgResult::JobStatus {
                    status,
                    progress,
                    message,
                    stems,
                    error,
                } => {
                    if status == "complete" {
                        if let Some(stems) = stems {
                            let job_id = match &self.job_state {
                                JobState::Running { job_id, .. } => job_id.clone(),
                                _ => String::new(),
                            };
                            self.job_state = JobState::Complete { job_id, stems };
                        }
                    } else if status == "failed" {
                        self.job_state = JobState::Failed {
                            error: error.unwrap_or_else(|| "Unknown error".into()),
                        };
                    } else {
                        // Still running -- update progress
                        let jid = match &self.job_state {
                            JobState::Running { job_id, .. } => job_id.clone(),
                            _ => String::new(),
                        };
                        self.job_state = JobState::Running {
                            job_id: jid,
                            status,
                            progress,
                            message,
                        };
                    }
                }
                BgResult::StemDownloaded { stem, path } => {
                    // Store for the show() function to handle with DawApp access
                    self._pending_stem = Some((stem, path));
                }
                BgResult::Error(err) => {
                    if !matches!(self.job_state, JobState::Idle) {
                        self.job_state = JobState::Failed { error: err };
                    }
                }
            }
        }
    }
}

/// Stem display info.
struct StemInfo {
    name: &'static str,
    label: &'static str,
    color: egui::Color32,
}

const STEMS: [StemInfo; 4] = [
    StemInfo {
        name: "vocals",
        label: "Vocals",
        color: egui::Color32::from_rgb(235, 130, 180),
    },
    StemInfo {
        name: "drums",
        label: "Drums",
        color: egui::Color32::from_rgb(235, 180, 60),
    },
    StemInfo {
        name: "bass",
        label: "Bass",
        color: egui::Color32::from_rgb(100, 180, 255),
    },
    StemInfo {
        name: "other",
        label: "Other",
        color: egui::Color32::from_rgb(140, 220, 140),
    },
];

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.stem_sep.show {
        return;
    }

    // Process any pending background results
    app.stem_sep.process_bg_result();

    // Handle pending stem imports (needs DawApp access)
    if let Some((stem, path)) = app.stem_sep._pending_stem.take() {
        if let Ok(audio) = jamhub_engine::load_audio(&path) {
            let buffer_id = uuid::Uuid::new_v4();
            let engine_sr = app.sample_rate();
            let samples = if audio.sample_rate != engine_sr {
                jamhub_engine::resample(&audio.samples, audio.sample_rate, engine_sr)
            } else {
                audio.samples
            };
            let duration = samples.len() as u64;

            app.waveform_cache.insert(buffer_id, &samples);
            app.send_command(jamhub_engine::EngineCommand::LoadAudioBuffer {
                id: buffer_id,
                samples: samples.clone(),
            });
            app.audio_buffers.insert(buffer_id, samples);

            let stem_label = capitalize(&stem);
            let track_name = format!("Stem: {stem_label}");
            if let Some(ti) = app.project.tracks.iter().rposition(|t| t.name == track_name) {
                let clip = jamhub_model::Clip {
                    id: uuid::Uuid::new_v4(),
                    name: stem_label.clone(),
                    start_sample: 0,
                    duration_samples: duration,
                    source: jamhub_model::ClipSource::AudioBuffer { buffer_id },
                    muted: false, content_offset: 0,
                    fade_in_samples: 0, fade_out_samples: 0,
                    color: None, playback_rate: 1.0, preserve_pitch: false,
                    loop_count: 1, gain_db: 0.0, take_index: 0,
                    transpose_semitones: 0, reversed: false,
                };
                app.project.tracks[ti].clips.push(clip);
                app.sync_project();
                app.set_status(&format!("{stem_label} stem imported!"));
            }
        } else {
            app.set_status(&format!("Failed to load {} stem audio", stem));
        }
    }

    // Periodic health check
    if app.stem_sep.last_health_check.elapsed() > Duration::from_secs(10) {
        app.stem_sep.last_health_check = Instant::now();
        app.stem_sep.check_health();
    }

    // Poll job status if running
    if let JobState::Running { ref job_id, .. } = app.stem_sep.job_state {
        if app.stem_sep.last_poll.elapsed() >= POLL_INTERVAL {
            app.stem_sep.last_poll = Instant::now();
            let jid = job_id.clone();
            app.stem_sep.poll_status(&jid);
            ctx.request_repaint_after(POLL_INTERVAL);
        } else {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    let mut open = true;
    egui::Window::new("AI Stem Separation")
        .open(&mut open)
        .default_width(420.0)
        .min_width(360.0)
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            // Service availability — Docker management
            match app.stem_sep.service_available {
                Some(false) | None => {
                    // Check Docker state if unknown
                    if app.stem_sep.docker_state == DockerState::Unknown {
                        if !check_docker_installed() {
                            app.stem_sep.docker_state = DockerState::NotInstalled;
                        } else if check_container_running() {
                            app.stem_sep.docker_state = DockerState::Running;
                        } else if check_docker_image_exists() {
                            app.stem_sep.docker_state = DockerState::ContainerStopped;
                        } else {
                            app.stem_sep.docker_state = DockerState::ImageMissing;
                        }
                    }

                    match &app.stem_sep.docker_state {
                        DockerState::NotInstalled => {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("!").size(14.0).strong().color(egui::Color32::from_rgb(232, 80, 80)));
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new("Docker is required for AI stem separation").size(12.0).color(egui::Color32::from_rgb(232, 80, 80)));
                                    ui.label(egui::RichText::new("Install Docker Desktop from https://docker.com/download").size(10.0).color(egui::Color32::from_rgb(140, 138, 132)));
                                });
                            });
                        }
                        DockerState::ImageMissing => {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("!").size(14.0).strong().color(egui::Color32::from_rgb(235, 180, 60)));
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new("Stem separator image not found").size(12.0).color(egui::Color32::from_rgb(200, 160, 60)));
                                    if app.stem_sep.docker_pulling {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label(egui::RichText::new("Building image... this may take a few minutes").size(10.0).color(egui::Color32::from_rgb(140, 140, 150)));
                                        });
                                    } else {
                                        if ui.button(egui::RichText::new("Build Docker Image").color(egui::Color32::from_rgb(235, 180, 60))).clicked() {
                                            app.stem_sep.docker_pulling = true;
                                            std::thread::spawn(|| {
                                                let dockerfile_dir = std::path::Path::new("tools/stem_separator");
                                                let _ = std::process::Command::new("docker")
                                                    .args(["build", "-t", DOCKER_IMAGE, "."])
                                                    .current_dir(dockerfile_dir)
                                                    .output();
                                            });
                                        }
                                        ui.label(egui::RichText::new("First time setup — downloads ~2GB of AI models").size(9.0).color(egui::Color32::from_rgb(110, 110, 120)));
                                    }
                                });
                            });
                            // Re-check periodically while pulling
                            if app.stem_sep.docker_pulling && app.stem_sep.last_health_check.elapsed() > Duration::from_secs(5) {
                                app.stem_sep.last_health_check = Instant::now();
                                if check_docker_image_exists() {
                                    app.stem_sep.docker_pulling = false;
                                    app.stem_sep.docker_state = DockerState::ContainerStopped;
                                }
                                ctx.request_repaint();
                            }
                        }
                        DockerState::Pulling { progress } => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(egui::RichText::new(format!("Pulling image... {progress}")).size(11.0).color(egui::Color32::from_rgb(140, 140, 150)));
                            });
                        }
                        DockerState::ContainerStopped => {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("!").size(14.0).strong().color(egui::Color32::from_rgb(235, 180, 60)));
                                ui.label(egui::RichText::new("Stem separator ready").size(12.0).color(egui::Color32::from_rgb(200, 160, 60)));
                                if ui.button(egui::RichText::new("Start").color(egui::Color32::from_rgb(80, 200, 80))).clicked() {
                                    match start_docker_container() {
                                        Ok(()) => {
                                            app.stem_sep.docker_state = DockerState::Running;
                                            app.stem_sep.service_available = None; // Re-check health
                                            app.stem_sep.last_health_check = Instant::now() - Duration::from_secs(60);
                                        }
                                        Err(e) => app.set_status(&format!("Docker start failed: {e}")),
                                    }
                                }
                            });
                        }
                        DockerState::Running => {
                            // Container running but service not responding yet — might be starting up
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(egui::RichText::new("Service starting up...").size(11.0).color(egui::Color32::from_rgb(140, 140, 150)));
                            });
                        }
                        DockerState::Unknown => {}
                    }

                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(4.0);
                }
                Some(true) => {
                    // Service is running — show green indicator
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("*").size(10.0).color(egui::Color32::from_rgb(80, 200, 80)));
                        ui.label(egui::RichText::new("AI service connected").size(9.0).color(egui::Color32::from_rgb(80, 200, 80)));
                    });
                    ui.add_space(4.0);
                }
            }

            match app.stem_sep.job_state.clone() {
                JobState::Idle => {
                    show_input_ui(app, ui);
                }
                JobState::Running {
                    progress, message, ..
                } => {
                    show_progress_ui(ui, progress, &message);
                }
                JobState::Complete {
                    ref job_id,
                    ref stems,
                } => {
                    let job_id = job_id.clone();
                    let stems = stems.clone();
                    show_results_ui(app, ui, &job_id, &stems);
                }
                JobState::Failed { ref error } => {
                    let err = error.clone();
                    show_error_ui(app, ui, &err);
                }
            }
        });

    if !open {
        app.stem_sep.show = false;
    }
}

fn show_input_ui(app: &mut DawApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.stem_sep.input_mode, InputMode::Url, "From URL");
        ui.selectable_value(&mut app.stem_sep.input_mode, InputMode::File, "Local File");
    });
    ui.add_space(6.0);

    match app.stem_sep.input_mode {
        InputMode::Url => {
            ui.label(
                egui::RichText::new("Paste a YouTube, SoundCloud, or Spotify URL:")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 160, 170)),
            );
            ui.add_space(4.0);

            let response = ui.add(
                egui::TextEdit::singleline(&mut app.stem_sep.url_input)
                    .hint_text("https://youtube.com/watch?v=...")
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let can_submit = !app.stem_sep.url_input.trim().is_empty()
                    && app.stem_sep.service_available == Some(true);

                if ui
                    .add_enabled(
                        can_submit,
                        egui::Button::new(
                            egui::RichText::new("Separate Stems")
                                .color(egui::Color32::from_rgb(18, 18, 22)),
                        )
                        .fill(egui::Color32::from_rgb(235, 180, 60))
                        .corner_radius(6.0),
                    )
                    .clicked()
                    || (response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && can_submit)
                {
                    let url = app.stem_sep.url_input.trim().to_string();
                    app.stem_sep.submit_url(url);
                }
            });
        }
        InputMode::File => {
            ui.label(
                egui::RichText::new("Select an audio file (WAV, MP3, FLAC, OGG):")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 160, 170)),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui.button("Choose File...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Audio Files", &["wav", "mp3", "flac", "ogg", "m4a", "aac"])
                        .pick_file()
                    {
                        app.stem_sep.selected_file = Some(path);
                    }
                }
                if let Some(ref path) = app.stem_sep.selected_file {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown".into());
                    ui.label(
                        egui::RichText::new(name)
                            .size(11.0)
                            .color(egui::Color32::from_rgb(200, 200, 210)),
                    );
                }
            });

            ui.add_space(6.0);
            let can_submit = app.stem_sep.selected_file.is_some()
                && app.stem_sep.service_available == Some(true);

            if ui
                .add_enabled(
                    can_submit,
                    egui::Button::new(
                        egui::RichText::new("Separate Stems")
                            .color(egui::Color32::from_rgb(18, 18, 22)),
                    )
                    .fill(egui::Color32::from_rgb(235, 180, 60))
                    .corner_radius(6.0),
                )
                .clicked()
            {
                if let Some(path) = app.stem_sep.selected_file.clone() {
                    app.stem_sep.submit_file(path);
                }
            }
        }
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Powered by Meta Demucs -- separates into Vocals, Drums, Bass, Other",
        )
        .size(10.0)
        .color(egui::Color32::from_rgb(100, 100, 110)),
    );
}

fn show_progress_ui(ui: &mut egui::Ui, progress: f32, message: &str) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("AI Processing...")
                .size(16.0)
                .color(egui::Color32::from_rgb(235, 180, 60)),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(message)
                .size(11.0)
                .color(egui::Color32::from_rgb(160, 160, 170)),
        );
        ui.add_space(12.0);

        let bar = egui::ProgressBar::new(progress)
            .show_percentage()
            .animate(true);
        ui.add(bar);

        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("This typically takes 30-60 seconds")
                .size(10.0)
                .color(egui::Color32::from_rgb(100, 100, 110)),
        );
    });
    ui.add_space(20.0);
}

fn show_results_ui(app: &mut DawApp, ui: &mut egui::Ui, job_id: &str, stems: &[String]) {
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("Separation Complete")
                .size(14.0)
                .color(egui::Color32::from_rgb(100, 220, 100)),
        );
    });
    ui.add_space(8.0);

    // Stem cards
    for stem_info in &STEMS {
        if !stems.contains(&stem_info.name.to_string()) {
            continue;
        }

        egui::Frame::default()
            .fill(egui::Color32::from_rgb(28, 29, 34))
            .corner_radius(8.0)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(egui::Stroke::new(1.0, stem_info.color.gamma_multiply(0.3)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(stem_info.label)
                            .size(13.0)
                            .color(stem_info.color)
                            .strong(),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let jid = job_id.to_string();
                        let stem_name = stem_info.name.to_string();
                        if ui
                            .button(egui::RichText::new("Import to Track").size(11.0))
                            .clicked()
                        {
                            import_stem_to_track(app, &jid, &stem_name);
                        }
                    });
                });
            });
        ui.add_space(2.0);
    }

    ui.add_space(8.0);

    // Import All + New Separation buttons
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("Import All as Tracks")
                        .color(egui::Color32::from_rgb(18, 18, 22)),
                )
                .fill(egui::Color32::from_rgb(235, 180, 60))
                .corner_radius(6.0),
            )
            .clicked()
        {
            let jid = job_id.to_string();
            for stem_info in &STEMS {
                if stems.contains(&stem_info.name.to_string()) {
                    import_stem_to_track(app, &jid, stem_info.name);
                }
            }
        }

        if ui
            .button(egui::RichText::new("New Separation").size(11.0))
            .clicked()
        {
            app.stem_sep.job_state = JobState::Idle;
            app.stem_sep.url_input.clear();
            app.stem_sep.selected_file = None;
        }
    });
}

fn show_error_ui(app: &mut DawApp, ui: &mut egui::Ui, error: &str) {
    ui.add_space(10.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("Separation Failed")
                .size(14.0)
                .color(egui::Color32::from_rgb(220, 80, 80)),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(error)
                .size(11.0)
                .color(egui::Color32::from_rgb(200, 160, 160)),
        );
    });
    ui.add_space(10.0);

    if ui.button("Try Again").clicked() {
        app.stem_sep.job_state = JobState::Idle;
    }
}

/// Download a stem WAV file and import it as a new track in the DAW.
fn import_stem_to_track(app: &mut DawApp, job_id: &str, stem: &str) {
    let stem_label = capitalize(stem);

    // Create a new audio track immediately
    use jamhub_model::TrackKind;
    app.project
        .add_track(&format!("Stem: {stem_label}"), TrackKind::Audio);

    // Download stem file in background and load it as a clip
    let url = format!("{SERVICE_URL}/stems/{job_id}/{stem}");
    let stem_copy = stem.to_string();
    let result = Arc::clone(&app.stem_sep.bg_result);

    std::thread::spawn(move || {
        let agent = ureq::Agent::new_with_defaults();
        match agent.get(&url).call() {
            Ok(resp) => {
                let tmp_dir = std::env::temp_dir().join("jamhub_stems");
                let _ = std::fs::create_dir_all(&tmp_dir);
                let dest = tmp_dir.join(format!("{stem_copy}.wav"));
                if let Ok(bytes) = resp.into_body().read_to_vec() {
                    let _ = std::fs::write(&dest, &bytes);
                    if let Ok(mut r) = result.lock() {
                        *r = Some(BgResult::StemDownloaded {
                            stem: stem_copy,
                            path: dest,
                        });
                    }
                }
            }
            Err(_) => {}
        }
    });

    app.status_message = Some((
        format!("Importing {stem_label} stem as new track..."),
        Instant::now(),
    ));
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
