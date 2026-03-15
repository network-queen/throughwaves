use eframe::egui;
use jamhub_network::client::SessionClient;
use jamhub_network::message::SessionMessage;

use crate::DawApp;

pub struct SessionPanel {
    pub client: Option<SessionClient>,
    pub server_url: String,
    pub session_id: String,
    pub peer_name: String,
    pub chat_input: String,
    pub chat_messages: Vec<(String, String)>,
    pub show_panel: bool,
}

impl Default for SessionPanel {
    fn default() -> Self {
        Self {
            client: None,
            server_url: "ws://127.0.0.1:9090".into(),
            session_id: "default".into(),
            peer_name: whoami::fallible::hostname().unwrap_or_else(|_| "Anonymous".into()),
            chat_input: String::new(),
            chat_messages: Vec::new(),
            show_panel: false,
        }
    }
}

impl SessionPanel {
    pub fn connect(&mut self) -> Result<(), String> {
        let client =
            SessionClient::connect(&self.server_url, &self.peer_name, &self.session_id)?;
        self.client = Some(client);
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.client = None;
    }

    pub fn is_connected(&self) -> bool {
        self.client.as_ref().map(|c| c.is_connected()).unwrap_or(false)
    }

    pub fn send(&self, msg: SessionMessage) {
        if let Some(ref client) = self.client {
            client.send(msg);
        }
    }

    /// Process incoming network messages. Returns messages that need to be applied to the project.
    pub fn poll(&mut self) -> Vec<SessionMessage> {
        let Some(ref client) = self.client else {
            return Vec::new();
        };

        let messages = client.recv();

        for msg in &messages {
            match msg {
                SessionMessage::Chat {
                    peer_name, message, ..
                } => {
                    self.chat_messages
                        .push((peer_name.clone(), message.clone()));
                }
                SessionMessage::PeerJoined { peer } => {
                    self.chat_messages
                        .push(("System".into(), format!("{} joined", peer.name)));
                }
                SessionMessage::PeerLeft { peer_id } => {
                    let name = self.client.as_ref()
                        .and_then(|c| c.state.read().peers.iter().find(|p| p.id == *peer_id).map(|p| p.name.clone()))
                        .unwrap_or_else(|| format!("{}", &peer_id.to_string()[..8]));
                    self.chat_messages
                        .push(("System".into(), format!("{name} left the session")));
                }
                _ => {}
            }
        }

        messages
    }
}

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.session.show_panel {
        return;
    }

    egui::SidePanel::right("session_panel")
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.heading("Session");
            ui.separator();

            if app.session.is_connected() {
                // Connected state
                ui.colored_label(
                    egui::Color32::from_rgb(80, 200, 80),
                    format!("Connected to {}", app.session.session_id),
                );

                if ui.button("Disconnect").clicked() {
                    app.session.disconnect();
                }

                ui.separator();

                // Peers list
                if let Some(ref client) = app.session.client {
                    let state = client.state.read();
                    ui.label(format!("Peers ({}):", state.peers.len()));
                    for peer in &state.peers {
                        ui.horizontal(|ui| {
                            ui.colored_label(egui::Color32::from_rgb(100, 180, 255), "●");
                            ui.label(&peer.name);
                        });
                    }
                }

                ui.separator();

                // Chat
                ui.label("Chat:");
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (name, msg) in &app.session.chat_messages {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(format!("{name}:"));
                                ui.label(msg);
                            });
                        }
                    });

                ui.horizontal(|ui| {
                    let response = ui.text_edit_singleline(&mut app.session.chat_input);
                    if (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Send").clicked()
                    {
                        if !app.session.chat_input.is_empty() {
                            let msg = app.session.chat_input.clone();
                            app.session.chat_input.clear();

                            if let Some(ref client) = app.session.client {
                                let peer_id = client.state.read().peer_id.unwrap_or_default();
                                app.session.send(SessionMessage::Chat {
                                    peer_id,
                                    peer_name: app.session.peer_name.clone(),
                                    message: msg.clone(),
                                });
                                app.session
                                    .chat_messages
                                    .push((app.session.peer_name.clone(), msg));
                            }
                        }
                    }
                });
            } else {
                // Not connected — show connection form
                ui.horizontal(|ui| {
                    ui.label("Server:");
                    ui.text_edit_singleline(&mut app.session.server_url);
                });
                ui.horizontal(|ui| {
                    ui.label("Session:");
                    ui.text_edit_singleline(&mut app.session.session_id);
                });
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut app.session.peer_name);
                });

                if ui.button("Connect").clicked() {
                    match app.session.connect() {
                        Ok(()) => app.set_status("Connected to session"),
                        Err(e) => app.set_status(&format!("Connection failed: {e}")),
                    }
                }
            }
        });
}
