use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;

/// EBU R128 LUFS loudness metering.
///
/// Implements K-weighting (high shelf +4dB at 1681Hz, high-pass at 38Hz)
/// followed by mean-square integration over configurable windows.
///
/// Provides:
/// - Momentary loudness (400ms sliding window)
/// - Short-term loudness (3s sliding window)
/// - Integrated loudness (gated, full session)

/// Shared LUFS readings accessible from both the engine thread and the UI.
#[derive(Clone)]
pub struct LufsMeter {
    inner: Arc<RwLock<LufsReadings>>,
}

#[derive(Debug, Clone, Copy)]
pub struct LufsReadings {
    pub momentary: f64,
    pub short_term: f64,
    pub integrated: f64,
    /// True when the master output has clipped (peak > 1.0) since last reset.
    pub clipping: bool,
}

impl Default for LufsReadings {
    fn default() -> Self {
        Self {
            momentary: -f64::INFINITY,
            short_term: -f64::INFINITY,
            integrated: -f64::INFINITY,
            clipping: false,
        }
    }
}

impl LufsMeter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(LufsReadings::default())),
        }
    }

    pub fn read(&self) -> LufsReadings {
        *self.inner.read()
    }

    pub fn write(&self, readings: LufsReadings) {
        *self.inner.write() = readings;
    }

    pub fn reset_integrated(&self) {
        let mut w = self.inner.write();
        w.integrated = -f64::INFINITY;
        w.clipping = false;
    }
}

/// Second-order biquad filter (Direct Form I).
#[derive(Clone)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    // State per channel
    x1: [f64; 2],
    x2: [f64; 2],
    y1: [f64; 2],
    y2: [f64; 2],
}

impl Biquad {
    fn new(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            x1: [0.0; 2],
            x2: [0.0; 2],
            y1: [0.0; 2],
            y2: [0.0; 2],
        }
    }

    fn process_sample(&mut self, x: f64, ch: usize) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1[ch] + self.b2 * self.x2[ch]
            - self.a1 * self.y1[ch]
            - self.a2 * self.y2[ch];
        self.x2[ch] = self.x1[ch];
        self.x1[ch] = x;
        self.y2[ch] = self.y1[ch];
        self.y1[ch] = y;
        y
    }

    fn reset(&mut self) {
        self.x1 = [0.0; 2];
        self.x2 = [0.0; 2];
        self.y1 = [0.0; 2];
        self.y2 = [0.0; 2];
    }
}

/// K-weighting pre-filter as specified by ITU-R BS.1770.
/// Stage 1: High shelf boosting ~4dB above 1681 Hz.
/// Stage 2: High-pass at ~38 Hz (removes DC and sub-bass).
fn k_weight_filters(sample_rate: u32) -> (Biquad, Biquad) {
    let fs = sample_rate as f64;

    // ---- Stage 1: High shelf ----
    // Pre-computed coefficients for common sample rates from the ITU spec.
    // For other rates we use the bilinear transform of the analog prototype.
    let stage1 = if sample_rate == 48000 {
        Biquad::new(
            1.53512485958697,
            -2.69169618940638,
            1.19839281085285,
            -1.69065929318241,
            0.73248077421585,
        )
    } else if sample_rate == 44100 {
        Biquad::new(
            1.53090959966702,
            -2.65091438671584,
            1.16905863214690,
            -1.66363794899498,
            0.71230833614706,
        )
    } else {
        // Generic bilinear transform approximation for the high-shelf filter.
        // Analog prototype: +4dB shelf above 1681Hz, Q=0.7071
        let f0 = 1681.974450955533;
        let g = 3.999843853973347_f64; // dB
        let q = 0.7071752369554196;

        let k = (std::f64::consts::PI * f0 / fs).tan();
        let k2 = k * k;
        let v0 = 10.0_f64.powf(g / 20.0);
        let vb = v0.powf(0.4996667741545416);

        let a0 = 1.0 + k / (q * vb) + k2;
        Biquad::new(
            (v0 + vb * k / q + k2) / a0,
            2.0 * (k2 - v0) / a0,
            (v0 - vb * k / q + k2) / a0,
            2.0 * (k2 - 1.0) / a0,
            (1.0 - k / (q * vb) + k2) / a0,
        )
    };

    // ---- Stage 2: High-pass at 38 Hz ----
    let stage2 = if sample_rate == 48000 {
        Biquad::new(
            1.0,
            -2.0,
            1.0,
            -1.99004745483398,
            0.99007225036621,
        )
    } else if sample_rate == 44100 {
        Biquad::new(
            1.0,
            -2.0,
            1.0,
            -1.98916108229670,
            0.98919186756478,
        )
    } else {
        // Generic second-order Butterworth high-pass at 38Hz.
        let f0 = 38.13547087602444;
        let q = 0.5003270373238773;
        let w0 = 2.0 * std::f64::consts::PI * f0 / fs;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();
        let a0_inv = 1.0 / (1.0 + alpha);
        Biquad::new(
            (1.0 + cos_w0) / 2.0 * a0_inv,
            -(1.0 + cos_w0) * a0_inv,
            (1.0 + cos_w0) / 2.0 * a0_inv,
            -2.0 * cos_w0 * a0_inv,
            (1.0 - alpha) * a0_inv,
        )
    };

    (stage1, stage2)
}

/// LUFS calculator running on the engine thread.
pub struct LufsCalculator {
    #[allow(dead_code)]
    sample_rate: u32,
    channels: usize,
    stage1: Biquad,
    stage2: Biquad,
    /// Ring buffer of mean-square values per 100ms block (EBU gating block).
    /// Each entry is the channel-summed mean square for a 100ms segment.
    block_ring: VecDeque<f64>,
    /// Accumulator for the current 100ms block.
    block_accum: f64,
    /// Number of samples accumulated in the current 100ms block.
    block_count: usize,
    /// Number of samples in a 100ms block.
    block_size_100ms: usize,
    /// All 100ms block values for integrated loudness (gated).
    integrated_blocks: Vec<f64>,
    /// Whether clipping has been detected.
    clipping: bool,
}

impl LufsCalculator {
    pub fn new(sample_rate: u32, channels: usize) -> Self {
        let (stage1, stage2) = k_weight_filters(sample_rate);
        let block_size_100ms = (sample_rate as usize) / 10; // 100ms worth of frames
        Self {
            sample_rate,
            channels,
            stage1,
            stage2,
            // Keep enough 100ms blocks for 3 seconds (short-term window)
            block_ring: VecDeque::with_capacity(32),
            block_accum: 0.0,
            block_count: 0,
            block_size_100ms,
            integrated_blocks: Vec::new(),
            clipping: false,
        }
    }

    /// Feed interleaved audio samples (post master volume) and return updated readings.
    pub fn process(&mut self, samples: &[f32]) -> LufsReadings {
        let ch = self.channels;
        if ch == 0 {
            return LufsReadings::default();
        }

        for frame in samples.chunks(ch) {
            // Check for clipping
            for &s in frame.iter() {
                if s.abs() > 1.0 {
                    self.clipping = true;
                }
            }

            // K-weight each channel and accumulate mean square
            let mut frame_ms = 0.0f64;
            for (c, &s) in frame.iter().enumerate().take(2) {
                let x = s as f64;
                let y1 = self.stage1.process_sample(x, c);
                let y2 = self.stage2.process_sample(y1, c);
                frame_ms += y2 * y2;
            }
            // If mono, count the single channel once (ITU spec weights front channels as 1.0).
            self.block_accum += frame_ms;
            self.block_count += 1;

            if self.block_count >= self.block_size_100ms {
                // Finalize this 100ms block
                let ms = self.block_accum / self.block_count as f64;
                self.block_ring.push_back(ms);
                // Keep at most 30 blocks (3 seconds)
                if self.block_ring.len() > 30 {
                    self.block_ring.pop_front();
                }
                self.integrated_blocks.push(ms);
                self.block_accum = 0.0;
                self.block_count = 0;
            }
        }

        // Compute momentary (400ms = 4 blocks)
        let momentary = self.window_lufs(4);
        // Compute short-term (3s = 30 blocks)
        let short_term = self.window_lufs(30);
        // Compute integrated (gated)
        let integrated = self.integrated_lufs();

        LufsReadings {
            momentary,
            short_term,
            integrated,
            clipping: self.clipping,
        }
    }

    fn window_lufs(&self, num_blocks: usize) -> f64 {
        let len = self.block_ring.len();
        if len == 0 {
            return -f64::INFINITY;
        }
        let start = if len > num_blocks { len - num_blocks } else { 0 };
        let mut sum = 0.0;
        let mut count = 0usize;
        for i in start..len {
            sum += self.block_ring[i];
            count += 1;
        }
        if count == 0 {
            return -f64::INFINITY;
        }
        let mean = sum / count as f64;
        if mean <= 0.0 {
            return -f64::INFINITY;
        }
        -0.691 + 10.0 * mean.log10()
    }

    /// EBU R128 integrated loudness with absolute gating at -70 LUFS
    /// and relative gating at -10 LU below the ungated value.
    fn integrated_lufs(&self) -> f64 {
        if self.integrated_blocks.is_empty() {
            return -f64::INFINITY;
        }
        // Absolute gate: -70 LUFS => mean_square threshold
        let abs_gate_ms = 10.0_f64.powf((-70.0 + 0.691) / 10.0);

        // First pass: compute mean of blocks above absolute gate
        let mut sum = 0.0;
        let mut count = 0usize;
        for &ms in &self.integrated_blocks {
            if ms > abs_gate_ms {
                sum += ms;
                count += 1;
            }
        }
        if count == 0 {
            return -f64::INFINITY;
        }
        let ungated_mean = sum / count as f64;

        // Relative gate: -10 LU below ungated loudness
        let rel_gate_ms = ungated_mean * 10.0_f64.powf(-10.0 / 10.0);

        // Second pass: compute mean of blocks above both gates
        sum = 0.0;
        count = 0;
        for &ms in &self.integrated_blocks {
            if ms > abs_gate_ms && ms > rel_gate_ms {
                sum += ms;
                count += 1;
            }
        }
        if count == 0 {
            return -f64::INFINITY;
        }
        let gated_mean = sum / count as f64;
        if gated_mean <= 0.0 {
            return -f64::INFINITY;
        }
        -0.691 + 10.0 * gated_mean.log10()
    }

    /// Reset integrated measurement and clipping flag.
    pub fn reset(&mut self) {
        self.integrated_blocks.clear();
        self.clipping = false;
        self.stage1.reset();
        self.stage2.reset();
        self.block_ring.clear();
        self.block_accum = 0.0;
        self.block_count = 0;
    }
}
