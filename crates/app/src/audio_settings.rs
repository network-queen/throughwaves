use eframe::egui;
use jamhub_engine::{list_devices, AudioDeviceInfo};

use crate::DawApp;

pub struct AudioSettings {
    pub show: bool,
    pub devices: Vec<AudioDeviceInfo>,
    pub scanned: bool,
    pub selected_input: String,
    pub selected_output: String,
    pub selected_sample_rate: u32,
    pub buffer_size: u32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            show: false,
            devices: Vec::new(),
            scanned: false,
            selected_input: String::new(),
            selected_output: String::new(),
            selected_sample_rate: 48000,
            buffer_size: 256,
        }
    }
}

impl AudioSettings {
    pub fn scan_if_needed(&mut self) {
        if !self.scanned {
            self.devices = list_devices();
            // Set defaults to current default devices
            for d in &self.devices {
                if d.is_default && d.is_input && self.selected_input.is_empty() {
                    self.selected_input = d.name.clone();
                }
                if d.is_default && d.is_output && self.selected_output.is_empty() {
                    self.selected_output = d.name.clone();
                }
            }
            self.scanned = true;
        }
    }
}

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.audio_settings.show {
        return;
    }

    app.audio_settings.scan_if_needed();

    let mut open = true;
    egui::Window::new("Audio Settings")
        .open(&mut open)
        .default_width(450.0)
        .show(ctx, |ui| {
            ui.heading("Audio Device Configuration");
            ui.separator();

            // Current status
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Status:").strong());
                ui.colored_label(
                    egui::Color32::from_rgb(80, 200, 80),
                    format!("Active — {}Hz", app.sample_rate()),
                );
            });

            ui.add_space(8.0);

            // Input device selection
            ui.label(egui::RichText::new("Input Device (Microphone):").strong());
            let input_devices: Vec<&AudioDeviceInfo> =
                app.audio_settings.devices.iter().filter(|d| d.is_input).collect();

            egui::ComboBox::from_id_salt("input_device")
                .selected_text(&app.audio_settings.selected_input)
                .width(350.0)
                .show_ui(ui, |ui| {
                    for device in &input_devices {
                        let label = if device.is_default {
                            format!("{} (default)", device.name)
                        } else {
                            device.name.clone()
                        };
                        let detail = format!(
                            "{}ch, {}",
                            device.max_channels,
                            device
                                .sample_rates
                                .iter()
                                .map(|r| format!("{}kHz", r / 1000))
                                .collect::<Vec<_>>()
                                .join("/"),
                        );
                        ui.selectable_value(
                            &mut app.audio_settings.selected_input,
                            device.name.clone(),
                            format!("{label}  ({detail})"),
                        );
                    }
                });

            // Show input device details
            if let Some(dev) = input_devices.iter().find(|d| d.name == app.audio_settings.selected_input) {
                ui.label(
                    egui::RichText::new(format!(
                        "  {} channels, rates: {}",
                        dev.max_channels,
                        dev.sample_rates
                            .iter()
                            .map(|r| format!("{}Hz", r))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ))
                    .small()
                    .color(egui::Color32::GRAY),
                );
            }

            ui.add_space(8.0);

            // Output device selection
            ui.label(egui::RichText::new("Output Device (Speakers/Headphones):").strong());
            let output_devices: Vec<&AudioDeviceInfo> =
                app.audio_settings.devices.iter().filter(|d| d.is_output).collect();

            egui::ComboBox::from_id_salt("output_device")
                .selected_text(&app.audio_settings.selected_output)
                .width(350.0)
                .show_ui(ui, |ui| {
                    for device in &output_devices {
                        let label = if device.is_default {
                            format!("{} (default)", device.name)
                        } else {
                            device.name.clone()
                        };
                        ui.selectable_value(
                            &mut app.audio_settings.selected_output,
                            device.name.clone(),
                            label,
                        );
                    }
                });

            ui.add_space(8.0);

            // Sample rate
            ui.label(egui::RichText::new("Sample Rate:").strong());
            ui.horizontal(|ui| {
                for &sr in &[44100u32, 48000, 88200, 96000] {
                    let label = format!("{} Hz", sr);
                    if ui
                        .selectable_label(app.audio_settings.selected_sample_rate == sr, &label)
                        .clicked()
                    {
                        app.audio_settings.selected_sample_rate = sr;
                    }
                }
            });

            ui.add_space(8.0);

            // Buffer size
            ui.label(egui::RichText::new("Buffer Size:").strong());
            ui.horizontal(|ui| {
                for &bs in &[64u32, 128, 256, 512, 1024] {
                    let latency_ms = bs as f64 / app.audio_settings.selected_sample_rate as f64 * 1000.0;
                    let label = format!("{bs} ({latency_ms:.1}ms)");
                    if ui
                        .selectable_label(app.audio_settings.buffer_size == bs, &label)
                        .clicked()
                    {
                        app.audio_settings.buffer_size = bs;
                    }
                }
            });
            ui.label(
                egui::RichText::new("Smaller = lower latency but higher CPU. 256 recommended.")
                    .small()
                    .color(egui::Color32::GRAY),
            );

            ui.add_space(12.0);
            ui.separator();

            // Device list
            if ui.button("Rescan Devices").clicked() {
                app.audio_settings.scanned = false;
                app.audio_settings.scan_if_needed();
            }

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "{} audio device(s) found",
                    app.audio_settings.devices.len()
                ))
                .small()
                .color(egui::Color32::GRAY),
            );

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Note: changing devices requires restarting the audio engine.\nDevice selection will take effect on next app restart.",
                )
                .small()
                .color(egui::Color32::from_rgb(200, 180, 100)),
            );
        });

    if !open {
        app.audio_settings.show = false;
    }
}
