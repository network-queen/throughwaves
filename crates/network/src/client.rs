use std::sync::Arc;

use crossbeam_channel::{bounded, Receiver, Sender};
use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
use tokio::runtime::Runtime;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::message::{PeerInfo, SessionMessage};

/// Client-side network handle for connecting to a session.
pub struct SessionClient {
    /// Send messages to the server.
    outgoing_tx: Sender<SessionMessage>,
    /// Receive messages from the server.
    incoming_rx: Receiver<SessionMessage>,
    /// Connected state.
    pub state: Arc<RwLock<ClientState>>,
    _runtime: Runtime,
}

#[derive(Default)]
pub struct ClientState {
    pub connected: bool,
    pub peer_id: Option<Uuid>,
    pub session_id: Option<String>,
    pub peers: Vec<PeerInfo>,
}

impl SessionClient {
    /// Connect to a session server.
    pub fn connect(server_url: &str, peer_name: &str, session_id: &str) -> Result<Self, String> {
        let rt = Runtime::new().map_err(|e| format!("Failed to create runtime: {e}"))?;

        let (outgoing_tx, outgoing_rx) = bounded::<SessionMessage>(256);
        let (incoming_tx, incoming_rx) = bounded::<SessionMessage>(256);
        let state = Arc::new(RwLock::new(ClientState::default()));

        let url = server_url.to_string();
        let peer_id = Uuid::new_v4();
        let peer_name = peer_name.to_string();
        let session_id_owned = session_id.to_string();
        let state_clone = state.clone();

        rt.spawn(async move {
            if let Err(e) = client_loop(
                &url,
                peer_id,
                &peer_name,
                &session_id_owned,
                outgoing_rx,
                incoming_tx,
                state_clone,
            )
            .await
            {
                eprintln!("Client connection error: {e}");
            }
        });

        Ok(Self {
            outgoing_tx,
            incoming_rx,
            state,
            _runtime: rt,
        })
    }

    /// Send a message to the session.
    pub fn send(&self, msg: SessionMessage) {
        let _ = self.outgoing_tx.send(msg);
    }

    /// Receive pending messages from the session (non-blocking).
    pub fn recv(&self) -> Vec<SessionMessage> {
        let mut messages = Vec::new();
        while let Ok(msg) = self.incoming_rx.try_recv() {
            messages.push(msg);
        }
        messages
    }

    pub fn is_connected(&self) -> bool {
        self.state.read().connected
    }
}

async fn client_loop(
    url: &str,
    peer_id: Uuid,
    peer_name: &str,
    session_id: &str,
    outgoing_rx: Receiver<SessionMessage>,
    incoming_tx: Sender<SessionMessage>,
    state: Arc<RwLock<ClientState>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Send Join message
    let join = SessionMessage::Join {
        peer: PeerInfo {
            id: peer_id,
            name: peer_name.to_string(),
        },
        session_id: session_id.to_string(),
    };
    ws_tx
        .send(Message::Text(serde_json::to_string(&join)?.into()))
        .await?;

    {
        let mut s = state.write();
        s.connected = true;
        s.peer_id = Some(peer_id);
        s.session_id = Some(session_id.to_string());
    }

    loop {
        tokio::select! {
            // Messages from server
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(session_msg) = serde_json::from_str::<SessionMessage>(&text.to_string()) {
                            // Update local state
                            match &session_msg {
                                SessionMessage::Welcome { peers, peer_id: pid, .. } => {
                                    let mut s = state.write();
                                    s.peer_id = Some(*pid);
                                    s.peers = peers.clone();
                                }
                                SessionMessage::PeerJoined { peer } => {
                                    state.write().peers.push(peer.clone());
                                }
                                SessionMessage::PeerLeft { peer_id } => {
                                    state.write().peers.retain(|p| &p.id != peer_id);
                                }
                                _ => {}
                            }
                            let _ = incoming_tx.send(session_msg);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // Messages from the app to send to server
            _ = tokio::task::spawn_blocking({
                let outgoing_rx = outgoing_rx.clone();
                move || outgoing_rx.recv()
            }) => {
                while let Ok(msg) = outgoing_rx.try_recv() {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if ws_tx.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }

    state.write().connected = false;
    Ok(())
}
