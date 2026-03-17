use std::collections::VecDeque;
use std::sync::Arc;

use crossbeam_channel::{bounded, Receiver, Sender};
use eframe::egui;
use parking_lot::Mutex;
use uuid::Uuid;

use crate::DawApp;

// ---------------------------------------------------------------------------
// JSON protocol types (mirror server types)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
enum JamControl {
    #[serde(rename = "join")]
    Join { username: String },
    #[serde(rename = "chat")]
    Chat { text: String },
    #[serde(rename = "mute")]
    Mute,
    #[serde(rename = "unmute")]
    Unmute,
    #[serde(rename = "set_volume")]
    SetVolume { user_id: Uuid, volume: f32 },
    #[serde(rename = "set_bpm")]
    SetBpm { bpm: f32 },
    #[serde(rename = "start_recording")]
    StartRecording,
    #[serde(rename = "stop_recording")]
    StopRecording,
    #[serde(rename = "leave")]
    Leave,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
enum JamEvent {
    #[serde(rename = "welcome")]
    Welcome { user_id: Uuid, room: RoomInfo },
    #[serde(rename = "participant_joined")]
    ParticipantJoined { user_id: Uuid, username: String },
    #[serde(rename = "participant_left")]
    ParticipantLeft { user_id: Uuid, username: String },
    #[serde(rename = "chat")]
    Chat {
        user_id: Uuid,
        username: String,
        text: String,
    },
    #[serde(rename = "bpm_changed")]
    BpmChanged { bpm: f32 },
    #[serde(rename = "participant_muted")]
    ParticipantMuted { user_id: Uuid, muted: bool },
    #[serde(rename = "recording_started")]
    RecordingStarted,
    #[serde(rename = "recording_stopped")]
    RecordingStopped,
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RoomInfo {
    id: Uuid,
    name: String,
    host_id: Uuid,
    bpm: f32,
    sample_rate: u32,
    recording: bool,
    participants: Vec<ParticipantInfo>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ParticipantInfo {
    user_id: Uuid,
    username: String,
    muted: bool,
    volume: f32,
}

// ---------------------------------------------------------------------------
// Jam participant (UI state)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct JamParticipant {
    pub user_id: Uuid,
    pub username: String,
    pub muted: bool,
    pub volume: f32,
}

// ---------------------------------------------------------------------------
// Jam session panel state
// ---------------------------------------------------------------------------

pub struct JamSessionPanel {
    pub show: bool,
    pub connected: bool,
    pub room_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub join_code: String,
    pub room_name: String,
    pub server_url: String,
    pub username: String,
    pub participants: Vec<JamParticipant>,
    pub bpm: f32,
    pub recording: bool,
    pub is_host: bool,
    pub latency_ms: f32,
    pub chat_messages: Vec<(String, String)>,
    pub chat_input: String,
    pub error_message: Option<String>,

    // Networking
    ctrl_tx: Option<Sender<JamControl>>,
    event_rx: Option<Receiver<JamEvent>>,
    audio_send_tx: Option<Sender<Vec<f32>>>,

    // Audio I/O
    audio_input_buffer: Arc<Mutex<VecDeque<f32>>>,
    audio_output_buffer: Arc<Mutex<VecDeque<f32>>>,
    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,

    // Runtime for async WebSocket
    _runtime: Option<tokio::runtime::Runtime>,
}

impl Default for JamSessionPanel {
    fn default() -> Self {
        Self {
            show: false,
            connected: false,
            room_id: None,
            user_id: None,
            join_code: String::new(),
            room_name: "My Jam".into(),
            server_url: "ws://127.0.0.1:3000".into(),
            username: whoami::fallible::hostname().unwrap_or_else(|_| "Musician".into()),
            participants: Vec::new(),
            bpm: 120.0,
            recording: false,
            is_host: false,
            latency_ms: 0.0,
            chat_messages: Vec::new(),
            chat_input: String::new(),
            error_message: None,
            ctrl_tx: None,
            event_rx: None,
            audio_send_tx: None,
            audio_input_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(8192))),
            audio_output_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(8192))),
            input_stream: None,
            output_stream: None,
            _runtime: None,
        }
    }
}

impl JamSessionPanel {
    /// Create a new jam room via REST, then connect via WebSocket.
    pub fn create_room(&mut self) -> Result<(), String> {
        let url = self.server_url.replace("ws://", "http://").replace("wss://", "https://");
        let create_url = format!("{}/api/jam/create", url);

        let body = serde_json::json!({
            "name": self.room_name,
        });

        let json_body = serde_json::to_string(&body)
            .map_err(|e| format!("Serialize: {e}"))?;
        let resp_str = ureq::post(&create_url)
            .header("Content-Type", "application/json")
            .send(json_body.as_bytes())
            .map_err(|e| format!("Create room failed: {e}"))?
            .into_body()
            .read_to_string()
            .map_err(|e| format!("Read response: {e}"))?;
        let resp: serde_json::Value = serde_json::from_str(&resp_str)
            .map_err(|e| format!("Parse response: {e}"))?;

        let room_id = resp["room_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or("Invalid room_id in response")?;
        let join_code = resp["join_code"]
            .as_str()
            .unwrap_or("")
            .to_string();

        self.room_id = Some(room_id);
        self.join_code = join_code;
        self.is_host = true;

        self.connect_ws(room_id)?;
        Ok(())
    }

    /// Join an existing room by code (first 8 chars of room UUID).
    pub fn join_room(&mut self) -> Result<(), String> {
        let url = self.server_url.replace("ws://", "http://").replace("wss://", "https://");
        let list_url = format!("{}/api/jam/rooms", url);

        let rooms_str = ureq::get(&list_url)
            .call()
            .map_err(|e| format!("List rooms failed: {e}"))?
            .into_body()
            .read_to_string()
            .map_err(|e| format!("Read rooms: {e}"))?;
        let rooms: Vec<serde_json::Value> = serde_json::from_str(&rooms_str)
            .map_err(|e| format!("Parse rooms: {e}"))?;

        let code = self.join_code.trim().to_uppercase();
        let room_id = rooms
            .iter()
            .find_map(|r| {
                let id_str = r["id"].as_str()?;
                if id_str.to_uppercase().starts_with(&code) {
                    Uuid::parse_str(id_str).ok()
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("No room found with code {code}"))?;

        self.room_id = Some(room_id);
        self.is_host = false;
        self.connect_ws(room_id)?;
        Ok(())
    }

    fn connect_ws(&mut self, room_id: Uuid) -> Result<(), String> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Runtime: {e}"))?;

        let (ctrl_tx, ctrl_rx) = bounded::<JamControl>(256);
        let (event_tx, event_rx) = bounded::<JamEvent>(256);
        let (audio_tx, audio_rx) = bounded::<Vec<f32>>(64);

        let ws_url = format!("{}/api/jam/{}", self.server_url, room_id);
        let username = self.username.clone();

        let audio_out_buf = self.audio_output_buffer.clone();

        rt.spawn(async move {
            if let Err(e) =
                ws_loop(ws_url, username, ctrl_rx, event_tx, audio_rx, audio_out_buf).await
            {
                eprintln!("Jam WS error: {e}");
            }
        });

        self.ctrl_tx = Some(ctrl_tx);
        self.event_rx = Some(event_rx);
        self.audio_send_tx = Some(audio_tx);
        self._runtime = Some(rt);

        // Start audio capture and playback
        self.start_audio()?;

        self.connected = true;
        self.error_message = None;
        Ok(())
    }

    fn start_audio(&mut self) -> Result<(), String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();

        // --- Input stream: capture mic -> send buffer ---
        let in_device = host.default_input_device().ok_or("No input device")?;
        let in_config: cpal::StreamConfig = in_device
            .default_input_config()
            .map_err(|e| format!("Input config: {e}"))?
            .into();
        let in_channels = in_config.channels as usize;

        let send_buf = self.audio_input_buffer.clone();
        let audio_tx = self.audio_send_tx.clone();

        let input_stream = in_device
            .build_input_stream(
                &in_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut buf = send_buf.lock();
                    // Mix to mono
                    if in_channels > 1 {
                        for frame in data.chunks(in_channels) {
                            let mono: f32 = frame.iter().sum::<f32>() / in_channels as f32;
                            buf.push_back(mono);
                        }
                    } else {
                        for &s in data {
                            buf.push_back(s);
                        }
                    }

                    // Send 256-sample blocks
                    while buf.len() >= 256 {
                        let block: Vec<f32> = buf.drain(..256).collect();
                        if let Some(ref tx) = audio_tx {
                            let _ = tx.try_send(block);
                        }
                    }
                },
                |e| eprintln!("Jam input error: {e}"),
                None,
            )
            .map_err(|e| format!("Build input: {e}"))?;

        input_stream
            .play()
            .map_err(|e| format!("Play input: {e}"))?;

        // --- Output stream: play received mix ---
        let out_device = host.default_output_device().ok_or("No output device")?;
        let out_config: cpal::StreamConfig = out_device
            .default_output_config()
            .map_err(|e| format!("Output config: {e}"))?
            .into();
        let out_channels = out_config.channels as usize;

        let play_buf = self.audio_output_buffer.clone();

        let output_stream = out_device
            .build_output_stream(
                &out_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut buf = play_buf.lock();
                    for frame in data.chunks_mut(out_channels) {
                        let sample = buf.pop_front().unwrap_or(0.0);
                        for ch in frame.iter_mut() {
                            *ch = sample;
                        }
                    }
                },
                |e| eprintln!("Jam output error: {e}"),
                None,
            )
            .map_err(|e| format!("Build output: {e}"))?;

        output_stream
            .play()
            .map_err(|e| format!("Play output: {e}"))?;

        self.input_stream = Some(input_stream);
        self.output_stream = Some(output_stream);
        Ok(())
    }

    pub fn disconnect(&mut self) {
        if let Some(ref tx) = self.ctrl_tx {
            let _ = tx.try_send(JamControl::Leave);
        }
        self.input_stream = None;
        self.output_stream = None;
        self.ctrl_tx = None;
        self.event_rx = None;
        self.audio_send_tx = None;
        self._runtime = None;
        self.connected = false;
        self.participants.clear();
        self.room_id = None;
        self.user_id = None;
    }

    fn send_control(&self, ctrl: JamControl) {
        if let Some(ref tx) = self.ctrl_tx {
            let _ = tx.try_send(ctrl);
        }
    }

    /// Poll events from the WebSocket connection.
    pub fn poll(&mut self) {
        let Some(ref rx) = self.event_rx else {
            return;
        };

        while let Ok(event) = rx.try_recv() {
            match event {
                JamEvent::Welcome { user_id, room } => {
                    self.user_id = Some(user_id);
                    self.bpm = room.bpm;
                    self.recording = room.recording;
                    self.participants = room
                        .participants
                        .iter()
                        .map(|p| JamParticipant {
                            user_id: p.user_id,
                            username: p.username.clone(),
                            muted: p.muted,
                            volume: p.volume,
                        })
                        .collect();
                    self.chat_messages
                        .push(("System".into(), format!("Joined room \"{}\"", room.name)));
                }
                JamEvent::ParticipantJoined { user_id, username } => {
                    self.participants.push(JamParticipant {
                        user_id,
                        username: username.clone(),
                        muted: false,
                        volume: 1.0,
                    });
                    self.chat_messages
                        .push(("System".into(), format!("{username} joined")));
                }
                JamEvent::ParticipantLeft { user_id, username } => {
                    self.participants.retain(|p| p.user_id != user_id);
                    self.chat_messages
                        .push(("System".into(), format!("{username} left")));
                }
                JamEvent::Chat {
                    username, text, ..
                } => {
                    self.chat_messages.push((username, text));
                }
                JamEvent::BpmChanged { bpm } => {
                    self.bpm = bpm;
                }
                JamEvent::ParticipantMuted { user_id, muted } => {
                    if let Some(p) = self.participants.iter_mut().find(|p| p.user_id == user_id) {
                        p.muted = muted;
                    }
                }
                JamEvent::RecordingStarted => {
                    self.recording = true;
                    self.chat_messages
                        .push(("System".into(), "Recording started".into()));
                }
                JamEvent::RecordingStopped => {
                    self.recording = false;
                    self.chat_messages
                        .push(("System".into(), "Recording stopped".into()));
                }
                JamEvent::Error { message } => {
                    self.error_message = Some(message.clone());
                    self.chat_messages.push(("Error".into(), message));
                }
            }
        }

        // Estimate latency from jitter buffer fill level
        let buf_len = self.audio_output_buffer.lock().len();
        self.latency_ms = (buf_len as f32 / 44100.0) * 1000.0;
    }
}

// ---------------------------------------------------------------------------
// WebSocket loop (runs in tokio runtime)
// ---------------------------------------------------------------------------

async fn ws_loop(
    url: String,
    username: String,
    ctrl_rx: Receiver<JamControl>,
    event_tx: Sender<JamEvent>,
    audio_rx: Receiver<Vec<f32>>,
    audio_out_buf: Arc<Mutex<VecDeque<f32>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Send Join
    let join = JamControl::Join {
        username: username.clone(),
    };
    ws_tx
        .send(Message::Text(serde_json::to_string(&join)?.into()))
        .await?;

    loop {
        tokio::select! {
            // Incoming from server
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        // Could be audio or JSON-prefixed message
                        if !data.is_empty() && data[0] == 0x00 {
                            // JSON event (skip the 0x00 prefix byte)
                            if let Ok(text) = std::str::from_utf8(&data[1..]) {
                                if let Ok(event) = serde_json::from_str::<JamEvent>(text) {
                                    let _ = event_tx.try_send(event);
                                }
                            }
                        } else {
                            // Audio: raw f32 LE samples
                            let samples: Vec<f32> = data
                                .chunks_exact(4)
                                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                                .collect();
                            let mut buf = audio_out_buf.lock();
                            buf.extend(samples.iter());
                            // Jitter buffer: keep 512-1024 samples, discard excess
                            while buf.len() > 2048 {
                                buf.pop_front();
                            }
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        // JSON event
                        if let Ok(event) = serde_json::from_str::<JamEvent>(&text) {
                            let _ = event_tx.try_send(event);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // Send audio blocks
            _ = tokio::task::spawn_blocking({
                let audio_rx = audio_rx.clone();
                move || audio_rx.recv()
            }) => {
                // Drain all available audio blocks
                while let Ok(block) = audio_rx.try_recv() {
                    let bytes: Vec<u8> = block.iter().flat_map(|s| s.to_le_bytes()).collect();
                    if ws_tx.send(Message::Binary(bytes.into())).await.is_err() {
                        return Ok(());
                    }
                }
            }
            // Send control messages
            _ = tokio::task::spawn_blocking({
                let ctrl_rx = ctrl_rx.clone();
                move || ctrl_rx.recv()
            }) => {
                while let Ok(ctrl) = ctrl_rx.try_recv() {
                    let json = serde_json::to_string(&ctrl)?;
                    if ws_tx.send(Message::Text(json.into())).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

pub fn show(app: &mut DawApp, ctx: &egui::Context) {
    if !app.jam.show {
        return;
    }

    // Poll events
    app.jam.poll();

    let accent = egui::Color32::from_rgb(240, 192, 64);
    let mut open = app.jam.show;
    let _teal = egui::Color32::from_rgb(80, 200, 190);
    let red = egui::Color32::from_rgb(220, 80, 80);
    let green = egui::Color32::from_rgb(80, 200, 80);

    egui::Window::new("Live Jam Session")
        .default_width(380.0)
        .default_height(520.0)
        .resizable(true)
        .collapsible(true)
        .open(&mut open)
        .show(ctx, |ui| {
            if app.jam.connected {
                // --- Connected state ---
                ui.horizontal(|ui| {
                    ui.colored_label(green, "CONNECTED");
                    if let Some(ref id) = app.jam.room_id {
                        ui.label(
                            egui::RichText::new(format!("Room: {}", &id.to_string()[..8]))
                                .color(accent)
                                .strong(),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(format!("Code: {}", app.jam.join_code));
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(app.jam.join_code.clone());
                    }
                });

                ui.separator();

                // BPM
                ui.horizontal(|ui| {
                    ui.label("BPM:");
                    ui.label(
                        egui::RichText::new(format!("{:.0}", app.jam.bpm))
                            .strong()
                            .color(accent),
                    );
                    if ui.small_button("-").clicked() {
                        let new_bpm = (app.jam.bpm - 1.0).max(20.0);
                        app.jam.send_control(JamControl::SetBpm { bpm: new_bpm });
                    }
                    if ui.small_button("+").clicked() {
                        let new_bpm = (app.jam.bpm + 1.0).min(300.0);
                        app.jam.send_control(JamControl::SetBpm { bpm: new_bpm });
                    }
                });

                // Latency
                ui.horizontal(|ui| {
                    ui.label("Latency:");
                    let color = if app.jam.latency_ms < 20.0 {
                        green
                    } else if app.jam.latency_ms < 50.0 {
                        accent
                    } else {
                        red
                    };
                    ui.colored_label(color, format!("{:.0} ms", app.jam.latency_ms));
                });

                // Recording
                ui.horizontal(|ui| {
                    if app.jam.recording {
                        ui.colored_label(red, "REC");
                        if ui.button("Stop Recording").clicked() {
                            app.jam.send_control(JamControl::StopRecording);
                        }
                    } else if ui.button("Start Recording").clicked() {
                        app.jam.send_control(JamControl::StartRecording);
                    }
                });

                ui.separator();

                // --- Participants ---
                ui.heading("Participants");
                let my_id = app.jam.user_id;
                let participants = app.jam.participants.clone();
                for p in &participants {
                    ui.horizontal(|ui| {
                        let is_me = my_id == Some(p.user_id);
                        let name_text = if is_me {
                            format!("{} (you)", p.username)
                        } else {
                            p.username.clone()
                        };

                        if p.muted {
                            ui.colored_label(
                                egui::Color32::from_rgb(120, 120, 120),
                                "M",
                            );
                        } else {
                            ui.colored_label(green, "~");
                        }

                        ui.label(name_text);

                        if !is_me {
                            // Volume slider for other participants
                            let mut vol = p.volume;
                            if ui
                                .add(egui::Slider::new(&mut vol, 0.0..=2.0).max_decimals(1))
                                .changed()
                            {
                                app.jam.send_control(JamControl::SetVolume {
                                    user_id: p.user_id,
                                    volume: vol,
                                });
                                // Update local state
                                if let Some(lp) = app
                                    .jam
                                    .participants
                                    .iter_mut()
                                    .find(|x| x.user_id == p.user_id)
                                {
                                    lp.volume = vol;
                                }
                            }
                        }
                    });
                }

                // Mute self button
                ui.horizontal(|ui| {
                    let is_muted = my_id
                        .and_then(|id| {
                            app.jam.participants.iter().find(|p| p.user_id == id)
                        })
                        .map(|p| p.muted)
                        .unwrap_or(false);

                    if is_muted {
                        if ui.button("Unmute").clicked() {
                            app.jam.send_control(JamControl::Unmute);
                            if let Some(id) = my_id {
                                if let Some(p) = app.jam.participants.iter_mut().find(|p| p.user_id == id) {
                                    p.muted = false;
                                }
                            }
                        }
                    } else if ui.button("Mute").clicked() {
                        app.jam.send_control(JamControl::Mute);
                        if let Some(id) = my_id {
                            if let Some(p) = app.jam.participants.iter_mut().find(|p| p.user_id == id) {
                                p.muted = true;
                            }
                        }
                    }
                });

                ui.separator();

                // --- Chat ---
                ui.heading("Chat");
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (name, msg) in &app.jam.chat_messages {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(format!("{name}:"));
                                ui.label(msg);
                            });
                        }
                    });

                ui.horizontal(|ui| {
                    let response = ui.text_edit_singleline(&mut app.jam.chat_input);
                    if (response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Send").clicked()
                    {
                        if !app.jam.chat_input.is_empty() {
                            let text = app.jam.chat_input.clone();
                            app.jam.chat_input.clear();
                            app.jam
                                .chat_messages
                                .push((app.jam.username.clone(), text.clone()));
                            app.jam.send_control(JamControl::Chat { text });
                        }
                    }
                });

                ui.separator();

                // Disconnect button
                if ui
                    .button(
                        egui::RichText::new("Leave Session")
                            .color(red)
                            .strong(),
                    )
                    .clicked()
                {
                    app.jam.disconnect();
                }
            } else {
                // --- Not connected ---
                ui.heading("Create or Join a Jam Room");
                ui.add_space(4.0);

                if let Some(ref err) = app.jam.error_message {
                    ui.colored_label(red, err.as_str());
                    ui.add_space(4.0);
                }

                ui.horizontal(|ui| {
                    ui.label("Server:");
                    ui.text_edit_singleline(&mut app.jam.server_url);
                });
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut app.jam.username);
                });

                ui.add_space(8.0);
                ui.separator();

                // Create room
                ui.label(egui::RichText::new("Create Room").strong());
                ui.horizontal(|ui| {
                    ui.label("Room name:");
                    ui.text_edit_singleline(&mut app.jam.room_name);
                });
                if ui.button("Create Room").clicked() {
                    match app.jam.create_room() {
                        Ok(()) => app.set_status("Jam room created"),
                        Err(e) => {
                            app.jam.error_message = Some(e.clone());
                            app.set_status(&format!("Jam error: {e}"));
                        }
                    }
                }

                ui.add_space(8.0);
                ui.separator();

                // Join room
                ui.label(egui::RichText::new("Join Room").strong());
                ui.horizontal(|ui| {
                    ui.label("Code:");
                    ui.text_edit_singleline(&mut app.jam.join_code);
                });
                if ui.button("Join Room").clicked() {
                    match app.jam.join_room() {
                        Ok(()) => app.set_status("Joined jam room"),
                        Err(e) => {
                            app.jam.error_message = Some(e.clone());
                            app.set_status(&format!("Jam error: {e}"));
                        }
                    }
                }
            }
        });
    app.jam.show = open;

    // Request repaint while connected (for latency updates / chat)
    if app.jam.connected {
        ctx.request_repaint();
    }
}
