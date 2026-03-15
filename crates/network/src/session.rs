use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use uuid::Uuid;

use jamhub_model::{Project, Tempo, TimeSignature, Track};

use crate::message::PeerInfo;

/// Server-side session state.
pub struct Session {
    pub id: String,
    pub project: RwLock<Project>,
    pub peers: RwLock<HashMap<Uuid, PeerInfo>>,
}

impl Session {
    pub fn new(id: String) -> Self {
        let mut project = Project::default();
        project.name = format!("Session {id}");

        Self {
            id,
            project: RwLock::new(project),
            peers: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_peer(&self, peer: PeerInfo) {
        self.peers.write().insert(peer.id, peer);
    }

    pub fn remove_peer(&self, peer_id: &Uuid) {
        self.peers.write().remove(peer_id);
    }

    pub fn peer_list(&self) -> Vec<PeerInfo> {
        self.peers.read().values().cloned().collect()
    }

    pub fn add_track(&self, track: Track) {
        self.project.write().tracks.push(track);
    }

    pub fn remove_track(&self, track_id: &Uuid) {
        self.project.write().tracks.retain(|t| &t.id != track_id);
    }

    pub fn update_track(
        &self,
        track_id: &Uuid,
        volume: Option<f32>,
        pan: Option<f32>,
        muted: Option<bool>,
        solo: Option<bool>,
    ) {
        let mut project = self.project.write();
        if let Some(track) = project.tracks.iter_mut().find(|t| &t.id == track_id) {
            if let Some(v) = volume {
                track.volume = v;
            }
            if let Some(p) = pan {
                track.pan = p;
            }
            if let Some(m) = muted {
                track.muted = m;
            }
            if let Some(s) = solo {
                track.solo = s;
            }
        }
    }

    pub fn set_tempo(&self, tempo: Tempo) {
        self.project.write().tempo = tempo;
    }

    pub fn get_tracks(&self) -> Vec<Track> {
        self.project.read().tracks.clone()
    }

    pub fn get_tempo(&self) -> Tempo {
        self.project.read().tempo
    }

    pub fn get_time_signature(&self) -> TimeSignature {
        self.project.read().time_signature
    }
}
