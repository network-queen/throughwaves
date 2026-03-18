use eframe::egui;

use crate::DawApp;
use jamhub_engine::{ExportFormat, ExportOptions};

/// Track metadata returned by the platform API.
#[derive(Default, Clone)]
pub struct PlatformBand {
    pub id: String,
    pub name: String,
}

#[derive(Default, Clone)]
pub struct PlatformTrack {
    pub id: String,
    pub title: String,
    pub play_count: u64,
    pub like_count: u64,
}

/// State for the ThroughWaves platform integration panel.
pub struct PlatformPanel {
    pub server_url: String,
    pub jwt_token: Option<String>,
    pub username: Option<String>,
    pub logged_in: bool,
    pub show_panel: bool,

    // Login / register form
    pub email: String,
    pub password: String,
    pub login_error: Option<String>,
    pub is_registering: bool,

    // Upload form
    pub upload_title: String,
    pub upload_description: String,
    pub upload_genre: String,
    pub upload_status: Option<String>,
    pub uploading: bool,

    // Upload-to-project form
    pub upload_project_id: String,

    // Checkout form
    pub checkout_project_id: String,
    pub checkout_status: Option<String>,

    // Import track form
    pub import_track_id: String,
    pub import_track_status: Option<String>,

    // Cloud project
    pub cloud_upload_status: Option<String>,
    pub cloud_download_id: String,
    pub cloud_download_status: Option<String>,

    // Bands
    pub bands: Vec<PlatformBand>,
    pub bands_loaded: bool,
    pub selected_band_idx: usize, // 0 = none, 1+ = band index

    // My tracks list
    pub my_tracks: Vec<PlatformTrack>,
    pub tracks_loaded: bool,
}

impl Default for PlatformPanel {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:3000".into(),
            jwt_token: None,
            username: None,
            logged_in: false,
            show_panel: false,
            email: String::new(),
            password: String::new(),
            login_error: None,
            is_registering: false,
            upload_title: String::new(),
            upload_description: String::new(),
            upload_genre: String::new(),
            upload_status: None,
            uploading: false,
            upload_project_id: String::new(),
            checkout_project_id: String::new(),
            checkout_status: None,
            import_track_id: String::new(),
            import_track_status: None,
            cloud_upload_status: None,
            cloud_download_id: String::new(),
            cloud_download_status: None,
            bands: Vec::new(),
            bands_loaded: false,
            selected_band_idx: 0,
            my_tracks: Vec::new(),
            tracks_loaded: false,
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

/// Perform an authenticated (or unauthenticated) JSON request to the platform API.
fn platform_request(
    method: &str,
    base_url: &str,
    path: &str,
    jwt: Option<&str>,
    body: Option<&str>,
) -> Result<String, String> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let agent = ureq::Agent::new_with_defaults();

    let auth_value;
    let auth_header: Option<(&str, &str)> = if let Some(token) = jwt {
        auth_value = format!("Bearer {token}");
        Some(("Authorization", &auth_value))
    } else {
        None
    };

    // ureq v3 uses different types for WithBody vs WithoutBody requests,
    // so we dispatch and read the response body in each branch.
    let read_body = |resp: ureq::http::Response<ureq::Body>| -> Result<String, String> {
        resp.into_body()
            .read_to_string()
            .map_err(|e| format!("Failed to read response body: {e}"))
    };

    match (method, body) {
        ("GET", _) => {
            let mut r = agent.get(&url);
            if let Some((k, v)) = auth_header { r = r.header(k, v); }
            r.call().map_err(|e| format!("HTTP request failed: {e}")).and_then(read_body)
        }
        ("DELETE", _) => {
            let mut r = agent.delete(&url);
            if let Some((k, v)) = auth_header { r = r.header(k, v); }
            r.call().map_err(|e| format!("HTTP request failed: {e}")).and_then(read_body)
        }
        ("POST", Some(json_body)) => {
            let mut r = agent.post(&url).header("Content-Type", "application/json");
            if let Some((k, v)) = auth_header { r = r.header(k, v); }
            r.send(json_body.as_bytes()).map_err(|e| format!("HTTP request failed: {e}")).and_then(read_body)
        }
        ("POST", None) => {
            let mut r = agent.post(&url);
            if let Some((k, v)) = auth_header { r = r.header(k, v); }
            r.send_empty().map_err(|e| format!("HTTP request failed: {e}")).and_then(read_body)
        }
        ("PUT", Some(json_body)) => {
            let mut r = agent.put(&url).header("Content-Type", "application/json");
            if let Some((k, v)) = auth_header { r = r.header(k, v); }
            r.send(json_body.as_bytes()).map_err(|e| format!("HTTP request failed: {e}")).and_then(read_body)
        }
        ("PUT", None) => {
            let mut r = agent.put(&url);
            if let Some((k, v)) = auth_header { r = r.header(k, v); }
            r.send_empty().map_err(|e| format!("HTTP request failed: {e}")).and_then(read_body)
        }
        _ => Err(format!("Unsupported HTTP method: {method}")),
    }
}

/// Upload a file via multipart POST.
fn platform_upload_file(
    base_url: &str,
    path: &str,
    jwt: &str,
    file_path: &std::path::Path,
    title: &str,
    description: &str,
    genre: &str,
    project_id: Option<&str>,
) -> Result<String, String> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);

    let file_data = std::fs::read(file_path)
        .map_err(|e| format!("Failed to read export file: {e}"))?;

    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("mixdown.wav");

    // Build a simple multipart body manually
    let boundary = "----ThroughWavesUploadBoundary9876543210";
    let mut body = Vec::new();

    // Title field
    append_multipart_field(&mut body, boundary, "title", title);
    // Description field
    append_multipart_field(&mut body, boundary, "description", description);
    // Genre field
    append_multipart_field(&mut body, boundary, "genre", genre);
    // Optional project ID
    if let Some(pid) = project_id {
        append_multipart_field(&mut body, boundary, "projectId", pid);
    }

    // File field — must be named "audio" to match the server's expected field name
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"audio\"; filename=\"{file_name}\"\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(&file_data);
    body.extend_from_slice(b"\r\n");

    // Closing boundary
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let content_type = format!("multipart/form-data; boundary={boundary}");

    let agent = ureq::Agent::new_with_defaults();
    let response = agent
        .post(&url)
        .header("Authorization", &format!("Bearer {jwt}"))
        .header("Content-Type", &content_type)
        .send(&*body);

    match response {
        Ok(resp) => resp
            .into_body()
            .read_to_string()
            .map_err(|e| format!("Failed to read upload response: {e}")),
        Err(e) => Err(format!("Upload failed: {e}")),
    }
}

fn append_multipart_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(value.as_bytes());
    body.extend_from_slice(b"\r\n");
}

// ---------------------------------------------------------------------------
// Panel implementation
// ---------------------------------------------------------------------------

impl PlatformPanel {
    pub fn login(&mut self) {
        let body = format!(
            r#"{{"email":"{}","password":"{}"}}"#,
            self.email.replace('"', "\\\""),
            self.password.replace('"', "\\\"")
        );

        match platform_request("POST", &self.server_url, "/api/auth/login", None, Some(&body)) {
            Ok(resp) => {
                // Expect JSON with { "token": "...", "user": { "username": "..." } }
                if let Some(token) = extract_json_string(&resp, "token") {
                    let username = extract_json_string(&resp, "username")
                        .or_else(|| extract_json_string(&resp, "name"))
                        .or_else(|| extract_json_string(&resp, "email"))
                        .unwrap_or_else(|| self.email.clone());
                    self.jwt_token = Some(token);
                    self.username = Some(username);
                    self.logged_in = true;
                    self.login_error = None;
                    self.password.clear();
                } else {
                    let msg = extract_json_string(&resp, "message")
                        .or_else(|| extract_json_string(&resp, "error"))
                        .unwrap_or_else(|| "Login failed — unexpected response".into());
                    self.login_error = Some(msg);
                }
            }
            Err(e) => {
                self.login_error = Some(e);
            }
        }
    }

    pub fn register(&mut self) {
        // Derive a username from the email (part before @) if not provided separately
        let username = self.email.split('@').next().unwrap_or("user").replace('"', "\\\"");
        let body = format!(
            r#"{{"username":"{}","email":"{}","password":"{}"}}"#,
            username,
            self.email.replace('"', "\\\""),
            self.password.replace('"', "\\\"")
        );

        match platform_request("POST", &self.server_url, "/api/auth/register", None, Some(&body))
        {
            Ok(resp) => {
                if let Some(token) = extract_json_string(&resp, "token") {
                    let username = extract_json_string(&resp, "username")
                        .or_else(|| extract_json_string(&resp, "name"))
                        .or_else(|| extract_json_string(&resp, "email"))
                        .unwrap_or_else(|| self.email.clone());
                    self.jwt_token = Some(token);
                    self.username = Some(username);
                    self.logged_in = true;
                    self.login_error = None;
                    self.password.clear();
                } else {
                    let msg = extract_json_string(&resp, "message")
                        .or_else(|| extract_json_string(&resp, "error"))
                        .unwrap_or_else(|| "Registration failed".into());
                    self.login_error = Some(msg);
                }
            }
            Err(e) => {
                self.login_error = Some(e);
            }
        }
    }

    pub fn logout(&mut self) {
        self.jwt_token = None;
        self.username = None;
        self.logged_in = false;
        self.my_tracks.clear();
        self.tracks_loaded = false;
    }

    pub fn fetch_my_tracks(&mut self) {
        let Some(ref token) = self.jwt_token else {
            return;
        };
        match platform_request("GET", &self.server_url, "/api/tracks/me", Some(token), None) {
            Ok(resp) => {
                self.my_tracks = parse_tracks_list(&resp);
                self.tracks_loaded = true;
            }
            Err(_e) => {
                self.tracks_loaded = true;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Minimal JSON helpers (avoid pulling in serde for platform-only use)
// ---------------------------------------------------------------------------

/// Extract a string value for a given key from a JSON string.
/// This is intentionally simple — handles `"key": "value"` patterns.
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let idx = json.find(&pattern)?;
    let after_key = &json[idx + pattern.len()..];
    // Skip `: ` or `:`
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    if after_colon.starts_with('"') {
        let inner = &after_colon[1..];
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    } else {
        None
    }
}

/// Parse a JSON array of track objects into PlatformTrack list.
fn parse_tracks_list(json: &str) -> Vec<PlatformTrack> {
    // Very simple parser: split on `{` to find objects
    let mut tracks = Vec::new();
    for chunk in json.split('{').skip(1) {
        let id = extract_json_string(chunk, "id").unwrap_or_default();
        // Also try "_id" for MongoDB-style responses
        let id = if id.is_empty() {
            extract_json_string(chunk, "_id").unwrap_or_default()
        } else {
            id
        };
        let title = extract_json_string(chunk, "title").unwrap_or_else(|| "Untitled".into());
        let play_count = extract_json_string(chunk, "playCount")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let like_count = extract_json_string(chunk, "likeCount")
            .or_else(|| extract_json_string(chunk, "likes"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        if !id.is_empty() || !title.is_empty() {
            tracks.push(PlatformTrack {
                id,
                title,
                play_count,
                like_count,
            });
        }
    }
    tracks
}

// ---------------------------------------------------------------------------
// UI rendering
// ---------------------------------------------------------------------------

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.platform.show_panel {
        return;
    }

    let mut open = true;
    egui::Window::new("Platform")
        .open(&mut open)
        .default_width(340.0)
        .resizable(true)
        .show(ctx, |ui| {
            // Server URL
            ui.horizontal(|ui| {
                ui.label("Server:");
                ui.text_edit_singleline(&mut app.platform.server_url);
            });
            ui.separator();

            if app.platform.logged_in {
                show_logged_in(app, ui);
            } else {
                show_login_form(app, ui);
            }
        });

    if !open {
        app.platform.show_panel = false;
    }
}

fn show_login_form(app: &mut DawApp, ui: &mut egui::Ui) {
    ui.heading(if app.platform.is_registering {
        "Register"
    } else {
        "Login"
    });

    ui.horizontal(|ui| {
        ui.label("Email:");
        ui.text_edit_singleline(&mut app.platform.email);
    });
    ui.horizontal(|ui| {
        ui.label("Password:");
        let response = ui.add(egui::TextEdit::singleline(&mut app.platform.password).password(true));
        // Submit on Enter
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if app.platform.is_registering {
                app.platform.register();
            } else {
                app.platform.login();
            }
        }
    });

    if let Some(ref err) = app.platform.login_error {
        ui.colored_label(egui::Color32::from_rgb(255, 100, 100), err);
    }

    ui.horizontal(|ui| {
        if app.platform.is_registering {
            if ui.button("Register").clicked() {
                app.platform.register();
            }
            if ui.button("Back to Login").clicked() {
                app.platform.is_registering = false;
                app.platform.login_error = None;
            }
        } else {
            if ui.button("Login").clicked() {
                app.platform.login();
            }
            if ui.button("Create Account").clicked() {
                app.platform.is_registering = true;
                app.platform.login_error = None;
            }
        }
    });
}

fn show_logged_in(app: &mut DawApp, ui: &mut egui::Ui) {
    let username = app
        .platform
        .username
        .clone()
        .unwrap_or_else(|| "User".into());
    ui.horizontal(|ui| {
        ui.colored_label(
            egui::Color32::from_rgb(80, 200, 80),
            format!("Logged in as {username}"),
        );
        if ui.button("Logout").clicked() {
            app.platform.logout();
        }
    });

    ui.separator();

    ui.separator();

    // ── Import Track into DAW ──
    egui::CollapsingHeader::new("Import Track into DAW")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Track ID or URL:");
                ui.text_edit_singleline(&mut app.platform.import_track_id);
            });

            if let Some(ref status) = app.platform.import_track_status {
                ui.label(status.as_str());
            }

            if ui.button("Import Track").clicked() {
                do_import_track(app);
            }
        });

    ui.separator();

    ui.separator();

    // Auto-fill upload title from project name if empty
    if app.platform.upload_title.is_empty() && !app.project.name.is_empty() && app.project.name != "Untitled Session" {
        app.platform.upload_title = app.project.name.clone();
    }

    // ── Cloud Project (Upload/Download full project) ──
    egui::CollapsingHeader::new("Cloud Project")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Push your project with all stems. Others hear the mixdown; you can pull all stems back.").size(10.0).weak());
            ui.add_space(6.0);

            // Upload form
            ui.group(|ui| {
                ui.label(egui::RichText::new("Push to Cloud").size(12.0).strong());
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Project Name:");
                    ui.text_edit_singleline(&mut app.platform.upload_title);
                });
                ui.horizontal(|ui| {
                    ui.label("Artist / Author:");
                    ui.text_edit_singleline(&mut app.platform.upload_description);
                });
                ui.horizontal(|ui| {
                    ui.label("Genre:");
                    ui.text_edit_singleline(&mut app.platform.upload_genre);
                });

                // Band selector
                if !app.platform.bands_loaded {
                    if let Some(ref jwt) = app.platform.jwt_token {
                        if let Ok(resp) = platform_request("GET", &app.platform.server_url, "/api/bands", Some(jwt), None) {
                            app.platform.bands.clear();
                            // Parse bands from JSON array
                            let mut idx = 0;
                            while let Some(pos) = resp[idx..].find("\"id\"") {
                                let abs = idx + pos;
                                if let Some(id) = extract_json_string(&resp[abs..], "id") {
                                    if let Some(name) = extract_json_string(&resp[abs..], "name") {
                                        app.platform.bands.push(PlatformBand { id, name });
                                    }
                                }
                                idx = abs + 4;
                            }
                        }
                        app.platform.bands_loaded = true;
                    }
                }

                if !app.platform.bands.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Band:");
                        let selected_name = if app.platform.selected_band_idx == 0 {
                            "No band (personal)".to_string()
                        } else {
                            app.platform.bands.get(app.platform.selected_band_idx - 1)
                                .map(|b| b.name.clone())
                                .unwrap_or("Select...".into())
                        };
                        egui::ComboBox::from_id_salt("band_select")
                            .selected_text(&selected_name)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(app.platform.selected_band_idx == 0, "No band (personal)").clicked() {
                                    app.platform.selected_band_idx = 0;
                                }
                                for (i, band) in app.platform.bands.iter().enumerate() {
                                    if ui.selectable_label(app.platform.selected_band_idx == i + 1, &band.name).clicked() {
                                        app.platform.selected_band_idx = i + 1;
                                    }
                                }
                            });
                    });
                } else {
                    ui.label(egui::RichText::new("No bands yet — create one on the website").size(10.0).weak());
                }

                if let Some(ref status) = app.platform.cloud_upload_status {
                    ui.add_space(2.0);
                    ui.label(status.as_str());
                }

                ui.add_space(4.0);
                if ui.button("Push Project to Cloud").clicked() {
                    // Validate required fields
                    if app.platform.upload_title.trim().is_empty() {
                        app.platform.cloud_upload_status = Some("Project name is required".into());
                    } else {
                        // Use the form title instead of the project name
                        let original_name = app.project.name.clone();
                        app.project.name = app.platform.upload_title.trim().to_string();
                        do_upload_cloud_project(app);
                        app.project.name = original_name;
                    }
                }
            });

            ui.add_space(8.0);

            // Download form
            ui.group(|ui| {
                ui.label(egui::RichText::new("Pull from Cloud").size(12.0).strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Project ID:");
                    ui.text_edit_singleline(&mut app.platform.cloud_download_id);
                });

                if let Some(ref status) = app.platform.cloud_download_status {
                    ui.add_space(2.0);
                    ui.label(status.as_str());
                }

                ui.add_space(4.0);
                if ui.button("Pull Project from Cloud").clicked() {
                    do_download_cloud_project(app);
                }
            });
        });

    ui.separator();

    // ── My Tracks ──
    egui::CollapsingHeader::new("My Tracks")
        .default_open(false)
        .show(ui, |ui| {
            if ui.button("Refresh").clicked() || !app.platform.tracks_loaded {
                app.platform.fetch_my_tracks();
            }

            if app.platform.my_tracks.is_empty() {
                ui.label("No tracks uploaded yet.");
            } else {
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for track in &app.platform.my_tracks {
                            ui.group(|ui| {
                                ui.strong(&track.title);
                                ui.horizontal(|ui| {
                                    ui.label(format!("Plays: {}", track.play_count));
                                    ui.label(format!("Likes: {}", track.like_count));
                                });
                            });
                        }
                    });
            }
        });
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

fn do_upload_mixdown(app: &mut DawApp, project_id: Option<&str>) {
    app.platform.uploading = true;
    app.platform.upload_status = Some("Exporting mixdown...".into());

    // Export to a temp WAV file
    let temp_dir = std::env::temp_dir();
    let export_path = temp_dir.join("jamhub_platform_upload.wav");

    let sample_rate = app.project.sample_rate;
    let options = ExportOptions {
        format: ExportFormat::Wav,
        channels: 2,
        bit_depth: 16,
        normalize: true,
        ..Default::default()
    };

    let result = jamhub_engine::export_with_options(
        &export_path,
        &app.project,
        &app.audio_buffers,
        sample_rate,
        &options,
    );

    match result {
        Ok(()) => {
            app.platform.upload_status = Some("Uploading to platform...".into());

            let jwt = match &app.platform.jwt_token {
                Some(t) => t.clone(),
                None => {
                    app.platform.upload_status = Some("Error: not logged in".into());
                    app.platform.uploading = false;
                    return;
                }
            };

            let title = if app.platform.upload_title.is_empty() {
                app.project.name.clone()
            } else {
                app.platform.upload_title.clone()
            };

            match platform_upload_file(
                &app.platform.server_url,
                "/api/tracks",
                &jwt,
                &export_path,
                &title,
                &app.platform.upload_description,
                &app.platform.upload_genre,
                project_id,
            ) {
                Ok(_resp) => {
                    app.platform.upload_status = Some("Upload complete!".into());
                    app.set_status("Track uploaded to platform");
                }
                Err(e) => {
                    app.platform.upload_status = Some(format!("Upload failed: {e}"));
                }
            }

            // Clean up temp file
            let _ = std::fs::remove_file(&export_path);
        }
        Err(e) => {
            app.platform.upload_status = Some(format!("Export failed: {e}"));
        }
    }

    app.platform.uploading = false;
}

fn do_checkout_project(app: &mut DawApp) {
    let project_id = app.platform.checkout_project_id.trim().to_string();
    if project_id.is_empty() {
        app.platform.checkout_status = Some("Enter a project ID".into());
        return;
    }

    app.platform.checkout_status = Some("Downloading project...".into());

    let jwt = match &app.platform.jwt_token {
        Some(t) => t.clone(),
        None => {
            app.platform.checkout_status = Some("Error: not logged in".into());
            return;
        }
    };

    let path = format!("/api/projects/{}/checkout", project_id);
    match platform_request("POST", &app.platform.server_url, &path, Some(&jwt), None) {
        Ok(resp) => {
            // The response should contain project JSON data.
            // For now, extract the project name and report success.
            let name = extract_json_string(&resp, "name")
                .unwrap_or_else(|| format!("Project {project_id}"));
            app.platform.checkout_status = Some(format!("Loaded: {name}"));
            app.set_status(&format!("Checked out project: {name}"));

            // Try to import tracks from the response into the current session.
            // The platform returns track metadata; actual audio files would be
            // downloaded separately. For v1, we just acknowledge success.
            import_project_tracks(app, &resp);
        }
        Err(e) => {
            app.platform.checkout_status = Some(format!("Checkout failed: {e}"));
        }
    }
}

/// Import a single track from the platform into the DAW as a new audio track.
/// Upload the full project to the cloud: renders a mixdown + exports each track as a stem
fn do_upload_cloud_project(app: &mut DawApp) {
    let jwt = match &app.platform.jwt_token {
        Some(t) => t.clone(),
        None => {
            app.platform.cloud_upload_status = Some("Error: not logged in".into());
            return;
        }
    };

    if app.project.tracks.is_empty() {
        app.platform.cloud_upload_status = Some("No tracks to upload".into());
        return;
    }

    app.platform.cloud_upload_status = Some("Rendering mixdown + stems...".into());

    let sr = app.sample_rate();

    // Render each track as a stem, then mix for the mixdown
    let mut stems: Vec<(String, Vec<u8>)> = Vec::new();
    let mut all_bounced: Vec<Vec<f32>> = Vec::new();
    for i in 0..app.project.tracks.len() {
        let name = app.project.tracks[i].name.clone();
        let stem = match jamhub_engine::bounce_track(
            &app.project, i, &app.audio_buffers, sr,
        ) {
            Ok(samples) => {
                if samples.iter().all(|s| s.abs() < 0.0001) {
                    println!("[CLOUD] Skipping silent stem: {name}");
                    continue;
                }
                samples
            },
            Err(e) => {
                println!("[CLOUD] Failed to bounce track {i} ({name}): {e}");
                continue;
            },
        };
        all_bounced.push(stem.clone());
        let stem_bytes = encode_wav_bytes(&stem, sr);
        stems.push((name, stem_bytes));
    }

    // Create mixdown by summing all stems
    let max_len = all_bounced.iter().map(|s| s.len()).max().unwrap_or(0);
    let mut mixdown = vec![0.0f32; max_len];
    for stem in &all_bounced {
        for (i, &s) in stem.iter().enumerate() {
            mixdown[i] += s;
        }
    }
    // Soft clip
    for s in &mut mixdown {
        *s = s.clamp(-1.0, 1.0);
    }
    let mixdown_bytes = encode_wav_bytes(&mixdown, sr);

    app.platform.cloud_upload_status = Some(format!("Uploading {} stems...", stems.len()));

    // Build multipart body
    let boundary = format!("----TW_Cloud_{}", uuid::Uuid::new_v4().simple());
    let mut body = Vec::new();

    // Title field
    append_multipart_field_bytes(&mut body, &boundary, "title", app.project.name.as_bytes());
    // Genre
    append_multipart_field_bytes(&mut body, &boundary, "genre", app.platform.upload_genre.as_bytes());
    // BPM
    let bpm_str = format!("{}", app.project.tempo.bpm as i32);
    append_multipart_field_bytes(&mut body, &boundary, "bpm", bpm_str.as_bytes());
    // Band ID
    if app.platform.selected_band_idx > 0 {
        if let Some(band) = app.platform.bands.get(app.platform.selected_band_idx - 1) {
            append_multipart_field_bytes(&mut body, &boundary, "band_id", band.id.as_bytes());
        }
    }
    // Mixdown file
    append_multipart_file(&mut body, &boundary, "mixdown", "mixdown.wav", &mixdown_bytes);
    // Stems
    for (name, data) in &stems {
        let field_name = format!("stem_{}", name.replace(' ', "_"));
        append_multipart_file(&mut body, &boundary, &field_name, &format!("{name}.wav"), data);
    }
    // Final boundary
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let url = format!("{}/api/cloud", app.platform.server_url.trim_end_matches('/'));
    let content_type = format!("multipart/form-data; boundary={boundary}");

    let agent = ureq::Agent::new_with_defaults();

    match agent.post(&url)
        .header("Authorization", &format!("Bearer {jwt}"))
        .header("Content-Type", &content_type)
        .send(&body[..])
    {
        Ok(resp) => {
            let body_str = resp.into_body().read_to_string().unwrap_or_default();
            let id = extract_json_string(&body_str, "id").unwrap_or_default();
            let version = extract_json_string(&body_str, "version").unwrap_or("1".into());
            let new_stems = extract_json_string(&body_str, "new_stems").unwrap_or("?".into());
            let reused = extract_json_string(&body_str, "reused_stems").unwrap_or("0".into());
            let ver_msg = if version == "1" {
                format!("Uploaded! ID: {id}")
            } else {
                format!("Updated to v{version}! ({new_stems} new, {reused} reused stems) ID: {id}")
            };
            app.platform.cloud_upload_status = Some(ver_msg.clone());
            app.set_status(&ver_msg);
        }
        Err(e) => {
            app.platform.cloud_upload_status = Some(format!("Upload failed: {e}"));
        }
    }
}

/// Download a cloud project and import all stems as tracks
fn do_download_cloud_project(app: &mut DawApp) {
    let input = app.platform.cloud_download_id.trim().to_string();
    if input.is_empty() {
        app.platform.cloud_download_status = Some("Enter a cloud project ID".into());
        return;
    }

    let jwt = match &app.platform.jwt_token {
        Some(t) => t.clone(),
        None => {
            app.platform.cloud_download_status = Some("Error: not logged in".into());
            return;
        }
    };

    // Extract ID from URL if needed
    let project_id = if input.contains("/cloud/") {
        input.rsplit("/cloud/").next().unwrap_or(&input).to_string()
    } else {
        input.clone()
    };

    app.platform.cloud_download_status = Some("Downloading project...".into());

    // Try as a project ID first, then as a version ID
    let project_path = format!("/api/cloud/{}/download", project_id);
    let version_path = format!("/api/cloud/version/{}", project_id);

    // First try project details for BPM
    let info_path = format!("/api/cloud/{}", project_id);
    if let Ok(info_resp) = platform_request("GET", &app.platform.server_url, &info_path, Some(&jwt), None) {
        if let Some(bpm_str) = extract_json_string(&info_resp, "bpm") {
            if let Ok(bpm) = bpm_str.parse::<f64>() {
                if bpm > 0.0 { app.project.tempo.bpm = bpm; }
            }
        }
    }

    // Try project download first, fall back to version download
    let resp_result = platform_request("POST", &app.platform.server_url, &project_path, Some(&jwt), None)
        .or_else(|_| platform_request("POST", &app.platform.server_url, &version_path, Some(&jwt), None));
    match resp_result {
        Ok(resp) => {
            // Parse stems from response
            let title = extract_json_string(&resp, "title").unwrap_or("Cloud Project".into());

            // Find all stem audio_urls and names
            let mut stem_count = 0;
            let mut idx = 0;
            while let Some(pos) = resp[idx..].find("\"audio_url\"") {
                let abs = idx + pos;
                if let Some(url) = extract_json_string(&resp[abs..], "audio_url") {
                    // Find the corresponding name (should be before audio_url in the JSON)
                    let name_search_start = if abs > 200 { abs - 200 } else { 0 };
                    let name = extract_json_string(&resp[name_search_start..abs], "name")
                        .unwrap_or_else(|| format!("Stem {}", stem_count + 1));

                    // Download stem audio
                    let full_url = format!("{}{}", app.platform.server_url.trim_end_matches('/'), url);
                    let agent = ureq::Agent::new_with_defaults();
                    if let Ok(audio_resp) = agent.get(&full_url).call() {
                        if let Ok(audio_bytes) = audio_resp.into_body().with_config().limit(500 * 1024 * 1024).read_to_vec() {
                            if let Ok(samples) = jamhub_engine::load_audio_buffer(&audio_bytes) {
                                let buffer_id = uuid::Uuid::new_v4();
                                let duration = samples.len() as u64;
                                app.audio_buffers.insert(buffer_id, samples);

                                let track_id = app.project.add_track(&name, jamhub_model::TrackKind::Audio);
                                if let Some(track) = app.project.tracks.iter_mut().find(|t| t.id == track_id) {
                                    track.clips.push(jamhub_model::Clip {
                                        id: uuid::Uuid::new_v4(),
                                        name: name.clone(),
                                        start_sample: 0,
                                        duration_samples: duration,
                                        source: jamhub_model::ClipSource::AudioBuffer { buffer_id },
                                        muted: false,
                                        fade_in_samples: 0, fade_out_samples: 0,
                                        fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                                        color: None, playback_rate: 1.0, preserve_pitch: false,
                                        loop_count: 1, gain_db: 0.0, take_index: 0,
                                        content_offset: 0, transpose_semitones: 0, reversed: false,
                                    });
                                }
                                stem_count += 1;
                            }
                        }
                    }
                }
                idx = abs + 11;
            }

            app.project.name = title.clone();
            app.sync_project();
            app.platform.cloud_download_status = Some(format!("Downloaded: {title} ({stem_count} tracks)"));
            app.set_status(&format!("Cloud project loaded: {title}"));
        }
        Err(e) => {
            app.platform.cloud_download_status = Some(format!("Download failed: {e}"));
        }
    }
}

fn encode_wav_bytes(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    let data_size = samples.len() * 4;
    let file_size = 36 + data_size;
    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(file_size as u32).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&3u16.to_le_bytes()); // float
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 4).to_le_bytes()); // byte rate
    buf.extend_from_slice(&4u16.to_le_bytes()); // block align
    buf.extend_from_slice(&32u16.to_le_bytes()); // bits per sample
    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&(data_size as u32).to_le_bytes());
    for &s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    buf
}

fn append_multipart_field_bytes(body: &mut Vec<u8>, boundary: &str, name: &str, value: &[u8]) {
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes());
    body.extend_from_slice(value);
    body.extend_from_slice(b"\r\n");
}

fn append_multipart_file(body: &mut Vec<u8>, boundary: &str, name: &str, filename: &str, data: &[u8]) {
    body.extend_from_slice(format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: audio/wav\r\n\r\n"
    ).as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(b"\r\n");
}

fn do_import_track(app: &mut DawApp) {
    // Extract track ID from input — could be a UUID or a URL like http://localhost:3000/#/track/<uuid>
    let input = app.platform.import_track_id.trim().to_string();
    if input.is_empty() {
        app.platform.import_track_status = Some("Enter a track ID or URL".into());
        return;
    }

    // Parse track ID from URL or direct UUID
    let track_id = if input.contains("/track/") {
        input.rsplit("/track/").next().unwrap_or(&input).to_string()
    } else {
        input.clone()
    };

    app.platform.import_track_status = Some("Fetching track info...".into());

    // Fetch track metadata
    let path = format!("/api/tracks/{}", track_id);
    let resp = match platform_request("GET", &app.platform.server_url, &path, None, None) {
        Ok(r) => r,
        Err(e) => {
            app.platform.import_track_status = Some(format!("Failed: {e}"));
            return;
        }
    };

    // Extract track title and audio URL from response
    let title = extract_json_string(&resp, "title").unwrap_or_else(|| "Imported Track".into());
    let audio_url = match extract_json_string(&resp, "audio_url") {
        Some(url) => url,
        None => {
            app.platform.import_track_status = Some("No audio URL in track data".into());
            return;
        }
    };

    // Download the audio file
    app.platform.import_track_status = Some(format!("Downloading {}...", title));
    let full_url = format!("{}{}", app.platform.server_url.trim_end_matches('/'), audio_url);

    let agent = ureq::Agent::new_with_defaults();
    let audio_bytes = match agent.get(&full_url).call() {
        Ok(resp) => {
            match resp.into_body().with_config().limit(500 * 1024 * 1024).read_to_vec() {
                Ok(buf) => buf,
                Err(e) => {
                    app.platform.import_track_status = Some(format!("Download error: {e}"));
                    return;
                }
            }
        }
        Err(e) => {
            app.platform.import_track_status = Some(format!("Download failed: {e}"));
            return;
        }
    };

    if audio_bytes.is_empty() {
        app.platform.import_track_status = Some("Downloaded file is empty".into());
        return;
    }

    // Decode audio and load into engine
    let buffer_id = uuid::Uuid::new_v4();
    match jamhub_engine::load_audio_buffer(&audio_bytes) {
        Ok(samples) => {
            let duration = samples.len() as u64;
            // Store buffer in engine
            app.audio_buffers.insert(buffer_id, samples);

            // Create a new track with the audio
            let track_id_uuid = app.project.add_track(&title, jamhub_model::TrackKind::Audio);
            if let Some(track) = app.project.tracks.iter_mut().find(|t| t.id == track_id_uuid) {
                track.clips.push(jamhub_model::Clip {
                    id: uuid::Uuid::new_v4(),
                    name: title.clone(),
                    start_sample: 0,
                    duration_samples: duration,
                    source: jamhub_model::ClipSource::AudioBuffer { buffer_id },
                    muted: false,
                    fade_in_samples: 0,
                    fade_out_samples: 0, fade_in_curve: Default::default(), fade_out_curve: Default::default(),
                    color: None,
                    playback_rate: 1.0,
                    preserve_pitch: false,
                    loop_count: 1,
                    gain_db: 0.0,
                    take_index: 0,
                    content_offset: 0,
                    transpose_semitones: 0,
                    reversed: false,
                });
            }

            app.platform.import_track_status = Some(format!("Imported: {} ({:.1}s)", title, duration as f32 / app.project.sample_rate as f32));
            app.set_status(&format!("Imported track: {title}"));
        }
        Err(e) => {
            app.platform.import_track_status = Some(format!("Failed to decode audio: {e}"));
        }
    }
}

/// Attempt to import tracks described in the checkout response into the DAW project.
/// This is a best-effort parser for v1 — it creates empty tracks with names from the
/// platform data so the user can see the project structure.
fn import_project_tracks(app: &mut DawApp, json: &str) {
    // Look for track names in the response
    // Expect something like "tracks": [ { "name": "...", "url": "..." }, ... ]
    let mut idx = 0;
    while let Some(pos) = json[idx..].find("\"name\"") {
        let abs = idx + pos;
        if let Some(name) = extract_json_string(&json[abs..], "name") {
            if !name.is_empty() {
                // Create a new audio track in the project with this name
                app.project.add_track(&name, jamhub_model::TrackKind::Audio);
            }
        }
        idx = abs + 6; // advance past "name"
    }
}
