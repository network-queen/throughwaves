use std::collections::HashMap;
use std::io::Write;
use std::fs::File;
use std::path::Path;

use jamhub_model::{ClipBufferId, Project};

use crate::mixer::Mixer;

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Wav,
    Flac,
    Aiff,
}

impl ExportFormat {
    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::Wav => "WAV",
            ExportFormat::Flac => "FLAC",
            ExportFormat::Aiff => "AIFF",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Wav => "wav",
            ExportFormat::Flac => "flac",
            ExportFormat::Aiff => "aiff",
        }
    }

    pub const ALL: [ExportFormat; 3] = [ExportFormat::Wav, ExportFormat::Flac, ExportFormat::Aiff];
}

pub struct ExportOptions {
    pub normalize: bool,
    pub bit_depth: u16,   // 16, 24, or 32
    pub channels: u16,    // 1 (mono) or 2 (stereo)
    pub tail_seconds: f32, // extra seconds for effects tail
    pub format: ExportFormat,
    pub sample_rate: u32,  // target sample rate (0 = use project rate)
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            normalize: false,
            bit_depth: 32,
            channels: 2,
            tail_seconds: 1.0,
            format: ExportFormat::Wav,
            sample_rate: 0,
        }
    }
}

/// Export the entire project as a WAV file (offline render).
/// Legacy convenience wrapper.
pub fn export_wav(
    path: &Path,
    project: &Project,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
    channels: u16,
) -> Result<(), String> {
    export_with_options(path, project, audio_buffers, sample_rate, &ExportOptions {
        channels,
        ..Default::default()
    })
}

/// Alias kept for backwards compatibility.
pub fn export_wav_with_options(
    path: &Path,
    project: &Project,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
    options: &ExportOptions,
) -> Result<(), String> {
    export_with_options(path, project, audio_buffers, sample_rate, options)
}

/// Render the project offline and write to the chosen format.
pub fn export_with_options(
    path: &Path,
    project: &Project,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
    options: &ExportOptions,
) -> Result<(), String> {
    let mut mixer = Mixer::new(sample_rate, options.channels);
    let block_size: usize = 1024;

    // Find the end of the last clip (only non-muted)
    let end_sample = project
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter())
        .filter(|c| !c.muted)
        .map(|c| c.start_sample + c.duration_samples)
        .max()
        .unwrap_or(0);

    if end_sample == 0 {
        return Err("Nothing to export — no active clips in project".into());
    }

    let total_samples = end_sample + (sample_rate as f32 * options.tail_seconds) as u64;

    // Render all audio
    let mut all_samples: Vec<f32> = Vec::new();
    let mut position: u64 = 0;
    while position < total_samples {
        let block = mixer.render_block(project, position, block_size, audio_buffers);
        all_samples.extend_from_slice(&block);
        position += block_size as u64;
    }

    // Normalize if requested
    if options.normalize {
        let peak = all_samples
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max);
        if peak > 0.001 {
            let gain = 0.99 / peak;
            for s in all_samples.iter_mut() {
                *s *= gain;
            }
        }
    }

    // Resample if a different target sample rate is requested
    let target_sr = if options.sample_rate > 0 && options.sample_rate != sample_rate {
        let resampled = resample_linear(&all_samples, options.channels, sample_rate, options.sample_rate);
        all_samples = resampled;
        options.sample_rate
    } else {
        sample_rate
    };

    match options.format {
        ExportFormat::Wav => write_wav(path, &all_samples, target_sr, options),
        ExportFormat::Flac => write_flac(path, &all_samples, target_sr, options),
        ExportFormat::Aiff => write_aiff(path, &all_samples, target_sr, options),
    }
}

// ---------------------------------------------------------------------------
// WAV writer (using hound)
// ---------------------------------------------------------------------------

fn write_wav(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    options: &ExportOptions,
) -> Result<(), String> {
    match options.bit_depth {
        16 => {
            let spec = hound::WavSpec {
                channels: options.channels,
                sample_rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = hound::WavWriter::create(path, spec)
                .map_err(|e| format!("Failed to create WAV: {e}"))?;
            for &s in samples {
                let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                writer.write_sample(v).map_err(|e| format!("Write error: {e}"))?;
            }
            writer.finalize().map_err(|e| format!("Finalize: {e}"))?;
        }
        24 => {
            let spec = hound::WavSpec {
                channels: options.channels,
                sample_rate,
                bits_per_sample: 24,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = hound::WavWriter::create(path, spec)
                .map_err(|e| format!("Failed to create WAV: {e}"))?;
            for &s in samples {
                let v = (s.clamp(-1.0, 1.0) * 8388607.0) as i32;
                writer.write_sample(v).map_err(|e| format!("Write error: {e}"))?;
            }
            writer.finalize().map_err(|e| format!("Finalize: {e}"))?;
        }
        _ => {
            // 32-bit float
            let spec = hound::WavSpec {
                channels: options.channels,
                sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut writer = hound::WavWriter::create(path, spec)
                .map_err(|e| format!("Failed to create WAV: {e}"))?;
            for &s in samples {
                writer.write_sample(s).map_err(|e| format!("Write error: {e}"))?;
            }
            writer.finalize().map_err(|e| format!("Finalize: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// AIFF writer (pure Rust — big-endian PCM)
// ---------------------------------------------------------------------------

fn write_aiff(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    options: &ExportOptions,
) -> Result<(), String> {
    let ch = options.channels as u32;
    let bps = if options.bit_depth == 16 { 16u16 } else if options.bit_depth == 24 { 24u16 } else { 32u16 };
    let bytes_per_sample = (bps / 8) as u32;
    let num_frames = samples.len() as u32 / ch;
    let sound_data_size = num_frames * ch * bytes_per_sample;

    let mut f = File::create(path).map_err(|e| format!("Failed to create AIFF: {e}"))?;

    // FORM header
    let comm_chunk_size: u32 = 26; // 18 base + 0 padding (we use extended 80-bit float for SR)
    let ssnd_chunk_size: u32 = sound_data_size + 8; // 8 bytes for offset + block size fields
    let form_size: u32 = 4 + (8 + comm_chunk_size) + (8 + ssnd_chunk_size);

    write_bytes(&mut f, b"FORM")?;
    write_be_u32(&mut f, form_size)?;
    write_bytes(&mut f, b"AIFF")?;

    // COMM chunk
    write_bytes(&mut f, b"COMM")?;
    write_be_u32(&mut f, comm_chunk_size)?;
    write_be_i16(&mut f, options.channels as i16)?;   // numChannels
    write_be_u32(&mut f, num_frames)?;                  // numSampleFrames
    write_be_i16(&mut f, bps as i16)?;                  // sampleSize (bits)
    write_extended_80(&mut f, sample_rate as f64)?;     // sampleRate (80-bit extended)

    // SSND chunk
    write_bytes(&mut f, b"SSND")?;
    write_be_u32(&mut f, ssnd_chunk_size)?;
    write_be_u32(&mut f, 0)?; // offset
    write_be_u32(&mut f, 0)?; // blockSize

    // Write sample data (big-endian)
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        match bps {
            16 => {
                let v = (clamped * 32767.0) as i16;
                write_be_i16(&mut f, v)?;
            }
            24 => {
                let v = (clamped * 8388607.0) as i32;
                let bytes = v.to_be_bytes();
                // Write only the lower 3 bytes (big-endian 24-bit)
                write_bytes(&mut f, &bytes[1..4])?;
            }
            _ => {
                // 32-bit int for AIFF (not float — AIFF doesn't have native float in the base spec)
                let v = (clamped * 2147483647.0) as i32;
                let bytes = v.to_be_bytes();
                write_bytes(&mut f, &bytes)?;
            }
        }
    }

    f.flush().map_err(|e| format!("Flush error: {e}"))?;
    Ok(())
}

/// Write an IEEE 754 80-bit extended precision float (big-endian).
/// Used for the sample rate field in AIFF COMM chunks.
fn write_extended_80(w: &mut impl Write, value: f64) -> Result<(), String> {
    let mut buf = [0u8; 10];
    if value == 0.0 {
        w.write_all(&buf).map_err(|e| format!("Write error: {e}"))?;
        return Ok(());
    }

    let sign: u16 = if value < 0.0 { 0x8000 } else { 0 };
    let val = value.abs();

    // Decompose: val = mantissa * 2^exponent where 1.0 <= mantissa < 2.0
    let mut exp = val.log2().floor() as i32;
    let mut mantissa = val / (2.0f64).powi(exp);

    // Normalize
    if mantissa < 1.0 {
        mantissa *= 2.0;
        exp -= 1;
    }

    let biased_exp = (exp + 16383) as u16;
    let exponent_field = sign | biased_exp;

    // 64-bit mantissa: integer bit is explicit in extended precision
    let mantissa_bits = (mantissa * (1u64 << 63) as f64) as u64;

    buf[0] = (exponent_field >> 8) as u8;
    buf[1] = exponent_field as u8;
    buf[2] = (mantissa_bits >> 56) as u8;
    buf[3] = (mantissa_bits >> 48) as u8;
    buf[4] = (mantissa_bits >> 40) as u8;
    buf[5] = (mantissa_bits >> 32) as u8;
    buf[6] = (mantissa_bits >> 24) as u8;
    buf[7] = (mantissa_bits >> 16) as u8;
    buf[8] = (mantissa_bits >> 8) as u8;
    buf[9] = mantissa_bits as u8;

    w.write_all(&buf).map_err(|e| format!("Write error: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// FLAC writer (pure Rust — no external crate)
// We write a minimal valid FLAC file: STREAMINFO + raw frames with VERBATIM subframes.
// This is lossless (stores exact PCM samples) though not compressed.
// ---------------------------------------------------------------------------

fn write_flac(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    options: &ExportOptions,
) -> Result<(), String> {
    let bps = match options.bit_depth {
        16 => 16u32,
        24 => 24u32,
        _ => 24u32, // FLAC doesn't do float, default to 24-bit
    };
    let ch = options.channels as u32;
    let num_frames = samples.len() as u64 / ch as u64;
    let block_size: u32 = 4096;

    let mut f = File::create(path).map_err(|e| format!("Failed to create FLAC: {e}"))?;

    // fLaC marker
    write_bytes(&mut f, b"fLaC")?;

    // STREAMINFO metadata block (is_last=1, type=0, length=34)
    let streaminfo_header: u32 = (1 << 31) | 34; // is_last | type=STREAMINFO | length=34
    write_be_u32(&mut f, streaminfo_header)?;

    // STREAMINFO: 34 bytes
    // min_block_size (16), max_block_size (16)
    write_be_u16(&mut f, block_size as u16)?;
    write_be_u16(&mut f, block_size as u16)?;
    // min_frame_size (24), max_frame_size (24) — 0 = unknown
    write_bytes(&mut f, &[0u8; 3])?;
    write_bytes(&mut f, &[0u8; 3])?;
    // sample_rate (20 bits) | channels-1 (3 bits) | bps-1 (5 bits) | total_samples high (4 bits)
    let sr_bits = sample_rate & 0xFFFFF;
    let ch_bits = (ch - 1) & 0x7;
    let bps_bits = (bps - 1) & 0x1F;
    let total_hi = ((num_frames >> 32) & 0xF) as u32;

    let dword = (sr_bits << 12) | (ch_bits << 9) | (bps_bits << 4) | total_hi;
    write_be_u32(&mut f, dword)?;
    // total_samples low (32 bits)
    write_be_u32(&mut f, (num_frames & 0xFFFFFFFF) as u32)?;
    // MD5 signature — 16 bytes, all zero (valid but not computed)
    write_bytes(&mut f, &[0u8; 16])?;

    // Write frames
    let frames_per_block = block_size as usize;
    let mut frame_number: u32 = 0;
    let mut sample_offset: usize = 0;

    while sample_offset < samples.len() {
        let remaining_frames = (samples.len() - sample_offset) / ch as usize;
        let this_block = remaining_frames.min(frames_per_block);
        if this_block == 0 {
            break;
        }

        write_flac_frame(
            &mut f,
            &samples[sample_offset..sample_offset + this_block * ch as usize],
            this_block as u32,
            ch as u8,
            bps as u8,
            sample_rate,
            frame_number,
        )?;

        sample_offset += this_block * ch as usize;
        frame_number += 1;
    }

    f.flush().map_err(|e| format!("Flush error: {e}"))?;
    Ok(())
}

/// Write a single FLAC frame using VERBATIM subframe encoding.
fn write_flac_frame(
    w: &mut impl Write,
    interleaved: &[f32],
    block_size: u32,
    channels: u8,
    bps: u8,
    sample_rate: u32,
    frame_number: u32,
) -> Result<(), String> {
    let mut frame_buf: Vec<u8> = Vec::new();

    // Frame header
    // sync code: 0xFFF8 (14 bits sync + 1 reserved + 1 blocking strategy=fixed)
    frame_buf.push(0xFF);
    frame_buf.push(0xF8);

    // Block size code + sample rate code
    let bs_code: u8 = encode_block_size_code(block_size);
    let sr_code: u8 = encode_sample_rate_code(sample_rate);
    frame_buf.push((bs_code << 4) | sr_code);

    // Channel assignment + sample size + reserved bit
    let ch_code: u8 = (channels - 1) & 0x0F;
    let bps_code: u8 = match bps {
        16 => 4,
        24 => 6,
        _ => 4,
    };
    frame_buf.push((ch_code << 4) | (bps_code << 1));

    // Frame number (UTF-8 coded u32)
    encode_utf8_u32(&mut frame_buf, frame_number);

    // Block size: if bs_code is 6, write 8-bit (blocksize-1); if 7, write 16-bit
    if bs_code == 6 {
        frame_buf.push((block_size - 1) as u8);
    } else if bs_code == 7 {
        let v = (block_size - 1) as u16;
        frame_buf.push((v >> 8) as u8);
        frame_buf.push(v as u8);
    }

    // Sample rate: if sr_code is 12, write 8-bit kHz
    if sr_code == 12 {
        frame_buf.push((sample_rate / 1000) as u8);
    } else if sr_code == 13 {
        let v = sample_rate as u16;
        frame_buf.push((v >> 8) as u8);
        frame_buf.push(v as u8);
    } else if sr_code == 14 {
        let v = (sample_rate / 10) as u16;
        frame_buf.push((v >> 8) as u8);
        frame_buf.push(v as u8);
    }

    // CRC-8 of frame header
    let crc8 = compute_crc8(&frame_buf);
    frame_buf.push(crc8);

    // Subframes: one per channel, using VERBATIM encoding
    // We need to write samples as a bitstream
    let mut bit_writer = BitWriter::new();

    for ch in 0..channels as usize {
        // Subframe header: 1 zero-padding bit + 6-bit type + 1 wasted-bits flag
        // VERBATIM type = 0b000001
        bit_writer.write_bits(0, 1); // padding
        bit_writer.write_bits(0b000001, 6); // SUBFRAME_VERBATIM
        bit_writer.write_bits(0, 1); // no wasted bits

        // Write each sample for this channel
        for frame_idx in 0..block_size as usize {
            let sample_idx = frame_idx * channels as usize + ch;
            let s = interleaved.get(sample_idx).copied().unwrap_or(0.0);
            let clamped = s.clamp(-1.0, 1.0);
            match bps {
                16 => {
                    let v = (clamped * 32767.0) as i16;
                    bit_writer.write_bits_signed(v as i32, 16);
                }
                24 => {
                    let v = (clamped * 8388607.0) as i32;
                    bit_writer.write_bits_signed(v, 24);
                }
                _ => {
                    let v = (clamped * 32767.0) as i16;
                    bit_writer.write_bits_signed(v as i32, 16);
                }
            }
        }
    }

    // Flush bits to byte boundary
    bit_writer.flush();
    frame_buf.extend_from_slice(&bit_writer.bytes);

    // CRC-16 of entire frame
    let crc16 = compute_crc16(&frame_buf);
    frame_buf.push((crc16 >> 8) as u8);
    frame_buf.push(crc16 as u8);

    w.write_all(&frame_buf).map_err(|e| format!("Write error: {e}"))?;
    Ok(())
}

fn encode_block_size_code(bs: u32) -> u8 {
    match bs {
        192 => 1,
        576 => 2,
        1152 => 3,
        2304 => 4,
        4608 => 5,
        256 => 8,
        512 => 9,
        1024 => 10,
        2048 => 11,
        4096 => 12,
        8192 => 13,
        16384 => 14,
        32768 => 15,
        _ if bs <= 256 => 6,  // 8-bit block size - 1
        _ => 7,               // 16-bit block size - 1
    }
}

fn encode_sample_rate_code(sr: u32) -> u8 {
    match sr {
        88200 => 1,
        176400 => 2,
        192000 => 3,
        8000 => 4,
        16000 => 5,
        22050 => 6,
        24000 => 7,
        32000 => 8,
        44100 => 9,
        48000 => 10,
        96000 => 11,
        _ if sr % 1000 == 0 && sr / 1000 <= 255 => 12,  // 8-bit kHz
        _ if sr <= 65535 => 13,                            // 16-bit Hz
        _ => 14,                                           // 16-bit deca-Hz
    }
}

fn encode_utf8_u32(buf: &mut Vec<u8>, val: u32) {
    if val < 0x80 {
        buf.push(val as u8);
    } else if val < 0x800 {
        buf.push(0xC0 | ((val >> 6) as u8));
        buf.push(0x80 | ((val & 0x3F) as u8));
    } else if val < 0x10000 {
        buf.push(0xE0 | ((val >> 12) as u8));
        buf.push(0x80 | (((val >> 6) & 0x3F) as u8));
        buf.push(0x80 | ((val & 0x3F) as u8));
    } else if val < 0x200000 {
        buf.push(0xF0 | ((val >> 18) as u8));
        buf.push(0x80 | (((val >> 12) & 0x3F) as u8));
        buf.push(0x80 | (((val >> 6) & 0x3F) as u8));
        buf.push(0x80 | ((val & 0x3F) as u8));
    } else if val < 0x4000000 {
        buf.push(0xF8 | ((val >> 24) as u8));
        buf.push(0x80 | (((val >> 18) & 0x3F) as u8));
        buf.push(0x80 | (((val >> 12) & 0x3F) as u8));
        buf.push(0x80 | (((val >> 6) & 0x3F) as u8));
        buf.push(0x80 | ((val & 0x3F) as u8));
    } else {
        buf.push(0xFC | ((val >> 30) as u8));
        buf.push(0x80 | (((val >> 24) & 0x3F) as u8));
        buf.push(0x80 | (((val >> 18) & 0x3F) as u8));
        buf.push(0x80 | (((val >> 12) & 0x3F) as u8));
        buf.push(0x80 | (((val >> 6) & 0x3F) as u8));
        buf.push(0x80 | ((val & 0x3F) as u8));
    }
}

struct BitWriter {
    bytes: Vec<u8>,
    current: u8,
    bits_left: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self { bytes: Vec::new(), current: 0, bits_left: 8 }
    }

    fn write_bits(&mut self, value: u32, num_bits: u8) {
        let mut remaining = num_bits;
        let mut val = value;

        while remaining > 0 {
            let to_write = remaining.min(self.bits_left);
            let shift = remaining - to_write;
            let mask = ((1u32 << to_write) - 1) as u8;
            let bits = ((val >> shift) & mask as u32) as u8;

            self.current |= bits << (self.bits_left - to_write);
            self.bits_left -= to_write;
            remaining -= to_write;
            val &= (1u32 << shift).wrapping_sub(1);

            if self.bits_left == 0 {
                self.bytes.push(self.current);
                self.current = 0;
                self.bits_left = 8;
            }
        }
    }

    fn write_bits_signed(&mut self, value: i32, num_bits: u8) {
        // Two's complement: mask to num_bits width
        let mask = (1u64 << num_bits) - 1;
        let unsigned = (value as u32 as u64 & mask) as u32;
        self.write_bits(unsigned, num_bits);
    }

    fn flush(&mut self) {
        if self.bits_left < 8 {
            self.bytes.push(self.current);
            self.current = 0;
            self.bits_left = 8;
        }
    }
}

fn compute_crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ 0x07;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

fn compute_crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x8005;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

// ---------------------------------------------------------------------------
// Linear resampling (simple but functional)
// ---------------------------------------------------------------------------

fn resample_linear(samples: &[f32], channels: u16, from_sr: u32, to_sr: u32) -> Vec<f32> {
    if from_sr == to_sr {
        return samples.to_vec();
    }

    let ch = channels as usize;
    let num_frames_in = samples.len() / ch;
    let ratio = to_sr as f64 / from_sr as f64;
    let num_frames_out = (num_frames_in as f64 * ratio) as usize;
    let mut output = Vec::with_capacity(num_frames_out * ch);

    for i in 0..num_frames_out {
        let src_pos = i as f64 / ratio;
        let src_idx = src_pos as usize;
        let frac = (src_pos - src_idx as f64) as f32;

        for c in 0..ch {
            let idx0 = src_idx * ch + c;
            let idx1 = ((src_idx + 1).min(num_frames_in - 1)) * ch + c;
            let s0 = samples.get(idx0).copied().unwrap_or(0.0);
            let s1 = samples.get(idx1).copied().unwrap_or(0.0);
            output.push(s0 + (s1 - s0) * frac);
        }
    }

    output
}

// ---------------------------------------------------------------------------
// Helper write functions
// ---------------------------------------------------------------------------

fn write_bytes(w: &mut impl Write, data: &[u8]) -> Result<(), String> {
    w.write_all(data).map_err(|e| format!("Write error: {e}"))
}

fn write_be_u32(w: &mut impl Write, v: u32) -> Result<(), String> {
    w.write_all(&v.to_be_bytes()).map_err(|e| format!("Write error: {e}"))
}

fn write_be_u16(w: &mut impl Write, v: u16) -> Result<(), String> {
    w.write_all(&v.to_be_bytes()).map_err(|e| format!("Write error: {e}"))
}

fn write_be_i16(w: &mut impl Write, v: i16) -> Result<(), String> {
    w.write_all(&v.to_be_bytes()).map_err(|e| format!("Write error: {e}"))
}

// ---------------------------------------------------------------------------
// Stem export — render each track individually to a separate file
// ---------------------------------------------------------------------------

/// Result of a stem export: track name and output path for each exported stem.
pub struct StemExportResult {
    pub stems: Vec<(String, std::path::PathBuf)>,
}

/// Export each non-muted track as a separate audio file in the given directory.
///
/// For each track, the project is temporarily modified to solo that track,
/// then rendered through the normal export path.  Files are named
/// `{track_name}.{ext}` (with filesystem-unsafe characters replaced).
///
/// The `progress` callback is invoked with `(current_index, total_count)` so
/// the caller can update a status message.
pub fn export_stems(
    dir: &Path,
    project: &Project,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
    options: &ExportOptions,
    mut progress: impl FnMut(usize, usize),
) -> Result<StemExportResult, String> {
    // Collect indices of exportable tracks (non-muted audio tracks with clips)
    let exportable: Vec<usize> = project
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.muted && t.clips.iter().any(|c| !c.muted))
        .map(|(i, _)| i)
        .collect();

    if exportable.is_empty() {
        return Err("No active tracks to export".into());
    }

    let total = exportable.len();
    let mut stems = Vec::new();

    for (idx, &track_idx) in exportable.iter().enumerate() {
        progress(idx + 1, total);

        // Build a temporary project with only this track
        let mut solo_project = project.clone();
        // Mute everything, then unmute + unsolo only the target track
        for (i, t) in solo_project.tracks.iter_mut().enumerate() {
            if i == track_idx {
                t.muted = false;
                t.solo = false;
            } else {
                t.muted = true;
                t.solo = false;
            }
        }

        // Sanitize track name for filesystem
        let track_name = &solo_project.tracks[track_idx].name;
        let safe_name: String = track_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let safe_name = safe_name.trim();
        let safe_name = if safe_name.is_empty() {
            format!("Track_{}", track_idx + 1)
        } else {
            safe_name.to_string()
        };

        let filename = format!("{}.{}", safe_name, options.format.extension());
        let path = dir.join(&filename);

        export_with_options(&path, &solo_project, audio_buffers, sample_rate, options)?;
        stems.push((track_name.clone(), path));
    }

    Ok(StemExportResult { stems })
}

// ---------------------------------------------------------------------------
// Bounce (unchanged)
// ---------------------------------------------------------------------------

/// Progress callback for bounce operations: (progress_fraction_0_to_1, cancel_check).
/// Returns true if the operation should continue, false if cancelled.
pub type BounceProgressFn<'a> = &'a mut dyn FnMut(f32) -> bool;

/// Bounce a single track to a new audio buffer (freeze/render effects).
pub fn bounce_track(
    project: &Project,
    track_idx: usize,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
) -> Result<Vec<f32>, String> {
    bounce_track_with_progress(project, track_idx, audio_buffers, sample_rate, &mut |_| true)
}

/// Bounce a single track with progress reporting and cancellation support.
pub fn bounce_track_with_progress(
    project: &Project,
    track_idx: usize,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
    progress: BounceProgressFn,
) -> Result<Vec<f32>, String> {
    if track_idx >= project.tracks.len() {
        return Err("Invalid track index".into());
    }

    // Create a temporary project with only this track
    let mut temp_project = project.clone();
    let track = temp_project.tracks[track_idx].clone();
    temp_project.tracks = vec![track];
    temp_project.tracks[0].muted = false;
    temp_project.tracks[0].solo = false;
    temp_project.tracks[0].volume = 1.0;
    temp_project.tracks[0].pan = 0.0;

    let end_sample = temp_project.tracks[0]
        .clips
        .iter()
        .filter(|c| !c.muted)
        .map(|c| c.start_sample + c.duration_samples)
        .max()
        .unwrap_or(0);

    if end_sample == 0 {
        return Err("Track has no active clips".into());
    }

    let total = end_sample + sample_rate as u64; // 1s tail
    let block_size: usize = 1024;
    let mut mixer = Mixer::new(sample_rate, 1); // render mono

    let mut output = Vec::new();
    let mut pos: u64 = 0;
    while pos < total {
        let fraction = pos as f32 / total as f32;
        if !progress(fraction) {
            return Err("Bounce cancelled".into());
        }
        let block = mixer.render_block(&temp_project, pos, block_size, audio_buffers);
        output.extend_from_slice(&block);
        pos += block_size as u64;
    }

    // Trim to exact length
    output.truncate(total as usize);
    Ok(output)
}

/// Bounce a selection range of a single track to a new audio buffer.
pub fn bounce_track_range(
    project: &Project,
    track_idx: usize,
    audio_buffers: &HashMap<ClipBufferId, Vec<f32>>,
    sample_rate: u32,
    range_start: u64,
    range_end: u64,
    progress: BounceProgressFn,
) -> Result<Vec<f32>, String> {
    if track_idx >= project.tracks.len() {
        return Err("Invalid track index".into());
    }
    if range_end <= range_start {
        return Err("Invalid range: end must be after start".into());
    }

    // Create a temporary project with only this track
    let mut temp_project = project.clone();
    let track = temp_project.tracks[track_idx].clone();
    temp_project.tracks = vec![track];
    temp_project.tracks[0].muted = false;
    temp_project.tracks[0].solo = false;
    temp_project.tracks[0].volume = 1.0;
    temp_project.tracks[0].pan = 0.0;

    let total_len = range_end - range_start;
    let block_size: usize = 1024;
    let mut mixer = Mixer::new(sample_rate, 1);

    let mut output = Vec::new();
    let mut pos: u64 = range_start;
    while pos < range_end {
        let fraction = (pos - range_start) as f32 / total_len as f32;
        if !progress(fraction) {
            return Err("Bounce cancelled".into());
        }
        let block = mixer.render_block(&temp_project, pos, block_size, audio_buffers);
        output.extend_from_slice(&block);
        pos += block_size as u64;
    }

    output.truncate(total_len as usize);
    Ok(output)
}
