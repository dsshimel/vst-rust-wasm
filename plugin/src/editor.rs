use crate::{NoteQueue, SimpleSynthParams, VisBuffer};
use nih_plug::prelude::*;
use nih_plug_egui::egui;
use nih_plug_egui::{create_egui_editor, widgets};
use std::sync::Arc;
use synth_ui::{render_synth_ui, ControlRenderer, KeyboardEvent, UiState};

pub fn create(
    params: Arc<SimpleSynthParams>,
    vis_buffer: Arc<VisBuffer>,
    note_queue: Arc<NoteQueue>,
) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        params.editor_state.clone(),
        UiState::new(),
        |egui_ctx, _| {
            egui_ctx.set_visuals(egui::Visuals::dark());
        },
        move |egui_ctx, setter, state| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                let samples = vis_buffer.read_front();

                let mut controls = NihPlugControls {
                    params: &params,
                    setter,
                };
                let events = render_synth_ui(
                    ui,
                    state,
                    &mut controls,
                    samples.as_slice(),
                    cfg!(feature = "octave-shift"),
                );

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

/// Adapts nih-plug's ParamSlider to the ControlRenderer trait.
struct NihPlugControls<'a> {
    params: &'a Arc<SimpleSynthParams>,
    setter: &'a ParamSetter<'a>,
}

impl<'a> ControlRenderer for NihPlugControls<'a> {
    fn render_osc_type(&mut self, ui: &mut egui::Ui) {
        ui.add(widgets::ParamSlider::for_param(&self.params.osc_type, self.setter));
    }

    fn render_gain(&mut self, ui: &mut egui::Ui) {
        ui.add(widgets::ParamSlider::for_param(&self.params.gain, self.setter));
    }

    fn render_attack(&mut self, ui: &mut egui::Ui) {
        ui.add(widgets::ParamSlider::for_param(&self.params.attack, self.setter));
    }

    fn render_decay(&mut self, ui: &mut egui::Ui) {
        ui.add(widgets::ParamSlider::for_param(&self.params.decay, self.setter));
    }

    fn render_sustain(&mut self, ui: &mut egui::Ui) {
        ui.add(widgets::ParamSlider::for_param(&self.params.sustain, self.setter));
    }

    fn render_release(&mut self, ui: &mut egui::Ui) {
        ui.add(widgets::ParamSlider::for_param(&self.params.release, self.setter));
    }
}
