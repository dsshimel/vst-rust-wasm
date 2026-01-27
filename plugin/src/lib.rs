mod editor;
mod keyboard;
mod visualizer;

use dsp_core::params::OscillatorType;
use dsp_core::Synth;
use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;

/// Size of the visualization buffer (power of 2 for efficient wrapping).
pub const VIS_BUFFER_SIZE: usize = 2048;

/// Maximum number of UI note events that can be queued per process cycle.
const NOTE_QUEUE_SIZE: usize = 64;

/// Lock-free double buffer for passing audio data from the audio thread to the UI.
///
/// Design: two buffers, an atomic index indicating which one is the "front"
/// (readable by UI). The audio thread always writes to the back buffer.
/// When the back buffer is full, it atomically swaps front/back.
///
/// Safety: the audio thread is the only writer. It accesses the back buffer
/// through `push()` via `&self` using `UnsafeCell`. This is sound because
/// only one thread ever calls `push()`.
pub struct VisBuffer {
    buffers: [std::cell::UnsafeCell<[f32; VIS_BUFFER_SIZE]>; 2],
    write_pos: AtomicUsize,
    /// Which buffer index (0 or 1) the UI should read.
    front: AtomicUsize,
}

// Safety: Only the audio thread writes (via push); the UI thread only reads
// the front buffer. The atomic swap ensures they never access the same buffer
// simultaneously.
unsafe impl Sync for VisBuffer {}
unsafe impl Send for VisBuffer {}

impl VisBuffer {
    pub fn new() -> Self {
        Self {
            buffers: [
                std::cell::UnsafeCell::new([0.0; VIS_BUFFER_SIZE]),
                std::cell::UnsafeCell::new([0.0; VIS_BUFFER_SIZE]),
            ],
            write_pos: AtomicUsize::new(0),
            front: AtomicUsize::new(0),
        }
    }

    /// Called from the audio thread only. Writes a sample to the back buffer.
    /// When the buffer wraps, atomically swaps front/back so the UI sees
    /// the completed buffer.
    ///
    /// # Safety
    /// Must only be called from one thread (the audio thread).
    pub fn push(&self, sample: f32) {
        let front = self.front.load(Ordering::Relaxed);
        let back = 1 - front;
        let pos = self.write_pos.load(Ordering::Relaxed);

        // Safety: only the audio thread writes to the back buffer,
        // and the UI thread only reads the front buffer.
        unsafe {
            (*self.buffers[back].get())[pos] = sample;
        }

        let next_pos = (pos + 1) % VIS_BUFFER_SIZE;
        self.write_pos.store(next_pos, Ordering::Relaxed);

        if next_pos == 0 {
            // Back buffer is full — swap it to front
            self.front.store(back, Ordering::Release);
        }
    }

    /// Called from the UI thread. Returns a reference to the most recently
    /// completed buffer. No allocation, no copy, no lock.
    pub fn read_front(&self) -> &[f32; VIS_BUFFER_SIZE] {
        let idx = self.front.load(Ordering::Acquire);
        // Safety: the UI only reads the front buffer, the audio thread
        // only writes to the back buffer.
        unsafe { &*self.buffers[idx].get() }
    }
}

/// Lock-free SPSC note event queue (UI → audio thread).
///
/// The UI thread pushes note on/off events; the audio thread drains them
/// at the start of each process() call.
pub struct NoteQueue {
    /// Each entry: high bit = on/off (0x80 = on), low 7 bits = MIDI note.
    /// 0xFF = empty slot.
    slots: [AtomicU8; NOTE_QUEUE_SIZE],
    /// Next slot the UI thread will write to.
    write_head: AtomicUsize,
    /// Next slot the audio thread will read from.
    read_head: AtomicUsize,
}

impl NoteQueue {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| AtomicU8::new(0xFF)),
            write_head: AtomicUsize::new(0),
            read_head: AtomicUsize::new(0),
        }
    }

    /// Push a note-on event from the UI thread.
    pub fn push_note_on(&self, note: u8) -> bool {
        self.push_raw(0x80 | (note & 0x7F))
    }

    /// Push a note-off event from the UI thread.
    pub fn push_note_off(&self, note: u8) -> bool {
        self.push_raw(note & 0x7F)
    }

    fn push_raw(&self, value: u8) -> bool {
        let head = self.write_head.load(Ordering::Relaxed);
        let next = (head + 1) % NOTE_QUEUE_SIZE;
        if next == self.read_head.load(Ordering::Acquire) {
            return false; // Queue full
        }
        self.slots[head].store(value, Ordering::Release);
        self.write_head.store(next, Ordering::Release);
        true
    }

    /// Drain all pending events from the audio thread.
    pub fn drain(&self, mut callback: impl FnMut(bool, u8)) {
        loop {
            let tail = self.read_head.load(Ordering::Relaxed);
            if tail == self.write_head.load(Ordering::Acquire) {
                break;
            }
            let raw = self.slots[tail].load(Ordering::Acquire);
            let is_on = raw & 0x80 != 0;
            let note = raw & 0x7F;
            callback(is_on, note);
            self.read_head
                .store((tail + 1) % NOTE_QUEUE_SIZE, Ordering::Release);
        }
    }
}

pub struct SimpleSynth {
    params: Arc<SimpleSynthParams>,
    synth: Synth,
    vis_buffer: Arc<VisBuffer>,
    note_queue: Arc<NoteQueue>,
}

#[derive(Params)]
pub struct SimpleSynthParams {
    #[persist = "editor-state"]
    editor_state: Arc<nih_plug_egui::EguiState>,

    #[id = "osc-type"]
    pub osc_type: IntParam,

    #[id = "gain"]
    pub gain: FloatParam,

    #[id = "attack"]
    pub attack: FloatParam,

    #[id = "decay"]
    pub decay: FloatParam,

    #[id = "sustain"]
    pub sustain: FloatParam,

    #[id = "release"]
    pub release: FloatParam,
}

impl Default for SimpleSynthParams {
    fn default() -> Self {
        Self {
            editor_state: nih_plug_egui::EguiState::from_size(800, 500),

            osc_type: IntParam::new("Oscillator", 0, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| {
                    OscillatorType::from_index(v as usize).name().to_string()
                })),

            gain: FloatParam::new(
                "Gain",
                0.8,
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_unit(" ")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            attack: FloatParam::new(
                "Attack",
                0.01,
                FloatRange::Skewed {
                    min: 0.001,
                    max: 2.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_rounded(3)),

            decay: FloatParam::new(
                "Decay",
                0.1,
                FloatRange::Skewed {
                    min: 0.001,
                    max: 2.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_rounded(3)),

            sustain: FloatParam::new(
                "Sustain",
                0.7,
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            release: FloatParam::new(
                "Release",
                0.3,
                FloatRange::Skewed {
                    min: 0.001,
                    max: 5.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_rounded(3)),
        }
    }
}

impl Default for SimpleSynth {
    fn default() -> Self {
        Self {
            params: Arc::new(SimpleSynthParams::default()),
            synth: Synth::new(),
            vis_buffer: Arc::new(VisBuffer::new()),
            note_queue: Arc::new(NoteQueue::new()),
        }
    }
}

impl Plugin for SimpleSynth {
    const NAME: &'static str = "Simple Synth";
    const VENDOR: &'static str = "vst-rust-wasm";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(
            self.params.clone(),
            self.vis_buffer.clone(),
            self.note_queue.clone(),
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.synth.prepare(buffer_config.sample_rate);
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Apply parameter changes
        self.synth
            .set_oscillator_type(OscillatorType::from_index(self.params.osc_type.value() as usize));
        self.synth.set_gain(self.params.gain.value());
        self.synth.set_attack(self.params.attack.value());
        self.synth.set_decay(self.params.decay.value());
        self.synth.set_sustain(self.params.sustain.value());
        self.synth.set_release(self.params.release.value());

        // Drain UI keyboard note events (lock-free)
        self.note_queue.drain(|is_on, note| {
            if is_on {
                self.synth.note_on(note, 0.8);
            } else {
                self.synth.note_off(note);
            }
        });

        // Process MIDI events with sample-accurate timing
        let num_samples = buffer.samples();
        let mut next_event = context.next_event();
        let mut block_start = 0usize;

        while block_start < num_samples {
            let block_end = match next_event {
                Some(ref event) => {
                    let timing = event.timing() as usize;
                    if timing <= block_start {
                        match event {
                            NoteEvent::NoteOn { note, velocity, .. } => {
                                self.synth.note_on(*note, *velocity);
                            }
                            NoteEvent::NoteOff { note, .. } => {
                                self.synth.note_off(*note);
                            }
                            _ => {}
                        }
                        next_event = context.next_event();
                        continue;
                    }
                    timing.min(num_samples)
                }
                None => num_samples,
            };

            // Render audio for this block
            let block_len = block_end - block_start;
            let mut mono_buf = [0.0f32; 512];
            let mut rendered = 0;
            while rendered < block_len {
                let chunk = (block_len - rendered).min(512);
                self.synth.process(&mut mono_buf[..chunk]);

                // Write to lock-free visualization buffer
                for &s in &mono_buf[..chunk] {
                    self.vis_buffer.push(s);
                }

                // Copy mono to stereo output
                let channel_slices = buffer.as_slice();
                for i in 0..chunk {
                    let sample_idx = block_start + rendered + i;
                    let val = mono_buf[i];
                    channel_slices[0][sample_idx] = val;
                    channel_slices[1][sample_idx] = val;
                }

                rendered += chunk;
            }

            block_start = block_end;
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for SimpleSynth {
    const CLAP_ID: &'static str = "com.vst-rust-wasm.simple-synth";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A simple monophonic synthesizer with oscilloscope and spectrum visualizer");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for SimpleSynth {
    const VST3_CLASS_ID: [u8; 16] = *b"SmpSynthRustWasm";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
    ];
}

nih_export_clap!(SimpleSynth);
nih_export_vst3!(SimpleSynth);
