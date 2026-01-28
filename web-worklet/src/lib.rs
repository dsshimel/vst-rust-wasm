use dsp_core::params::OscillatorType;
use dsp_core::Synth;
use wasm_bindgen::prelude::*;

const VIS_BUFFER_SIZE: usize = 2048;
/// AudioWorklet quantum size.
const RENDER_QUANTUM: usize = 128;

#[wasm_bindgen]
pub struct WasmSynth {
    synth: Synth,
    /// Internal audio output buffer (128 samples = 1 render quantum).
    audio_buf: Vec<f32>,
    vis_buffer: Vec<f32>,
    vis_write_pos: usize,
    vis_ready: bool,
}

#[wasm_bindgen]
impl WasmSynth {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            synth: Synth::new(),
            audio_buf: vec![0.0; RENDER_QUANTUM],
            vis_buffer: vec![0.0; VIS_BUFFER_SIZE],
            vis_write_pos: 0,
            vis_ready: false,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.synth.prepare(sample_rate);
    }

    pub fn note_on(&mut self, note: u8) {
        self.synth.note_on(note, 0.8);
    }

    pub fn note_off(&mut self, note: u8) {
        self.synth.note_off(note);
    }

    pub fn set_osc_type(&mut self, index: u32) {
        self.synth
            .set_oscillator_type(OscillatorType::from_index(index as usize));
    }

    pub fn set_gain(&mut self, v: f32) {
        self.synth.set_gain(v);
    }

    pub fn set_attack(&mut self, v: f32) {
        self.synth.set_attack(v);
    }

    pub fn set_decay(&mut self, v: f32) {
        self.synth.set_decay(v);
    }

    pub fn set_sustain(&mut self, v: f32) {
        self.synth.set_sustain(v);
    }

    pub fn set_release(&mut self, v: f32) {
        self.synth.set_release(v);
    }

    /// Process 128 samples of audio and return them as a Float32Array.
    /// wasm-bindgen converts Vec<f32> to a JS Float32Array automatically.
    pub fn process_audio(&mut self) -> Vec<f32> {
        self.synth.process(&mut self.audio_buf);

        // Accumulate samples into the visualization buffer
        for &sample in self.audio_buf.iter() {
            self.vis_buffer[self.vis_write_pos] = sample;
            self.vis_write_pos += 1;
            if self.vis_write_pos >= VIS_BUFFER_SIZE {
                self.vis_write_pos = 0;
                self.vis_ready = true;
            }
        }

        self.audio_buf.clone()
    }

    /// Returns true if a full visualization buffer is ready, then clears the flag.
    pub fn vis_ready(&mut self) -> bool {
        let ready = self.vis_ready;
        self.vis_ready = false;
        ready
    }

    /// Returns a copy of the visualization buffer as a JS-compatible Vec.
    pub fn get_vis_data(&self) -> Vec<f32> {
        self.vis_buffer.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Construction ---

    #[test]
    fn new_creates_valid_state() {
        let s = WasmSynth::new();
        assert_eq!(s.audio_buf.len(), RENDER_QUANTUM);
        assert_eq!(s.vis_buffer.len(), VIS_BUFFER_SIZE);
        assert_eq!(s.vis_write_pos, 0);
        assert!(!s.vis_ready);
    }

    #[test]
    fn constants_have_expected_values() {
        assert_eq!(VIS_BUFFER_SIZE, 2048);
        assert_eq!(RENDER_QUANTUM, 128);
    }

    #[test]
    fn audio_buf_initialized_to_zeros() {
        let s = WasmSynth::new();
        assert!(s.audio_buf.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn vis_buffer_initialized_to_zeros() {
        let s = WasmSynth::new();
        assert!(s.vis_buffer.iter().all(|&x| x == 0.0));
    }

    // --- prepare ---

    #[test]
    fn prepare_does_not_panic() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        s.prepare(48000.0);
        s.prepare(96000.0);
    }

    // --- process_audio ---

    #[test]
    fn process_audio_returns_render_quantum_samples() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        let output = s.process_audio();
        assert_eq!(output.len(), RENDER_QUANTUM);
    }

    #[test]
    fn process_audio_silent_when_no_note() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        let output = s.process_audio();
        assert!(
            output.iter().all(|&x| x == 0.0),
            "expected silence with no note playing"
        );
    }

    #[test]
    fn process_audio_produces_sound_after_note_on() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        s.note_on(60); // C4
        let output = s.process_audio();
        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.0, "expected non-silent output after note_on");
    }

    #[test]
    fn process_audio_silent_after_note_off_with_release() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        s.set_attack(0.0);
        s.set_release(0.001); // Very short release
        s.note_on(60);
        s.process_audio(); // Play a few quanta
        s.note_off(60);
        // Process enough quanta for the release to finish
        let mut last_output = vec![0.0; RENDER_QUANTUM];
        for _ in 0..100 {
            last_output = s.process_audio();
        }
        let max = last_output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(
            max < 0.001,
            "expected near-silence after release, got max={}",
            max
        );
    }

    // --- Vis buffer ring buffer ---

    #[test]
    fn vis_write_pos_advances_by_render_quantum() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        assert_eq!(s.vis_write_pos, 0);
        s.process_audio();
        assert_eq!(s.vis_write_pos, RENDER_QUANTUM);
        s.process_audio();
        assert_eq!(s.vis_write_pos, 2 * RENDER_QUANTUM);
    }

    #[test]
    fn vis_ready_false_before_buffer_full() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        // Process 15 quanta = 15 * 128 = 1920 samples, still < 2048
        for _ in 0..15 {
            s.process_audio();
        }
        assert!(!s.vis_ready, "vis_ready should be false before buffer is full");
        assert_eq!(s.vis_write_pos, 15 * RENDER_QUANTUM); // 1920
    }

    #[test]
    fn vis_ready_true_after_buffer_full() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        // Need to fill 2048 samples. 2048 / 128 = 16 quanta
        for _ in 0..16 {
            s.process_audio();
        }
        assert!(s.vis_ready, "vis_ready should be true after 16 quanta (2048 samples)");
    }

    #[test]
    fn vis_write_pos_wraps_at_buffer_size() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        // 16 quanta fills exactly 2048 samples, write_pos wraps to 0
        for _ in 0..16 {
            s.process_audio();
        }
        assert_eq!(s.vis_write_pos, 0, "vis_write_pos should wrap to 0");
    }

    #[test]
    fn vis_ready_clears_after_read() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        for _ in 0..16 {
            s.process_audio();
        }
        assert!(s.vis_ready());
        assert!(!s.vis_ready(), "vis_ready should be false after first read");
    }

    #[test]
    fn vis_ready_not_set_again_until_next_full_cycle() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        // Fill first cycle
        for _ in 0..16 {
            s.process_audio();
        }
        assert!(s.vis_ready());
        // Process a few more quanta — not enough for a full second cycle
        for _ in 0..5 {
            s.process_audio();
        }
        assert!(!s.vis_ready(), "should not be ready until next full cycle");
    }

    #[test]
    fn get_vis_data_returns_correct_size() {
        let s = WasmSynth::new();
        let data = s.get_vis_data();
        assert_eq!(data.len(), VIS_BUFFER_SIZE);
    }

    #[test]
    fn vis_data_contains_audio_after_processing() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        s.note_on(60);
        for _ in 0..16 {
            s.process_audio();
        }
        let data = s.get_vis_data();
        let max = data.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.0, "vis data should contain non-zero audio samples");
    }

    // --- Parameter setters ---

    #[test]
    fn set_osc_type_does_not_panic() {
        let mut s = WasmSynth::new();
        s.set_osc_type(0); // Sine
        s.set_osc_type(1); // Triangle
        s.set_osc_type(2); // Square
        s.set_osc_type(3); // Saw
    }

    #[test]
    fn set_gain_does_not_panic() {
        let mut s = WasmSynth::new();
        s.set_gain(0.0);
        s.set_gain(0.5);
        s.set_gain(1.0);
    }

    #[test]
    fn set_envelope_params_do_not_panic() {
        let mut s = WasmSynth::new();
        s.set_attack(0.001);
        s.set_decay(0.1);
        s.set_sustain(0.7);
        s.set_release(0.3);
    }

    #[test]
    fn different_osc_types_produce_different_waveforms() {
        let mut outputs = Vec::new();
        for osc in 0..4u32 {
            let mut s = WasmSynth::new();
            s.prepare(44100.0);
            s.set_osc_type(osc);
            s.note_on(69); // A4
            // Process a few quanta to get past the attack transient
            for _ in 0..4 {
                s.process_audio();
            }
            let out = s.process_audio();
            outputs.push(out);
        }
        // At least some pairs should differ
        let mut any_different = false;
        for i in 0..4 {
            for j in (i + 1)..4 {
                if outputs[i] != outputs[j] {
                    any_different = true;
                }
            }
        }
        assert!(any_different, "at least some oscillator types should produce different output");
    }

    #[test]
    fn gain_zero_produces_silence() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        s.set_gain(0.0);
        s.note_on(60);
        let output = s.process_audio();
        assert!(
            output.iter().all(|&x| x == 0.0),
            "gain=0 should produce silence"
        );
    }

    // --- note_on / note_off ---

    #[test]
    fn note_on_off_does_not_panic_for_all_midi_notes() {
        let mut s = WasmSynth::new();
        s.prepare(44100.0);
        for note in 0..=127u8 {
            s.note_on(note);
            s.process_audio();
            s.note_off(note);
        }
    }
}
