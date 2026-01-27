use crate::keyboard::PianoKeyboard;
use crate::visualizer::{FftResources, VisMode, VisualizerWidget};
use crate::{NoteQueue, SimpleSynthParams, VisBuffer};
use nih_plug::prelude::*;
use nih_plug_egui::egui;
use nih_plug_egui::{create_egui_editor, widgets};
use std::sync::Arc;

/// Persistent UI state that lives across editor open/close cycles.
pub struct EditorState {
    pub vis_mode: VisMode,
    /// Notes currently held via the on-screen keyboard (by MIDI note number).
    pub held_notes: Vec<u8>,
    /// Pre-allocated FFT resources — avoids heap allocation on every frame.
    pub fft_resources: FftResources,
}

pub fn create(
    params: Arc<SimpleSynthParams>,
    vis_buffer: Arc<VisBuffer>,
    note_queue: Arc<NoteQueue>,
) -> Option<Box<dyn Editor>> {
    let fft_resources = FftResources::new();
    create_egui_editor(
        params.editor_state.clone(),
        EditorState {
            vis_mode: VisMode::Oscilloscope,
            held_notes: Vec::new(),
            fft_resources,
        },
        |egui_ctx, _| {
            egui_ctx.set_visuals(egui::Visuals::dark());
        },
        move |egui_ctx, setter, state| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

                // --- Top section: oscillator type + ADSR knobs ---
                ui.horizontal(|ui| {
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Oscillator");
                            ui.add(widgets::ParamSlider::for_param(&params.osc_type, setter));
                        });
                    });

                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Gain");
                            ui.add(widgets::ParamSlider::for_param(&params.gain, setter));
                        });
                    });

                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Attack");
                            ui.add(widgets::ParamSlider::for_param(&params.attack, setter));
                        });
                    });

                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Decay");
                            ui.add(widgets::ParamSlider::for_param(&params.decay, setter));
                        });
                    });

                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Sustain");
                            ui.add(widgets::ParamSlider::for_param(&params.sustain, setter));
                        });
                    });

                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Release");
                            ui.add(widgets::ParamSlider::for_param(&params.release, setter));
                        });
                    });
                });

                ui.separator();

                // --- Middle section: visualizer ---
                ui.horizontal(|ui| {
                    ui.label("Visualizer:");
                    if ui
                        .selectable_label(state.vis_mode == VisMode::Oscilloscope, "Oscilloscope")
                        .clicked()
                    {
                        state.vis_mode = VisMode::Oscilloscope;
                    }
                    if ui
                        .selectable_label(state.vis_mode == VisMode::Spectrum, "Spectrum")
                        .clicked()
                    {
                        state.vis_mode = VisMode::Spectrum;
                    }
                });

                let vis_height = 200.0;
                let vis_size = egui::vec2(ui.available_width(), vis_height);
                let (vis_rect, _) = ui.allocate_exact_size(vis_size, egui::Sense::hover());

                // Read audio data from lock-free buffer (no allocation, no lock)
                let samples = vis_buffer.read_front();

                let mut widget = VisualizerWidget {
                    samples: samples.as_slice(),
                    mode: state.vis_mode,
                    rect: vis_rect,
                    fft: Some(&mut state.fft_resources),
                };
                widget.paint(ui);

                ui.separator();

                // --- Bottom section: piano keyboard ---
                let kb_height = ui.available_height().max(80.0);
                let kb_size = egui::vec2(ui.available_width(), kb_height);
                let (kb_rect, kb_response) =
                    ui.allocate_exact_size(kb_size, egui::Sense::click_and_drag());

                let keyboard = PianoKeyboard {
                    rect: kb_rect,
                    held_notes: &state.held_notes,
                };
                let events = keyboard.paint_and_interact(ui, &kb_response);

                // Process keyboard events — update UI state AND send to audio thread
                for event in events {
                    match event {
                        KeyboardEvent::NoteOn(note) => {
                            if !state.held_notes.contains(&note) {
                                state.held_notes.push(note);
                                note_queue.push_note_on(note);
                            }
                        }
                        KeyboardEvent::NoteOff(note) => {
                            state.held_notes.retain(|&n| n != note);
                            note_queue.push_note_off(note);
                        }
                    }
                }
            });

            // Repaint at ~30fps for the visualizer (not unbounded)
            egui_ctx.request_repaint_after(std::time::Duration::from_millis(33));
        },
    )
}

pub enum KeyboardEvent {
    NoteOn(u8),
    NoteOff(u8),
}
