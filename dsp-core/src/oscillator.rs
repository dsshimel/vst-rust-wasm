use crate::params::OscillatorType;
use core::f32::consts::PI;

/// A phase-accumulator oscillator with PolyBLEP anti-aliasing.
///
/// PolyBLEP (Polynomial Band-Limited Step) applies a small correction near
/// waveform discontinuities, dramatically reducing aliasing artifacts in
/// square, saw, and triangle waves without the cost of oversampling.
pub struct Oscillator {
    phase: f32,
    phase_delta: f32,
    sample_rate: f32,
    frequency: f32,
    osc_type: OscillatorType,
    // Running sum for PolyBLEP-integrated triangle wave
    tri_integrator: f32,
}

impl Oscillator {
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            phase_delta: 0.0,
            sample_rate: 44100.0,
            frequency: 440.0,
            osc_type: OscillatorType::Sine,
            tri_integrator: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.update_phase_delta();
    }

    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
        self.update_phase_delta();
    }

    pub fn set_type(&mut self, osc_type: OscillatorType) {
        self.osc_type = osc_type;
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.tri_integrator = 0.0;
    }

    /// Generate the next sample and advance the phase.
    pub fn tick(&mut self) -> f32 {
        let dt = self.phase_delta;
        let sample = match self.osc_type {
            OscillatorType::Sine => generate_sine(self.phase),
            OscillatorType::Saw => generate_saw_polyblep(self.phase, dt),
            OscillatorType::Square => generate_square_polyblep(self.phase, dt),
            OscillatorType::Triangle => {
                // PolyBLEP triangle: integrate a PolyBLEP square wave, then
                // normalize. This produces a band-limited triangle with smooth
                // peaks instead of the sharp corners of a naive triangle.
                let square = generate_square_polyblep(self.phase, dt);
                // Leaky integrator: the 4.0 * dt factor normalizes amplitude;
                // the leak term (1.0 - dt) prevents DC drift.
                self.tri_integrator = dt * square + (1.0 - dt) * self.tri_integrator;
                // Scale to approximately [-1, 1] range
                self.tri_integrator * 4.0
            }
        };

        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        sample
    }

    fn update_phase_delta(&mut self) {
        self.phase_delta = self.frequency / self.sample_rate;
    }
}

// --- Waveform generators ---

fn generate_sine(phase: f32) -> f32 {
    (phase * 2.0 * PI).sin()
}

/// Naive saw: rises from -1 to +1 over one period.
/// PolyBLEP correction is applied at the discontinuity (phase ≈ 0/1).
fn generate_saw_polyblep(phase: f32, dt: f32) -> f32 {
    let naive = 2.0 * phase - 1.0;
    // Discontinuity at phase = 1.0 (wrapping to 0.0), amplitude = 2.0
    naive - polyblep(phase, dt)
}

/// Naive square: +1 for first half, -1 for second half.
/// PolyBLEP corrections at both transitions (phase ≈ 0 and phase ≈ 0.5).
fn generate_square_polyblep(phase: f32, dt: f32) -> f32 {
    let naive = if phase < 0.5 { 1.0 } else { -1.0 };
    // Correction at the rising edge (phase ≈ 0)
    let mut sample = naive + polyblep(phase, dt);
    // Correction at the falling edge (phase ≈ 0.5)
    sample -= polyblep((phase + 0.5) % 1.0, dt);
    sample
}

/// PolyBLEP residual function.
///
/// This is the 2nd-order polynomial correction applied near a discontinuity.
/// `t` is the phase position relative to the discontinuity (at t=0),
/// `dt` is the phase increment per sample.
///
/// Near the discontinuity (within one sample), it returns a small correction
/// value that smooths the hard edge:
/// - Just after the edge (0 <= t < dt):   parabola curving one way
/// - Just before the edge (1-dt <= t < 1): parabola curving the other way
/// - Everywhere else: 0.0
fn polyblep(t: f32, dt: f32) -> f32 {
    if t < dt {
        // t/dt is in [0, 1) — we just passed the discontinuity
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        // (t-1)/dt is in (-1, 0] — we're about to hit the discontinuity
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_samples(osc_type: OscillatorType, freq: f32, sample_rate: f32, n: usize) -> Vec<f32> {
        let mut osc = Oscillator::new();
        osc.set_sample_rate(sample_rate);
        osc.set_frequency(freq);
        osc.set_type(osc_type);
        osc.reset();
        (0..n).map(|_| osc.tick()).collect()
    }

    fn count_zero_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count()
    }

    #[test]
    fn test_sine_output_range() {
        let samples = collect_samples(OscillatorType::Sine, 440.0, 44100.0, 44100);
        for (i, &s) in samples.iter().enumerate() {
            assert!(s >= -1.0 && s <= 1.0, "sample {} out of range: {}", i, s);
        }
    }

    #[test]
    fn test_saw_output_range() {
        let samples = collect_samples(OscillatorType::Saw, 440.0, 44100.0, 44100);
        for (i, &s) in samples.iter().enumerate() {
            assert!(
                s >= -1.05 && s <= 1.05,
                "saw sample {} out of range: {}",
                i, s
            );
        }
    }

    #[test]
    fn test_square_output_range() {
        let samples = collect_samples(OscillatorType::Square, 440.0, 44100.0, 44100);
        for (i, &s) in samples.iter().enumerate() {
            assert!(
                s >= -1.05 && s <= 1.05,
                "square sample {} out of range: {}",
                i, s
            );
        }
    }

    #[test]
    fn test_triangle_output_range() {
        let samples = collect_samples(OscillatorType::Triangle, 440.0, 44100.0, 44100);
        // Skip first ~100 samples (1 cycle at 440Hz/44100SR) — the leaky integrator
        // has a startup transient that settles within a few cycles.
        for (i, &s) in samples.iter().enumerate().skip(200) {
            assert!(
                s >= -1.2 && s <= 1.2,
                "triangle sample {} out of range: {}",
                i, s
            );
        }
    }

    #[test]
    fn test_sine_known_values() {
        // 1 Hz at 4 SR = phase_delta 0.25, so 4 samples per cycle
        // tick reads phase THEN advances, so samples at phase 0, 0.25, 0.5, 0.75
        let samples = collect_samples(OscillatorType::Sine, 1.0, 4.0, 4);
        let eps = 1e-5;
        assert!((samples[0] - 0.0).abs() < eps, "sin(0) = {}", samples[0]);
        assert!((samples[1] - 1.0).abs() < eps, "sin(pi/2) = {}", samples[1]);
        assert!((samples[2] - 0.0).abs() < eps, "sin(pi) = {}", samples[2]);
        assert!((samples[3] - -1.0).abs() < eps, "sin(3pi/2) = {}", samples[3]);
    }

    #[test]
    fn test_sine_frequency_accuracy() {
        let samples = collect_samples(OscillatorType::Sine, 440.0, 44100.0, 44100);
        let crossings = count_zero_crossings(&samples);
        // 440 Hz = 880 zero-crossings per second
        assert!(
            (crossings as i32 - 880).abs() <= 2,
            "expected ~880 crossings, got {}",
            crossings
        );
    }

    #[test]
    fn test_saw_frequency_accuracy() {
        let samples = collect_samples(OscillatorType::Saw, 100.0, 44100.0, 44100);
        let crossings = count_zero_crossings(&samples);
        // 100 Hz saw: ~200 zero-crossings per second
        assert!(
            (crossings as i32 - 200).abs() <= 4,
            "expected ~200 crossings, got {}",
            crossings
        );
    }

    #[test]
    fn test_phase_reset_deterministic() {
        let mut osc = Oscillator::new();
        osc.set_sample_rate(44100.0);
        osc.set_frequency(440.0);
        osc.set_type(OscillatorType::Sine);
        osc.reset();

        // Generate some samples, then reset
        for _ in 0..100 {
            osc.tick();
        }
        osc.reset();
        let after_reset: Vec<f32> = (0..10).map(|_| osc.tick()).collect();

        // Fresh oscillator with same settings
        let mut osc2 = Oscillator::new();
        osc2.set_sample_rate(44100.0);
        osc2.set_frequency(440.0);
        osc2.set_type(OscillatorType::Sine);
        osc2.reset();
        let fresh: Vec<f32> = (0..10).map(|_| osc2.tick()).collect();

        assert_eq!(after_reset, fresh);
    }

    #[test]
    fn test_sample_rate_change_updates_phase_delta() {
        let buf_a = collect_samples(OscillatorType::Sine, 440.0, 44100.0, 100);
        let buf_b = collect_samples(OscillatorType::Sine, 440.0, 48000.0, 100);
        assert_ne!(buf_a, buf_b);
    }

    #[test]
    fn test_all_waveforms_produce_nonsilent_output() {
        for &osc_type in OscillatorType::VARIANTS {
            let samples = collect_samples(osc_type, 440.0, 44100.0, 1000);
            assert!(
                samples.iter().any(|s| s.abs() > 0.001),
                "{:?} produced silence",
                osc_type
            );
        }
    }

    #[test]
    fn test_sine_dc_offset_near_zero() {
        let samples = collect_samples(OscillatorType::Sine, 440.0, 44100.0, 44100);
        let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < 0.01, "sine DC offset: {}", mean);
    }

    #[test]
    fn test_saw_dc_offset_near_zero() {
        let samples = collect_samples(OscillatorType::Saw, 440.0, 44100.0, 44100);
        let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < 0.02, "saw DC offset: {}", mean);
    }

    #[test]
    fn test_square_dc_offset_near_zero() {
        let samples = collect_samples(OscillatorType::Square, 440.0, 44100.0, 44100);
        let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < 0.02, "square DC offset: {}", mean);
    }

    #[test]
    fn test_polyblep_function_directly() {
        let dt = 0.01;
        // Far from discontinuity
        assert_eq!(polyblep(0.5, dt), 0.0);

        // At t=0 (right at discontinuity): t/dt=0, 2*0 - 0 - 1 = -1
        let val = polyblep(0.0, dt);
        assert!((val - (-1.0)).abs() < 1e-6, "polyblep(0, dt) = {}", val);

        // Midpoint of post-discontinuity region: t=dt/2, t/dt=0.5
        // 2*0.5 - 0.25 - 1 = -0.25
        let val = polyblep(dt / 2.0, dt);
        assert!((val - (-0.25)).abs() < 1e-6, "polyblep(dt/2, dt) = {}", val);

        // Pre-discontinuity region: t = 1-dt/2, (t-1)/dt = -0.5
        // (-0.5)^2 + 2*(-0.5) + 1 = 0.25 - 1 + 1 = 0.25
        let val = polyblep(1.0 - dt / 2.0, dt);
        assert!((val - 0.25).abs() < 1e-6, "polyblep(1-dt/2, dt) = {}", val);
    }
}
