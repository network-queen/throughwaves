use jamhub_model::TrackEffect;

/// Processes audio through an effect chain.
pub struct EffectProcessor {
    lp_state: [f32; 2],
    hp_state: [f32; 2],
    delay_buffer: Vec<f32>,
    delay_write_pos: usize,
    reverb_buffer: Vec<Vec<f32>>,
    reverb_pos: Vec<usize>,
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

        Self {
            lp_state: [0.0; 2],
            hp_state: [0.0; 2],
            delay_buffer: vec![0.0; max_delay_samples],
            delay_write_pos: 0,
            reverb_buffer,
            reverb_pos,
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
                    self.lp_state[0] = self.lp_state[0] + alpha * (*s - self.lp_state[0]);
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
            TrackEffect::Delay {
                time_ms,
                feedback,
                mix,
            } => {
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
        }
    }
}
