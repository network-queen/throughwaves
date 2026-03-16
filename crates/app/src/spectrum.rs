use eframe::egui;
use jamhub_engine::SpectrumBuffer;

/// FFT size — must be a power of 2.
const FFT_SIZE: usize = 2048;

/// Number of frequency bands displayed in the analyzer.
const NUM_BANDS: usize = 64;

pub struct SpectrumAnalyzer {
    pub show: bool,
    /// Smoothed magnitude values per band (in dB, smoothed for display).
    smoothed: Vec<f32>,
    /// Last generation counter from the spectrum buffer (for change detection).
    last_generation: u64,
    /// Pre-computed Hann window coefficients.
    window: Vec<f32>,
    /// Pre-computed band edge frequencies (logarithmic scale 20Hz..20kHz).
    band_edges: Vec<f32>,
}

impl Default for SpectrumAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        // Hann window
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos())
            })
            .collect();

        // Logarithmic band edges from 20 Hz to 20000 Hz
        let log_min = (20.0f32).ln();
        let log_max = (20000.0f32).ln();
        let band_edges: Vec<f32> = (0..=NUM_BANDS)
            .map(|i| {
                let t = i as f32 / NUM_BANDS as f32;
                (log_min + t * (log_max - log_min)).exp()
            })
            .collect();

        Self {
            show: false,
            smoothed: vec![-90.0; NUM_BANDS],
            last_generation: 0,
            window,
            band_edges,
        }
    }

    /// Run the FFT and update smoothed bands.
    fn update_spectrum(&mut self, spectrum_buf: &SpectrumBuffer, sample_rate: u32) {
        let (samples, gen) = spectrum_buf.read_recent(FFT_SIZE);
        if gen == self.last_generation {
            // No new data — just decay
            for v in self.smoothed.iter_mut() {
                *v = (*v - 1.0).max(-90.0); // decay ~1 dB per frame
            }
            return;
        }
        self.last_generation = gen;

        if samples.len() < FFT_SIZE {
            return;
        }

        // Apply window and compute FFT
        let mut real = vec![0.0f32; FFT_SIZE];
        let mut imag = vec![0.0f32; FFT_SIZE];
        for i in 0..FFT_SIZE {
            real[i] = samples[i] * self.window[i];
        }

        fft_in_place(&mut real, &mut imag);

        // Compute magnitude spectrum (only first half — positive frequencies)
        let half = FFT_SIZE / 2;
        let mut magnitudes = vec![0.0f32; half];
        let norm = 2.0 / FFT_SIZE as f32;
        for i in 0..half {
            let mag = (real[i] * real[i] + imag[i] * imag[i]).sqrt() * norm;
            magnitudes[i] = mag;
        }

        // Map FFT bins to logarithmic bands
        let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
        for band in 0..NUM_BANDS {
            let f_lo = self.band_edges[band];
            let f_hi = self.band_edges[band + 1];

            let bin_lo = (f_lo / bin_hz).floor() as usize;
            let bin_hi = (f_hi / bin_hz).ceil() as usize;
            let bin_lo = bin_lo.max(1).min(half - 1);
            let bin_hi = bin_hi.max(bin_lo + 1).min(half);

            // Average magnitude in this band
            let mut sum = 0.0f32;
            let mut count = 0;
            for b in bin_lo..bin_hi {
                sum += magnitudes[b];
                count += 1;
            }
            let avg = if count > 0 { sum / count as f32 } else { 0.0 };

            // Convert to dB
            let db = if avg > 1e-10 {
                20.0 * avg.log10()
            } else {
                -90.0
            };

            // Smooth: fast attack, slow release
            let target = db.max(-90.0).min(6.0);
            if target > self.smoothed[band] {
                self.smoothed[band] = self.smoothed[band] * 0.3 + target * 0.7; // fast attack
            } else {
                self.smoothed[band] = self.smoothed[band] * 0.85 + target * 0.15; // slow release
            }
        }
    }
}

/// Show the spectrum analyzer as a floating egui window.
pub fn show(app: &mut super::DawApp, ctx: &egui::Context) {
    if !app.spectrum_analyzer.show {
        return;
    }

    // Get the spectrum buffer from the engine
    let (spectrum_buf, sample_rate) = match &app.engine {
        Some(eng) => (eng.spectrum.clone(), eng.state.read().sample_rate),
        None => return,
    };

    // Update the analysis
    app.spectrum_analyzer.update_spectrum(&spectrum_buf, sample_rate);

    // Request repaint for smooth animation
    ctx.request_repaint();

    let mut open = app.spectrum_analyzer.show;
    egui::Window::new("Spectrum Analyzer")
        .open(&mut open)
        .default_size([480.0, 260.0])
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            let available = ui.available_size();
            let height = available.y.max(100.0);
            let width = available.x.max(200.0);

            let (response, painter) = ui.allocate_painter(
                egui::vec2(width, height),
                egui::Sense::hover(),
            );
            let rect = response.rect;

            // Background
            painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(18, 18, 22));

            // Grid lines (horizontal dB markers)
            let db_min = -72.0f32;
            let db_max = 0.0f32;
            let db_range = db_max - db_min;
            let grid_color = egui::Color32::from_rgba_premultiplied(60, 60, 70, 40);
            let text_color = egui::Color32::from_rgb(90, 90, 100);

            for &db in &[-60.0, -48.0, -36.0, -24.0, -12.0, 0.0f32] {
                let y = rect.bottom() - ((db - db_min) / db_range) * rect.height();
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    egui::Stroke::new(0.5, grid_color),
                );
                painter.text(
                    egui::pos2(rect.left() + 2.0, y - 10.0),
                    egui::Align2::LEFT_TOP,
                    format!("{db:.0} dB"),
                    egui::FontId::proportional(9.0),
                    text_color,
                );
            }

            // Frequency labels along bottom
            let log_min = (20.0f32).ln();
            let log_max = (20000.0f32).ln();
            for &freq in &[50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0f32] {
                let t = (freq.ln() - log_min) / (log_max - log_min);
                let x = rect.left() + t * rect.width();
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    egui::Stroke::new(0.5, grid_color),
                );
                let label = if freq >= 1000.0 {
                    format!("{:.0}k", freq / 1000.0)
                } else {
                    format!("{freq:.0}")
                };
                painter.text(
                    egui::pos2(x, rect.bottom() - 12.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    egui::FontId::proportional(9.0),
                    text_color,
                );
            }

            // Draw spectrum bars
            let num_bands = app.spectrum_analyzer.smoothed.len();
            let bar_width = rect.width() / num_bands as f32;
            let gap = 1.0f32;

            // Gradient colors: blue -> cyan -> green -> yellow at high levels
            for (i, &db_val) in app.spectrum_analyzer.smoothed.iter().enumerate() {
                let normalized = ((db_val - db_min) / db_range).clamp(0.0, 1.0);
                let x = rect.left() + i as f32 * bar_width;
                let bar_h = normalized * rect.height();
                let y_top = rect.bottom() - bar_h;

                // Color based on level
                let color = if normalized > 0.85 {
                    // Hot: red-orange
                    let t = (normalized - 0.85) / 0.15;
                    egui::Color32::from_rgb(
                        (255.0) as u8,
                        (180.0 - t * 100.0) as u8,
                        (50.0) as u8,
                    )
                } else if normalized > 0.6 {
                    // Warm: yellow-green
                    let t = (normalized - 0.6) / 0.25;
                    egui::Color32::from_rgb(
                        (100.0 + t * 155.0) as u8,
                        (200.0 - t * 20.0) as u8,
                        (50.0) as u8,
                    )
                } else {
                    // Cool: blue to cyan
                    let t = normalized / 0.6;
                    egui::Color32::from_rgb(
                        (40.0 + t * 60.0) as u8,
                        (100.0 + t * 100.0) as u8,
                        (200.0 + t * 55.0) as u8,
                    )
                };

                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(x + gap * 0.5, y_top),
                    egui::pos2(x + bar_width - gap * 0.5, rect.bottom()),
                );
                painter.rect_filled(bar_rect, 1.0, color);
            }

            // Draw a smooth line on top of the bars for a polished look
            let points: Vec<egui::Pos2> = app.spectrum_analyzer.smoothed.iter().enumerate().map(|(i, &db_val)| {
                let normalized = ((db_val - db_min) / db_range).clamp(0.0, 1.0);
                let x = rect.left() + (i as f32 + 0.5) * bar_width;
                let y = rect.bottom() - normalized * rect.height();
                egui::pos2(x, y)
            }).collect();

            if points.len() >= 2 {
                let line_color = egui::Color32::from_rgba_premultiplied(200, 220, 255, 160);
                for pair in points.windows(2) {
                    painter.line_segment(
                        [pair[0], pair[1]],
                        egui::Stroke::new(1.5, line_color),
                    );
                }
            }
        });
    app.spectrum_analyzer.show = open;
}

// ---------------------------------------------------------------------------
// Radix-2 Cooley-Tukey FFT (in-place, iterative)
// ---------------------------------------------------------------------------

fn fft_in_place(real: &mut [f32], imag: &mut [f32]) {
    let n = real.len();
    assert!(n.is_power_of_two(), "FFT size must be power of 2");
    assert_eq!(real.len(), imag.len());

    // Bit-reversal permutation
    let mut j = 0usize;
    for i in 0..n {
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
        let mut m = n >> 1;
        while m >= 1 && j >= m {
            j -= m;
            m >>= 1;
        }
        j += m;
    }

    // Butterfly stages
    let mut stage_len = 2;
    while stage_len <= n {
        let half = stage_len / 2;
        let angle_step = -2.0 * std::f32::consts::PI / stage_len as f32;

        for k in (0..n).step_by(stage_len) {
            for j in 0..half {
                let angle = angle_step * j as f32;
                let wr = angle.cos();
                let wi = angle.sin();

                let idx_even = k + j;
                let idx_odd = k + j + half;

                let tr = wr * real[idx_odd] - wi * imag[idx_odd];
                let ti = wr * imag[idx_odd] + wi * real[idx_odd];

                real[idx_odd] = real[idx_even] - tr;
                imag[idx_odd] = imag[idx_even] - ti;
                real[idx_even] += tr;
                imag[idx_even] += ti;
            }
        }

        stage_len <<= 1;
    }
}
