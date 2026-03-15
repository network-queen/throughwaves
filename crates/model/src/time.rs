use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Tempo {
    pub bpm: f64,
}

impl Default for Tempo {
    fn default() -> Self {
        Self { bpm: 120.0 }
    }
}

impl Tempo {
    pub fn samples_per_beat(&self, sample_rate: f64) -> f64 {
        60.0 / self.bpm * sample_rate
    }

    pub fn beat_at_sample(&self, sample: u64, sample_rate: f64) -> f64 {
        sample as f64 / self.samples_per_beat(sample_rate)
    }

    pub fn sample_at_beat(&self, beat: f64, sample_rate: f64) -> u64 {
        (beat * self.samples_per_beat(sample_rate)) as u64
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportState {
    Stopped,
    Playing,
    Recording,
}

impl Default for TransportState {
    fn default() -> Self {
        Self::Stopped
    }
}

/// A tempo change point in the tempo map.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TempoChange {
    pub sample: u64,
    pub bpm: f64,
}

/// Tempo map — supports tempo changes over time.
/// If empty, uses the project's global tempo.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TempoMap {
    pub changes: Vec<TempoChange>,
}

impl TempoMap {
    /// Get the tempo (BPM) at a given sample position.
    pub fn bpm_at(&self, sample: u64, default_bpm: f64) -> f64 {
        if self.changes.is_empty() {
            return default_bpm;
        }

        let mut bpm = default_bpm;
        for change in &self.changes {
            if change.sample <= sample {
                bpm = change.bpm;
            } else {
                break;
            }
        }
        bpm
    }

    /// Convert a sample position to beat number, accounting for tempo changes.
    pub fn beat_at_sample(&self, sample: u64, sample_rate: f64, default_bpm: f64) -> f64 {
        if self.changes.is_empty() {
            let spb = 60.0 / default_bpm * sample_rate;
            return sample as f64 / spb;
        }

        let mut beats = 0.0;
        let mut prev_sample = 0u64;
        let mut current_bpm = default_bpm;

        for change in &self.changes {
            if change.sample >= sample {
                break;
            }
            // Count beats from prev_sample to change.sample at current_bpm
            let span = change.sample - prev_sample;
            let spb = 60.0 / current_bpm * sample_rate;
            beats += span as f64 / spb;
            prev_sample = change.sample;
            current_bpm = change.bpm;
        }

        // Count remaining beats from last change to sample
        let span = sample - prev_sample;
        let spb = 60.0 / current_bpm * sample_rate;
        beats += span as f64 / spb;

        beats
    }

    /// Convert a beat number to sample position, accounting for tempo changes.
    pub fn sample_at_beat(&self, beat: f64, sample_rate: f64, default_bpm: f64) -> u64 {
        if self.changes.is_empty() {
            let spb = 60.0 / default_bpm * sample_rate;
            return (beat * spb) as u64;
        }

        let mut remaining_beats = beat;
        let _sample_pos = 0u64;
        let mut prev_sample = 0u64;
        let mut current_bpm = default_bpm;

        for change in &self.changes {
            let spb = 60.0 / current_bpm * sample_rate;
            let span = change.sample - prev_sample;
            let beats_in_span = span as f64 / spb;

            if remaining_beats <= beats_in_span {
                return prev_sample + (remaining_beats * spb) as u64;
            }

            remaining_beats -= beats_in_span;
            prev_sample = change.sample;
            current_bpm = change.bpm;
        }

        // Past last change
        let spb = 60.0 / current_bpm * sample_rate;
        prev_sample + (remaining_beats * spb) as u64
    }

    pub fn add_change(&mut self, sample: u64, bpm: f64) {
        // Remove existing change at same position
        self.changes.retain(|c| c.sample != sample);
        self.changes.push(TempoChange { sample, bpm });
        self.changes.sort_by_key(|c| c.sample);
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}
