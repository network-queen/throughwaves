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
    #[serde(default)]
    pub markers: Vec<Marker>,
}

/// A named position on the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub id: Uuid,
    pub name: String,
    pub sample: u64,
    pub color: [u8; 3],
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: "Untitled Session".into(),
            tempo: Tempo::default(),
            time_signature: TimeSignature::default(),
            sample_rate: 44100,
            tracks: Vec::new(),
            markers: Vec::new(),
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
            effects: Vec::new(),
            lanes_expanded: false,
            custom_height: 0.0,
            automation: Vec::new(),
            sends: Vec::new(),
            group_id: None,
            frozen: false,
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
    #[serde(default)]
    pub effects: Vec<TrackEffect>,
    /// Whether take lanes are expanded (showing all takes) or collapsed (showing only active)
    #[serde(default)]
    pub lanes_expanded: bool,
    /// Custom track height set by user dragging the bottom edge (0.0 = auto)
    #[serde(default)]
    pub custom_height: f32,
    /// Automation lanes for this track
    #[serde(default)]
    pub automation: Vec<AutomationLane>,
    /// Send routing: send this track's audio to other tracks (for submixes/buses)
    #[serde(default)]
    pub sends: Vec<TrackSend>,
    /// Group/folder ID — tracks with the same group_id belong together
    #[serde(default)]
    pub group_id: Option<Uuid>,
    /// Frozen track — effects baked, original preserved, CPU saved
    #[serde(default)]
    pub frozen: bool,
}

/// A send routes audio from this track to another track at a given level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackSend {
    pub target_track_id: Uuid,
    pub level: f32, // 0.0 to 1.0
    pub pre_fader: bool,
}

/// An automation lane controls a parameter over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationLane {
    pub parameter: AutomationParam,
    pub points: Vec<AutomationPoint>,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutomationParam {
    Volume,
    Pan,
    Mute,
}

impl AutomationParam {
    pub fn name(&self) -> &str {
        match self {
            AutomationParam::Volume => "Volume",
            AutomationParam::Pan => "Pan",
            AutomationParam::Mute => "Mute",
        }
    }

    pub fn default_value(&self) -> f32 {
        match self {
            AutomationParam::Volume => 1.0,
            AutomationParam::Pan => 0.0,
            AutomationParam::Mute => 0.0,
        }
    }

    pub fn range(&self) -> (f32, f32) {
        match self {
            AutomationParam::Volume => (0.0, 1.5),
            AutomationParam::Pan => (-1.0, 1.0),
            AutomationParam::Mute => (0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub sample: u64,
    pub value: f32,
}

/// Effect types that can be applied to a track.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TrackEffect {
    Gain { db: f32 },
    LowPass { cutoff_hz: f32 },
    HighPass { cutoff_hz: f32 },
    Delay { time_ms: f32, feedback: f32, mix: f32 },
    Reverb { decay: f32, mix: f32 },
    Compressor { threshold_db: f32, ratio: f32, attack_ms: f32, release_ms: f32 },
    EqBand { freq_hz: f32, gain_db: f32, q: f32 },
    Chorus { rate_hz: f32, depth: f32, mix: f32 },
    Distortion { drive: f32, mix: f32 },
}

impl TrackEffect {
    pub fn name(&self) -> &str {
        match self {
            TrackEffect::Gain { .. } => "Gain",
            TrackEffect::LowPass { .. } => "Low Pass",
            TrackEffect::HighPass { .. } => "High Pass",
            TrackEffect::Delay { .. } => "Delay",
            TrackEffect::Reverb { .. } => "Reverb",
            TrackEffect::Compressor { .. } => "Compressor",
            TrackEffect::EqBand { .. } => "EQ Band",
            TrackEffect::Chorus { .. } => "Chorus",
            TrackEffect::Distortion { .. } => "Distortion",
        }
    }
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
    /// Muted clips are visible but don't play (used for takes management)
    #[serde(default)]
    pub muted: bool,
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
