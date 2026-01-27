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
