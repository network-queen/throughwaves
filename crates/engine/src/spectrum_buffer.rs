use std::sync::Arc;
use parking_lot::Mutex;

/// Fixed-size ring buffer for sharing audio data between the engine thread
/// and the UI spectrum analyzer. The engine writes blocks of audio; the UI
/// reads the most recent N samples for FFT analysis.
const SPECTRUM_BUF_SIZE: usize = 4096;

#[derive(Clone)]
pub struct SpectrumBuffer {
    inner: Arc<Mutex<RingBuf>>,
}

struct RingBuf {
    data: Vec<f32>,
    write_pos: usize,
    generation: u64,
}

impl SpectrumBuffer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RingBuf {
                data: vec![0.0; SPECTRUM_BUF_SIZE],
                write_pos: 0,
                generation: 0,
            })),
        }
    }

    /// Called by the engine thread to push interleaved stereo samples.
    /// We mix to mono before storing.
    pub fn push_block(&self, interleaved: &[f32], channels: usize) {
        let mut buf = self.inner.lock();
        for frame in interleaved.chunks(channels) {
            let mono: f32 = frame.iter().sum::<f32>() / channels as f32;
            let wp = buf.write_pos;
            buf.data[wp] = mono;
            buf.write_pos = (wp + 1) % SPECTRUM_BUF_SIZE;
        }
        buf.generation = buf.generation.wrapping_add(1);
    }

    /// Read the most recent `count` mono samples (ordered oldest to newest).
    /// Returns the samples and the current generation counter (for change detection).
    pub fn read_recent(&self, count: usize) -> (Vec<f32>, u64) {
        let buf = self.inner.lock();
        let count = count.min(SPECTRUM_BUF_SIZE);
        let mut out = Vec::with_capacity(count);

        // Start reading from (write_pos - count) wrapped around
        let start = if buf.write_pos >= count {
            buf.write_pos - count
        } else {
            SPECTRUM_BUF_SIZE - (count - buf.write_pos)
        };

        for i in 0..count {
            let idx = (start + i) % SPECTRUM_BUF_SIZE;
            out.push(buf.data[idx]);
        }

        (out, buf.generation)
    }

    /// The buffer size (number of mono samples stored).
    pub fn size(&self) -> usize {
        SPECTRUM_BUF_SIZE
    }
}
