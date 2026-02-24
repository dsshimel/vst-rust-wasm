use std::cell::RefCell;
use std::rc::Rc;

use eframe::egui;
use synth_ui::{render_synth_ui, KeyboardEvent, UiState};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::audio_bridge::AudioBridge;
use crate::web_controls::{WebControls, WebParams};

pub struct SynthWebApp {
    state: UiState,
    params: WebParams,
    audio: Option<Rc<RefCell<AudioBridge>>>,
    vis_samples: Vec<f32>,
    audio_started: bool,
    /// Shared buffer for receiving vis data from the worklet callback
    shared_vis: Rc<RefCell<Option<Vec<f32>>>>,
}

impl SynthWebApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Self {
            state: UiState::new(),
            params: WebParams::default(),
            audio: None,
            vis_samples: vec![0.0; 2048],
            audio_started: false,
            shared_vis: Rc::new(RefCell::new(None)),
        }
    }

    fn start_audio(&mut self, ctx: egui::Context) {
        let shared_vis = self.shared_vis.clone();
        let egui_ctx = ctx.clone();

        wasm_bindgen_futures::spawn_local(async move {
            match AudioBridge::start().await {
                Ok(bridge) => {
                    let bridge = Rc::new(RefCell::new(bridge));

                    // Set up vis data callback from worklet
                    let vis_ref = shared_vis.clone();
                    let ctx_ref = egui_ctx.clone();
                    let callback = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                        let data = event.data();

                        // Check for Float32Array FIRST (vis data from worklet).
                        // Float32Array is also a JS Object, so we must check this before
                        // the generic Object check below.
                        if data.is_instance_of::<js_sys::Float32Array>() {
                            let arr: js_sys::Float32Array = data.unchecked_into();
                            let mut buf = vec![0.0f32; arr.length() as usize];
                            arr.copy_to(&mut buf);
                            *vis_ref.borrow_mut() = Some(buf);
                            ctx_ref.request_repaint();
                            return;
                        }

                        // Also check for ArrayBuffer (transferred buffers arrive as ArrayBuffer)
                        if data.is_instance_of::<js_sys::ArrayBuffer>() {
                            let ab: js_sys::ArrayBuffer = data.unchecked_into();
                            let arr = js_sys::Float32Array::new(&ab);
                            let mut buf = vec![0.0f32; arr.length() as usize];
                            arr.copy_to(&mut buf);
                            *vis_ref.borrow_mut() = Some(buf);
                            ctx_ref.request_repaint();
                        }

                        // Otherwise it's a control message (ready/error) — ignore
                    })
                        as Box<dyn FnMut(web_sys::MessageEvent)>);

                    bridge.borrow().set_vis_callback(callback);

                    // Store the bridge via thread-local (can't get &mut App from async block)
                    BRIDGE.with(|b| {
                        *b.borrow_mut() = Some(bridge);
                    });
                }
                Err(e) => {
                    web_sys::console::error_1(
                        &format!("Audio start failed: {:?}", e).into(),
                    );
                }
            }
        });

        self.audio_started = true;
    }

    fn sync_bridge(&mut self) {
        if self.audio.is_none() {
            BRIDGE.with(|b| {
                if let Some(bridge) = b.borrow_mut().take() {
                    self.audio = Some(bridge);
                }
            });
        }
    }

    fn send_dirty_params(&mut self) {
        let bridge = match self.audio.as_ref() {
            Some(b) => b,
            None => return,
        };
        let b = bridge.borrow();
        let p = &self.params;
        let d = &p.dirty;

        if d.osc_type {
            let _ = b.send_param("osc_type", p.osc_type as f64);
        }
        if d.gain {
            let _ = b.send_param("gain", p.gain as f64);
        }
        if d.attack {
            let _ = b.send_param("attack", p.attack as f64);
        }
        if d.decay {
            let _ = b.send_param("decay", p.decay as f64);
        }
        if d.sustain {
            let _ = b.send_param("sustain", p.sustain as f64);
        }
        if d.release {
            let _ = b.send_param("release", p.release as f64);
        }

        self.params.dirty.clear();
    }

    fn process_keyboard_events(&mut self, events: Vec<KeyboardEvent>) {
        for event in events {
            match event {
                KeyboardEvent::NoteOn(note) => {
                    if !self.state.held_notes.contains(&note) {
                        self.state.held_notes.push(note);
                        if let Some(bridge) = &self.audio {
                            let _ = bridge.borrow().send_note_on(note);
                        }
                    }
                }
                KeyboardEvent::NoteOff(note) => {
                    self.state.held_notes.retain(|&n| n != note);
                    if let Some(bridge) = &self.audio {
                        let _ = bridge.borrow().send_note_off(note);
                    }
                }
            }
        }
    }
}

thread_local! {
    static BRIDGE: RefCell<Option<Rc<RefCell<AudioBridge>>>> = RefCell::new(None);
}

impl eframe::App for SynthWebApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        self.sync_bridge();

        // Poll vis data from the shared buffer
        if let Some(data) = self.shared_vis.borrow_mut().take() {
            self.vis_samples = data;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.audio_started {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    if ui
                        .button(egui::RichText::new("Start Audio").size(24.0))
                        .clicked()
                    {
                        self.start_audio(ctx.clone());
                    }
                    ui.add_space(10.0);
                    ui.label("Click to enable audio (required by browser policy)");
                });
                ui.separator();
            }

            let mut controls = WebControls {
                params: &mut self.params,
            };
            let events = render_synth_ui(ui, &mut self.state, &mut controls, &self.vis_samples, true);
            self.process_keyboard_events(events);
        });

        // Send any dirty params to the worklet
        self.send_dirty_params();

        // Repaint at ~30fps for the visualizer
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}
