use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::message::{PeerInfo, SessionMessage};
use crate::session::Session;

type Sessions = Arc<RwLock<HashMap<String, Arc<Session>>>>;
type BroadcastTx = broadcast::Sender<(Uuid, String)>;

pub struct SessionServer {
    sessions: Sessions,
    broadcast_channels: Arc<RwLock<HashMap<String, BroadcastTx>>>,
}

impl SessionServer {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            broadcast_channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn run(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(addr).await?;
        println!("JamHub server listening on {addr}");

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let sessions = self.sessions.clone();
            let broadcast_channels = self.broadcast_channels.clone();

            tokio::spawn(async move {
                if let Err(e) =
                    handle_connection(stream, peer_addr, sessions, broadcast_channels).await
                {
                    eprintln!("Connection error from {peer_addr}: {e}");
                }
            });
        }
    }

    pub fn get_or_create_session(&self, session_id: &str) -> Arc<Session> {
        let mut sessions = self.sessions.write();
        sessions
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(Session::new(session_id.to_string())))
            .clone()
    }
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    sessions: Sessions,
    broadcast_channels: Arc<RwLock<HashMap<String, BroadcastTx>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    println!("New WebSocket connection from {addr}");

    // Wait for Join message
    let join_msg = loop {
        if let Some(msg) = ws_rx.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                if let Ok(session_msg) = serde_json::from_str::<SessionMessage>(&text) {
                    break session_msg;
                }
            }
        } else {
            return Ok(());
        }
    };

    let (peer_info, session_id) = match join_msg {
        SessionMessage::Join { peer, session_id } => (peer, session_id),
        _ => {
            let err = SessionMessage::Error {
                message: "Expected Join message".into(),
            };
            ws_tx
                .send(Message::Text(serde_json::to_string(&err)?.into()))
                .await?;
            return Ok(());
        }
    };

    let peer_id = peer_info.id;

    // Get or create session
    let session = {
        let mut sessions = sessions.write();
        sessions
            .entry(session_id.clone())
            .or_insert_with(|| Arc::new(Session::new(session_id.clone())))
            .clone()
    };

    // Get or create broadcast channel for this session
    let broadcast_tx = {
        let mut channels = broadcast_channels.write();
        channels
            .entry(session_id.clone())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(256);
                tx
            })
            .clone()
    };
    let mut broadcast_rx = broadcast_tx.subscribe();

    // Add peer to session
    session.add_peer(peer_info.clone());

    // Send Welcome
    let welcome = SessionMessage::Welcome {
        peer_id,
        session_id: session_id.clone(),
        peers: session.peer_list(),
        tracks: session.get_tracks(),
        tempo: session.get_tempo(),
        time_signature: session.get_time_signature(),
    };
    ws_tx
        .send(Message::Text(serde_json::to_string(&welcome)?.into()))
        .await?;

    // Broadcast PeerJoined to others
    let joined_msg = serde_json::to_string(&SessionMessage::PeerJoined {
        peer: peer_info.clone(),
    })?;
    let _ = broadcast_tx.send((peer_id, joined_msg));

    // Main message loop
    loop {
        tokio::select! {
            // Messages from this peer
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(session_msg) = serde_json::from_str::<SessionMessage>(&text.to_string()) {
                            handle_session_message(&session, &session_msg);
                            // Broadcast to all peers
                            let _ = broadcast_tx.send((peer_id, text.to_string()));
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // Messages from other peers
            msg = broadcast_rx.recv() => {
                if let Ok((sender_id, text)) = msg {
                    // Don't echo messages back to sender
                    if sender_id != peer_id {
                        if ws_tx.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }

    // Cleanup
    session.remove_peer(&peer_id);
    let left_msg = serde_json::to_string(&SessionMessage::PeerLeft { peer_id })?;
    let _ = broadcast_tx.send((peer_id, left_msg));

    println!("Peer {peer_id} disconnected from session {session_id}");
    Ok(())
}

fn handle_session_message(session: &Session, msg: &SessionMessage) {
    match msg {
        SessionMessage::TrackAdded { track, .. } => {
            session.add_track(track.clone());
        }
        SessionMessage::TrackRemoved { track_id, .. } => {
            session.remove_track(track_id);
        }
        SessionMessage::TrackUpdated {
            track_id,
            volume,
            pan,
            muted,
            solo,
            ..
        } => {
            session.update_track(track_id, *volume, *pan, *muted, *solo);
        }
        SessionMessage::TempoChange { tempo, .. } => {
            session.set_tempo(*tempo);
        }
        _ => {}
    }
}
