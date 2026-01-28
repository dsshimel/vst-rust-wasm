use egui;
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisMode {
    Oscilloscope,
    Spectrum,
}

/// Pre-allocated FFT resources to avoid per-frame heap allocations.
/// Created once when the editor opens, reused every frame.
pub struct FftResources {
    fft: Arc<dyn Fft<f32>>,
    buffer: Vec<Complex<f32>>,
    magnitudes: Vec<f32>,
    mag_db: Vec<f32>,
}

const FFT_SIZE: usize = 1024;

impl FftResources {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        Self {
            fft,
            buffer: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            magnitudes: vec![0.0; FFT_SIZE / 2],
            mag_db: vec![0.0; FFT_SIZE / 2],
        }
    }
}

pub struct VisualizerWidget<'a> {
    pub samples: &'a [f32],
    pub mode: VisMode,
    pub rect: egui::Rect,
    pub fft: Option<&'a mut FftResources>,
}

impl<'a> VisualizerWidget<'a> {
    pub fn paint(&mut self, ui: &egui::Ui) {
        let painter = ui.painter_at(self.rect);

        // Background
        painter.rect_filled(self.rect, 4.0, egui::Color32::from_rgb(20, 20, 30));

        match self.mode {
            VisMode::Oscilloscope => self.paint_oscilloscope(&painter),
            VisMode::Spectrum => self.paint_spectrum(&painter),
        }

        // Border
        painter.rect_stroke(
            self.rect,
            4.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 80)),
            egui::StrokeKind::Middle,
        );
    }

    fn paint_oscilloscope(&self, painter: &egui::Painter) {
        if self.samples.is_empty() {
            return;
        }

        let width = self.rect.width();
        let height = self.rect.height();
        let center_y = self.rect.center().y;

        // Draw center line
        painter.line_segment(
            [
                egui::pos2(self.rect.left(), center_y),
                egui::pos2(self.rect.right(), center_y),
            ],
            egui::Stroke::new(0.5, egui::Color32::from_rgb(40, 40, 60)),
        );

        // We display the last ~1024 samples, downsampled to fit the width
        let display_samples = self.samples.len().min(1024);
        let start = self.samples.len().saturating_sub(display_samples);
        let slice = &self.samples[start..];

        let num_points = (width as usize).min(slice.len());
        if num_points < 2 {
            return;
        }

        let step = slice.len() as f32 / num_points as f32;
        let amplitude = height * 0.45;

        let points: Vec<egui::Pos2> = (0..num_points)
            .map(|i| {
                let sample_idx = (i as f32 * step) as usize;
                let sample = slice[sample_idx.min(slice.len() - 1)];
                let x = self.rect.left() + (i as f32 / num_points as f32) * width;
                let y = center_y - sample * amplitude;
                egui::pos2(x, y)
            })
            .collect();

        painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 200, 120)),
        ));
    }

    fn paint_spectrum(&mut self, painter: &egui::Painter) {
        if self.samples.len() < 64 {
            return;
        }

        let fft_res = match self.fft.as_mut() {
            Some(res) => res,
            None => return,
        };

        let fft_size = FFT_SIZE;
        let start = self.samples.len().saturating_sub(fft_size);
        let slice = &self.samples[start..];

        // Apply Hann window and fill pre-allocated buffer (zero allocation)
        let sample_count = slice.len().min(fft_size);
        for i in 0..sample_count {
            let window =
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos());
            fft_res.buffer[i] = Complex::new(slice[i] * window, 0.0);
        }
        for i in sample_count..fft_size {
            fft_res.buffer[i] = Complex::new(0.0, 0.0);
        }

        // Run FFT using cached plan (no allocation)
        fft_res.fft.process(&mut fft_res.buffer);

        // Compute magnitudes into pre-allocated vec
        let half = fft_size / 2;
        for i in 0..half {
            fft_res.magnitudes[i] = (fft_res.buffer[i].norm() / fft_size as f32).max(1e-10);
        }

        // Convert to dB
        let mut max_db = f32::NEG_INFINITY;
        for i in 0..half {
            let db = 20.0 * fft_res.magnitudes[i].log10();
            fft_res.mag_db[i] = db;
            if db > max_db {
                max_db = db;
            }
        }
        let min_db = max_db - 80.0; // 80 dB dynamic range

        let width = self.rect.width();
        let height = self.rect.height();
        let num_bars = (width as usize).min(half);

        // Use logarithmic frequency mapping for more musical display
        let points: Vec<egui::Pos2> = (0..num_bars)
            .map(|i| {
                let t = i as f32 / num_bars as f32;
                let freq_idx =
                    ((half as f32).powf(t) - 1.0).round() as usize;
                let freq_idx = freq_idx.min(half - 1);
                let db = fft_res.mag_db[freq_idx];
                let normalized = ((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0);

                let x = self.rect.left() + (i as f32 / num_bars as f32) * width;
                let y = self.rect.bottom() - normalized * height;
                egui::pos2(x, y)
            })
            .collect();

        if points.len() >= 2 {
            // Fill under the curve
            let mut fill_points = vec![egui::pos2(self.rect.left(), self.rect.bottom())];
            fill_points.extend_from_slice(&points);
            fill_points.push(egui::pos2(self.rect.right(), self.rect.bottom()));

            painter.add(egui::Shape::convex_polygon(
                fill_points,
                egui::Color32::from_rgba_premultiplied(40, 100, 200, 60),
                egui::Stroke::NONE,
            ));

            // Line on top
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 160, 255)),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- VisMode ---

    #[test]
    fn vis_mode_equality() {
        assert_eq!(VisMode::Oscilloscope, VisMode::Oscilloscope);
        assert_eq!(VisMode::Spectrum, VisMode::Spectrum);
        assert_ne!(VisMode::Oscilloscope, VisMode::Spectrum);
    }

    #[test]
    fn vis_mode_clone() {
        let mode = VisMode::Spectrum;
        let cloned = mode;
        assert_eq!(mode, cloned);
    }

    #[test]
    fn vis_mode_debug_format() {
        assert_eq!(format!("{:?}", VisMode::Oscilloscope), "Oscilloscope");
        assert_eq!(format!("{:?}", VisMode::Spectrum), "Spectrum");
    }

    // --- FftResources ---

    #[test]
    fn fft_resources_new_creates_valid_state() {
        let res = FftResources::new();
        assert_eq!(res.buffer.len(), FFT_SIZE);
        assert_eq!(res.magnitudes.len(), FFT_SIZE / 2);
        assert_eq!(res.mag_db.len(), FFT_SIZE / 2);
    }

    #[test]
    fn fft_size_is_1024() {
        assert_eq!(FFT_SIZE, 1024);
    }

    #[test]
    fn fft_resources_buffer_initialized_to_zero() {
        let res = FftResources::new();
        for c in &res.buffer {
            assert_eq!(c.re, 0.0);
            assert_eq!(c.im, 0.0);
        }
    }

    #[test]
    fn fft_resources_magnitudes_initialized_to_zero() {
        let res = FftResources::new();
        for &m in &res.magnitudes {
            assert_eq!(m, 0.0);
        }
    }

    #[test]
    fn fft_resources_mag_db_initialized_to_zero() {
        let res = FftResources::new();
        for &d in &res.mag_db {
            assert_eq!(d, 0.0);
        }
    }

    #[test]
    fn fft_resources_fft_plan_works() {
        // Verify the FFT plan can actually process data without panicking
        let mut res = FftResources::new();
        // Fill with a simple sine wave
        for i in 0..FFT_SIZE {
            let t = i as f32 / FFT_SIZE as f32;
            res.buffer[i] = Complex::new((2.0 * std::f32::consts::PI * t * 10.0).sin(), 0.0);
        }
        res.fft.process(&mut res.buffer);
        // After FFT, the buffer should have non-zero values (energy at bin 10)
        let magnitude_at_bin_10 = res.buffer[10].norm();
        assert!(
            magnitude_at_bin_10 > 1.0,
            "expected significant energy at bin 10, got {}",
            magnitude_at_bin_10
        );
    }

    #[test]
    fn fft_resources_dc_component_for_constant_signal() {
        let mut res = FftResources::new();
        // Constant signal of 1.0 should put all energy in bin 0 (DC)
        for i in 0..FFT_SIZE {
            res.buffer[i] = Complex::new(1.0, 0.0);
        }
        res.fft.process(&mut res.buffer);
        let dc_magnitude = res.buffer[0].norm();
        assert!(
            (dc_magnitude - FFT_SIZE as f32).abs() < 0.1,
            "DC bin magnitude should be ~{}, got {}",
            FFT_SIZE,
            dc_magnitude
        );
    }

    #[test]
    fn fft_resources_silent_signal_produces_zero() {
        let mut res = FftResources::new();
        // Buffer is already zeros
        res.fft.process(&mut res.buffer);
        for c in &res.buffer {
            assert!(c.norm() < 1e-10, "expected zero, got {}", c.norm());
        }
    }
}
