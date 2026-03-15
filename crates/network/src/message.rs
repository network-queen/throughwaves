use serde::{Deserialize, Serialize};
use uuid::Uuid;

use jamhub_model::{Clip, Tempo, TimeSignature, Track};

/// A unique peer in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: Uuid,
    pub name: String,
}

/// Messages sent between peers and the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionMessage {
    // -- Connection --
    /// Client requests to join a session.
    Join {
        peer: PeerInfo,
        session_id: String,
    },
    /// Server confirms join and sends current state.
    Welcome {
        peer_id: Uuid,
        session_id: String,
        peers: Vec<PeerInfo>,
        tracks: Vec<Track>,
        tempo: Tempo,
        time_signature: TimeSignature,
    },
    /// A new peer joined.
    PeerJoined {
        peer: PeerInfo,
    },
    /// A peer left.
    PeerLeft {
        peer_id: Uuid,
    },

    // -- Transport sync --
    /// Sync transport state across all peers.
    TransportSync {
        peer_id: Uuid,
        playing: bool,
        position_samples: u64,
    },
    /// Tempo change.
    TempoChange {
        peer_id: Uuid,
        tempo: Tempo,
    },

    // -- Track operations --
    /// A track was added.
    TrackAdded {
        peer_id: Uuid,
        track: Track,
    },
    /// Track property changed (volume, pan, mute, solo).
    TrackUpdated {
        peer_id: Uuid,
        track_id: Uuid,
        volume: Option<f32>,
        pan: Option<f32>,
        muted: Option<bool>,
        solo: Option<bool>,
    },
    /// A track was removed.
    TrackRemoved {
        peer_id: Uuid,
        track_id: Uuid,
    },

    // -- Clip operations --
    /// A clip was added to a track.
    ClipAdded {
        peer_id: Uuid,
        track_id: Uuid,
        clip: Clip,
    },
    /// A clip was moved.
    ClipMoved {
        peer_id: Uuid,
        track_id: Uuid,
        clip_id: Uuid,
        new_start_sample: u64,
    },
    /// A clip was removed.
    ClipRemoved {
        peer_id: Uuid,
        track_id: Uuid,
        clip_id: Uuid,
    },

    // -- Audio data --
    /// Request to upload audio buffer data (sent in chunks).
    AudioBufferChunk {
        buffer_id: Uuid,
        offset: usize,
        total_samples: usize,
        /// Base64-encoded f32 samples
        data: String,
    },

    // -- Chat --
    Chat {
        peer_id: Uuid,
        peer_name: String,
        message: String,
    },

    // -- Error --
    Error {
        message: String,
    },
}
