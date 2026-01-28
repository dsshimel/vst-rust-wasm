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
