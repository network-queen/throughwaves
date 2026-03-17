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
    /// True peak levels (intersample peaks via 4x oversampling), in linear amplitude.
    true_peak: (f32, f32),
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

    /// Set the true peak levels (linear amplitude).
    pub fn set_true_peak(&self, left: f32, right: f32) {
        self.inner.write().true_peak = (left, right);
    }

    /// Read the current true peak levels (linear amplitude).
    pub fn get_true_peak(&self) -> (f32, f32) {
        self.inner.read().true_peak
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
        state.true_peak.0 *= factor;
        state.true_peak.1 *= factor;
    }
}

/// Compute true peak level from interleaved samples using 4x oversampling.
///
/// For each pair of consecutive samples per channel, 3 intermediate points are
/// interpolated using cubic Hermite interpolation. The maximum absolute value
/// across all original and interpolated points is the true peak. This catches
/// intersample peaks that regular sample-peak metering misses.
pub fn true_peak_level(samples: &[f32], channels: usize) -> (f32, f32) {
    if channels == 0 || samples.len() < channels * 2 {
        return peak_level(samples, channels);
    }

    let mut left_peak: f32 = 0.0;
    let mut right_peak: f32 = 0.0;

    let frame_count = samples.len() / channels;
    for ch in 0..channels.min(2) {
        let mut peak = 0.0f32;

        // We need at least 4 frames for proper cubic interpolation;
        // for the first/last frames, use the simpler neighbor approach.
        for i in 0..frame_count {
            let s0 = samples[i * channels + ch];
            peak = peak.max(s0.abs());

            if i + 1 < frame_count {
                let s1 = samples[(i + 1) * channels + ch];
                // Get neighboring samples for cubic interpolation (clamp at boundaries)
                let sm1 = if i > 0 { samples[(i - 1) * channels + ch] } else { s0 };
                let s2 = if i + 2 < frame_count { samples[(i + 2) * channels + ch] } else { s1 };

                // Cubic Hermite interpolation at 3 intermediate points (t = 0.25, 0.5, 0.75)
                for k in 1..=3 {
                    let t = k as f32 * 0.25;
                    let t2 = t * t;
                    let t3 = t2 * t;
                    // Catmull-Rom basis functions
                    let h0 = -0.5 * t3 + t2 - 0.5 * t;
                    let h1 = 1.5 * t3 - 2.5 * t2 + 1.0;
                    let h2 = -1.5 * t3 + 2.0 * t2 + 0.5 * t;
                    let h3 = 0.5 * t3 - 0.5 * t2;
                    let interpolated = h0 * sm1 + h1 * s0 + h2 * s1 + h3 * s2;
                    peak = peak.max(interpolated.abs());
                }
            }
        }

        if ch == 0 {
            left_peak = peak;
        } else {
            right_peak = peak;
        }
    }

    // For mono, mirror left to right
    if channels == 1 {
        right_peak = left_peak;
    }

    (left_peak, right_peak)
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
