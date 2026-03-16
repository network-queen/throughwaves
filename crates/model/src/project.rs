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
    #[serde(default)]
    pub groups: Vec<TrackGroup>,
    #[serde(default)]
    pub tempo_map: crate::time::TempoMap,
    /// Free-text notes/comments saved with the project
    #[serde(default)]
    pub notes: String,
    /// ISO-8601 timestamp when the project was first created
    #[serde(default)]
    pub created_at: String,
    /// Master track effect chain — applied to the summed output before final volume.
    #[serde(default)]
    pub master_effects: Vec<EffectSlot>,
    /// Scenes for session/clip launcher view
    #[serde(default)]
    pub scenes: Vec<Scene>,
    /// MIDI CC learn/mapping table
    #[serde(default)]
    pub midi_mappings: Vec<MidiMapping>,
    /// Macro controls (up to 8 assignable knobs)
    #[serde(default)]
    pub macros: Vec<MacroControl>,
    /// Saved loop regions (named, colored loop areas)
    #[serde(default)]
    pub regions: Vec<Region>,
}

/// A saved loop region on the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub id: Uuid,
    pub name: String,
    pub start: u64,
    pub end: u64,
    pub color: [u8; 3],
}

/// A scene in session view — a row of clip slots across all tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: Uuid,
    pub name: String,
}

/// A clip in session view — lives in a track's slot for a particular scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionClip {
    pub clip_id: Uuid,
    pub name: String,
    pub color: Option<[u8; 3]>,
    /// Source audio/MIDI for this session clip
    pub source: ClipSource,
    /// Duration in samples (for looping)
    pub duration_samples: u64,
}

/// A named position on the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub id: Uuid,
    pub name: String,
    pub sample: u64,
    pub color: [u8; 3],
}

/// A track group/folder that can contain multiple tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackGroup {
    pub id: Uuid,
    pub name: String,
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
            groups: Vec::new(),
            tempo_map: crate::time::TempoMap::default(),
            notes: String::new(),
            created_at: String::new(),
            master_effects: Vec::new(),
            scenes: Vec::new(),
            midi_mappings: Vec::new(),
            macros: default_macros(),
            regions: Vec::new(),
        }
    }
}

fn default_macros() -> Vec<MacroControl> {
    (1..=8)
        .map(|i| MacroControl {
            name: format!("Macro {i}"),
            value: 0.0,
            assignments: Vec::new(),
        })
        .collect()
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
            frozen_buffer_id: None,
            pre_freeze_clips: None,
            pre_freeze_effects: None,
            sidechain_track_id: None,
            input_channel: None,
            output_target: None,
            session_clips: Vec::new(),
            synth_wave: default_synth_wave(),
            synth_attack: default_synth_attack(),
            synth_decay: default_synth_decay(),
            synth_sustain: default_synth_sustain(),
            synth_release: default_synth_release(),
            synth_cutoff: default_synth_cutoff(),
            instrument_plugin: None,
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
    pub effects: Vec<EffectSlot>,
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
    /// ID of the frozen audio buffer (rendered with effects baked in)
    #[serde(default)]
    pub frozen_buffer_id: Option<Uuid>,
    /// Original clips preserved when track is frozen (for unfreeze restore)
    #[serde(default)]
    pub pre_freeze_clips: Option<Vec<Clip>>,
    /// Original effects preserved when track is frozen (for unfreeze restore)
    #[serde(default)]
    pub pre_freeze_effects: Option<Vec<EffectSlot>>,
    /// Sidechain source: if set, the compressor on this track uses the specified
    /// track's pre-effect audio for level detection instead of the local signal.
    #[serde(default)]
    pub sidechain_track_id: Option<Uuid>,
    /// Hardware input channel selection (None = default input).
    #[serde(default)]
    pub input_channel: Option<u16>,
    /// Output routing target (None = master bus). If set, audio is sent to
    /// this track (typically a Bus) instead of the master output.
    #[serde(default)]
    pub output_target: Option<Uuid>,
    /// Session clip slots — one per scene (None = empty slot)
    #[serde(default)]
    pub session_clips: Vec<Option<SessionClip>>,
    /// Built-in synth waveform: "Saw", "Sine", "Square", "Triangle"
    #[serde(default = "default_synth_wave")]
    pub synth_wave: String,
    /// Synth ADSR attack time in milliseconds
    #[serde(default = "default_synth_attack")]
    pub synth_attack: f32,
    /// Synth ADSR decay time in milliseconds
    #[serde(default = "default_synth_decay")]
    pub synth_decay: f32,
    /// Synth ADSR sustain level (0.0 - 1.0)
    #[serde(default = "default_synth_sustain")]
    pub synth_sustain: f32,
    /// Synth ADSR release time in milliseconds
    #[serde(default = "default_synth_release")]
    pub synth_release: f32,
    /// Synth low-pass filter cutoff in Hz
    #[serde(default = "default_synth_cutoff")]
    pub synth_cutoff: f32,
    /// Optional VST3 instrument plugin for MIDI tracks.
    /// When set, MIDI notes are routed to this plugin instead of the built-in synth.
    #[serde(default)]
    pub instrument_plugin: Option<EffectSlot>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutomationParam {
    Volume,
    Pan,
    Mute,
    /// Automate a built-in effect parameter by slot index and parameter name.
    EffectParam {
        slot_index: usize,
        param_name: String,
    },
}

impl AutomationParam {
    pub fn name(&self) -> String {
        match self {
            AutomationParam::Volume => "Volume".to_string(),
            AutomationParam::Pan => "Pan".to_string(),
            AutomationParam::Mute => "Mute".to_string(),
            AutomationParam::EffectParam { slot_index, param_name } => {
                format!("FX{}: {}", slot_index + 1, param_name)
            }
        }
    }

    pub fn default_value(&self) -> f32 {
        match self {
            AutomationParam::Volume => 1.0,
            AutomationParam::Pan => 0.0,
            AutomationParam::Mute => 0.0,
            AutomationParam::EffectParam { param_name, .. } => {
                effect_param_default(param_name)
            }
        }
    }

    pub fn range(&self) -> (f32, f32) {
        match self {
            AutomationParam::Volume => (0.0, 1.5),
            AutomationParam::Pan => (-1.0, 1.0),
            AutomationParam::Mute => (0.0, 1.0),
            AutomationParam::EffectParam { param_name, .. } => {
                effect_param_range(param_name)
            }
        }
    }
}

/// Default values for built-in effect parameters.
fn effect_param_default(param_name: &str) -> f32 {
    match param_name {
        "Gain dB" => 0.0,
        "Cutoff Hz" => 1000.0,
        "Time ms" => 250.0,
        "Feedback" => 0.3,
        "Decay" => 0.5,
        "Mix" => 0.5,
        "Threshold dB" => -10.0,
        "Ratio" => 4.0,
        "Attack ms" => 10.0,
        "Release ms" => 100.0,
        "Freq Hz" => 1000.0,
        "Gain dB (EQ)" => 0.0,
        "Q" => 1.0,
        "Rate Hz" => 1.0,
        "Depth" => 0.5,
        "Drive" => 6.0,
        _ => 0.5,
    }
}

/// Ranges for built-in effect parameters.
fn effect_param_range(param_name: &str) -> (f32, f32) {
    match param_name {
        "Gain dB" => (-24.0, 24.0),
        "Cutoff Hz" => (20.0, 20000.0),
        "Time ms" => (1.0, 2000.0),
        "Feedback" => (0.0, 0.95),
        "Decay" => (0.0, 0.99),
        "Mix" => (0.0, 1.0),
        "Threshold dB" => (-60.0, 0.0),
        "Ratio" => (1.0, 20.0),
        "Attack ms" => (0.1, 200.0),
        "Release ms" => (10.0, 2000.0),
        "Freq Hz" => (20.0, 20000.0),
        "Gain dB (EQ)" => (-24.0, 24.0),
        "Q" => (0.1, 10.0),
        "Rate Hz" => (0.1, 10.0),
        "Depth" => (0.0, 1.0),
        "Drive" => (0.0, 36.0),
        _ => (0.0, 1.0),
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub sample: u64,
    pub value: f32,
}

/// A slot in a track's effect chain — wraps an effect with identity and enable state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectSlot {
    pub id: Uuid,
    pub enabled: bool,
    pub effect: TrackEffect,
}

impl EffectSlot {
    pub fn new(effect: TrackEffect) -> Self {
        Self {
            id: Uuid::new_v4(),
            enabled: true,
            effect,
        }
    }

    pub fn name(&self) -> &str {
        self.effect.name()
    }
}

/// EQ band filter type for parametric EQ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EqBandType {
    LowShelf,
    HighShelf,
    Peak,
    LowPass,
    HighPass,
    Notch,
}

impl Default for EqBandType {
    fn default() -> Self {
        EqBandType::Peak
    }
}

impl EqBandType {
    pub const ALL: [EqBandType; 6] = [
        EqBandType::LowShelf,
        EqBandType::HighShelf,
        EqBandType::Peak,
        EqBandType::LowPass,
        EqBandType::HighPass,
        EqBandType::Notch,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            EqBandType::LowShelf => "Low Shelf",
            EqBandType::HighShelf => "High Shelf",
            EqBandType::Peak => "Peak",
            EqBandType::LowPass => "Low Pass",
            EqBandType::HighPass => "High Pass",
            EqBandType::Notch => "Notch",
        }
    }
}

/// Parameters for a single band in a parametric EQ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBandParams {
    pub freq_hz: f32,
    pub gain_db: f32,
    pub q: f32,
    #[serde(default)]
    pub band_type: EqBandType,
}

impl Default for EqBandParams {
    fn default() -> Self {
        Self {
            freq_hz: 1000.0,
            gain_db: 0.0,
            q: 1.0,
            band_type: EqBandType::Peak,
        }
    }
}

/// Maximum number of bands in a parametric EQ.
pub const MAX_EQ_BANDS: usize = 8;

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
    /// Multi-band parametric EQ with up to 8 bands and various filter types.
    ParametricEq { bands: Vec<EqBandParams> },
    Chorus { rate_hz: f32, depth: f32, mix: f32 },
    Distortion { drive: f32, mix: f32 },
    /// External VST3 plugin — path to the .vst3 bundle
    Vst3Plugin { path: String, name: String },
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
            TrackEffect::ParametricEq { .. } => "Parametric EQ",
            TrackEffect::Chorus { .. } => "Chorus",
            TrackEffect::Distortion { .. } => "Distortion",
            TrackEffect::Vst3Plugin { ref name, .. } => name.as_str(),
        }
    }

    pub fn is_vst(&self) -> bool {
        matches!(self, TrackEffect::Vst3Plugin { .. })
    }

    /// Returns the list of automatable parameter names for this effect type.
    pub fn automatable_params(&self) -> Vec<&'static str> {
        match self {
            TrackEffect::Gain { .. } => vec!["Gain dB"],
            TrackEffect::LowPass { .. } => vec!["Cutoff Hz"],
            TrackEffect::HighPass { .. } => vec!["Cutoff Hz"],
            TrackEffect::Delay { .. } => vec!["Time ms", "Feedback", "Mix"],
            TrackEffect::Reverb { .. } => vec!["Decay", "Mix"],
            TrackEffect::Compressor { .. } => vec!["Threshold dB", "Ratio", "Attack ms", "Release ms"],
            TrackEffect::EqBand { .. } => vec!["Freq Hz", "Gain dB (EQ)", "Q"],
            TrackEffect::ParametricEq { .. } => vec![], // Bands are edited via the EQ visualization
            TrackEffect::Chorus { .. } => vec!["Rate Hz", "Depth", "Mix"],
            TrackEffect::Distortion { .. } => vec!["Drive", "Mix"],
            TrackEffect::Vst3Plugin { .. } => vec![], // VST3 automation deferred
        }
    }

    /// Get the current value of a named parameter.
    pub fn get_param(&self, param_name: &str) -> Option<f32> {
        match (self, param_name) {
            (TrackEffect::Gain { db }, "Gain dB") => Some(*db),
            (TrackEffect::LowPass { cutoff_hz }, "Cutoff Hz") => Some(*cutoff_hz),
            (TrackEffect::HighPass { cutoff_hz }, "Cutoff Hz") => Some(*cutoff_hz),
            (TrackEffect::Delay { time_ms, .. }, "Time ms") => Some(*time_ms),
            (TrackEffect::Delay { feedback, .. }, "Feedback") => Some(*feedback),
            (TrackEffect::Delay { mix, .. }, "Mix") => Some(*mix),
            (TrackEffect::Reverb { decay, .. }, "Decay") => Some(*decay),
            (TrackEffect::Reverb { mix, .. }, "Mix") => Some(*mix),
            (TrackEffect::Compressor { threshold_db, .. }, "Threshold dB") => Some(*threshold_db),
            (TrackEffect::Compressor { ratio, .. }, "Ratio") => Some(*ratio),
            (TrackEffect::Compressor { attack_ms, .. }, "Attack ms") => Some(*attack_ms),
            (TrackEffect::Compressor { release_ms, .. }, "Release ms") => Some(*release_ms),
            (TrackEffect::EqBand { freq_hz, .. }, "Freq Hz") => Some(*freq_hz),
            (TrackEffect::EqBand { gain_db, .. }, "Gain dB (EQ)") => Some(*gain_db),
            (TrackEffect::EqBand { q, .. }, "Q") => Some(*q),
            (TrackEffect::Chorus { rate_hz, .. }, "Rate Hz") => Some(*rate_hz),
            (TrackEffect::Chorus { depth, .. }, "Depth") => Some(*depth),
            (TrackEffect::Chorus { mix, .. }, "Mix") => Some(*mix),
            (TrackEffect::Distortion { drive, .. }, "Drive") => Some(*drive),
            (TrackEffect::Distortion { mix, .. }, "Mix") => Some(*mix),
            _ => None,
        }
    }

    /// Return a copy of this effect with the named parameter overridden.
    pub fn with_param(&self, param_name: &str, value: f32) -> TrackEffect {
        let mut effect = self.clone();
        match (&mut effect, param_name) {
            (TrackEffect::Gain { db }, "Gain dB") => *db = value,
            (TrackEffect::LowPass { cutoff_hz }, "Cutoff Hz") => *cutoff_hz = value,
            (TrackEffect::HighPass { cutoff_hz }, "Cutoff Hz") => *cutoff_hz = value,
            (TrackEffect::Delay { time_ms, .. }, "Time ms") => *time_ms = value,
            (TrackEffect::Delay { feedback, .. }, "Feedback") => *feedback = value,
            (TrackEffect::Delay { mix, .. }, "Mix") => *mix = value,
            (TrackEffect::Reverb { decay, .. }, "Decay") => *decay = value,
            (TrackEffect::Reverb { mix, .. }, "Mix") => *mix = value,
            (TrackEffect::Compressor { threshold_db, .. }, "Threshold dB") => *threshold_db = value,
            (TrackEffect::Compressor { ratio, .. }, "Ratio") => *ratio = value,
            (TrackEffect::Compressor { attack_ms, .. }, "Attack ms") => *attack_ms = value,
            (TrackEffect::Compressor { release_ms, .. }, "Release ms") => *release_ms = value,
            (TrackEffect::EqBand { freq_hz, .. }, "Freq Hz") => *freq_hz = value,
            (TrackEffect::EqBand { gain_db, .. }, "Gain dB (EQ)") => *gain_db = value,
            (TrackEffect::EqBand { q, .. }, "Q") => *q = value,
            (TrackEffect::Chorus { rate_hz, .. }, "Rate Hz") => *rate_hz = value,
            (TrackEffect::Chorus { depth, .. }, "Depth") => *depth = value,
            (TrackEffect::Chorus { mix, .. }, "Mix") => *mix = value,
            (TrackEffect::Distortion { drive, .. }, "Drive") => *drive = value,
            (TrackEffect::Distortion { mix, .. }, "Mix") => *mix = value,
            _ => {}
        }
        effect
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Audio,
    Midi,
    /// Legacy Bus/Aux variant — kept for backward compatibility with saved projects.
    /// Treated identically to Audio everywhere. Any track can receive sends.
    Bus,
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
    /// Fade in length in samples (linear gain ramp from 0 to 1 at clip start)
    #[serde(default)]
    pub fade_in_samples: u64,
    /// Fade out length in samples (linear gain ramp from 1 to 0 at clip end)
    #[serde(default)]
    pub fade_out_samples: u64,
    /// Custom clip color (None = inherit from track)
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Playback rate: 1.0 = normal, 2.0 = double speed, 0.5 = half speed
    #[serde(default = "default_playback_rate")]
    pub playback_rate: f32,
    /// When true, pitch is preserved when changing speed (basic OLA time-stretch)
    #[serde(default)]
    pub preserve_pitch: bool,
    /// Loop count: 1 = play once (no looping), 2 = repeat twice, etc.
    #[serde(default = "default_loop_count")]
    pub loop_count: u32,
    /// Clip gain in dB, applied before track volume. 0.0 = unity (no change).
    #[serde(default)]
    pub gain_db: f32,
    /// Take index: clips with the same time region are different takes.
    /// Higher index = recorded later. 0 = original, 1 = first re-record, etc.
    #[serde(default)]
    pub take_index: u32,
    /// Content offset: how many samples into the source buffer to start reading.
    /// Used for slip editing — the clip boundaries stay fixed but the audio content shifts.
    #[serde(default)]
    pub content_offset: u64,
    /// Pitch transpose in semitones. Adjusts playback rate by 2^(semitones/12).
    /// Default 0 = no transposition. Range: -24 to +24.
    #[serde(default)]
    pub transpose_semitones: i32,
    /// Non-destructive reverse: when true, audio buffer is read backwards during playback.
    #[serde(default)]
    pub reversed: bool,
}

fn default_synth_wave() -> String { "Saw".to_string() }
fn default_synth_attack() -> f32 { 10.0 }
fn default_synth_decay() -> f32 { 100.0 }
fn default_synth_sustain() -> f32 { 0.7 }
fn default_synth_release() -> f32 { 200.0 }
fn default_synth_cutoff() -> f32 { 8000.0 }

fn default_playback_rate() -> f32 {
    1.0
}

fn default_loop_count() -> u32 {
    1
}

impl Clip {
    /// Visual duration in samples, accounting for playback rate and loop count.
    /// A rate of 2.0 means the clip plays twice as fast, so it appears half as long.
    /// A loop_count of 2 means the clip repeats twice, doubling the visual duration.
    pub fn visual_duration_samples(&self) -> u64 {
        if self.playback_rate <= 0.0 {
            return self.duration_samples * self.effective_loop_count() as u64;
        }
        let single = (self.duration_samples as f64 / self.playback_rate as f64) as u64;
        single * self.effective_loop_count() as u64
    }

    /// Effective loop count (at least 1).
    pub fn effective_loop_count(&self) -> u32 {
        self.loop_count.max(1)
    }

    /// Duration of a single loop iteration in visual samples (accounting for rate).
    pub fn single_loop_visual_duration(&self) -> u64 {
        if self.playback_rate <= 0.0 {
            return self.duration_samples;
        }
        (self.duration_samples as f64 / self.playback_rate as f64) as u64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipSource {
    AudioFile { path: String },
    AudioBuffer { buffer_id: Uuid },
    Midi {
        notes: Vec<MidiNote>,
        #[serde(default)]
        cc_events: Vec<MidiCC>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiNote {
    pub pitch: u8,
    pub velocity: u8,
    pub start_tick: u64,
    pub duration_ticks: u64,
}

/// A MIDI Continuous Controller event at a point in time.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MidiCC {
    pub tick: u64,
    pub cc_number: u8,
    pub value: u8,
}

impl Track {
    /// Count how many overlapping takes exist at a given sample position.
    pub fn take_count_at(&self, sample: u64) -> usize {
        self.clips
            .iter()
            .filter(|c| {
                let c_end = c.start_sample + c.visual_duration_samples();
                sample >= c.start_sample && sample < c_end
            })
            .count()
    }

    /// Maximum number of overlapping takes on this track.
    pub fn max_take_count(&self) -> usize {
        if self.clips.is_empty() {
            return 0;
        }
        // Collect all clip boundaries
        let mut events: Vec<(u64, i32)> = Vec::new();
        for c in &self.clips {
            let end = c.start_sample + c.visual_duration_samples();
            events.push((c.start_sample, 1));
            events.push((end, -1));
        }
        events.sort_by_key(|&(pos, delta)| (pos, -delta));
        let mut depth = 0i32;
        let mut max_depth = 0i32;
        for (_, delta) in events {
            depth += delta;
            max_depth = max_depth.max(depth);
        }
        max_depth as usize
    }

    /// Returns true if this track has any overlapping clips (takes).
    pub fn has_takes(&self) -> bool {
        self.max_take_count() > 1
    }
}

pub type ClipBufferId = Uuid;

/// Predefined palette of 12 evenly-spaced hues for new tracks.
/// Each call cycles through the palette using a global counter so
/// consecutive tracks always get distinct, predictable colors.
fn random_track_color() -> [u8; 3] {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static PALETTE_INDEX: AtomicUsize = AtomicUsize::new(0);

    const PALETTE_HUES: [f32; 12] = [
        0.0, 30.0, 60.0, 90.0, 120.0, 150.0,
        180.0, 210.0, 240.0, 270.0, 300.0, 330.0,
    ];

    let idx = PALETTE_INDEX.fetch_add(1, Ordering::Relaxed) % PALETTE_HUES.len();
    hsv_to_rgb(PALETTE_HUES[idx], 0.6, 0.85)
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

// ── MIDI Mapping & Macro Types ───────────────────────────────────────

/// Target for a MIDI CC mapping or macro assignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MidiMappingTarget {
    /// Track volume fader (track index)
    TrackVolume(usize),
    /// Track pan knob (track index)
    TrackPan(usize),
    /// Built-in effect parameter
    EffectParam {
        track_idx: usize,
        slot_idx: usize,
        param_name: String,
    },
    /// Master volume fader
    MasterVolume,
}

impl MidiMappingTarget {
    /// Human-readable label for this target.
    pub fn label(&self, tracks: &[Track]) -> String {
        match self {
            MidiMappingTarget::TrackVolume(i) => {
                let name = tracks.get(*i).map(|t| t.name.as_str()).unwrap_or("?");
                format!("{name} — Volume")
            }
            MidiMappingTarget::TrackPan(i) => {
                let name = tracks.get(*i).map(|t| t.name.as_str()).unwrap_or("?");
                format!("{name} — Pan")
            }
            MidiMappingTarget::EffectParam { track_idx, slot_idx, param_name } => {
                let tname = tracks.get(*track_idx).map(|t| t.name.as_str()).unwrap_or("?");
                let ename = tracks.get(*track_idx)
                    .and_then(|t| t.effects.get(*slot_idx))
                    .map(|s| s.name())
                    .unwrap_or("?");
                format!("{tname} > {ename} > {param_name}")
            }
            MidiMappingTarget::MasterVolume => "Master Volume".to_string(),
        }
    }

    /// Get the current value of this target from a project + master_volume.
    pub fn get_value(&self, project: &Project, master_volume: f32) -> f32 {
        match self {
            MidiMappingTarget::TrackVolume(i) => {
                project.tracks.get(*i).map(|t| t.volume).unwrap_or(1.0)
            }
            MidiMappingTarget::TrackPan(i) => {
                project.tracks.get(*i).map(|t| t.pan).unwrap_or(0.0)
            }
            MidiMappingTarget::EffectParam { track_idx, slot_idx, param_name } => {
                project.tracks.get(*track_idx)
                    .and_then(|t| t.effects.get(*slot_idx))
                    .and_then(|s| s.effect.get_param(param_name))
                    .unwrap_or(0.0)
            }
            MidiMappingTarget::MasterVolume => master_volume,
        }
    }

    /// The natural (min, max) range for this target.
    pub fn range(&self, project: &Project) -> (f32, f32) {
        match self {
            MidiMappingTarget::TrackVolume(_) => (0.0, 1.5),
            MidiMappingTarget::TrackPan(_) => (-1.0, 1.0),
            MidiMappingTarget::EffectParam { track_idx, slot_idx, param_name } => {
                project.tracks.get(*track_idx)
                    .and_then(|t| t.effects.get(*slot_idx))
                    .map(|_| effect_param_range(param_name))
                    .unwrap_or((0.0, 1.0))
            }
            MidiMappingTarget::MasterVolume => (0.0, 1.5),
        }
    }
}

/// A single MIDI CC-to-parameter mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiMapping {
    pub cc_number: u8,
    pub channel: u8,
    pub target: MidiMappingTarget,
}

/// A macro control knob with multiple parameter assignments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroControl {
    pub name: String,
    /// Current value 0.0-1.0
    pub value: f32,
    pub assignments: Vec<MacroAssignment>,
}

/// One assignment from a macro to a parameter target with independent range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroAssignment {
    pub target: MidiMappingTarget,
    /// Value when macro is at 0.0
    pub min_value: f32,
    /// Value when macro is at 1.0
    pub max_value: f32,
}

/// Apply a MIDI CC value (0-127) to a mapping target.
/// Returns true if a value was changed.
pub fn apply_midi_cc_to_target(
    target: &MidiMappingTarget,
    cc_value: u8,
    project: &mut Project,
    master_volume: &mut f32,
) -> bool {
    let (min, max) = target.range(project);
    let normalized = cc_value as f32 / 127.0;
    let value = min + normalized * (max - min);

    match target {
        MidiMappingTarget::TrackVolume(i) => {
            if let Some(t) = project.tracks.get_mut(*i) {
                t.volume = value;
                return true;
            }
        }
        MidiMappingTarget::TrackPan(i) => {
            if let Some(t) = project.tracks.get_mut(*i) {
                t.pan = value;
                return true;
            }
        }
        MidiMappingTarget::EffectParam { track_idx, slot_idx, param_name } => {
            if let Some(track) = project.tracks.get_mut(*track_idx) {
                if let Some(slot) = track.effects.get_mut(*slot_idx) {
                    slot.effect = slot.effect.with_param(param_name, value);
                    return true;
                }
            }
        }
        MidiMappingTarget::MasterVolume => {
            *master_volume = value;
            return true;
        }
    }
    false
}

/// Apply a macro's current value to all its assignments.
/// Returns true if any parameter was changed.
pub fn apply_macro_value(
    macro_ctrl: &MacroControl,
    project: &mut Project,
    master_volume: &mut f32,
) -> bool {
    let t = macro_ctrl.value.clamp(0.0, 1.0);
    let mut changed = false;

    for assign in &macro_ctrl.assignments {
        let value = assign.min_value + t * (assign.max_value - assign.min_value);
        match &assign.target {
            MidiMappingTarget::TrackVolume(i) => {
                if let Some(tr) = project.tracks.get_mut(*i) {
                    tr.volume = value;
                    changed = true;
                }
            }
            MidiMappingTarget::TrackPan(i) => {
                if let Some(tr) = project.tracks.get_mut(*i) {
                    tr.pan = value;
                    changed = true;
                }
            }
            MidiMappingTarget::EffectParam { track_idx, slot_idx, param_name } => {
                if let Some(track) = project.tracks.get_mut(*track_idx) {
                    if let Some(slot) = track.effects.get_mut(*slot_idx) {
                        slot.effect = slot.effect.with_param(param_name, value);
                        changed = true;
                    }
                }
            }
            MidiMappingTarget::MasterVolume => {
                *master_volume = value;
                changed = true;
            }
        }
    }
    changed
}
