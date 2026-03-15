/// Offline clip processing operations — destructive edits applied to audio buffers.

/// Reverse audio samples in-place.
pub fn reverse(samples: &mut [f32]) {
    samples.reverse();
}

/// Apply a fade-in over the given number of samples.
pub fn fade_in(samples: &mut [f32], fade_samples: usize) {
    let fade_len = fade_samples.min(samples.len());
    for i in 0..fade_len {
        let gain = i as f32 / fade_len as f32;
        // Smooth curve (equal-power-ish)
        let gain = gain * gain * (3.0 - 2.0 * gain);
        samples[i] *= gain;
    }
}

/// Apply a fade-out over the given number of samples at the end.
pub fn fade_out(samples: &mut [f32], fade_samples: usize) {
    let len = samples.len();
    let fade_len = fade_samples.min(len);
    let start = len - fade_len;
    for i in 0..fade_len {
        let gain = 1.0 - (i as f32 / fade_len as f32);
        let gain = gain * gain * (3.0 - 2.0 * gain);
        samples[start + i] *= gain;
    }
}

/// Normalize audio to peak level (0 dB = 1.0).
pub fn normalize(samples: &mut [f32]) {
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if peak > 0.0001 {
        let gain = 0.99 / peak;
        for s in samples.iter_mut() {
            *s *= gain;
        }
    }
}

/// Invert polarity (phase flip).
pub fn invert(samples: &mut [f32]) {
    for s in samples.iter_mut() {
        *s = -*s;
    }
}

/// Remove silence from the beginning (trim leading silence below threshold).
pub fn trim_silence_start(samples: &[f32], threshold: f32) -> usize {
    samples
        .iter()
        .position(|s| s.abs() > threshold)
        .unwrap_or(0)
}

/// Remove silence from the end.
pub fn trim_silence_end(samples: &[f32], threshold: f32) -> usize {
    let last = samples
        .iter()
        .rposition(|s| s.abs() > threshold)
        .unwrap_or(samples.len());
    last + 1
}

/// Generate a crossfade between two overlapping buffers.
/// Returns a buffer of `crossfade_samples` length where buf_a fades out and buf_b fades in.
pub fn crossfade(buf_a: &[f32], buf_b: &[f32], crossfade_samples: usize) -> Vec<f32> {
    let len = crossfade_samples.min(buf_a.len()).min(buf_b.len());
    let mut output = Vec::with_capacity(len);
    for i in 0..len {
        let t = i as f32 / len as f32;
        // Equal-power crossfade
        let gain_a = ((1.0 - t) * std::f32::consts::FRAC_PI_2).sin();
        let gain_b = (t * std::f32::consts::FRAC_PI_2).sin();
        output.push(buf_a[buf_a.len() - len + i] * gain_a + buf_b[i] * gain_b);
    }
    output
}

/// Change gain by dB amount.
pub fn apply_gain_db(samples: &mut [f32], db: f32) {
    let gain = 10.0_f32.powf(db / 20.0);
    for s in samples.iter_mut() {
        *s *= gain;
    }
}
