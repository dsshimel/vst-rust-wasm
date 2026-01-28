use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{AudioContext, AudioWorkletNode, MessagePort};

/// Manages the Web Audio pipeline: AudioContext -> AudioWorkletNode -> destination.
/// Communication with the worklet happens via MessagePort.
pub struct AudioBridge {
    _context: AudioContext,
    _worklet_node: AudioWorkletNode,
    port: MessagePort,
}

impl AudioBridge {
    /// Create and start the audio pipeline. Must be called from a user gesture handler.
    pub async fn start() -> Result<Self, JsValue> {
        let context = AudioContext::new()?;

        // Load the worklet processor script
        let worklet = context.audio_worklet()?;
        let promise = worklet.add_module("worklet-processor.js")?;
        wasm_bindgen_futures::JsFuture::from(promise).await?;

        // Create the worklet node
        let node = AudioWorkletNode::new(&context, "synth-processor")?;
        node.connect_with_audio_node(&context.destination())?;

        let port = node.port()?;

        // Fetch the WASM bytes on the main thread
        let wasm_url = "worklet-pkg/web_worklet_bg.wasm";
        let window = web_sys::window().expect("no window");
        let resp_promise = window.fetch_with_str(wasm_url);
        let resp_value = wasm_bindgen_futures::JsFuture::from(resp_promise).await?;
        let resp: web_sys::Response = resp_value.dyn_into()?;

        let array_buffer_promise = resp.array_buffer()?;
        let array_buffer = wasm_bindgen_futures::JsFuture::from(array_buffer_promise).await?;

        // Send the raw WASM bytes to the worklet. The worklet instantiates the WASM
        // module directly with hand-written minimal imports (no JS glue needed).
        let sample_rate = context.sample_rate();
        let init_msg = js_sys::Object::new();
        js_sys::Reflect::set(&init_msg, &"type".into(), &"init".into())?;
        js_sys::Reflect::set(&init_msg, &"wasmBytes".into(), &array_buffer)?;
        js_sys::Reflect::set(
            &init_msg,
            &"sampleRate".into(),
            &sample_rate.into(),
        )?;
        // Transfer the ArrayBuffer for zero-copy
        let transfer = js_sys::Array::new();
        transfer.push(&array_buffer);
        port.post_message_with_transferable(&init_msg, &transfer)?;

        Ok(Self {
            _context: context,
            _worklet_node: node,
            port,
        })
    }

    pub fn send_note_on(&self, note: u8) -> Result<(), JsValue> {
        let msg = js_sys::Object::new();
        js_sys::Reflect::set(&msg, &"type".into(), &"noteOn".into())?;
        js_sys::Reflect::set(&msg, &"note".into(), &(note as f64).into())?;
        self.port.post_message(&msg)
    }

    pub fn send_note_off(&self, note: u8) -> Result<(), JsValue> {
        let msg = js_sys::Object::new();
        js_sys::Reflect::set(&msg, &"type".into(), &"noteOff".into())?;
        js_sys::Reflect::set(&msg, &"note".into(), &(note as f64).into())?;
        self.port.post_message(&msg)
    }

    pub fn send_param(&self, name: &str, value: f64) -> Result<(), JsValue> {
        let msg = js_sys::Object::new();
        js_sys::Reflect::set(&msg, &"type".into(), &"param".into())?;
        js_sys::Reflect::set(&msg, &"name".into(), &name.into())?;
        js_sys::Reflect::set(&msg, &"value".into(), &value.into())?;
        self.port.post_message(&msg)
    }

    /// Set the callback that receives visualization data from the worklet.
    pub fn set_vis_callback(&self, callback: Closure<dyn FnMut(web_sys::MessageEvent)>) {
        self.port
            .set_onmessage(Some(callback.as_ref().unchecked_ref()));
        callback.forget();
    }
}
