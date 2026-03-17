use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use uuid::Uuid;

/// Real-time audio level meters.
#[derive(Clone)]
pub struct LevelMeters {
    inner: Arc<RwLock<LevelState>>,
}

#[derive(Default)]
struct LevelState {
    track_levels: HashMap<Uuid, (f32, f32)>, // (left_peak, right_peak)
    master_level: (f32, f32),
    /// Stereo correlation: -1.0 (out of phase) to +1.0 (mono/correlated).
    correlation: f32,
}

impl LevelMeters {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(LevelState::default())),
        }
    }

    pub fn set_track_level(&self, track_id: Uuid, left: f32, right: f32) {
        self.inner.write().track_levels.insert(track_id, (left, right));
    }

    pub fn set_master_level(&self, left: f32, right: f32) {
        self.inner.write().master_level = (left, right);
    }

    /// Update the stereo phase correlation value.
    pub fn set_correlation(&self, value: f32) {
        self.inner.write().correlation = value;
    }

    /// Read the current stereo phase correlation.
    pub fn get_correlation(&self) -> f32 {
        self.inner.read().correlation
    }

    pub fn get_track_level(&self, track_id: &Uuid) -> (f32, f32) {
        self.inner
            .read()
            .track_levels
            .get(track_id)
            .copied()
            .unwrap_or((0.0, 0.0))
    }

    pub fn get_master_level(&self) -> (f32, f32) {
        self.inner.read().master_level
    }

    /// Decay all levels toward zero (call once per UI frame for smooth meters).
    pub fn decay(&self, factor: f32) {
        let mut state = self.inner.write();
        for (_, (l, r)) in state.track_levels.iter_mut() {
            *l *= factor;
            *r *= factor;
        }
        state.master_level.0 *= factor;
        state.master_level.1 *= factor;
    }
}

/// Compute peak level from interleaved samples.
pub fn peak_level(samples: &[f32], channels: usize) -> (f32, f32) {
    let mut left_peak: f32 = 0.0;
    let mut right_peak: f32 = 0.0;

    for frame in samples.chunks(channels) {
        if let Some(&l) = frame.first() {
            left_peak = left_peak.max(l.abs());
        }
        if channels > 1 {
            if let Some(&r) = frame.get(1) {
                right_peak = right_peak.max(r.abs());
            }
        } else {
            right_peak = left_peak;
        }
    }

    (left_peak, right_peak)
}
