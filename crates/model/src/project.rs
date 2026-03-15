use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::time::{Tempo, TimeSignature};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub tempo: Tempo,
    pub time_signature: TimeSignature,
    pub sample_rate: u32,
    pub tracks: Vec<Track>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: "Untitled Session".into(),
            tempo: Tempo::default(),
            time_signature: TimeSignature::default(),
            sample_rate: 44100,
            tracks: Vec::new(),
        }
    }
}

impl Project {
    pub fn add_track(&mut self, name: &str, kind: TrackKind) -> Uuid {
        let id = Uuid::new_v4();
        self.tracks.push(Track {
            id,
            name: name.to_string(),
            kind,
            clips: Vec::new(),
            volume: 1.0,
            pan: 0.0,
            muted: false,
            solo: false,
            armed: false,
            color: random_track_color(),
        });
        id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: Uuid,
    pub name: String,
    pub kind: TrackKind,
    pub clips: Vec<Clip>,
    pub volume: f32,
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
    pub armed: bool,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Audio,
    Midi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: Uuid,
    pub name: String,
    pub start_sample: u64,
    pub duration_samples: u64,
    pub source: ClipSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipSource {
    AudioFile { path: String },
    AudioBuffer { buffer_id: Uuid },
    Midi { notes: Vec<MidiNote> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiNote {
    pub pitch: u8,
    pub velocity: u8,
    pub start_tick: u64,
    pub duration_ticks: u64,
}

pub type ClipBufferId = Uuid;

fn random_track_color() -> [u8; 3] {
    let hue = (uuid::Uuid::new_v4().as_u128() % 360) as f32;
    hsv_to_rgb(hue, 0.6, 0.85)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h as u32) / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}
