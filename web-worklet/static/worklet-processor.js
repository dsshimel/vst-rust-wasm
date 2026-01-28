// AudioWorkletProcessor that runs the Rust synth engine via WASM.
//
// AudioWorkletGlobalScope does NOT support importScripts() or TextDecoder,
// so we instantiate the WASM module directly with hand-written minimal imports
// instead of using the wasm-bindgen JS glue.
//
// Initialization flow:
//   1. Main thread fetches WASM bytes as ArrayBuffer
//   2. Main thread sends { type: "init", wasmBytes, sampleRate } to worklet
//   3. Worklet compiles + instantiates WASM directly, creates synth

class SynthProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.wasm = null;
    this.synthPtr = 0;
    this.port.onmessage = this.handleMessage.bind(this);
  }

  handleMessage(event) {
    const msg = event.data;

    if (msg.type === "init") {
      this.initWasm(msg.wasmBytes, msg.sampleRate);
      return;
    }

    if (!this.wasm) return;

    switch (msg.type) {
      case "noteOn":
        this.wasm.wasmsynth_note_on(this.synthPtr, msg.note);
        break;
      case "noteOff":
        this.wasm.wasmsynth_note_off(this.synthPtr, msg.note);
        break;
      case "param":
        this.setParam(msg.name, msg.value);
        break;
    }
  }

  initWasm(wasmBytes, sampleRate) {
    try {
      // Minimal imports required by the wasm-bindgen output.
      const imports = {
        "./web_worklet_bg.js": {
          __wbg___wbindgen_throw_be289d5034ed271b: function(ptr, len) {
            throw new Error("WASM panic (ptr=" + ptr + ", len=" + len + ")");
          },
          __wbindgen_init_externref_table: function() {
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

      if (this.wasm.__wbindgen_start) {
        this.wasm.__wbindgen_start();
      }

      this.synthPtr = this.wasm.wasmsynth_new() >>> 0;
      this.wasm.wasmsynth_prepare(this.synthPtr, sampleRate);

      this.port.postMessage({ type: "ready" });
    } catch (e) {
      console.error("[Worklet] Failed to init WASM:", e);
      this.port.postMessage({ type: "error", message: String(e) });
    }
  }

  readF32Array(retVal) {
    const ptr = retVal[0] >>> 0;
    const len = retVal[1] >>> 0;
    const f32 = new Float32Array(this.wasm.memory.buffer, ptr, len);
    const copy = new Float32Array(f32);
    this.wasm.__wbindgen_free(ptr, len * 4, 4);
    return copy;
  }

  setParam(name, value) {
    if (!this.wasm) return;
    switch (name) {
      case "osc_type":
        this.wasm.wasmsynth_set_osc_type(this.synthPtr, value);
        break;
      case "gain":
        this.wasm.wasmsynth_set_gain(this.synthPtr, value);
        break;
      case "attack":
        this.wasm.wasmsynth_set_attack(this.synthPtr, value);
        break;
      case "decay":
        this.wasm.wasmsynth_set_decay(this.synthPtr, value);
        break;
      case "sustain":
        this.wasm.wasmsynth_set_sustain(this.synthPtr, value);
        break;
      case "release":
        this.wasm.wasmsynth_set_release(this.synthPtr, value);
        break;
    }
  }

  process(inputs, outputs, parameters) {
    if (!this.wasm) return true;

    const output = outputs[0];
    if (!output || output.length === 0) return true;

    const channel0 = output[0];

    const ret = this.wasm.wasmsynth_process_audio(this.synthPtr);
    const samples = this.readF32Array(ret);
    channel0.set(samples);

    for (let ch = 1; ch < output.length; ch++) {
      output[ch].set(channel0);
    }

    const visReady = this.wasm.wasmsynth_vis_ready(this.synthPtr);
    if (visReady !== 0) {
      const visRet = this.wasm.wasmsynth_get_vis_data(this.synthPtr);
      const visData = this.readF32Array(visRet);
      this.port.postMessage(visData, [visData.buffer]);
    }

    return true;
  }
}

registerProcessor("synth-processor", SynthProcessor);
