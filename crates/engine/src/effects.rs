use jamhub_model::{EqBandParams, EqBandType, TrackEffect, MAX_EQ_BANDS};

/// Biquad filter state for a single second-order section.
#[derive(Clone)]
struct BiquadState {
    x: [f32; 2], // input history
    y: [f32; 2], // output history
}

impl Default for BiquadState {
    fn default() -> Self {
        Self {
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }
}

/// Biquad filter coefficients (normalized by a0).
#[derive(Clone, Copy)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoeffs {
    /// Compute biquad coefficients for a given band type, frequency, gain, Q, and sample rate.
    pub fn from_band(band: &EqBandParams, sample_rate: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * band.freq_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * band.q.max(0.1));
        let a = 10.0_f32.powf(band.gain_db / 40.0);

        let (b0, b1, b2, a0, a1, a2) = match band.band_type {
            EqBandType::Peak => {
                let b0 = 1.0 + alpha * a;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0 - alpha * a;
                let a0 = 1.0 + alpha / a;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha / a;
                (b0, b1, b2, a0, a1, a2)
            }
            EqBandType::LowShelf => {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
                let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
                let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
                let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
                let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
                let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            EqBandType::HighShelf => {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
                let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
                let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
                let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
                let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
                let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            EqBandType::LowPass => {
                let b0 = (1.0 - cos_w0) / 2.0;
                let b1 = 1.0 - cos_w0;
                let b2 = (1.0 - cos_w0) / 2.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            EqBandType::HighPass => {
                let b0 = (1.0 + cos_w0) / 2.0;
                let b1 = -(1.0 + cos_w0);
                let b2 = (1.0 + cos_w0) / 2.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            EqBandType::Notch => {
                let b0 = 1.0;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
        };

        // Normalize
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Evaluate the frequency response magnitude (in dB) at a given frequency.
    pub fn magnitude_db(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        let w = 2.0 * std::f32::consts::PI * freq_hz / sample_rate;
        let cos_w = w.cos();
        let cos_2w = (2.0 * w).cos();
        let sin_w = w.sin();
        let sin_2w = (2.0 * w).sin();

        let num_re = self.b0 + self.b1 * cos_w + self.b2 * cos_2w;
        let num_im = -(self.b1 * sin_w + self.b2 * sin_2w);
        let den_re = 1.0 + self.a1 * cos_w + self.a2 * cos_2w;
        let den_im = -(self.a1 * sin_w + self.a2 * sin_2w);

        let num_mag_sq = num_re * num_re + num_im * num_im;
        let den_mag_sq = den_re * den_re + den_im * den_im;

        if den_mag_sq < 1e-20 {
            return 0.0;
        }

        10.0 * (num_mag_sq / den_mag_sq).log10()
    }
}

/// Processes audio through an effect chain.
pub struct EffectProcessor {
    lp_state: [f32; 2],
    hp_state: [f32; 2],
    delay_buffer: Vec<f32>,
    delay_write_pos: usize,
    reverb_buffer: Vec<Vec<f32>>,
    reverb_pos: Vec<usize>,
    // Compressor state
    comp_envelope: f32,
    // EQ state (biquad) for legacy single-band EqBand
    eq_x: [f32; 2],
    eq_y: [f32; 2],
    // Chorus state
    chorus_buffer: Vec<f32>,
    chorus_write_pos: usize,
    chorus_phase: f32,
    // Parametric EQ state: up to MAX_EQ_BANDS cascaded biquad filters
    peq_states: Vec<BiquadState>,
}

impl EffectProcessor {
    pub fn new(sample_rate: u32) -> Self {
        let max_delay_samples = (sample_rate as f32 * 2.0) as usize;
        let reverb_times = [0.029, 0.037, 0.041, 0.053, 0.067, 0.073];
        let reverb_buffer: Vec<Vec<f32>> = reverb_times
            .iter()
            .map(|t| vec![0.0; (t * sample_rate as f64) as usize])
            .collect();
        let reverb_pos = vec![0usize; reverb_times.len()];
        let chorus_max = (sample_rate as f32 * 0.05) as usize; // 50ms max

        Self {
            lp_state: [0.0; 2],
            hp_state: [0.0; 2],
            delay_buffer: vec![0.0; max_delay_samples],
            delay_write_pos: 0,
            reverb_buffer,
            reverb_pos,
            comp_envelope: 0.0,
            eq_x: [0.0; 2],
            eq_y: [0.0; 2],
            chorus_buffer: vec![0.0; chorus_max],
            chorus_write_pos: 0,
            chorus_phase: 0.0,
            peq_states: (0..MAX_EQ_BANDS).map(|_| BiquadState::default()).collect(),
        }
    }

    pub fn reset(&mut self) {
        self.lp_state = [0.0; 2];
        self.hp_state = [0.0; 2];
        self.delay_buffer.fill(0.0);
        self.delay_write_pos = 0;
        for buf in &mut self.reverb_buffer {
            buf.fill(0.0);
        }
        for pos in &mut self.reverb_pos {
            *pos = 0;
        }
        self.comp_envelope = 0.0;
        self.eq_x = [0.0; 2];
        self.eq_y = [0.0; 2];
        self.chorus_buffer.fill(0.0);
        self.chorus_write_pos = 0;
        self.chorus_phase = 0.0;
        for state in &mut self.peq_states {
            *state = BiquadState::default();
        }
    }

    pub fn process(&mut self, samples: &mut [f32], effect: &TrackEffect, sample_rate: u32) {
        match effect {
            TrackEffect::Gain { db } => {
                let gain = 10.0_f32.powf(*db / 20.0);
                for s in samples.iter_mut() {
                    *s *= gain;
                }
            }
            TrackEffect::LowPass { cutoff_hz } => {
                let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
                let dt = 1.0 / sample_rate as f32;
                let alpha = dt / (rc + dt);
                for s in samples.iter_mut() {
                    self.lp_state[0] += alpha * (*s - self.lp_state[0]);
                    *s = self.lp_state[0];
                }
            }
            TrackEffect::HighPass { cutoff_hz } => {
                let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
                let dt = 1.0 / sample_rate as f32;
                let alpha = rc / (rc + dt);
                for s in samples.iter_mut() {
                    let input = *s;
                    *s = alpha * (self.hp_state[0] + input - self.hp_state[1]);
                    self.hp_state[0] = *s;
                    self.hp_state[1] = input;
                }
            }
            TrackEffect::Delay { time_ms, feedback, mix } => {
                let delay_samples = (*time_ms / 1000.0 * sample_rate as f32) as usize;
                let delay_samples = delay_samples.min(self.delay_buffer.len() - 1);
                let feedback = feedback.clamp(0.0, 0.95);
                for s in samples.iter_mut() {
                    let read_pos = (self.delay_write_pos + self.delay_buffer.len() - delay_samples)
                        % self.delay_buffer.len();
                    let delayed = self.delay_buffer[read_pos];
                    self.delay_buffer[self.delay_write_pos] = *s + delayed * feedback;
                    self.delay_write_pos = (self.delay_write_pos + 1) % self.delay_buffer.len();
                    *s = *s * (1.0 - mix) + delayed * mix;
                }
            }
            TrackEffect::Reverb { decay, mix } => {
                let decay = decay.clamp(0.0, 0.99);
                for s in samples.iter_mut() {
                    let dry = *s;
                    let mut wet = 0.0;
                    for (i, buf) in self.reverb_buffer.iter_mut().enumerate() {
                        let pos = self.reverb_pos[i];
                        wet += buf[pos];
                        buf[pos] = dry + buf[pos] * decay;
                        self.reverb_pos[i] = (pos + 1) % buf.len();
                    }
                    wet /= self.reverb_buffer.len() as f32;
                    *s = dry * (1.0 - mix) + wet * mix;
                }
            }
            TrackEffect::Compressor { threshold_db, ratio, attack_ms, release_ms } => {
                self.process_compressor(samples, None, *threshold_db, *ratio, *attack_ms, *release_ms, sample_rate);
            }
            TrackEffect::EqBand { freq_hz, gain_db, q } => {
                // Peaking EQ biquad filter (legacy single-band)
                let a = 10.0_f32.powf(*gain_db / 40.0);
                let w0 = 2.0 * std::f32::consts::PI * freq_hz / sample_rate as f32;
                let alpha = w0.sin() / (2.0 * q);

                let b0 = 1.0 + alpha * a;
                let b1 = -2.0 * w0.cos();
                let b2 = 1.0 - alpha * a;
                let a0 = 1.0 + alpha / a;
                let a1 = -2.0 * w0.cos();
                let a2 = 1.0 - alpha / a;

                for s in samples.iter_mut() {
                    let x0 = *s;
                    let y0 = (b0 / a0) * x0 + (b1 / a0) * self.eq_x[0] + (b2 / a0) * self.eq_x[1]
                        - (a1 / a0) * self.eq_y[0] - (a2 / a0) * self.eq_y[1];
                    self.eq_x[1] = self.eq_x[0];
                    self.eq_x[0] = x0;
                    self.eq_y[1] = self.eq_y[0];
                    self.eq_y[0] = y0;
                    *s = y0;
                }
            }
            TrackEffect::ParametricEq { bands } => {
                self.process_parametric_eq(samples, bands, sample_rate);
            }
            TrackEffect::Chorus { rate_hz, depth, mix } => {
                let buf_len = self.chorus_buffer.len();
                if buf_len == 0 { return; }
                let max_delay = buf_len as f32 * 0.8;
                let phase_inc = rate_hz / sample_rate as f32;
                for s in samples.iter_mut() {
                    self.chorus_buffer[self.chorus_write_pos] = *s;
                    self.chorus_write_pos = (self.chorus_write_pos + 1) % buf_len;

                    let delay = (max_delay * 0.5 * depth * (1.0 + (self.chorus_phase * 2.0 * std::f32::consts::PI).sin())) as usize;
                    let delay = delay.min(buf_len - 1);
                    let read_pos = (self.chorus_write_pos + buf_len - delay) % buf_len;
                    let wet = self.chorus_buffer[read_pos];

                    self.chorus_phase = (self.chorus_phase + phase_inc) % 1.0;
                    *s = *s * (1.0 - mix) + wet * mix;
                }
            }
            TrackEffect::Distortion { drive, mix } => {
                let gain = 10.0_f32.powf(*drive / 20.0);
                for s in samples.iter_mut() {
                    let dry = *s;
                    let driven = (*s * gain).tanh();
                    *s = dry * (1.0 - mix) + driven * mix;
                }
            }
            TrackEffect::Vst3Plugin { .. } => {
                // VST3 processing is handled by the Vst3Plugin instance
                // in the mixer, not through the EffectProcessor.
                // This is a passthrough placeholder.
            }
        }
    }

    /// Process multi-band parametric EQ using cascaded biquad filters.
    fn process_parametric_eq(&mut self, samples: &mut [f32], bands: &[EqBandParams], sample_rate: u32) {
        let num_bands = bands.len().min(MAX_EQ_BANDS);
        if num_bands == 0 {
            return;
        }

        // Pre-compute coefficients for all active bands (stack-allocated, no heap alloc)
        let mut coeffs = [BiquadCoeffs { b0: 0.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }; MAX_EQ_BANDS];
        for (i, band) in bands[..num_bands].iter().enumerate() {
            coeffs[i] = BiquadCoeffs::from_band(band, sample_rate as f32);
        }

        // Process each sample through cascaded biquads
        for s in samples.iter_mut() {
            let mut x = *s;
            for (bi, c) in coeffs[..num_bands].iter().enumerate() {
                let state = &mut self.peq_states[bi];
                let y = c.b0 * x + c.b1 * state.x[0] + c.b2 * state.x[1]
                    - c.a1 * state.y[0] - c.a2 * state.y[1];
                state.x[1] = state.x[0];
                state.x[0] = x;
                state.y[1] = state.y[0];
                state.y[0] = y;
                x = y;
            }
            *s = x;
        }
    }

    /// Process a compressor with an optional sidechain signal.
    /// If `sidechain` is Some, its samples are used for level detection
    /// instead of the main audio signal. The gain reduction is still
    /// applied to `samples`.
    pub fn process_compressor(
        &mut self,
        samples: &mut [f32],
        sidechain: Option<&[f32]>,
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        sample_rate: u32,
    ) {
        let threshold = 10.0_f32.powf(threshold_db / 20.0);
        let attack_coeff = (-1.0 / (attack_ms * 0.001 * sample_rate as f32)).exp();
        let release_coeff = (-1.0 / (release_ms * 0.001 * sample_rate as f32)).exp();
        for (idx, s) in samples.iter_mut().enumerate() {
            let level = if let Some(sc) = sidechain {
                // Use sidechain signal for detection, fall back to 0.0 if out-of-bounds
                sc.get(idx).map(|v| v.abs()).unwrap_or(0.0)
            } else {
                s.abs()
            };
            let coeff = if level > self.comp_envelope { attack_coeff } else { release_coeff };
            self.comp_envelope = coeff * self.comp_envelope + (1.0 - coeff) * level;

            if self.comp_envelope > threshold {
                let db_over = 20.0 * (self.comp_envelope / threshold).log10();
                let db_reduction = db_over * (1.0 - 1.0 / ratio);
                let gain = 10.0_f32.powf(-db_reduction / 20.0);
                *s *= gain;
            }
        }
    }
}

/// Compute the combined frequency response (in dB) of multiple EQ bands at a given frequency.
/// Used by the UI visualization.
pub fn compute_eq_response(bands: &[EqBandParams], freq_hz: f32, sample_rate: f32) -> f32 {
    let mut total_db = 0.0;
    for band in bands {
        let coeffs = BiquadCoeffs::from_band(band, sample_rate);
        total_db += coeffs.magnitude_db(freq_hz, sample_rate);
    }
    total_db
}
