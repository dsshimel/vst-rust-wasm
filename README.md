# Simple Synth

A monophonic synthesizer built in Rust, targeting both native DAW plugins (VST3/CLAP) and WebAssembly.

## Features

- 4 oscillator types: sine, triangle, square, saw
- ADSR envelope (attack, decay, sustain, release)
- Dual-mode visualizer: oscilloscope (waveform) and frequency spectrum (FFT)
- 2-octave piano keyboard with mouse and computer keyboard input
- MIDI input support (NoteOn/NoteOff)

## Project Structure

```
vst-rust-wasm/
├── dsp-core/       # Pure Rust DSP engine (no dependencies, WASM-safe)
├── synth-ui/       # Shared egui widgets (keyboard, visualizer, layout)
├── plugin/         # nih-plug VST3/CLAP wrapper + egui GUI
├── web/            # eframe web app (egui rendered to <canvas>)
├── web-worklet/    # AudioWorklet WASM module (runs dsp-core in browser)
└── xtask/          # Plugin bundler
```

`dsp-core` contains the audio engine with zero external dependencies. It compiles to
both native and `wasm32-unknown-unknown`, making it the shared core for the native
plugin and the web app.

`synth-ui` contains shared egui widgets (piano keyboard, visualizer, parameter layout)
with a `ControlRenderer` trait that abstracts parameter rendering. The native plugin
implements it with nih-plug's `ParamSlider`, while the web app uses plain egui sliders.

`plugin` wraps `dsp-core` with [nih-plug](https://github.com/robbert-vdh/nih-plug) for
DAW integration and [egui](https://github.com/emilk/egui) for the GUI.

`web` is an [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) web
application that renders the same egui UI to an HTML `<canvas>` element.

`web-worklet` compiles `dsp-core` to a small WASM module that runs inside a Web Audio
`AudioWorkletProcessor` for real-time audio in the browser.

## Prerequisites

- [Rust](https://rustup.rs/) (1.80+)
- WASM target:
  ```
  rustup target add wasm32-unknown-unknown
  ```
- For the web app: [trunk](https://trunkrs.dev/) and [wasm-pack](https://rustwasm.github.io/wasm-pack/):
  ```
  cargo install trunk wasm-pack
  ```

### What is `wasm32-unknown-unknown`?

Rust target triples follow the format `<arch>-<vendor>-<os>`. For this target:

- **wasm32** — 32-bit WebAssembly architecture
- **unknown** (vendor) — no specific vendor
- **unknown** (OS) — no specific operating system

This produces a pure `.wasm` binary with no assumptions about the host environment
(browser, Node.js, etc.). The host provides the runtime at load time. This is the
standard target for portable WASM modules, as opposed to `wasm32-wasi` which assumes
a POSIX-like system interface. Since our DSP engine just processes float buffers, it
needs no OS services and `unknown-unknown` is the correct choice.

### Why not `wasm64`?

The 32 vs 64 in `wasm32`/`wasm64` refers to **pointer and address size**, not numeric
precision. Both targets compute with the same IEEE 754 `f32` and `f64` floating-point
types — switching to wasm64 would not improve sound quality. Audio DSP universally uses
`f32` (32-bit float), which provides ~150 dB of dynamic range, far exceeding human
hearing (~120 dB). Professional DAWs like Ableton also process at 32-bit float.

What actually affects sound quality is sample rate (set by the host), band-limited
oscillator algorithms (this project uses PolyBLEP to reduce aliasing), and
oversampling. `wasm64` (the "memory64" proposal) exists in the WebAssembly spec but
has limited browser support and is unnecessary for audio workloads.

## Building

### Native plugin (VST3 + CLAP)

```
cargo xtask bundle plugin --release
```

Output files:
- `target/bundled/plugin.vst3`
- `target/bundled/plugin.clap`

### Standalone (no DAW needed)

```
cargo run -p plugin --bin simple-synth-standalone --release
```

### DSP core only (for development/testing)

```
cargo build -p dsp-core
```

### Web app (development)

Build the AudioWorklet WASM module, then build and serve the web app:

```
wasm-pack build web-worklet --target no-modules --out-dir ../target/web-dist/worklet-pkg
cd web
trunk serve --port 8080
```

Open `http://127.0.0.1:8080/` in your browser and click **Start Audio**.

### Web app (production)

Build a release version for static hosting:

```
wasm-pack build web-worklet --target no-modules --out-dir ../target/web-dist/worklet-pkg
cd web
trunk build --release
```

The output in `web/dist/` is a self-contained static site ready to deploy to any
static host. `web/Trunk.toml` sets `public_url` to control the base path for asset
URLs — adjust this to match your deployment path (e.g. `/synth/` if hosting at
`example.com/synth`).

### Verify WASM compilation

```
cargo build -p dsp-core --target wasm32-unknown-unknown
```

### Running tests

```
cargo test --workspace
```

## Installation

### VST3

Copy `target/bundled/plugin.vst3` to your system's VST3 directory:

| OS      | Path                                        |
|---------|---------------------------------------------|
| Windows | `C:\Program Files\Common Files\VST3\`       |
| macOS   | `~/Library/Audio/Plug-Ins/VST3/`            |
| Linux   | `~/.vst3/`                                  |

### CLAP

Copy `target/bundled/plugin.clap` to your CLAP directory:

| OS      | Path                                        |
|---------|---------------------------------------------|
| Windows | `C:\Program Files\Common Files\CLAP\`       |
| macOS   | `~/Library/Audio/Plug-Ins/CLAP/`            |
| Linux   | `~/.clap/`                                  |

After copying, rescan plugins in your DAW. The plugin appears as **Simple Synth**.

## Usage

### In a DAW

Load Simple Synth as an instrument plugin. Send MIDI notes to it from a MIDI track
or a connected MIDI controller.

### GUI controls

- **Oscillator** — select waveform type (Sine, Triangle, Square, Saw)
- **Gain** — output volume (0.0 to 1.0)
- **Attack / Decay / Sustain / Release** — ADSR envelope parameters
- **Visualizer** — toggle between Oscilloscope and Spectrum modes
- **Piano keyboard** — click keys with the mouse, or use the computer keyboard:

| Key | Note | Key | Note |
|-----|------|-----|------|
| A   | C3   | W   | C#3  |
| S   | D3   | E   | D#3  |
| D   | E3   | T   | F#3  |
| F   | F3   | Y   | G#3  |
| G   | G3   | U   | A#3  |
| H   | A3   | O   | C#4  |
| J   | B3   |     |      |
| K   | C4   |     |      |
| L   | D4   |     |      |

## License

VST3 bindings in nih-plug are GPLv3. CLAP has no licensing restrictions. See
[nih-plug's licensing notes](https://github.com/robbert-vdh/nih-plug#licensing) for
details.
