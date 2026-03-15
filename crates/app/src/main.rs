mod timeline;
mod mixer_view;
mod transport_bar;

use eframe::egui;
use jamhub_engine::{EngineCommand, EngineHandle};
use jamhub_model::{Project, TrackKind, TransportState};

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

struct DawApp {
    project: Project,
    engine: Option<EngineHandle>,
    engine_error: Option<String>,
    view: View,
    zoom: f32,
    scroll_x: f32,
}

#[derive(PartialEq)]
enum View {
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
        }
    }

    fn transport_state(&self) -> TransportState {
        self.engine
            .as_ref()
            .map(|e| e.state.read().transport)
            .unwrap_or(TransportState::Stopped)
    }

    fn position_samples(&self) -> u64 {
        self.engine
            .as_ref()
            .map(|e| e.state.read().position_samples)
            .unwrap_or(0)
    }

    fn sample_rate(&self) -> u32 {
        self.engine
            .as_ref()
            .map(|e| e.state.read().sample_rate)
            .unwrap_or(44100)
    }

    fn send_command(&self, cmd: EngineCommand) {
        if let Some(ref engine) = self.engine {
            engine.send(cmd);
        }
    }

    fn sync_project(&self) {
        self.send_command(EngineCommand::UpdateProject(self.project.clone()));
    }
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request repaint for smooth animation during playback
        if self.transport_state() == TransportState::Playing {
            ctx.request_repaint();
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Session").clicked() {
                        self.project = Project::default();
                        self.project.add_track("Track 1", TrackKind::Audio);
                        self.sync_project();
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.selectable_label(self.view == View::Arrange, "Arrange").clicked() {
                        self.view = View::Arrange;
                        ui.close_menu();
                    }
                    if ui.selectable_label(self.view == View::Mixer, "Mixer").clicked() {
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

        // Error banner
        if let Some(ref err) = self.engine_error {
            egui::TopBottomPanel::top("error").show(ctx, |ui| {
                ui.colored_label(egui::Color32::RED, format!("Engine error: {err}"));
            });
        }

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.view {
                View::Arrange => timeline::show(self, ui),
                View::Mixer => mixer_view::show(self, ui),
            }
        });
    }
}
