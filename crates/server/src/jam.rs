use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use parking_lot::RwLock;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub user_id: Uuid,
    pub username: String,
    pub connected: bool,
    pub muted: bool,
    pub volume: f32,
}

#[derive(Debug)]
pub(crate) struct ParticipantState {
    info: Participant,
    audio_buffer: VecDeque<f32>,
    ws_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
}

pub struct JamRoom {
    pub id: Uuid,
    pub name: String,
    pub host_id: Uuid,
    #[allow(dead_code)]
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub participants: HashMap<Uuid, ParticipantState>,
    pub bpm: f32,
    pub sample_rate: u32,
    pub recording: bool,
    pub max_participants: usize,
    /// Recorded audio per participant (user_id -> samples)
    pub recorded_tracks: HashMap<Uuid, Vec<f32>>,
}

impl JamRoom {
    fn new(name: String, host_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            host_id,
            created_at: chrono::Utc::now(),
            participants: HashMap::new(),
            bpm: 120.0,
            sample_rate: 44100,
            recording: false,
            max_participants: 8,
            recorded_tracks: HashMap::new(),
        }
    }

    /// Mix audio from all participants except `exclude_id`, applying per-participant volume.
    fn mix_for(&self, exclude_id: &Uuid, block_size: usize) -> Vec<f32> {
        let mut mix = vec![0.0f32; block_size];
        for (uid, p) in &self.participants {
            if uid == exclude_id || p.info.muted {
                continue;
            }
            let vol = p.info.volume;
            let buf = &p.audio_buffer;
            let available = buf.len().min(block_size);
            for i in 0..available {
                mix[i] += buf[i] * vol;
            }
        }
        // Soft-clip
        for s in mix.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
        mix
    }
}

type Rooms = Arc<RwLock<HashMap<Uuid, Arc<RwLock<JamRoom>>>>>;

#[derive(Clone)]
pub struct JamState {
    rooms: Rooms,
}

impl JamState {
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

// ---------------------------------------------------------------------------
// JSON messages (control plane)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum JamControl {
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum JamEvent {
    #[serde(rename = "welcome")]
    Welcome {
        user_id: Uuid,
        room: RoomInfo,
    },
    #[serde(rename = "participant_joined")]
    ParticipantJoined { user_id: Uuid, username: String },
    #[serde(rename = "participant_left")]
    ParticipantLeft { user_id: Uuid, username: String },
    #[serde(rename = "chat")]
    Chat { user_id: Uuid, username: String, text: String },
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub id: Uuid,
    pub name: String,
    pub host_id: Uuid,
    pub bpm: f32,
    pub sample_rate: u32,
    pub recording: bool,
    pub participants: Vec<ParticipantInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantInfo {
    pub user_id: Uuid,
    pub username: String,
    pub muted: bool,
    pub volume: f32,
}

// ---------------------------------------------------------------------------
// REST endpoints
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateRoomReq {
    name: String,
    host_id: Option<Uuid>,
}

#[derive(Serialize)]
struct CreateRoomResp {
    room_id: Uuid,
    join_code: String,
}

async fn create_room(
    State(jam): State<JamState>,
    Json(body): Json<CreateRoomReq>,
) -> impl IntoResponse {
    let host_id = body.host_id.unwrap_or_else(Uuid::new_v4);
    let room = JamRoom::new(body.name, host_id);
    let room_id = room.id;
    // Use first 8 chars of UUID as join code
    let join_code = room_id.to_string()[..8].to_uppercase();
    let room = Arc::new(RwLock::new(room));
    jam.rooms.write().insert(room_id, room);

    Json(CreateRoomResp { room_id, join_code })
}

async fn list_rooms(State(jam): State<JamState>) -> impl IntoResponse {
    let rooms = jam.rooms.read();
    let infos: Vec<RoomInfo> = rooms
        .values()
        .map(|r| {
            let r = r.read();
            RoomInfo {
                id: r.id,
                name: r.name.clone(),
                host_id: r.host_id,
                bpm: r.bpm,
                sample_rate: r.sample_rate,
                recording: r.recording,
                participants: r
                    .participants
                    .values()
                    .map(|p| ParticipantInfo {
                        user_id: p.info.user_id,
                        username: p.info.username.clone(),
                        muted: p.info.muted,
                        volume: p.info.volume,
                    })
                    .collect(),
            }
        })
        .collect();
    Json(infos)
}

async fn room_details(
    State(jam): State<JamState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let rooms = jam.rooms.read();
    match rooms.get(&id) {
        Some(r) => {
            let r = r.read();
            let info = RoomInfo {
                id: r.id,
                name: r.name.clone(),
                host_id: r.host_id,
                bpm: r.bpm,
                sample_rate: r.sample_rate,
                recording: r.recording,
                participants: r
                    .participants
                    .values()
                    .map(|p| ParticipantInfo {
                        user_id: p.info.user_id,
                        username: p.info.username.clone(),
                        muted: p.info.muted,
                        volume: p.info.volume,
                    })
                    .collect(),
            };
            (axum::http::StatusCode::OK, Json(serde_json::to_value(info).unwrap()))
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Room not found"})),
        ),
    }
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

async fn ws_upgrade(
    State(jam): State<JamState>,
    Path(room_id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, jam, room_id))
}

async fn handle_ws(socket: WebSocket, jam: JamState, room_id: Uuid) {
    use futures_util::{SinkExt, StreamExt};

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Wait for the Join control message first
    let (username, user_id) = loop {
        match ws_rx.next().await {
            Some(Ok(Message::Text(text))) => {
                if let Ok(ctrl) = serde_json::from_str::<JamControl>(&text) {
                    if let JamControl::Join { username } = ctrl {
                        break (username, Uuid::new_v4());
                    }
                }
            }
            Some(Ok(_)) => continue,
            _ => return, // connection closed before join
        }
    };

    // Check that the room exists
    let room = jam.rooms.read().get(&room_id).cloned();
    let room = match room {
        Some(r) => r,
        None => {
            let err = JamEvent::Error {
                message: "Room not found".into(),
            };
            let _ = ws_tx
                .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                .await;
            return;
        }
    };

    // Check max participants
    let is_full = {
        let r = room.read();
        r.participants.len() >= r.max_participants
    };
    if is_full {
        let err = JamEvent::Error {
            message: "Room is full".into(),
        };
        let _ = ws_tx
            .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
            .await;
        return;
    }

    // Create channel for sending messages back to this participant
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

    // Build room info for welcome and add participant
    let room_info = {
        let mut r = room.write();
        r.participants.insert(
            user_id,
            ParticipantState {
                info: Participant {
                    user_id,
                    username: username.clone(),
                    connected: true,
                    muted: false,
                    volume: 1.0,
                },
                audio_buffer: VecDeque::with_capacity(4096),
                ws_tx: tx.clone(),
            },
        );
        RoomInfo {
            id: r.id,
            name: r.name.clone(),
            host_id: r.host_id,
            bpm: r.bpm,
            sample_rate: r.sample_rate,
            recording: r.recording,
            participants: r
                .participants
                .values()
                .map(|p| ParticipantInfo {
                    user_id: p.info.user_id,
                    username: p.info.username.clone(),
                    muted: p.info.muted,
                    volume: p.info.volume,
                })
                .collect(),
        }
    };

    // Send Welcome
    let welcome = JamEvent::Welcome {
        user_id,
        room: room_info,
    };
    let _ = ws_tx
        .send(Message::Text(
            serde_json::to_string(&welcome).unwrap().into(),
        ))
        .await;

    // Broadcast participant_joined to all others
    broadcast_event(
        &room,
        &user_id,
        &JamEvent::ParticipantJoined {
            user_id,
            username: username.clone(),
        },
    );

    const BLOCK_SIZE: usize = 256;

    // Spawn task to forward outgoing messages (mixed audio) to this client
    let send_task = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if ws_tx.send(Message::Binary(data.into())).await.is_err() {
                break;
            }
        }
    });

    // Spawn mixer task: periodically mix and send audio to all participants
    let room_for_mixer = room.clone();
    let mixer_handle = tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_micros(5800)); // ~256 samples @ 44100
        loop {
            interval.tick().await;

            // Mix and send in a sync block, then release the lock
            let should_break = {
                let r = room_for_mixer.read();
                if r.participants.is_empty() {
                    true
                } else {
                    let user_ids: Vec<Uuid> = r.participants.keys().copied().collect();
                    for uid in &user_ids {
                        let mix = r.mix_for(uid, BLOCK_SIZE);
                        let bytes: Vec<u8> = mix
                            .iter()
                            .flat_map(|s| s.to_le_bytes())
                            .collect();
                        if let Some(p) = r.participants.get(uid) {
                            let _ = p.ws_tx.send(bytes);
                        }
                    }
                    false
                }
            };
            if should_break {
                break;
            }

            // Drain consumed samples from all buffers
            {
                let mut rw = room_for_mixer.write();
                for p in rw.participants.values_mut() {
                    let drain = p.audio_buffer.len().min(BLOCK_SIZE);
                    p.audio_buffer.drain(..drain);
                }
            }
        }
    });

    // Main receive loop
    loop {
        match ws_rx.next().await {
            Some(Ok(Message::Binary(data))) => {
                // Audio data: raw f32 LE samples
                let samples: Vec<f32> = data
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();

                let mut r = room.write();
                if let Some(p) = r.participants.get_mut(&user_id) {
                    p.audio_buffer.extend(samples.iter());
                    // Limit buffer to prevent unbounded growth
                    while p.audio_buffer.len() > 8192 {
                        p.audio_buffer.pop_front();
                    }
                }
                // Record if enabled
                if r.recording {
                    r.recorded_tracks
                        .entry(user_id)
                        .or_insert_with(Vec::new)
                        .extend_from_slice(&samples);
                }
            }
            Some(Ok(Message::Text(text))) => {
                // Control message
                if let Ok(ctrl) = serde_json::from_str::<JamControl>(&text) {
                    match ctrl {
                        JamControl::Chat { text: chat_text } => {
                            broadcast_event(
                                &room,
                                &user_id,
                                &JamEvent::Chat {
                                    user_id,
                                    username: username.clone(),
                                    text: chat_text,
                                },
                            );
                        }
                        JamControl::Mute => {
                            {
                                let mut r = room.write();
                                if let Some(p) = r.participants.get_mut(&user_id) {
                                    p.info.muted = true;
                                }
                            }
                            broadcast_event(
                                &room,
                                &user_id,
                                &JamEvent::ParticipantMuted {
                                    user_id,
                                    muted: true,
                                },
                            );
                        }
                        JamControl::Unmute => {
                            {
                                let mut r = room.write();
                                if let Some(p) = r.participants.get_mut(&user_id) {
                                    p.info.muted = false;
                                }
                            }
                            broadcast_event(
                                &room,
                                &user_id,
                                &JamEvent::ParticipantMuted {
                                    user_id,
                                    muted: false,
                                },
                            );
                        }
                        JamControl::SetVolume { user_id: target, volume } => {
                            let mut r = room.write();
                            if let Some(p) = r.participants.get_mut(&target) {
                                p.info.volume = volume.clamp(0.0, 2.0);
                            }
                        }
                        JamControl::SetBpm { bpm } => {
                            let new_bpm = {
                                let mut r = room.write();
                                r.bpm = bpm.clamp(20.0, 300.0);
                                r.bpm
                            };
                            broadcast_event_all(
                                &room,
                                &JamEvent::BpmChanged { bpm: new_bpm },
                            );
                        }
                        JamControl::StartRecording => {
                            {
                                let mut r = room.write();
                                r.recording = true;
                                r.recorded_tracks.clear();
                            }
                            broadcast_event_all(&room, &JamEvent::RecordingStarted);
                        }
                        JamControl::StopRecording => {
                            room.write().recording = false;
                            broadcast_event_all(&room, &JamEvent::RecordingStopped);
                        }
                        JamControl::Leave | JamControl::Join { .. } => break,
                    }
                }
            }
            Some(Ok(Message::Close(_))) | None => break,
            _ => {}
        }
    }

    // Cleanup
    {
        room.write().participants.remove(&user_id);
    }
    broadcast_event(
        &room,
        &user_id,
        &JamEvent::ParticipantLeft {
            user_id,
            username: username.clone(),
        },
    );

    mixer_handle.abort();
    send_task.abort();

    // Remove empty rooms
    {
        let is_empty = room.read().participants.is_empty();
        if is_empty {
            let id = room.read().id;
            jam.rooms.write().remove(&id);
        }
    }
}

/// Send a JSON event to all participants except `exclude_id`.
fn broadcast_event(room: &Arc<RwLock<JamRoom>>, exclude_id: &Uuid, event: &JamEvent) {
    let json = serde_json::to_string(event).unwrap();
    let bytes = json.into_bytes();
    let r = room.read();
    for (uid, p) in &r.participants {
        if uid != exclude_id {
            let mut msg = Vec::with_capacity(1 + bytes.len());
            msg.push(0x00); // JSON marker byte
            msg.extend_from_slice(&bytes);
            let _ = p.ws_tx.send(msg);
        }
    }
}

/// Send a JSON event to ALL participants (including sender).
fn broadcast_event_all(room: &Arc<RwLock<JamRoom>>, event: &JamEvent) {
    let json = serde_json::to_string(event).unwrap();
    let bytes = json.into_bytes();
    let r = room.read();
    for p in r.participants.values() {
        let mut msg = Vec::with_capacity(1 + bytes.len());
        msg.push(0x00);
        msg.extend_from_slice(&bytes);
        let _ = p.ws_tx.send(msg);
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<PgPool> {
    let jam_state = JamState::new();

    let jam_routes = Router::new()
        .route("/jam/create", post(create_room))
        .route("/jam/rooms", get(list_rooms))
        .route("/jam/rooms/{id}", get(room_details))
        .route("/jam/{room_id}", get(ws_upgrade))
        .with_state(jam_state);

    jam_routes
}
