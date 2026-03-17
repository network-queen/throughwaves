use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use uuid::Uuid;

/// Precomputed waveform peaks for rendering in the UI.
#[derive(Clone)]
pub struct WaveformPeaks {
    /// Min/max pairs at various resolutions.
    /// Level 0 = 1 sample per peak, Level 1 = 2 samples per peak, etc.
    /// We store levels at powers of 2: 256, 512, 1024, 2048, 4096 samples per peak.
    pub levels: Vec<Vec<(f32, f32)>>,
    /// RMS values at the same resolutions as `levels`.
    pub rms_levels: Vec<Vec<f32>>,
    pub total_samples: usize,
}

impl WaveformPeaks {
    pub fn from_samples(samples: &[f32]) -> Self {
        let mut levels = Vec::new();
        let mut rms_levels = Vec::new();

        // Build mip-map levels: 256, 512, 1024, 2048, 4096 samples per peak
        for &block_size in &[256, 512, 1024, 2048, 4096] {
            let num_peaks = (samples.len() + block_size - 1) / block_size;
            let mut peaks = Vec::with_capacity(num_peaks);
            let mut rms_vals = Vec::with_capacity(num_peaks);

            for chunk in samples.chunks(block_size) {
                let mut min = f32::MAX;
                let mut max = f32::MIN;
                let mut sum_sq = 0.0_f64;
                for &s in chunk {
                    if s < min {
                        min = s;
                    }
                    if s > max {
                        max = s;
                    }
                    sum_sq += (s as f64) * (s as f64);
                }
                peaks.push((min, max));
                rms_vals.push((sum_sq / chunk.len() as f64).sqrt() as f32);
            }
            levels.push(peaks);
            rms_levels.push(rms_vals);
        }

        Self {
            levels,
            rms_levels,
            total_samples: samples.len(),
        }
    }

    /// Get the best mip-map level for the given samples-per-pixel ratio.
    pub fn get_peaks_for_resolution(&self, samples_per_pixel: f64) -> &[(f32, f32)] {
        let block_sizes = [256, 512, 1024, 2048, 4096];
        let mut best = 0;
        for (i, &bs) in block_sizes.iter().enumerate() {
            if (bs as f64) <= samples_per_pixel * 2.0 {
                best = i;
            }
        }
        &self.levels[best]
    }

    /// Get the RMS values for the best mip-map level.
    pub fn get_rms_for_resolution(&self, samples_per_pixel: f64) -> &[f32] {
        let block_sizes = [256, 512, 1024, 2048, 4096];
        let mut best = 0;
        for (i, &bs) in block_sizes.iter().enumerate() {
            if (bs as f64) <= samples_per_pixel * 2.0 {
                best = i;
            }
        }
        &self.rms_levels[best]
    }

    pub fn block_size_for_level(&self, samples_per_pixel: f64) -> usize {
        let block_sizes = [256, 512, 1024, 2048, 4096];
        let mut best = 0;
        for (i, &bs) in block_sizes.iter().enumerate() {
            if (bs as f64) <= samples_per_pixel * 2.0 {
                best = i;
            }
        }
        block_sizes[best]
    }
}

/// Maximum number of waveform cache entries. Prevents unbounded memory growth
/// when many clips are imported/recorded across project sessions without restart.
const MAX_WAVEFORM_CACHE_ENTRIES: usize = 512;

/// Shared cache of waveform peaks, keyed by buffer ID.
#[derive(Clone)]
pub struct WaveformCache {
    inner: Arc<RwLock<HashMap<Uuid, WaveformPeaks>>>,
}

impl WaveformCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn insert(&self, id: Uuid, samples: &[f32]) {
        let peaks = WaveformPeaks::from_samples(samples);
        let mut cache = self.inner.write();
        cache.insert(id, peaks);
        // Evict oldest entries if cache exceeds limit (simple size cap).
        // In practice this is rarely hit since clips are removed via remove(),
        // but it guards against slow leaks.
        if cache.len() > MAX_WAVEFORM_CACHE_ENTRIES {
            // Remove arbitrary entries to get back under limit.
            // HashMap iteration order is arbitrary, which is acceptable here.
            let excess = cache.len() - MAX_WAVEFORM_CACHE_ENTRIES;
            let keys_to_remove: Vec<Uuid> = cache.keys().take(excess).copied().collect();
            for key in keys_to_remove {
                cache.remove(&key);
            }
        }
    }

    pub fn get(&self, id: &Uuid) -> Option<WaveformPeaks> {
        self.inner.read().get(id).cloned()
    }

    pub fn remove(&self, id: Uuid) {
        self.inner.write().remove(&id);
    }

    /// Clear all cached waveform data. Called when creating a new project
    /// to ensure stale data from the previous project is freed.
    pub fn clear(&self) {
        self.inner.write().clear();
    }

    /// Number of entries currently in the cache.
    pub fn len(&self) -> usize {
        self.inner.read().len()
    }
}
