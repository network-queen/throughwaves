//! Built-in polyphonic synthesizer for rendering MIDI tracks to audio.

use jamhub_model::{MidiNote, Tempo};

/// Ticks per beat (quarter note) — must match the piano roll constant.
const TICKS_PER_BEAT: f64 = 480.0;

/// Maximum number of simultaneous voices.
const MAX_VOICES: usize = 32;

/// Waveform shape for the oscillator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WaveShape {
    Sine,
    Saw,
    Square,
    Triangle,
}

impl WaveShape {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "sine" | "sin" => WaveShape::Sine,
            "square" | "sq" => WaveShape::Square,
            "triangle" | "tri" => WaveShape::Triangle,
            _ => WaveShape::Saw,
        }
    }
}

/// ADSR envelope phase.
#[derive(Debug, Clone, Copy, PartialEq)]
enum EnvelopePhase {
    Attack,
    Decay,
    Sustain,
    Release,
    Off,
}

/// A single voice in the polyphonic synthesizer.
#[derive(Debug, Clone)]
struct Voice {
    pitch: u8,
    velocity: f32, // 0.0 - 1.0
    phase: f64,    // oscillator phase accumulator (0.0 - 1.0)
    envelope_phase: EnvelopePhase,
    envelope_level: f32,
    /// Global sample when this note started (for note-on timing)
    start_sample: u64,
    /// Global sample when note-off (release) began, 0 if still held
    release_sample: u64,
    /// One-pole low-pass filter state
    filter_state: f32,
}

impl Voice {
    fn new(pitch: u8, velocity: u8, start_sample: u64) -> Self {
        Self {
            pitch,
            velocity: velocity as f32 / 127.0,
            phase: 0.0,
            envelope_phase: EnvelopePhase::Attack,
            envelope_level: 0.0,
            start_sample,
            release_sample: 0,
            filter_state: 0.0,
        }
    }

    fn is_off(&self) -> bool {
        self.envelope_phase == EnvelopePhase::Off
    }

    /// Generate one sample of the oscillator waveform.
    fn oscillate(&mut self, freq_hz: f64, sample_rate: f64, shape: WaveShape) -> f32 {
        let out = match shape {
            WaveShape::Sine => (self.phase * std::f64::consts::TAU).sin() as f32,
            WaveShape::Saw => (2.0 * self.phase - 1.0) as f32,
            WaveShape::Square => {
                if self.phase < 0.5 { 1.0 } else { -1.0 }
            }
            WaveShape::Triangle => {
                let v = if self.phase < 0.5 {
                    4.0 * self.phase - 1.0
                } else {
                    3.0 - 4.0 * self.phase
                };
                v as f32
            }
        };

        self.phase += freq_hz / sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        out
    }

    /// Advance the ADSR envelope by one sample and return the current level.
    fn advance_envelope(
        &mut self,
        attack_samples: f32,
        decay_samples: f32,
        sustain_level: f32,
        release_samples: f32,
    ) -> f32 {
        match self.envelope_phase {
            EnvelopePhase::Attack => {
                if attack_samples <= 1.0 {
                    self.envelope_level = 1.0;
                    self.envelope_phase = EnvelopePhase::Decay;
                } else {
                    self.envelope_level += 1.0 / attack_samples;
                    if self.envelope_level >= 1.0 {
                        self.envelope_level = 1.0;
                        self.envelope_phase = EnvelopePhase::Decay;
                    }
                }
            }
            EnvelopePhase::Decay => {
                if decay_samples <= 1.0 {
                    self.envelope_level = sustain_level;
                    self.envelope_phase = EnvelopePhase::Sustain;
                } else {
                    self.envelope_level -= (1.0 - sustain_level) / decay_samples;
                    if self.envelope_level <= sustain_level {
                        self.envelope_level = sustain_level;
                        self.envelope_phase = EnvelopePhase::Sustain;
                    }
                }
            }
            EnvelopePhase::Sustain => {
                self.envelope_level = sustain_level;
            }
            EnvelopePhase::Release => {
                if release_samples <= 1.0 {
                    self.envelope_level = 0.0;
                    self.envelope_phase = EnvelopePhase::Off;
                } else {
                    self.envelope_level -= self.envelope_level / release_samples;
                    // Use a threshold to avoid very long tails
                    if self.envelope_level < 0.0001 {
                        self.envelope_level = 0.0;
                        self.envelope_phase = EnvelopePhase::Off;
                    }
                }
            }
            EnvelopePhase::Off => {
                self.envelope_level = 0.0;
            }
        }

        self.envelope_level
    }

    /// Apply a simple one-pole low-pass filter.
    fn apply_filter(&mut self, sample: f32, coefficient: f32) -> f32 {
        self.filter_state += coefficient * (sample - self.filter_state);
        self.filter_state
    }
}

/// Polyphonic synthesizer that renders MIDI notes to audio.
pub struct Synth {
    voices: Vec<Voice>,
    pub wave_shape: WaveShape,
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain_level: f32,
    pub release_ms: f32,
    pub filter_cutoff: f32,
}

impl Synth {
    pub fn new() -> Self {
        Self {
            voices: Vec::with_capacity(MAX_VOICES),
            wave_shape: WaveShape::Saw,
            attack_ms: 10.0,
            decay_ms: 100.0,
            sustain_level: 0.7,
            release_ms: 200.0,
            filter_cutoff: 8000.0,
        }
    }

    /// Update synth parameters from track settings.
    pub fn update_params(
        &mut self,
        wave: &str,
        attack_ms: f32,
        decay_ms: f32,
        sustain: f32,
        release_ms: f32,
        cutoff: f32,
    ) {
        self.wave_shape = WaveShape::from_str(wave);
        self.attack_ms = attack_ms;
        self.decay_ms = decay_ms;
        self.sustain_level = sustain.clamp(0.0, 1.0);
        self.release_ms = release_ms;
        self.filter_cutoff = cutoff.clamp(20.0, 20000.0);
    }

    /// Convert a MIDI tick position to a sample position relative to the clip start.
    fn tick_to_sample(tick: u64, tempo: &Tempo, sample_rate: u32) -> u64 {
        let samples_per_beat = tempo.samples_per_beat(sample_rate as f64);
        let samples_per_tick = samples_per_beat / TICKS_PER_BEAT;
        (tick as f64 * samples_per_tick) as u64
    }

    /// Render a block of audio from MIDI notes.
    ///
    /// - `notes`: all MIDI notes in the clip
    /// - `clip_start_sample`: the global sample position where the clip begins
    /// - `position_samples`: the global sample position of the start of this block
    /// - `block_size`: number of samples to render
    /// - `sample_rate`: audio sample rate
    /// - `tempo`: project tempo for tick-to-sample conversion
    ///
    /// Returns a mono buffer of `block_size` samples.
    pub fn render_block(
        &mut self,
        notes: &[MidiNote],
        clip_start_sample: u64,
        position_samples: u64,
        block_size: usize,
        sample_rate: u32,
        tempo: &Tempo,
    ) -> Vec<f32> {
        let mut output = vec![0.0f32; block_size];
        let sr = sample_rate as f64;

        let attack_samples = self.attack_ms * 0.001 * sample_rate as f32;
        let decay_samples = self.decay_ms * 0.001 * sample_rate as f32;
        let release_samples = self.release_ms * 0.001 * sample_rate as f32;

        // One-pole low-pass filter coefficient
        let filter_coeff = (1.0
            - (-std::f32::consts::TAU * self.filter_cutoff / sample_rate as f32).exp())
        .clamp(0.0, 1.0);

        for i in 0..block_size {
            let global_sample = position_samples + i as u64;

            // Check each MIDI note for note-on / note-off events at this sample
            for note in notes {
                let note_on_sample =
                    clip_start_sample + Self::tick_to_sample(note.start_tick, tempo, sample_rate);
                let note_off_sample = clip_start_sample
                    + Self::tick_to_sample(
                        note.start_tick + note.duration_ticks,
                        tempo,
                        sample_rate,
                    );

                // Note-on: allocate a voice
                if global_sample == note_on_sample {
                    self.note_on(note.pitch, note.velocity, global_sample);
                }

                // Note-off: trigger release
                if global_sample == note_off_sample {
                    self.note_off(note.pitch, global_sample);
                }
            }

            // Render all active voices
            let mut sample_sum = 0.0f32;
            for voice in &mut self.voices {
                if voice.is_off() {
                    continue;
                }

                let freq = midi_to_freq(voice.pitch);
                let osc = voice.oscillate(freq, sr, self.wave_shape);
                let env = voice.advance_envelope(
                    attack_samples,
                    decay_samples,
                    self.sustain_level,
                    release_samples,
                );
                let filtered = voice.apply_filter(osc * env * voice.velocity, filter_coeff);
                sample_sum += filtered;
            }

            output[i] = sample_sum;

            // Clean up finished voices periodically (every 64 samples to reduce overhead)
            if i & 63 == 0 {
                self.voices.retain(|v| !v.is_off());
            }
        }

        // Final cleanup
        self.voices.retain(|v| !v.is_off());

        output
    }

    fn note_on(&mut self, pitch: u8, velocity: u8, sample: u64) {
        // If already playing this pitch, retrigger it
        for voice in &mut self.voices {
            if voice.pitch == pitch && voice.envelope_phase != EnvelopePhase::Off {
                voice.velocity = velocity as f32 / 127.0;
                voice.phase = 0.0;
                voice.envelope_phase = EnvelopePhase::Attack;
                voice.envelope_level = 0.0;
                voice.start_sample = sample;
                voice.release_sample = 0;
                return;
            }
        }

        // Voice stealing: if at max, remove the oldest voice
        if self.voices.len() >= MAX_VOICES {
            // Find the oldest voice (earliest start_sample)
            if let Some(oldest_idx) = self
                .voices
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| v.start_sample)
                .map(|(i, _)| i)
            {
                self.voices.remove(oldest_idx);
            }
        }

        self.voices.push(Voice::new(pitch, velocity, sample));
    }

    fn note_off(&mut self, pitch: u8, sample: u64) {
        for voice in &mut self.voices {
            if voice.pitch == pitch
                && voice.envelope_phase != EnvelopePhase::Release
                && voice.envelope_phase != EnvelopePhase::Off
            {
                voice.envelope_phase = EnvelopePhase::Release;
                voice.release_sample = sample;
                break;
            }
        }
    }
}

/// Convert a MIDI note number to frequency in Hz (A4 = 440Hz = note 69).
fn midi_to_freq(note: u8) -> f64 {
    440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_to_freq() {
        let a4 = midi_to_freq(69);
        assert!((a4 - 440.0).abs() < 0.01);

        let c4 = midi_to_freq(60);
        assert!((c4 - 261.63).abs() < 0.1);
    }

    #[test]
    fn test_synth_renders_audio() {
        let mut synth = Synth::new();
        let notes = vec![MidiNote {
            pitch: 60,
            velocity: 100,
            start_tick: 0,
            duration_ticks: 480,
        }];
        let tempo = Tempo { bpm: 120.0 };
        let output = synth.render_block(&notes, 0, 0, 1024, 44100, &tempo);
        assert_eq!(output.len(), 1024);
        // Should have some non-zero audio
        assert!(output.iter().any(|&s| s.abs() > 0.001));
    }

    #[test]
    fn test_synth_silent_when_no_notes() {
        let mut synth = Synth::new();
        let notes: Vec<MidiNote> = vec![];
        let tempo = Tempo { bpm: 120.0 };
        let output = synth.render_block(&notes, 0, 0, 512, 44100, &tempo);
        assert!(output.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn test_voice_stealing() {
        let mut synth = Synth::new();
        // Allocate MAX_VOICES + 1 voices
        for i in 0..=MAX_VOICES {
            synth.note_on(i as u8, 100, i as u64);
        }
        assert!(synth.voices.len() <= MAX_VOICES);
    }

    #[test]
    fn test_waveshape_from_str() {
        assert_eq!(WaveShape::from_str("Sine"), WaveShape::Sine);
        assert_eq!(WaveShape::from_str("Saw"), WaveShape::Saw);
        assert_eq!(WaveShape::from_str("Square"), WaveShape::Square);
        assert_eq!(WaveShape::from_str("Triangle"), WaveShape::Triangle);
        assert_eq!(WaveShape::from_str("unknown"), WaveShape::Saw);
    }
}
