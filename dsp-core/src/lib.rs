pub mod envelope;
pub mod oscillator;
pub mod params;

use envelope::Envelope;
use oscillator::Oscillator;
use params::OscillatorType;

/// Convert a MIDI note number to frequency in Hz.
pub fn midi_note_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

/// A monophonic synthesizer engine.
///
/// This is the shared DSP core that runs identically on native and WASM.
/// It owns one oscillator and one ADSR envelope, producing mono audio output.
pub struct Synth {
    oscillator: Oscillator,
    envelope: Envelope,
    sample_rate: f32,
    gain: f32,
    current_note: Option<u8>,
}

impl Synth {
    pub fn new() -> Self {
        Self {
            oscillator: Oscillator::new(),
            envelope: Envelope::new(),
            sample_rate: 44100.0,
            gain: 0.8,
            current_note: None,
        }
    }

    /// Call once when the host provides sample rate and buffer size info.
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.oscillator.set_sample_rate(sample_rate);
        self.envelope.set_sample_rate(sample_rate);
    }

    pub fn set_oscillator_type(&mut self, osc_type: OscillatorType) {
        self.oscillator.set_type(osc_type);
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 1.0);
    }

    pub fn set_attack(&mut self, seconds: f32) {
        self.envelope.set_attack(seconds);
    }

    pub fn set_decay(&mut self, seconds: f32) {
        self.envelope.set_decay(seconds);
    }

    pub fn set_sustain(&mut self, level: f32) {
        self.envelope.set_sustain(level);
    }

    pub fn set_release(&mut self, seconds: f32) {
        self.envelope.set_release(seconds);
    }

    pub fn note_on(&mut self, note: u8, _velocity: f32) {
        self.current_note = Some(note);
        self.oscillator.set_frequency(midi_note_to_freq(note));
        self.oscillator.reset();
        self.envelope.note_on();
    }

    pub fn note_off(&mut self, note: u8) {
        // Only release if this is the note currently playing
        if self.current_note == Some(note) {
            self.envelope.note_off();
            self.current_note = None;
        }
    }

    /// Fill `output` with mono audio samples. No allocations.
    pub fn process(&mut self, output: &mut [f32]) {
        for sample in output.iter_mut() {
            if self.envelope.is_active() {
                let osc = self.oscillator.tick();
                let env = self.envelope.tick();
                *sample = osc * env * self.gain;
            } else {
                *sample = 0.0;
            }
        }
    }
}
