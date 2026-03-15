use jamhub_model::Tempo;

pub struct Metronome {
    pub enabled: bool,
    pub volume: f32,
}

impl Default for Metronome {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0.5,
        }
    }
}

impl Metronome {
    /// Generate a metronome click into the output buffer.
    /// `output` is interleaved stereo/multi-channel.
    pub fn render(
        &self,
        output: &mut [f32],
        position_samples: u64,
        block_size: usize,
        channels: usize,
        sample_rate: u32,
        tempo: &Tempo,
        beats_per_bar: u8,
    ) {
        if !self.enabled {
            return;
        }

        let samples_per_beat = tempo.samples_per_beat(sample_rate as f64) as u64;
        let click_duration: u64 = (sample_rate as f64 * 0.02) as u64; // 20ms click

        for i in 0..block_size {
            let global_sample = position_samples + i as u64;
            let beat_pos = global_sample % samples_per_beat;

            if beat_pos < click_duration {
                // Which beat in the bar?
                let beat_number =
                    ((global_sample / samples_per_beat) % beats_per_bar as u64) as u32;
                let freq = if beat_number == 0 { 1200.0 } else { 800.0 };
                let amp = if beat_number == 0 {
                    self.volume
                } else {
                    self.volume * 0.6
                };

                let t = beat_pos as f32 / sample_rate as f32;
                let envelope = 1.0 - (beat_pos as f32 / click_duration as f32);
                let sample = (t * freq * std::f32::consts::TAU).sin() * amp * envelope;

                for ch in 0..channels {
                    output[i * channels + ch] += sample;
                }
            }
        }
    }
}
