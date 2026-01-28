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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_note_to_freq_a4() {
        assert!((midi_note_to_freq(69) - 440.0).abs() < 0.01);
    }

    #[test]
    fn test_midi_note_to_freq_c4() {
        assert!((midi_note_to_freq(60) - 261.63).abs() < 0.01);
    }

    #[test]
    fn test_midi_note_to_freq_a3() {
        assert!((midi_note_to_freq(57) - 220.0).abs() < 0.01);
    }

    #[test]
    fn test_midi_note_to_freq_octave_relationship() {
        let a4 = midi_note_to_freq(69);
        let a5 = midi_note_to_freq(81);
        let a3 = midi_note_to_freq(57);
        assert!((a5 - 2.0 * a4).abs() < 0.01, "A5 should be 2x A4");
        assert!((a3 - a4 / 2.0).abs() < 0.01, "A3 should be A4/2");
    }

    #[test]
    fn test_midi_note_to_freq_extremes() {
        let low = midi_note_to_freq(0);
        let high = midi_note_to_freq(127);
        assert!(low > 0.0 && low.is_finite(), "note 0 freq: {}", low);
        assert!(high > 0.0 && high.is_finite(), "note 127 freq: {}", high);
    }

    #[test]
    fn test_note_on_produces_nonsilent_output() {
        let mut synth = Synth::new();
        synth.prepare(44100.0);
        synth.note_on(69, 0.8);
        let mut buf = [0.0f32; 512];
        synth.process(&mut buf);
        assert!(
            buf.iter().any(|s| s.abs() > 0.001),
            "note_on should produce sound"
        );
    }

    #[test]
    fn test_note_off_wrong_note_keeps_playing() {
        let mut synth = Synth::new();
        synth.prepare(44100.0);
        synth.note_on(69, 0.8);
        let mut buf = [0.0f32; 256];
        synth.process(&mut buf);
        synth.note_off(60);
        let mut buf2 = [0.0f32; 256];
        synth.process(&mut buf2);
        assert!(
            buf2.iter().any(|s| s.abs() > 0.001),
            "wrong note_off should not stop playback"
        );
    }

    #[test]
    fn test_note_off_correct_note_eventually_silent() {
        let mut synth = Synth::new();
        synth.prepare(44100.0);
        synth.set_release(0.01);
        synth.note_on(69, 0.8);
        let mut buf = [0.0f32; 4410];
        synth.process(&mut buf);
        synth.note_off(69);
        let mut buf2 = [0.0f32; 1000];
        synth.process(&mut buf2);
        assert!(
            buf2[900..].iter().all(|s| s.abs() < 1e-6),
            "should be silent after release"
        );
    }

    #[test]
    fn test_process_empty_buffer_is_safe() {
        let mut synth = Synth::new();
        synth.prepare(44100.0);
        synth.process(&mut []);
    }

    #[test]
    fn test_set_gain_zero_produces_silence() {
        let mut synth = Synth::new();
        synth.prepare(44100.0);
        synth.set_gain(0.0);
        synth.note_on(69, 0.8);
        let mut buf = [0.0f32; 512];
        synth.process(&mut buf);
        assert!(
            buf.iter().all(|s| *s == 0.0),
            "gain=0 should produce silence"
        );
    }

    #[test]
    fn test_set_gain_one_produces_full_volume() {
        let mut synth = Synth::new();
        synth.prepare(44100.0);
        synth.set_gain(1.0);
        synth.note_on(69, 0.8);
        let mut buf = [0.0f32; 512];
        synth.process(&mut buf);
        assert!(
            buf.iter().any(|s| s.abs() > 0.5),
            "gain=1 should produce audible output"
        );
    }

    #[test]
    fn test_set_gain_scales_output() {
        let mut synth_full = Synth::new();
        synth_full.prepare(44100.0);
        synth_full.set_gain(1.0);
        synth_full.note_on(69, 0.8);
        let mut buf_full = [0.0f32; 512];
        synth_full.process(&mut buf_full);

        let mut synth_half = Synth::new();
        synth_half.prepare(44100.0);
        synth_half.set_gain(0.5);
        synth_half.note_on(69, 0.8);
        let mut buf_half = [0.0f32; 512];
        synth_half.process(&mut buf_half);

        for i in 0..512 {
            let expected = buf_full[i] * 0.5;
            assert!(
                (buf_half[i] - expected).abs() < 1e-6,
                "sample {}: half={} expected={}",
                i,
                buf_half[i],
                expected
            );
        }
    }

    #[test]
    fn test_oscillator_type_switching() {
        let mut synth = Synth::new();
        synth.prepare(44100.0);
        synth.note_on(69, 0.8);
        let mut buf_sine = [0.0f32; 512];
        synth.process(&mut buf_sine);

        synth.set_oscillator_type(OscillatorType::Saw);
        let mut buf_saw = [0.0f32; 512];
        synth.process(&mut buf_saw);

        assert!(
            buf_saw.iter().any(|s| s.abs() > 0.001),
            "saw should produce sound"
        );
        assert_ne!(buf_sine, buf_saw, "different waveforms should differ");
    }

    #[test]
    fn test_prepare_sets_sample_rate() {
        let mut synth = Synth::new();
        synth.prepare(48000.0);
        synth.note_on(69, 0.8);
        let mut buf = [0.0f32; 48000];
        synth.process(&mut buf);
        let crossings = buf
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();
        // 440 Hz sine = 880 crossings/sec
        assert!(
            (crossings as i32 - 880).abs() <= 4,
            "expected ~880 crossings at 48kHz, got {}",
            crossings
        );
    }
}
