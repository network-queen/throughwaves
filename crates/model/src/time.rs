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
