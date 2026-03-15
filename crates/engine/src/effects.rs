use jamhub_model::TrackEffect;

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
    // EQ state (biquad)
    eq_x: [f32; 2],
    eq_y: [f32; 2],
    // Chorus state
    chorus_buffer: Vec<f32>,
    chorus_write_pos: usize,
    chorus_phase: f32,
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
                let threshold = 10.0_f32.powf(*threshold_db / 20.0);
                let attack_coeff = (-1.0 / (attack_ms * 0.001 * sample_rate as f32)).exp();
                let release_coeff = (-1.0 / (release_ms * 0.001 * sample_rate as f32)).exp();
                for s in samples.iter_mut() {
                    let level = s.abs();
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
            TrackEffect::EqBand { freq_hz, gain_db, q } => {
                // Peaking EQ biquad filter
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
        }
    }
}
