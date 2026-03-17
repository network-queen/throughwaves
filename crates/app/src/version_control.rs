use std::fs;
use std::path::PathBuf;

use eframe::egui;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use jamhub_model::{Project, ProjectVersion};

use crate::DawApp;

// ── Panel state ──────────────────────────────────────────────────────

/// Persistent state for the version control panel.
pub struct VersionControlPanel {
    pub show: bool,
    pub commit_message: String,
    pub new_branch_name: String,
    pub show_new_branch_input: bool,
    pub show_all_branches: bool,
    pub merge_source: Option<String>,
    /// Error/info toast inside the panel
    pub panel_message: Option<(String, std::time::Instant)>,
}

impl Default for VersionControlPanel {
    fn default() -> Self {
        Self {
            show: false,
            commit_message: String::new(),
            new_branch_name: String::new(),
            show_new_branch_input: false,
            show_all_branches: false,
            merge_source: None,
            panel_message: None,
        }
    }
}

// ── Branch colors ────────────────────────────────────────────────────

/// Deterministic color for a branch name based on its hash.
fn branch_color(name: &str) -> egui::Color32 {
    if name == "main" {
        return egui::Color32::from_rgb(100, 200, 140); // green for main
    }
    let mut h: u32 = 5381;
    for b in name.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u32);
    }
    let hue = (h % 360) as f32;
    let (r, g, b) = hsv_to_rgb(hue, 0.6, 0.85);
    egui::Color32::from_rgb(r, g, b)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h as u32) / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

// ── Snapshot I/O ─────────────────────────────────────────────────────

/// A minimal snapshot: the full project JSON (without version_history to avoid
/// recursive growth) plus the audio buffer IDs present at that point.
#[derive(Serialize, Deserialize)]
struct Snapshot {
    project_json: String,
    audio_buffer_ids: Vec<Uuid>,
}

/// Directory where version snapshots live inside the project folder.
fn versions_dir(project_path: &PathBuf) -> PathBuf {
    project_path.join("versions")
}

/// Compute a SHA-256 hex digest of a byte slice.
fn sha256_hex(data: &[u8]) -> String {
    // Minimal inline SHA-256 — we only need a fingerprint, not crypto security.
    // Use a simple hash of the data length + first/last bytes + djb2 as a
    // lightweight stand-in.  For a real product you'd pull in `sha2` crate.
    let mut h: u64 = 14695981039346656037; // FNV offset basis
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211); // FNV prime
    }
    format!("{:016x}{:016x}", h, data.len() as u64)
}

/// Save a snapshot to disk.  Returns the hash of the project JSON.
fn save_snapshot(
    project_path: &PathBuf,
    version_id: Uuid,
    project: &Project,
    audio_buffer_ids: &[Uuid],
) -> Result<String, String> {
    let vdir = versions_dir(project_path);
    fs::create_dir_all(&vdir).map_err(|e| format!("Cannot create versions dir: {e}"))?;

    // Serialize project WITHOUT version_history to keep snapshots lean.
    let mut snap_project = project.clone();
    snap_project.version_history.clear();
    snap_project.current_version_id = None;

    let project_json = serde_json::to_string(&snap_project)
        .map_err(|e| format!("Serialize snapshot: {e}"))?;

    let hash = sha256_hex(project_json.as_bytes());

    let snapshot = Snapshot {
        project_json,
        audio_buffer_ids: audio_buffer_ids.to_vec(),
    };
    let snap_json = serde_json::to_string(&snapshot)
        .map_err(|e| format!("Serialize snapshot wrapper: {e}"))?;

    let snap_path = vdir.join(format!("{version_id}.json"));
    fs::write(&snap_path, snap_json).map_err(|e| format!("Write snapshot: {e}"))?;

    Ok(hash)
}

/// Load a snapshot from disk.  Returns the project and list of buffer IDs.
fn load_snapshot(
    project_path: &PathBuf,
    version_id: Uuid,
) -> Result<(Project, Vec<Uuid>), String> {
    let snap_path = versions_dir(project_path).join(format!("{version_id}.json"));
    let data = fs::read_to_string(&snap_path)
        .map_err(|e| format!("Read snapshot {version_id}: {e}"))?;
    let snapshot: Snapshot =
        serde_json::from_str(&data).map_err(|e| format!("Parse snapshot: {e}"))?;
    let project: Project = serde_json::from_str(&snapshot.project_json)
        .map_err(|e| format!("Parse project in snapshot: {e}"))?;
    Ok((project, snapshot.audio_buffer_ids))
}

// ── Core operations (called from DawApp) ─────────────────────────────

impl DawApp {
    /// Create a version commit of the current project state.
    pub fn version_commit(&mut self, message: &str) {
        let project_path = match &self.project_path {
            Some(p) => p.clone(),
            None => {
                self.set_status("Save the project first before committing a version");
                return;
            }
        };

        let branch = self.project.current_branch.clone();
        let parent_id = self.project.current_version_id;
        let version_id = Uuid::new_v4();
        let timestamp = chrono::Local::now().to_rfc3339();

        let buffer_ids: Vec<Uuid> = self.audio_buffers.keys().copied().collect();

        match save_snapshot(&project_path, version_id, &self.project, &buffer_ids) {
            Ok(hash) => {
                let version = ProjectVersion {
                    id: version_id,
                    branch: branch.clone(),
                    parent_id,
                    message: message.to_string(),
                    timestamp,
                    project_hash: hash,
                };
                self.project.version_history.push(version);
                self.project.current_version_id = Some(version_id);
                self.set_status(&format!("Version committed: {message}"));
            }
            Err(e) => {
                self.set_status(&format!("Version commit failed: {e}"));
            }
        }
    }

    /// Create a new branch from the current version and switch to it.
    pub fn version_create_branch(&mut self, name: &str) {
        if name.is_empty() || name == "main" {
            self.set_status("Invalid branch name");
            return;
        }
        // Check for duplicate
        let exists = self.project.version_history.iter().any(|v| v.branch == name);
        if exists {
            self.set_status(&format!("Branch '{name}' already exists"));
            return;
        }
        self.project.current_branch = name.to_string();
        self.set_status(&format!("Switched to new branch: {name}"));
    }

    /// Switch to an existing branch, loading its latest version.
    pub fn version_switch_branch(&mut self, branch: &str) {
        let project_path = match &self.project_path {
            Some(p) => p.clone(),
            None => {
                self.set_status("No project path — save first");
                return;
            }
        };

        // Find the latest version on the target branch
        let latest = self
            .project
            .version_history
            .iter()
            .filter(|v| v.branch == branch)
            .last()
            .cloned();

        let version = match latest {
            Some(v) => v,
            None => {
                // No commits on that branch yet — just change the branch name
                self.project.current_branch = branch.to_string();
                self.set_status(&format!("Switched to branch: {branch} (no commits yet)"));
                return;
            }
        };

        // Auto-commit current state if dirty
        if self.dirty {
            self.version_commit("Auto-save before branch switch");
        }

        match load_snapshot(&project_path, version.id) {
            Ok((mut loaded_project, _buffer_ids)) => {
                // Preserve version history and branch metadata from the live project
                loaded_project.version_history = self.project.version_history.clone();
                loaded_project.current_branch = branch.to_string();
                loaded_project.current_version_id = Some(version.id);

                self.push_undo("Switch branch");
                self.project = loaded_project;
                self.dirty = false;
                self.sync_project();
                self.set_status(&format!("Switched to branch: {branch}"));
            }
            Err(e) => {
                self.set_status(&format!("Switch branch failed: {e}"));
            }
        }
    }

    /// Restore a specific version by its ID.
    pub fn version_restore(&mut self, version_id: Uuid) {
        let project_path = match &self.project_path {
            Some(p) => p.clone(),
            None => {
                self.set_status("No project path");
                return;
            }
        };

        let version = self
            .project
            .version_history
            .iter()
            .find(|v| v.id == version_id)
            .cloned();

        let version = match version {
            Some(v) => v,
            None => {
                self.set_status("Version not found");
                return;
            }
        };

        match load_snapshot(&project_path, version_id) {
            Ok((mut loaded_project, _buffer_ids)) => {
                loaded_project.version_history = self.project.version_history.clone();
                loaded_project.current_branch = version.branch.clone();
                loaded_project.current_version_id = Some(version_id);

                self.push_undo("Restore version");
                self.project = loaded_project;
                self.dirty = false;
                self.sync_project();
                self.set_status(&format!("Restored: {}", version.message));
            }
            Err(e) => {
                self.set_status(&format!("Restore failed: {e}"));
            }
        }
    }

    /// Merge another branch into the current one (takes the source branch's latest state).
    pub fn version_merge_branch(&mut self, source_branch: &str) {
        let project_path = match &self.project_path {
            Some(p) => p.clone(),
            None => {
                self.set_status("No project path");
                return;
            }
        };

        let latest = self
            .project
            .version_history
            .iter()
            .filter(|v| v.branch == source_branch)
            .last()
            .cloned();

        let source_version = match latest {
            Some(v) => v,
            None => {
                self.set_status(&format!("No commits on branch '{source_branch}'"));
                return;
            }
        };

        match load_snapshot(&project_path, source_version.id) {
            Ok((mut merged_project, _buffer_ids)) => {
                let current_branch = self.project.current_branch.clone();
                merged_project.version_history = self.project.version_history.clone();
                merged_project.current_branch = current_branch.clone();
                merged_project.current_version_id = self.project.current_version_id;

                self.push_undo("Merge branch");
                self.project = merged_project;
                self.dirty = true;

                // Auto-commit the merge
                let msg = format!("Merged from {source_branch}");
                self.version_commit(&msg);
                self.sync_project();
            }
            Err(e) => {
                self.set_status(&format!("Merge failed: {e}"));
            }
        }
    }

    /// Delete a branch and all its snapshots from disk.
    pub fn version_delete_branch(&mut self, branch: &str) {
        if branch == "main" {
            self.set_status("Cannot delete the main branch");
            return;
        }
        if branch == self.project.current_branch {
            self.set_status("Cannot delete the current branch — switch first");
            return;
        }

        // Remove snapshot files
        if let Some(ref project_path) = self.project_path {
            let vdir = versions_dir(project_path);
            let ids_to_remove: Vec<Uuid> = self
                .project
                .version_history
                .iter()
                .filter(|v| v.branch == branch)
                .map(|v| v.id)
                .collect();
            for id in &ids_to_remove {
                let snap_path = vdir.join(format!("{id}.json"));
                let _ = fs::remove_file(snap_path);
            }
        }

        self.project
            .version_history
            .retain(|v| v.branch != branch);
        self.set_status(&format!("Deleted branch: {branch}"));
    }

    /// Get list of all unique branch names.
    pub fn version_branches(&self) -> Vec<String> {
        let mut branches: Vec<String> = Vec::new();
        // Always include "main" first
        branches.push("main".to_string());
        // Include current branch
        if self.project.current_branch != "main"
            && !branches.contains(&self.project.current_branch)
        {
            branches.push(self.project.current_branch.clone());
        }
        // Add any branches from history
        for v in &self.project.version_history {
            if !branches.contains(&v.branch) {
                branches.push(v.branch.clone());
            }
        }
        branches
    }

    /// Get versions filtered for a branch (or all if show_all).
    /// Returns cloned data to avoid borrow conflicts in the UI.
    pub fn version_history_filtered(&self, show_all: bool) -> Vec<ProjectVersion> {
        let branch = &self.project.current_branch;
        let mut versions: Vec<ProjectVersion> = if show_all {
            self.project.version_history.clone()
        } else {
            self.project
                .version_history
                .iter()
                .filter(|v| v.branch == *branch)
                .cloned()
                .collect()
        };
        versions.reverse(); // newest first
        versions.truncate(20); // limit display
        versions
    }

    /// Quick commit (no dialog) — uses auto-generated message.
    pub fn version_quick_commit(&mut self) {
        let msg = format!("Quick save — {}", chrono::Local::now().format("%H:%M:%S"));
        self.version_commit(&msg);
    }
}

// ── UI ───────────────────────────────────────────────────────────────

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.version_panel.show {
        return;
    }

    let mut open = true;
    egui::Window::new("Version Control")
        .open(&mut open)
        .default_width(380.0)
        .default_height(520.0)
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            let accent = egui::Color32::from_rgb(240, 192, 64);
            let text_dim = egui::Color32::from_rgb(128, 126, 135);

            // ── Branch selector ──────────────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Branch:")
                        .strong()
                        .color(accent),
                );

                let branches = app.version_branches();
                let current = app.project.current_branch.clone();

                egui::ComboBox::from_id_salt("branch_selector")
                    .selected_text(&current)
                    .show_ui(ui, |ui| {
                        for b in &branches {
                            let color = branch_color(b);
                            let label = egui::RichText::new(b).color(color);
                            if ui.selectable_label(*b == current, label).clicked() && *b != current {
                                let branch = b.clone();
                                app.version_switch_branch(&branch);
                            }
                        }
                    });

                if ui
                    .button(egui::RichText::new("+").strong())
                    .on_hover_text("Create new branch")
                    .clicked()
                {
                    app.version_panel.show_new_branch_input =
                        !app.version_panel.show_new_branch_input;
                    app.version_panel.new_branch_name.clear();
                }
            });

            // ── New branch input ─────────────────────────────────
            if app.version_panel.show_new_branch_input {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let resp = ui.text_edit_singleline(&mut app.version_panel.new_branch_name);
                    if (resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Create").clicked()
                    {
                        let name = app.version_panel.new_branch_name.clone();
                        if !name.is_empty() {
                            app.version_create_branch(&name);
                            app.version_panel.show_new_branch_input = false;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        app.version_panel.show_new_branch_input = false;
                    }
                });
            }

            ui.separator();

            // ── Commit section ───────────────────────────────────
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Commit:").strong());
                let hint = "Describe your changes...";
                ui.add(
                    egui::TextEdit::singleline(&mut app.version_panel.commit_message)
                        .hint_text(hint)
                        .desired_width(200.0),
                );
                let can_commit = app.project_path.is_some();
                if ui
                    .add_enabled(can_commit, egui::Button::new("Commit"))
                    .on_hover_text("Save a version snapshot (Cmd+Shift+C)")
                    .clicked()
                {
                    let msg = if app.version_panel.commit_message.trim().is_empty() {
                        format!("Version {}", app.project.version_history.len() + 1)
                    } else {
                        app.version_panel.commit_message.clone()
                    };
                    app.version_commit(&msg);
                    app.version_panel.commit_message.clear();
                }
            });

            ui.separator();

            // ── Merge section ────────────────────────────────────
            let branches = app.version_branches();
            let current = app.project.current_branch.clone();
            let other_branches: Vec<&String> =
                branches.iter().filter(|b| **b != current).collect();

            if !other_branches.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Merge from:").small());
                    egui::ComboBox::from_id_salt("merge_source")
                        .selected_text(
                            app.version_panel
                                .merge_source
                                .as_deref()
                                .unwrap_or("Select branch..."),
                        )
                        .show_ui(ui, |ui| {
                            for b in &other_branches {
                                let color = branch_color(b);
                                let label = egui::RichText::new(b.as_str()).color(color);
                                if ui
                                    .selectable_label(
                                        app.version_panel.merge_source.as_deref()
                                            == Some(b.as_str()),
                                        label,
                                    )
                                    .clicked()
                                {
                                    app.version_panel.merge_source = Some(b.to_string());
                                }
                            }
                        });

                    if let Some(ref src) = app.version_panel.merge_source.clone() {
                        if ui
                            .button(egui::RichText::new("Merge").color(egui::Color32::from_rgb(255, 160, 60)))
                            .clicked()
                        {
                            app.version_merge_branch(src);
                            app.version_panel.merge_source = None;
                        }
                    }
                });
                ui.separator();
            }

            // ── Delete branch ────────────────────────────────────
            if current != "main" {
                ui.horizontal(|ui| {
                    if ui
                        .button(
                            egui::RichText::new("Delete this branch")
                                .small()
                                .color(egui::Color32::from_rgb(200, 80, 80)),
                        )
                        .clicked()
                    {
                        // Switch to main then delete
                        let branch_to_delete = current.clone();
                        app.version_switch_branch("main");
                        app.version_delete_branch(&branch_to_delete);
                    }
                });
                ui.separator();
            }

            // ── Filter toggle ────────────────────────────────────
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut app.version_panel.show_all_branches,
                    egui::RichText::new("Show all branches").small(),
                );
            });

            ui.separator();

            // ── Branch graph + commit history ────────────────────
            let show_all = app.version_panel.show_all_branches;
            let versions = app.version_history_filtered(show_all);
            let current_head = app.project.current_version_id;

            if versions.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);
                    ui.label(
                        egui::RichText::new("No versions yet")
                            .color(text_dim)
                            .italics(),
                    );
                    ui.label(
                        egui::RichText::new("Commit to save your first snapshot")
                            .small()
                            .color(text_dim),
                    );
                    ui.add_space(30.0);
                });
            } else {
                egui::ScrollArea::vertical()
                    .max_height(350.0)
                    .show(ui, |ui| {
                        let mut restore_id: Option<Uuid> = None;

                        for (i, version) in versions.iter().enumerate() {
                            let is_head = current_head == Some(version.id);
                            let bcolor = branch_color(&version.branch);

                            // Commit row
                            let frame_fill = if is_head {
                                egui::Color32::from_rgba_premultiplied(
                                    bcolor.r() / 5,
                                    bcolor.g() / 5,
                                    bcolor.b() / 5,
                                    60,
                                )
                            } else {
                                egui::Color32::TRANSPARENT
                            };

                            egui::Frame::default()
                                .fill(frame_fill)
                                .inner_margin(egui::Margin::symmetric(6, 4))
                                .corner_radius(6.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        // ── Graph dot ──
                                        let (dot_rect, _) = ui.allocate_exact_size(
                                            egui::vec2(14.0, 14.0),
                                            egui::Sense::hover(),
                                        );
                                        let dot_center = dot_rect.center();
                                        let radius = if is_head { 5.0 } else { 3.5 };
                                        let dot_color = if is_head {
                                            accent
                                        } else {
                                            bcolor
                                        };
                                        ui.painter()
                                            .circle_filled(dot_center, radius, dot_color);

                                        // Vertical line connecting to next dot
                                        if i + 1 < versions.len() {
                                            ui.painter().line_segment(
                                                [
                                                    dot_center + egui::vec2(0.0, radius + 1.0),
                                                    dot_center + egui::vec2(0.0, 20.0),
                                                ],
                                                egui::Stroke::new(1.0, bcolor.gamma_multiply(0.4)),
                                            );
                                        }

                                        // ── Info ──
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                // Branch badge
                                                let badge_text = egui::RichText::new(&version.branch)
                                                    .small()
                                                    .color(bcolor);
                                                egui::Frame::default()
                                                    .fill(egui::Color32::from_rgba_premultiplied(
                                                        bcolor.r() / 6,
                                                        bcolor.g() / 6,
                                                        bcolor.b() / 6,
                                                        80,
                                                    ))
                                                    .inner_margin(egui::Margin::symmetric(5, 1))
                                                    .corner_radius(8.0)
                                                    .show(ui, |ui| {
                                                        ui.label(badge_text);
                                                    });

                                                if is_head {
                                                    ui.label(
                                                        egui::RichText::new("HEAD")
                                                            .small()
                                                            .strong()
                                                            .color(accent),
                                                    );
                                                }
                                            });

                                            ui.label(
                                                egui::RichText::new(&version.message)
                                                    .color(if is_head {
                                                        egui::Color32::WHITE
                                                    } else {
                                                        egui::Color32::from_rgb(200, 200, 200)
                                                    }),
                                            );

                                            // Timestamp
                                            let ts = format_timestamp(&version.timestamp);
                                            ui.label(
                                                egui::RichText::new(ts)
                                                    .small()
                                                    .color(text_dim),
                                            );
                                        });

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if !is_head {
                                                    if ui
                                                        .small_button("Restore")
                                                        .on_hover_text(
                                                            "Restore project to this version",
                                                        )
                                                        .clicked()
                                                    {
                                                        restore_id = Some(version.id);
                                                    }
                                                }
                                            },
                                        );
                                    });
                                });
                        }

                        if let Some(id) = restore_id {
                            app.version_restore(id);
                        }
                    });
            }
        });

    if !open {
        app.version_panel.show = false;
    }
}

/// Format an ISO-8601 timestamp into a short human-readable form.
fn format_timestamp(ts: &str) -> String {
    // Try to parse and show relative or short date
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        let local = dt.with_timezone(&chrono::Local);
        local.format("%b %d, %H:%M").to_string()
    } else {
        ts.to_string()
    }
}
