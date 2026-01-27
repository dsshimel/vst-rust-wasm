# Plan: Monophonic Synth VST Plugin (Rust + nih-plug + egui)

## Overview

Build a monophonic synthesizer as a native VST3/CLAP plugin using Rust. The synth
features 4 oscillator types (sine, triangle, square, saw), a piano keyboard (for
standalone/web use), and a dual-mode visualizer (oscilloscope + frequency spectrum).
The DSP engine is structured as a separate crate so it compiles to both native and
WASM targets.

## Design Decisions

- **Monophonic** (single voice) — simplest MVP
- **Keyboard input**: mouse clicks + computer keyboard (ASDF row) in standalone/web
- **FFT**: `rustfft` crate for spectrum analysis
- **Scope**: Native VST3/CLAP plugin first; DSP crate must always compile to
  `wasm32-unknown-unknown` (verified in CI/build script), but the full web app is a
  follow-up phase

## Prerequisites

- Install Rust via rustup (not currently installed)
- Add WASM target: `rustup target add wasm32-unknown-unknown`

## Project Structure

```
vst-rust-wasm/
├── Cargo.toml                  # Workspace root
├── .cargo/
│   └── config.toml             # xtask alias
├── dsp-core/                   # Pure DSP engine (no_std compatible, WASM-safe)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # Public API: Synth struct, prepare/process
│       ├── oscillator.rs       # Sine, triangle, square, saw generators
│       ├── envelope.rs         # Simple AR or ADSR envelope
│       └── params.rs           # Parameter definitions (shared source of truth)
├── plugin/                     # nih-plug VST3/CLAP wrapper + egui UI
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # Plugin trait impl, exports
│       ├── editor.rs           # egui editor (knobs, keyboard, visualizer)
│       ├── keyboard.rs         # Piano keyboard widget
│       └── visualizer.rs       # Oscilloscope + spectrum display
├── xtask/                      # Plugin bundler
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
└── initial-conversation.txt    # (existing) reference conversation
```

## Implementation Steps

### Phase 1: Scaffolding

1. **Install Rust** — `rustup` + `wasm32-unknown-unknown` target
2. **Create workspace** — Root `Cargo.toml` with members `dsp-core`, `plugin`, `xtask`
3. **Set up `.cargo/config.toml`** — xtask alias
4. **Create xtask** — Minimal bundler using `nih_plug_xtask`

### Phase 2: DSP Core (`dsp-core` crate)

5. **Oscillator module** — `oscillator.rs`
   - Phase-accumulator based oscillators
   - Enum `OscillatorType { Sine, Triangle, Square, Saw }`
   - `fn generate(phase: f32, osc_type: OscillatorType) -> f32`
   - Band-limited variants not required for MVP (naive waveforms OK)

6. **Envelope module** — `envelope.rs`
   - Simple ADSR envelope (attack, decay, sustain, release)
   - State machine: Idle → Attack → Decay → Sustain → Release → Idle
   - `fn tick(&mut self) -> f32` returns gain multiplier

7. **Synth engine** — `lib.rs`
   - `Synth` struct: owns one oscillator + one envelope
   - `fn prepare(sample_rate: f32)`
   - `fn note_on(note: u8, velocity: f32)`
   - `fn note_off()`
   - `fn process(output: &mut [f32])` — fills a buffer with mono samples
   - No heap allocations in `process`

8. **Parameter definitions** — `params.rs`
   - `OscillatorType` selection
   - Gain/volume
   - Attack, Decay, Sustain, Release
   - (These are defined here so both native plugin and future web app share them)

9. **WASM compile check** — Verify `cargo build -p dsp-core --target wasm32-unknown-unknown` succeeds

### Phase 3: Plugin Wrapper (`plugin` crate)

10. **Plugin struct + nih-plug boilerplate** — `lib.rs`
    - Implement `Plugin`, `ClapPlugin`, `Vst3Plugin` traits
    - `MIDI_INPUT = MidiConfig::Basic`
    - No main input channels (instrument), stereo output
    - Forward MIDI note on/off → `dsp-core::Synth`
    - Copy mono DSP output to both stereo channels

11. **egui editor shell** — `editor.rs`
    - `create_egui_editor()` with ~800x500 window
    - Layout: top = oscillator selector + ADSR knobs, middle = visualizer, bottom = keyboard
    - Share audio data with UI via `Arc<Mutex<RingBuffer>>` or lock-free ring buffer for visualizer

12. **Visualizer widget** — `visualizer.rs`
    - Ring buffer of recent audio samples (e.g., 2048 samples)
    - **Oscilloscope mode**: draw waveform as a polyline on egui painter
    - **Spectrum mode**: run `rustfft` on the buffer, plot magnitude as bars/line
    - Toggle button to switch modes

13. **Piano keyboard widget** — `keyboard.rs`
    - Draw 2-octave piano (C3–B4 or similar) using egui painter
    - Mouse click on keys sends note_on/note_off to synth
    - Computer keyboard mapping (ASDF row = white keys, WER row = black keys)
    - Visual feedback: highlight pressed keys

### Phase 4: Build & Test

14. **Build plugin** — `cargo xtask bundle plugin --release`
15. **Verify WASM** — `cargo build -p dsp-core --target wasm32-unknown-unknown`
16. **Manual test** — Load VST3/CLAP in a DAW, play MIDI notes, verify audio + UI

## Key Dependencies

| Crate | Purpose | Used in |
|-------|---------|---------|
| `nih_plug` (git) | VST3/CLAP plugin framework | plugin |
| `nih_plug_egui` (git) | egui integration for nih-plug | plugin |
| `rustfft` | FFT for spectrum visualizer | plugin |
| (none beyond core) | DSP engine is dependency-light | dsp-core |

## Architecture Notes

- **dsp-core** has zero nih-plug dependency — it's a pure Rust library that takes
  note events and produces audio samples. This is what will compile to WASM later.
- **plugin** depends on both `dsp-core` and `nih_plug`/`nih_plug_egui`.
- The visualizer reads audio data from a ring buffer that the audio thread writes to.
  We use a lock-free single-producer single-consumer ring buffer to avoid blocking the
  audio thread.
- Parameter smoothing is handled by nih-plug's built-in `Smoother` for the plugin
  params, but the DSP core's envelope is self-contained.

## Future Work (not in this MVP)

- Web app: AudioWorklet + WASM + egui-web or React UI
- Polyphony (multiple voices)
- More oscillator types / wavetable
- Effects (filter, reverb, delay)
- Preset system
