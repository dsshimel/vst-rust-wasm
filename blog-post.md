# Building a Cross-Platform Synthesizer in Rust: From VST Plugin to WebAssembly

*How we built a monophonic synthesizer that runs as a native DAW plugin (VST3/CLAP), a standalone desktop app, and a browser-based web app — all from a single Rust codebase.*

---

## The Goal

Build a synthesizer once in Rust and ship it everywhere: inside Ableton Live as a VST3 plugin, as a standalone desktop application, and in the browser via WebAssembly. The same DSP engine, the same UI widgets, the same sound — three deployment targets from one codebase.

This post covers the architecture, the interesting problems we hit (an OpenGL call that froze Ableton, AudioWorklet environments that lack basic Web APIs, a JS type system quirk that broke our visualizer), and how we solved each one.

---

## Architecture Overview

### High-Level Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Shared Rust Codebase                        │
│                                                                     │
│  ┌─────────────┐    ┌──────────────┐                                │
│  │  dsp-core    │    │   synth-ui   │                                │
│  │             │    │              │                                │
│  │ Oscillator  │    │ Keyboard     │                                │
│  │ Envelope    │    │ Visualizer   │                                │
│  │ Synth       │    │ Layout       │                                │
│  │ Params      │    │ ControlRend. │                                │
│  └──────┬──────┘    └──────┬───────┘                                │
│         │                  │                                        │
│   ┌─────┴──────────────────┴─────┐                                  │
│   │    Used by all three targets │                                  │
│   └──┬──────────┬────────────┬───┘                                  │
│      │          │            │                                      │
│  ┌───┴───┐  ┌──┴────┐  ┌───┴────────┐                              │
│  │plugin │  │  web   │  │web-worklet │                              │
│  │       │  │        │  │            │                              │
│  │nih-   │  │eframe  │  │WasmSynth  │                              │
│  │plug   │  │app     │  │AudioWork- │                              │
│  │VST3/  │  │WebAudio│  │let WASM   │                              │
│  │CLAP   │  │bridge  │  │module     │                              │
│  └───┬───┘  └──┬─────┘  └───┬───────┘                              │
│      │         │             │                                      │
└──────┼─────────┼─────────────┼──────────────────────────────────────┘
       │         │             │
       ▼         ▼             ▼
   ┌───────┐  ┌──────┐  ┌──────────┐
   │Ableton│  │Browser│  │AudioWork-│
   │/ DAW  │  │  UI   │  │let Thread│
   └───────┘  └──────┘  └──────────┘
```

The architecture follows a strict layering principle:

- **`dsp-core`** — Pure Rust audio engine with zero dependencies. Compiles to both native and `wasm32-unknown-unknown`. Contains the oscillator (with PolyBLEP antialiasing), ADSR envelope, and the main `Synth` struct.
- **`synth-ui`** — Shared [egui](https://github.com/emilk/egui) widgets: piano keyboard, oscilloscope/spectrum visualizer, and the parameter layout. Defines a `ControlRenderer` trait that abstracts how parameter sliders are rendered.
- **`plugin`** — Wraps `dsp-core` with [nih-plug](https://github.com/robbert-vdh/nih-plug) for DAW integration. Implements `ControlRenderer` using nih-plug's `ParamSlider` (which handles DAW automation, undo, etc.).
- **`web`** — An [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) web application. Implements `ControlRenderer` with plain egui sliders and dirty flags for parameter syncing.
- **`web-worklet`** — A thin wrapper around `dsp-core::Synth` that compiles to a WASM module and runs inside a Web Audio `AudioWorkletProcessor`.

The key insight: `dsp-core` needs no OS services. It just processes float buffers. This makes it trivially portable to any target that supports `f32` arithmetic — which is everything.

---

## The Native VST Path

### Native Plugin Architecture

```
┌─────────────────── DAW (Ableton Live) ───────────────────────────┐
│                                                                   │
│  ┌─── Audio Thread ────────────────┐  ┌─── UI Thread ──────────┐ │
│  │                                 │  │                         │ │
│  │  SimpleSynth (nih-plug Plugin)  │  │  egui Editor            │ │
│  │  ┌───────────┐                  │  │  ┌─────────────┐        │ │
│  │  │ dsp-core  │  process()       │  │  │  synth-ui   │        │ │
│  │  │ ::Synth   ├──────────────►   │  │  │  Keyboard   │        │ │
│  │  └─────┬─────┘  audio buffers   │  │  │  Visualizer ◄──┐    │ │
│  │        │                        │  │  │  Params     │   │    │ │
│  │        │ ▲ MIDI NoteOn/Off      │  │  └──────┬──────┘   │    │ │
│  │        │ │                      │  │         │          │    │ │
│  │        │ ┌────────────┐         │  │  ┌──────┴──────┐   │    │ │
│  │        │ │ NoteQueue  │◄────────┼──┼──│ Keyboard    │   │    │ │
│  │        │ │ (lock-free │         │  │  │ Events      │   │    │ │
│  │        │ │  ring buf) │         │  │  └─────────────┘   │    │ │
│  │        │ └────────────┘         │  │                    │    │ │
│  │        │ samples                │  │  ┌─────────────┐   │    │ │
│  │        ▼                        │  │  │VisBuffer    │   │    │ │
│  │  ┌─────────────┐               │  │  │(read front) ├───┘    │ │
│  │  │ VisBuffer   ├───────────────►┼──┼──│             │        │ │
│  │  │ (push back) │  vis samples   │  │  └─────────────┘        │ │
│  │  └─────────────┘                │  │                         │ │
│  └─────────────────────────────────┘  └─────────────────────────┘ │
│                                                                   │
│    ◄── Lock-free communication: no mutexes cross threads ──►      │
└───────────────────────────────────────────────────────────────────┘
```

Audio plugins have a strict rule: **the audio thread must never block.** No mutexes, no allocations, no syscalls. Our audio thread runs `dsp-core::Synth::process()`, which fills a buffer of `f32` samples by ticking the oscillator and envelope once per sample:

```rust
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
```

Zero allocations, deterministic, takes a pre-allocated `&mut [f32]` slice. The same function runs identically on native and in WebAssembly.

### Lock-Free Communication

Two custom data structures handle cross-thread communication without locks:

**`VisBuffer`** — A double-buffered visualization pipeline. The audio thread writes samples to a back buffer. When the buffer fills (2048 samples), it atomically swaps the front/back index with `AtomicUsize`. The UI thread reads the front buffer without any synchronization beyond an atomic load.

```rust
pub struct VisBuffer {
    buffers: [UnsafeCell<[f32; 2048]>; 2],
    write_pos: AtomicUsize,
    front: AtomicUsize,
}
```

**`NoteQueue`** — A single-producer, single-consumer ring buffer. The UI thread pushes note events (from the on-screen keyboard or computer keyboard input), and the audio thread drains them at the start of each process block. Each slot is a single `AtomicU8` where the high bit encodes on/off and the low 7 bits encode the MIDI note number.

```rust
pub struct NoteQueue {
    slots: [AtomicU8; 64],
    write_head: AtomicUsize,
    read_head: AtomicUsize,
}
```

### The `ControlRenderer` Trait

Both the native plugin and the web app need to render the same set of parameter controls (oscillator type, gain, ADSR), but with different slider implementations. The `ControlRenderer` trait abstracts this:

```rust
pub trait ControlRenderer {
    fn render_osc_type(&mut self, ui: &mut egui::Ui);
    fn render_gain(&mut self, ui: &mut egui::Ui);
    fn render_attack(&mut self, ui: &mut egui::Ui);
    fn render_decay(&mut self, ui: &mut egui::Ui);
    fn render_sustain(&mut self, ui: &mut egui::Ui);
    fn render_release(&mut self, ui: &mut egui::Ui);
}
```

The native plugin implements this with nih-plug's `ParamSlider`, which automatically integrates with DAW automation, undo/redo, and parameter hosting. The web app implements it with plain egui `Slider` widgets plus a `DirtyFlags` struct that tracks which parameters changed each frame, so only modified values get sent to the audio worklet.

### The Standalone Target

The same `SimpleSynth` struct also runs as a standalone desktop app — no DAW required. In the VST path, the DAW owns the audio thread and hands our plugin an output buffer to fill with samples each callback. nih-plug's `standalone` feature replaces this: [CPAL](https://github.com/RustAudio/cpal) opens an audio device, spawns its own audio thread, and provides the output buffer instead. From `SimpleSynth`'s perspective, `process()` still receives a `&mut Buffer` to fill — it doesn't know or care whether a DAW or CPAL allocated it. The architecture is identical — same two-thread model, same lock-free `VisBuffer` and `NoteQueue`, same egui editor. The only difference is what sits below: CPAL instead of a DAW.

The one complication was device selection. On a machine with a Realtek device configured for 7.1 surround, CPAL reported **8 channels**, but our synth outputs 2 (stereo). nih-plug's CPAL backend requires an exact channel count match, so every configuration was rejected. The fix: enable CPAL's `asio` feature (ASIO drivers typically expose stereo I/O regardless of physical speaker configuration), patch nih-plug to expose ASIO as a backend option, and write a custom launcher in `main.rs` that auto-detects a working device. The launcher tries ASIO first (lower latency, no channel mismatch), scores devices to prefer dedicated hardware over wrappers like ASIO4ALL, and falls back to WASAPI. A `--probe` flag dumps every host, device, and supported config range for debugging audio on unfamiliar machines.

---

## The Web Path

### Web Architecture

```
┌──────────────────── Browser ──────────────────────────────────────┐
│                                                                   │
│  ┌──── Main Thread ───────────────────────────────────────────┐   │
│  │                                                             │   │
│  │  eframe WebRunner (requestAnimationFrame loop)             │   │
│  │    │ calls update() each frame                              │   │
│  │    ▼                                                        │   │
│  │  ┌──────────────────────────────────────┐                  │   │
│  │  │ SynthWebApp (impl eframe::App)       │                  │   │
│  │  │  ┌───────────┐  ┌────────────────┐   │                  │   │
│  │  │  │ synth-ui  │  │ WebControls    │   │                  │   │
│  │  │  │ Keyboard  │  │ (ControlRend.) │   │                  │   │
│  │  │  │ Visualizer│  │ DirtyFlags     │   │                  │   │
│  │  │  └───────────┘  └────────────────┘   │                  │   │
│  │  └──────────┬───────────────────────────┘                  │   │
│  │             │ notes, params ↓  ↑ vis samples               │   │
│  │  ┌──────────┴───────────────────────────┐                  │   │
│  │  │ AudioBridge                          │                  │   │
│  │  │  ┌─────────────────────────────────┐ │                  │   │
│  │  │  │ AudioContext                    │ │                  │   │
│  │  │  │  AudioWorkletNode              │ │                  │   │
│  │  │  │         ↓                       │ │                  │   │
│  │  │  │  context.destination (speakers) │ │                  │   │
│  │  │  └─────────────────────────────────┘ │                  │   │
│  │  └───────────────────┬──────────────────┘                  │   │
│  │                      │ MessagePort (bidirectional)          │   │
│  │                      │  ↓ init, noteOn/Off, params          │   │
│  │                      │  ↑ ready, Float32Array vis data      │   │
│  └──────────────────────┼─────────────────────────────────────┘   │
│                         │                                         │
│  ┌──────────────────────┼──── AudioWorklet Thread ────────────┐   │
│  │                      ▼                                     │   │
│  │  ┌─────────────────────────────────────────────────────┐   │   │
│  │  │ worklet-processor.js                                │   │   │
│  │  │                                                     │   │   │
│  │  │  WebAssembly.Module (web-worklet crate)             │   │   │
│  │  │  ┌───────────────────────────────────────────┐      │   │   │
│  │  │  │ WasmSynth                                 │      │   │   │
│  │  │  │  ┌─────────┐  ┌───────────┐  ┌─────────┐ │      │   │   │
│  │  │  │  │dsp-core │  │ vis_buffer│  │audio_buf│ │      │   │   │
│  │  │  │  │ ::Synth │  │ (2048)    │  │ (128)   │ │      │   │   │
│  │  │  │  └─────────┘  └───────────┘  └────┬────┘ │      │   │   │
│  │  │  └───────────────────────────────────┼──────┘      │   │   │
│  │  │                                      │             │   │   │
│  │  │  process() called every 128 samples  │ (~2.9ms)    │   │   │
│  │  │                    ┌─────────────────┘              │   │   │
│  │  │                    ▼                                │   │   │
│  │  │  Web Audio output buffer ──► AudioContext graph     │   │   │
│  │  │  (128 samples written directly, never via port)     │   │   │
│  │  └─────────────────────────────────────────────────────┘   │   │
│  └────────────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────────────┘
```

The web version has a fundamentally different audio architecture than the native plugin. In a DAW, the host calls your `process()` function on its audio thread. In the browser, you set up a [Web Audio API](https://developer.mozilla.org/en-US/docs/Web/API/Web_Audio_API) graph and the browser calls your `AudioWorkletProcessor.process()` method on a dedicated real-time thread.

The main thread runs the eframe/egui UI (compiled to WASM via [Trunk](https://trunkrs.dev/)) and renders to a `<canvas>` element. The `AudioBridge` struct manages the Web Audio pipeline: it creates an `AudioContext` (the browser's audio session — equivalent to opening a CPAL device in the standalone path), registers our `worklet-processor.js` script via `audioWorklet.addModule()`, then creates an `AudioWorkletNode` inside that context and connects it to `context.destination()` (the speakers). The `AudioContext` owns the audio graph — the `AudioWorkletNode` is a node within it, wired to the destination output. Creating the node gives us a `MessagePort` — the only communication channel between the main thread and the worklet thread. The bridge then fetches the `web-worklet` WASM bytes, transfers them to the worklet via `postMessage`, and exposes methods like `send_note_on()`, `send_note_off()`, and `send_param()` that post JSON messages through the same port.

The worklet thread runs a second, separate WASM module (`web-worklet` crate) that wraps `dsp-core::Synth` and processes 128 samples per callback (the Web Audio "render quantum"). The browser provides an output buffer to `process()` — the same pattern as a DAW providing `&mut Buffer` in the native path. The worklet writes 128 samples directly into that browser-provided buffer, which flows through the audio graph to the speakers. The hot-path audio never crosses the `MessagePort`.

Visualization data does go through the port: the `WasmSynth` wrapper accumulates audio samples into a 2048-sample ring buffer, and when it fills, the worklet sends the `Float32Array` back to the main thread via `postMessage` with a [transferable buffer](https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Transferable_objects) (zero-copy transfer of ownership). So the `MessagePort` carries only control messages (down) and occasional vis snapshots (up) — the real-time audio stays entirely within the worklet thread.

---

## Interesting Problems and How We Solved Them

### Problem 1: OpenGL `SwapBuffers()` Freezing Ableton

This was the hardest bug in the entire project, and the one that taught us the most about how DAW plugin hosting actually works.

**The symptom:** Loading our VST3 plugin in Ableton Live would render exactly one frame of the UI, then the entire DAW would freeze. No crash dialog, no error — just a hang that required force-killing the process.

**The investigation:** Since nih-plug logs to `OutputDebugString` on Windows (invisible without a debugger attached), and Ableton swallows stderr, we had no way to see what was happening. We built a custom file-based debug logger that wrote timestamped messages directly to disk:

```rust
pub(crate) mod filelog {
    use std::io::Write;
    use std::sync::Mutex;
    static LOG_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);
    // Opens on first write, appends with timestamps
}
```

The logs told the story: frame 1 rendered successfully, frame 2 never started. We added more instrumentation inside egui-baseview's OpenGL renderer and narrowed it down to a single function call:

```
on_frame START frame=1
frame=1 calling user_update
frame=1 user_update done
frame=1 tessellated N primitives
frame=1 paint_primitives done
frame=1 entering swap_buffers
                                    ← never returns
```

`context.swap_buffers()` entered and never came back.

**The root cause:** VST3 plugin editors are *child windows* — they're embedded inside the DAW's window hierarchy. On Windows, the DWM (Desktop Window Manager) compositor may not deliver vsync signals to OpenGL contexts on non-top-level windows. With `vsync: true` in the GL config, `SwapBuffers()` waits for a vblank signal that never arrives.

We tried the obvious fix first: set `vsync: false` to call `wglSwapIntervalEXT(0)`. It didn't work — baseview loaded the WGL extension function from a temporary context during pixel format selection, and calling it on a different HDC silently failed on some GPU drivers.

**How JUCE handles this:** We researched how [JUCE](https://juce.com/) (the dominant C++ audio plugin framework) solves the same problem. JUCE runs OpenGL rendering on a **dedicated background thread**, not the UI message thread. Even if `SwapBuffers()` blocks, only the render thread stalls — the UI message pump keeps running. This is a more robust solution but requires significant threading infrastructure.

**Our fix:** JUCE's solution works *around* the blocking call — it still calls `SwapBuffers()`, but isolates the damage on a background thread. We took a different approach: instead of containing the block, we eliminated the reason it blocks in the first place. JUCE needs that threading infrastructure anyway as a general-purpose framework, but for us it would have been massive overengineering to work around one call we could simply remove.

1. Requested a **single-buffered** OpenGL pixel format (`double_buffer: false`). With no back buffer, there's nothing to swap — the vsync-dependent code path is never entered.
2. Replaced `context.swap_buffers()` with `glow_context.flush()`. All rendering goes directly to the visible framebuffer, and `glFlush()` submits pending GL commands to the GPU without blocking. The DWM compositor picks up the updated content on its next composition pass.

```rust
// BEFORE: blocks indefinitely in VST3 child windows
context.swap_buffers();

// AFTER: non-blocking, works as child window
self.glow_context.flush();
```

This required patching our local forks of both `baseview` and `egui-baseview`. It doesn't affect the web target at all — WebGL has a completely different presentation model where the browser handles compositing automatically.

---

### Problem 2: No Audio from the Standalone App (WASAPI Channel Mismatch)

The standalone binary (which uses [CPAL](https://github.com/RustAudio/cpal) for audio output) refused to produce sound:

```
[ERROR] Could not initialize either the JACK or the WASAPI backends,
falling back to the dummy audio backend: The audio output device does
not support 2 audio channels at a sample rate of 48000 Hz
```

The user's Realtek audio device reported **8 channels** (7.1 surround), but our synth outputs 2 channels (stereo). nih-plug's CPAL backend requires an exact channel count match. Every sample rate and buffer size combination was rejected.

**The fix** involved three layers:

1. **ASIO support:** The user had [ASIO4ALL](https://www.asio4all.org/) and a Focusrite audio interface. We enabled CPAL's `asio` feature, which required installing LLVM/Clang for C++ FFI bindings to the Steinberg ASIO SDK.

2. **nih-plug patching:** nih-plug's standalone wrapper didn't expose ASIO as a backend option. We added an `Asio` variant to the backend enum and included it in the auto-detection fallback chain.

3. **Smart device probing:** We wrote a custom launcher in `main.rs` that queries all available audio hosts and devices, finds a working configuration (matching sample rate and channel count), and passes the correct `--backend`, `--output-device`, `--sample-rate`, and `--period-size` arguments to nih-plug's standalone entry point.

---

### Problem 3: The AudioWorklet WASM Initialization Saga

Getting WebAssembly running inside a Web Audio `AudioWorkletProcessor` required solving three problems in sequence, each revealed only after the previous one was fixed.

#### Round 1: WebAssembly.Module Silently Dropped

We compiled the WASM bytes into a `WebAssembly.Module` on the main thread and sent it to the worklet via `postMessage`. The worklet's `handleMessage` callback never fired for the init message — it was silently dropped.

`WebAssembly.Module` is supposed to be transferable via structured clone, but in practice, the transfer to an `AudioWorkletGlobalScope` silently failed in our testing. The fix: send the raw `ArrayBuffer` of WASM bytes instead, using the transferable mechanism for zero-copy transfer:

```rust
// In audio_bridge.rs
let transfer = js_sys::Array::new();
transfer.push(&array_buffer);
port.post_message_with_transferable(&init_msg, &transfer)?;
```

The worklet compiles the module itself from the received bytes.

#### Round 2: `importScripts is not defined`

With the bytes arriving correctly, the worklet tried to load the wasm-bindgen JavaScript glue code. wasm-bindgen's `--target no-modules` output uses `importScripts()` to load itself. But `AudioWorkletGlobalScope` is **not** a regular Worker — it doesn't support `importScripts()`.

We tried a workaround: fetch the JS glue as text on the main thread, send it to the worklet, and evaluate it with `new Function(...)`. This got past `importScripts` but immediately hit the next wall.

#### Round 3: `TextDecoder is not defined`

The wasm-bindgen glue code uses `TextDecoder` to decode strings from WASM linear memory (for error messages, panic info, etc.). `AudioWorkletGlobalScope` doesn't have `TextDecoder` either.

At this point, we abandoned the wasm-bindgen JS glue entirely and wrote a minimal hand-crafted WASM instantiation directly in `worklet-processor.js`. The wasm-bindgen runtime only requires two imports to function — a panic handler and an externref table initializer:

```javascript
const imports = {
    "./web_worklet_bg.js": {
        __wbg___wbindgen_throw_be289d5034ed271b: (ptr, len) => {
            throw new Error("WASM panic (ptr=" + ptr + ", len=" + len + ")");
        },
        __wbindgen_init_externref_table: () => {
            const table = instance.exports.__wbindgen_externrefs;
            if (table) {
                const offset = table.grow(4);
                table.set(0, undefined);
                table.set(offset + 0, undefined);
                table.set(offset + 1, null);
                table.set(offset + 2, true);
                table.set(offset + 3, false);
            }
        },
    },
};

const wasmModule = new WebAssembly.Module(wasmBytes);
const instance = new WebAssembly.Instance(wasmModule, imports);
this.wasm = instance.exports;
```

No `TextDecoder`, no `importScripts`, no eval of generated code. Just two import functions, direct module compilation, and raw calls to WASM exports by name (`wasmsynth_new`, `wasmsynth_process_audio`, etc.).

**Reading multi-value returns:** wasm-bindgen encodes `Vec<f32>` returns as `[pointer, length]` pairs. We read them by viewing WASM linear memory as a `Float32Array`, copying the data, and freeing the WASM-side allocation:

```javascript
readF32Array(retVal) {
    const ptr = retVal[0] >>> 0;
    const len = retVal[1] >>> 0;
    const f32 = new Float32Array(this.wasm.memory.buffer, ptr, len);
    const copy = new Float32Array(f32);
    this.wasm.__wbindgen_free(ptr, len * 4, 4);
    return copy;
}
```

---

### Problem 4: The Visualizer That Saw Everything as an Object

After getting audio working in the browser, the oscilloscope showed a flat line. Debug logs revealed hundreds of messages like:

```
vis callback: got object message type=''
```

The worklet was sending `Float32Array` visualization data, and the main thread was receiving it — but misidentifying it.

**The root cause** was a classic JavaScript type system gotcha. In our Rust/wasm-bindgen callback, we checked message types in this order:

```rust
// 1. Try to interpret as a JS Object (for structured messages like {type: "ready"})
if let Ok(obj) = msg.dyn_into::<js_sys::Object>() {
    let msg_type = Reflect::get(&obj, &"type".into())...
    // Handle based on msg_type
}
// 2. Try to interpret as Float32Array (for vis data)
if let Ok(arr) = msg.dyn_into::<js_sys::Float32Array>() {
    // Handle vis data
}
```

The problem: in JavaScript, **`Float32Array` is an `Object`**. Every typed array inherits from `Object`. So `dyn_into::<js_sys::Object>()` succeeded on the Float32Array data, read its `.type` property (which doesn't exist on typed arrays — returns `undefined`, converted to empty string `""`), and returned early. The Float32Array handler was never reached.

**The fix:** Check for specific types before generic ones:

```rust
if msg.is_instance_of::<js_sys::Float32Array>() {
    // Handle vis data — check this FIRST
} else if msg.is_instance_of::<js_sys::ArrayBuffer>() {
    // Handle raw buffer
} else if let Ok(obj) = msg.dyn_into::<js_sys::Object>() {
    // Handle structured messages — check this LAST
}
```

A simple ordering bug, but one that produced confusing symptoms: the data was arriving, the callback was firing, but the wrong branch was handling it.

---

### Problem 5: The Dark Theme That Wouldn't Stick

The native VST had a dark theme (matching Ableton's aesthetic), but the web version appeared with egui's default light theme despite calling `ctx.set_visuals(egui::Visuals::dark())` during app construction.

**Root cause:** eframe's web backend resets or overrides visuals set during `App::new()`. The dark theme applied in the constructor was silently reverted by the framework before the first frame rendered.

**The fix:** Apply dark visuals on every frame in `update()`, not just during initialization:

```rust
fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    ctx.set_visuals(egui::Visuals::dark());
    // ... rest of UI
}
```

This bit us a second time during cleanup: when we removed debug logging code, we accidentally removed the `frame_count` variable that gated the dark theme re-application, reverting the app to a light theme. The unconditional call at the top of `update()` is the robust solution.

---

## DSP Design: PolyBLEP Antialiasing

A quick note for readers unfamiliar with audio DSP. Digital synthesizers generate waveforms by computing samples at a fixed rate (e.g., 44,100 samples per second). Simple waveforms like square and sawtooth waves have instantaneous discontinuities — vertical jumps in the signal. These jumps contain infinite frequency content, but our digital system can only represent frequencies up to half the sample rate (the [Nyquist frequency](https://en.wikipedia.org/wiki/Nyquist_frequency)). The unrepresentable frequencies fold back as audible artifacts called *aliasing* — a metallic, buzzy distortion.

The traditional fix is *oversampling*: generate at 2x or 4x the sample rate, apply a low-pass filter, then downsample. This works but costs 2-4x the CPU.

Our synth uses [PolyBLEP](https://en.wikipedia.org/wiki/BLIT#PolyBLEP) (Polynomial Band-Limited Step), a more elegant approach. Instead of oversampling, PolyBLEP applies a small polynomial correction to the samples immediately surrounding each discontinuity. The correction smooths the transition just enough to suppress aliasing while preserving the waveform's character:

```rust
fn polyblep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}
```

The function takes `t` (current phase, 0.0 to 1.0) and `dt` (phase increment per sample, proportional to frequency). It only applies a correction when `t` is within one sample of a discontinuity — the rest of the time it returns 0.0 and the naive waveform passes through unchanged. This makes it nearly free in CPU cost.

For the **triangle wave**, we use an integration trick: a PolyBLEP'd square wave is integrated sample-by-sample with a leaky integrator, producing a smooth triangle with minimal aliasing. The leaky integrator prevents DC offset drift.

---

## Testing Strategy

We ended up with 144 tests across the workspace:

| Crate | Tests | What's Covered |
|-------|-------|----------------|
| `dsp-core` | 50 | Oscillator output ranges, frequency accuracy, PolyBLEP DC offset, envelope stages, MIDI-to-frequency conversion, parameter interactions |
| `plugin` | 15 | Lock-free NoteQueue (wrapping, capacity, encoding), VisBuffer (double-buffer swap, partial push, value correctness) |
| `synth-ui` | 36 | Keyboard layout geometry (25 keys, 15 white + 10 black, correct MIDI mapping, key dimensions), KEY_MAP validation, FFT resource initialization, VisMode enum |
| `web-worklet` | 23 | WasmSynth construction, vis ring buffer behavior (write position advancing, wrapping, ready flag lifecycle), audio processing (silence/sound/release), parameter setters, all 128 MIDI notes |
| `web` | 20 | DirtyFlags (all flag combinations, clear behavior), WebParams defaults, oscillator type mapping |

The DSP tests are particularly interesting because they verify audio properties: that a sine wave's output stays within [-1, 1], that frequency accuracy is within 1 Hz, that the DC offset of each waveform is near zero (important for preventing speaker damage), and that the envelope reaches the correct sustain level.

The web-worklet tests exercise the vis buffer ring buffer exhaustively: verifying that `vis_ready` becomes true after exactly 16 render quanta (16 * 128 = 2048 samples), that the write position wraps correctly, and that the flag clears after being read.

---

## Build Commands

For anyone wanting to reproduce this setup:

```bash
# Native VST3/CLAP plugin (for DAWs)
cargo xtask bundle plugin --release

# Standalone desktop app (no DAW needed)
cargo run -p plugin --bin simple-synth-standalone --release

# Web app (two-step: build worklet WASM, then serve)
wasm-pack build web-worklet --target no-modules --out-dir ../target/web-dist/worklet-pkg
cd web && trunk serve --port 8080

# Run all tests
cargo test --workspace
```

---

## Lessons Learned

**1. Audio threads are sacred ground.** No mutexes, no allocations, no blocking calls. This principle drove us to lock-free data structures for cross-thread communication and cached FFT resources for the visualizer. The same discipline that makes native audio plugins reliable also makes the code naturally portable to WASM (which is single-threaded within each module).

**2. AudioWorkletGlobalScope is a hostile environment.** It's not a regular Web Worker. It lacks `importScripts()`, `TextDecoder`, `TextEncoder`, `fetch()`, and many other APIs you take for granted. If you need to run WASM in an AudioWorklet, be prepared to bypass your toolchain's generated glue code and write minimal imports by hand.

**3. Test the type hierarchy, not just the types.** JavaScript's `Float32Array instanceof Object === true` is well-known, but it's easy to forget when writing Rust code that interoperates with JS via wasm-bindgen. When matching on JS types, always check specific types before generic ones.

**4. VST plugin windows are not normal windows.** They're child windows embedded in another application's window hierarchy. This affects OpenGL context behavior (vsync signals may not arrive), window messages, and focus handling. If your rendering framework assumes a top-level window, you'll need to patch it.

**5. `dsp-core` with zero dependencies was the best architectural decision.** By keeping the audio engine free of any platform-specific code, it compiled to WASM without a single `#[cfg]` gate. The entire portability challenge was pushed to the edges — the plugin wrapper and the web glue code — while the core audio processing stayed identical everywhere.
