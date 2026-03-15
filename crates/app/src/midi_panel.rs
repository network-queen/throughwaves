use eframe::egui;
use jamhub_engine::midi_input::{MidiPortInfo, MidiRecorder};

use crate::DawApp;

pub struct MidiPanel {
    pub show: bool,
    pub ports: Vec<MidiPortInfo>,
    pub scanned: bool,
    pub selected_port: Option<usize>,
    pub recorder: MidiRecorder,
}

impl Default for MidiPanel {
    fn default() -> Self {
        Self {
            show: false,
            ports: Vec::new(),
            scanned: false,
            selected_port: None,
            recorder: MidiRecorder::new(),
        }
    }
}

impl MidiPanel {
    pub fn scan_if_needed(&mut self) {
        if !self.scanned {
            self.ports = MidiRecorder::list_ports();
            self.scanned = true;
        }
    }
}

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.midi_panel.show {
        return;
    }

    app.midi_panel.scan_if_needed();

    let mut open = true;
    egui::Window::new("MIDI Input")
        .open(&mut open)
        .default_width(350.0)
        .show(ctx, |ui| {
            ui.heading("MIDI Devices");

            if app.midi_panel.ports.is_empty() {
                ui.label("No MIDI input devices found.");
                ui.label(
                    egui::RichText::new("Connect a MIDI controller and click Rescan")
                        .small()
                        .color(egui::Color32::GRAY),
                );
            } else {
                ui.label(format!("{} device(s) found:", app.midi_panel.ports.len()));
                ui.separator();

                let ports: Vec<(usize, String)> = app.midi_panel.ports
                    .iter()
                    .map(|p| (p.index, p.name.clone()))
                    .collect();

                for (idx, name) in &ports {
                    ui.horizontal(|ui| {
                        let selected = app.midi_panel.selected_port == Some(*idx);
                        if ui.selectable_label(selected, name).clicked() {
                            app.midi_panel.selected_port = Some(*idx);
                        }
                    });
                }
            }

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Rescan").clicked() {
                    app.midi_panel.scanned = false;
                    app.midi_panel.scan_if_needed();
                }

                if app.midi_panel.recorder.is_recording() {
                    if ui.button("Stop MIDI Recording").clicked() {
                        let events = app.midi_panel.recorder.stop();
                        let bpm = app.project.tempo.bpm;
                        let us_per_beat = 60_000_000.0 / bpm;
                        let notes = jamhub_engine::midi_input::events_to_notes(
                            &events,
                            480,
                            us_per_beat,
                        );
                        app.set_status(&format!("Captured {} MIDI notes", notes.len()));

                        // Add notes to selected MIDI track
                        if let Some(ti) = app.selected_track {
                            if ti < app.project.tracks.len()
                                && app.project.tracks[ti].kind == jamhub_model::TrackKind::Midi
                            {
                                app.push_undo("Record MIDI");
                                // Find or create MIDI clip
                                let has_midi_clip = app.project.tracks[ti]
                                    .clips
                                    .iter()
                                    .any(|c| matches!(c.source, jamhub_model::ClipSource::Midi { .. }));

                                if has_midi_clip {
                                    for clip in &mut app.project.tracks[ti].clips {
                                        if let jamhub_model::ClipSource::Midi { notes: ref mut existing } = clip.source {
                                            existing.extend(notes.clone());
                                        }
                                    }
                                } else {
                                    let sr = app.sample_rate() as f64;
                                    let max_tick = notes.iter().map(|n| n.start_tick + n.duration_ticks).max().unwrap_or(0);
                                    let samples_per_tick = app.project.tempo.samples_per_beat(sr) / 480.0;
                                    app.project.tracks[ti].clips.push(jamhub_model::Clip {
                                        id: uuid::Uuid::new_v4(),
                                        name: "MIDI Recording".into(),
                                        start_sample: 0,
                                        duration_samples: (max_tick as f64 * samples_per_tick) as u64,
                                        source: jamhub_model::ClipSource::Midi { notes },
                                        muted: false,
                                    });
                                }
                                app.sync_project();
                            } else {
                                app.set_status("Select a MIDI track first (Track > Add MIDI Track)");
                            }
                        }
                    }

                    let event_count = app.midi_panel.recorder.peek_events().len();
                    ui.label(
                        egui::RichText::new(format!("Recording... {event_count} events"))
                            .color(egui::Color32::from_rgb(220, 50, 50)),
                    );
                } else {
                    let can_record = app.midi_panel.selected_port.is_some();
                    if ui
                        .add_enabled(can_record, egui::Button::new("Start MIDI Recording"))
                        .on_hover_text("Record MIDI from selected device")
                        .clicked()
                    {
                        if let Some(port) = app.midi_panel.selected_port {
                            match app.midi_panel.recorder.start(port) {
                                Ok(()) => app.set_status("MIDI recording started"),
                                Err(e) => app.set_status(&format!("MIDI error: {e}")),
                            }
                        }
                    }
                }
            });
        });

    if !open {
        app.midi_panel.show = false;
    }
}
