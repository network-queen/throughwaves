use eframe::egui;
use jamhub_model::ClipSource;

use crate::DawApp;

/// Format a sample count as MM:SS.mmm given a sample rate.
fn format_duration(samples: u64, sample_rate: u32) -> String {
    if sample_rate == 0 {
        return "0:00.000".to_string();
    }
    let total_secs = samples as f64 / sample_rate as f64;
    let mins = (total_secs / 60.0) as u64;
    let secs = total_secs % 60.0;
    format!("{}:{:05.2}", mins, secs)
}

/// Format byte count in human-readable form.
fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.show_project_info {
        return;
    }

    let mut open = true;
    let dim = egui::Color32::from_rgb(140, 140, 148);
    let accent = egui::Color32::from_rgb(90, 160, 255);

    // Pre-compute stats
    let num_tracks = app.project.tracks.len();
    let num_clips: usize = app.project.tracks.iter().map(|t| t.clips.len()).sum();
    let num_effects: usize = app.project.tracks.iter().map(|t| t.effects.len()).sum();

    // Total duration: furthest clip end
    let furthest_end: u64 = app
        .project
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .map(|c| c.start_sample + c.visual_duration_samples())
        .max()
        .unwrap_or(0);

    let sample_rate = app.project.sample_rate;
    let duration_str = format_duration(furthest_end, sample_rate);

    // Total audio data size (sum of all audio buffers in memory)
    let total_audio_bytes: usize = app
        .audio_buffers
        .values()
        .map(|buf| buf.len() * std::mem::size_of::<f32>())
        .sum();

    let bpm = app.project.tempo.bpm;
    let time_sig = format!(
        "{}/{}",
        app.project.time_signature.numerator, app.project.time_signature.denominator
    );

    let file_path_str = app
        .project_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(not saved)".to_string());

    let created_at_str = if app.project.created_at.is_empty() {
        "(unknown)".to_string()
    } else {
        app.project.created_at.clone()
    };

    // Collect how many audio vs midi clips
    let mut audio_clips = 0usize;
    let mut midi_clips = 0usize;
    for track in &app.project.tracks {
        for clip in &track.clips {
            match &clip.source {
                ClipSource::AudioFile { .. } | ClipSource::AudioBuffer { .. } => audio_clips += 1,
                ClipSource::Midi { .. } => midi_clips += 1,
            }
        }
    }

    let mut name_changed = false;
    let mut notes_changed = false;

    egui::Window::new("Project Info")
        .open(&mut open)
        .default_width(360.0)
        .min_width(300.0)
        .resizable(true)
        .show(ctx, |ui| {
            egui::Grid::new("project_info_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    // Project name (editable)
                    ui.label("Project Name:");
                    let resp = ui.text_edit_singleline(&mut app.project_info_name_buf);
                    if resp.lost_focus() || resp.changed() {
                        name_changed = true;
                    }
                    ui.end_row();

                    // Created date
                    ui.label("Created:");
                    ui.label(egui::RichText::new(&created_at_str).color(dim));
                    ui.end_row();

                    // Last modified
                    ui.label("File Path:");
                    ui.label(
                        egui::RichText::new(&file_path_str)
                            .color(dim)
                            .small(),
                    );
                    ui.end_row();

                    ui.separator();
                    ui.separator();
                    ui.end_row();

                    // Duration
                    ui.label("Total Duration:");
                    ui.label(
                        egui::RichText::new(&duration_str).color(accent),
                    );
                    ui.end_row();

                    // Tracks
                    ui.label("Tracks:");
                    ui.label(format!("{}", num_tracks));
                    ui.end_row();

                    // Clips
                    ui.label("Clips:");
                    ui.label(format!(
                        "{} ({} audio, {} MIDI)",
                        num_clips, audio_clips, midi_clips
                    ));
                    ui.end_row();

                    // Effects
                    ui.label("Effects:");
                    ui.label(format!("{}", num_effects));
                    ui.end_row();

                    // Audio data size
                    ui.label("Audio Data:");
                    ui.label(format_bytes(total_audio_bytes));
                    ui.end_row();

                    ui.separator();
                    ui.separator();
                    ui.end_row();

                    // Sample rate
                    ui.label("Sample Rate:");
                    ui.label(format!("{} Hz", sample_rate));
                    ui.end_row();

                    // Bit depth (always 32-bit float internally)
                    ui.label("Bit Depth:");
                    ui.label("32-bit float");
                    ui.end_row();

                    // BPM
                    ui.label("BPM:");
                    ui.label(format!("{:.1}", bpm));
                    ui.end_row();

                    // Time signature
                    ui.label("Time Signature:");
                    ui.label(&time_sig);
                    ui.end_row();
                });

            ui.separator();
            ui.label("Notes:");
            let notes_resp = ui.add(
                egui::TextEdit::multiline(&mut app.project_info_notes_buf)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY)
                    .hint_text("Project notes and comments..."),
            );
            if notes_resp.changed() {
                notes_changed = true;
            }
        });

    // Apply changes outside the closure
    if name_changed && app.project_info_name_buf != app.project.name {
        app.project.name = app.project_info_name_buf.clone();
        app.dirty = true;
    }
    if notes_changed && app.project_info_notes_buf != app.project.notes {
        app.project.notes = app.project_info_notes_buf.clone();
        app.dirty = true;
    }

    if !open {
        // Final sync of name and notes when closing
        if app.project_info_name_buf != app.project.name {
            app.project.name = app.project_info_name_buf.clone();
            app.dirty = true;
        }
        if app.project_info_notes_buf != app.project.notes {
            app.project.notes = app.project_info_notes_buf.clone();
            app.dirty = true;
        }
        app.show_project_info = false;
    }
}
