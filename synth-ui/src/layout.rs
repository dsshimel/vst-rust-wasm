use egui;

use crate::keyboard::PianoKeyboard;
use crate::visualizer::{FftResources, VisMode, VisualizerWidget};
use crate::KeyboardEvent;

/// Persistent UI state that lives across frames.
pub struct UiState {
    pub vis_mode: VisMode,
    pub held_notes: Vec<u8>,
    pub fft_resources: FftResources,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            vis_mode: VisMode::Oscilloscope,
            held_notes: Vec::new(),
            fft_resources: FftResources::new(),
        }
    }
}

/// Trait for rendering parameter controls.
/// Each backend (nih-plug plugin, eframe web) provides its own implementation.
pub trait ControlRenderer {
    fn render_osc_type(&mut self, ui: &mut egui::Ui);
    fn render_gain(&mut self, ui: &mut egui::Ui);
    fn render_attack(&mut self, ui: &mut egui::Ui);
    fn render_decay(&mut self, ui: &mut egui::Ui);
    fn render_sustain(&mut self, ui: &mut egui::Ui);
    fn render_release(&mut self, ui: &mut egui::Ui);
}

/// Render the full synthesizer UI layout. Returns keyboard events for the caller to process.
///
/// This function is shared between the native plugin and the web app. The `controls`
/// parameter abstracts over nih-plug's ParamSlider (plugin) vs plain egui sliders (web).
pub fn render_synth_ui(
    ui: &mut egui::Ui,
    state: &mut UiState,
    controls: &mut dyn ControlRenderer,
    vis_samples: &[f32],
) -> Vec<KeyboardEvent> {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

    // --- Top section: oscillator type + ADSR knobs ---
    ui.horizontal(|ui| {
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label("Oscillator");
                controls.render_osc_type(ui);
            });
        });

        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label("Gain");
                controls.render_gain(ui);
            });
        });

        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label("Attack");
                controls.render_attack(ui);
            });
        });

        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label("Decay");
                controls.render_decay(ui);
            });
        });

        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label("Sustain");
                controls.render_sustain(ui);
            });
        });

        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label("Release");
                controls.render_release(ui);
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

    let mut widget = VisualizerWidget {
        samples: vis_samples,
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
    keyboard.paint_and_interact(ui, &kb_response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_state_default_vis_mode_is_oscilloscope() {
        let state = UiState::new();
        assert_eq!(state.vis_mode, VisMode::Oscilloscope);
    }

    #[test]
    fn ui_state_starts_with_no_held_notes() {
        let state = UiState::new();
        assert!(state.held_notes.is_empty());
    }

    #[test]
    fn ui_state_fft_resources_initialized() {
        // FftResources::new() allocates the FFT plan and buffers.
        // Detailed field checks are in visualizer::tests; here we just
        // verify construction doesn't panic.
        let _state = UiState::new();
    }

    #[test]
    fn ui_state_vis_mode_can_be_changed() {
        let mut state = UiState::new();
        state.vis_mode = VisMode::Spectrum;
        assert_eq!(state.vis_mode, VisMode::Spectrum);
        state.vis_mode = VisMode::Oscilloscope;
        assert_eq!(state.vis_mode, VisMode::Oscilloscope);
    }

    #[test]
    fn ui_state_held_notes_can_be_modified() {
        let mut state = UiState::new();
        state.held_notes.push(60); // C4
        state.held_notes.push(64); // E4
        assert_eq!(state.held_notes.len(), 2);
        assert!(state.held_notes.contains(&60));
        state.held_notes.retain(|&n| n != 60);
        assert_eq!(state.held_notes.len(), 1);
        assert!(!state.held_notes.contains(&60));
    }
}
