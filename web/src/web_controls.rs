use dsp_core::params::OscillatorType;
use eframe::egui;
use synth_ui::ControlRenderer;

/// Parameter values held on the main (UI) thread.
/// Each frame, changed values are sent to the AudioWorklet.
pub struct WebParams {
    pub osc_type: i32,
    pub gain: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    /// Tracks which params changed this frame so we can batch-send to the worklet.
    pub dirty: DirtyFlags,
}

#[derive(Default)]
pub struct DirtyFlags {
    pub osc_type: bool,
    pub gain: bool,
    pub attack: bool,
    pub decay: bool,
    pub sustain: bool,
    pub release: bool,
}

impl DirtyFlags {
    pub fn any(&self) -> bool {
        self.osc_type || self.gain || self.attack || self.decay || self.sustain || self.release
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

impl Default for WebParams {
    fn default() -> Self {
        Self {
            osc_type: 0,
            gain: 0.8,
            attack: 0.01,
            decay: 0.1,
            sustain: 0.7,
            release: 0.3,
            dirty: DirtyFlags::default(),
        }
    }
}

/// Wraps WebParams to implement ControlRenderer using plain egui sliders.
pub struct WebControls<'a> {
    pub params: &'a mut WebParams,
}

impl<'a> ControlRenderer for WebControls<'a> {
    fn render_osc_type(&mut self, ui: &mut egui::Ui) {
        let prev = self.params.osc_type;
        let name = OscillatorType::from_index(self.params.osc_type as usize).name();
        egui::ComboBox::from_id_salt("osc_type")
            .selected_text(name)
            .show_ui(ui, |ui: &mut egui::Ui| {
                for (i, variant) in OscillatorType::VARIANTS.iter().enumerate() {
                    ui.selectable_value(&mut self.params.osc_type, i as i32, variant.name());
                }
            });
        if self.params.osc_type != prev {
            self.params.dirty.osc_type = true;
        }
    }

    fn render_gain(&mut self, ui: &mut egui::Ui) {
        let prev = self.params.gain;
        ui.add(egui::Slider::new(&mut self.params.gain, 0.0..=1.0).text(""));
        if (self.params.gain - prev).abs() > f32::EPSILON {
            self.params.dirty.gain = true;
        }
    }

    fn render_attack(&mut self, ui: &mut egui::Ui) {
        let prev = self.params.attack;
        ui.add(
            egui::Slider::new(&mut self.params.attack, 0.001..=2.0)
                .logarithmic(true)
                .suffix(" s")
                .text(""),
        );
        if (self.params.attack - prev).abs() > f32::EPSILON {
            self.params.dirty.attack = true;
        }
    }

    fn render_decay(&mut self, ui: &mut egui::Ui) {
        let prev = self.params.decay;
        ui.add(
            egui::Slider::new(&mut self.params.decay, 0.001..=2.0)
                .logarithmic(true)
                .suffix(" s")
                .text(""),
        );
        if (self.params.decay - prev).abs() > f32::EPSILON {
            self.params.dirty.decay = true;
        }
    }

    fn render_sustain(&mut self, ui: &mut egui::Ui) {
        let prev = self.params.sustain;
        ui.add(egui::Slider::new(&mut self.params.sustain, 0.0..=1.0).text(""));
        if (self.params.sustain - prev).abs() > f32::EPSILON {
            self.params.dirty.sustain = true;
        }
    }

    fn render_release(&mut self, ui: &mut egui::Ui) {
        let prev = self.params.release;
        ui.add(
            egui::Slider::new(&mut self.params.release, 0.001..=5.0)
                .logarithmic(true)
                .suffix(" s")
                .text(""),
        );
        if (self.params.release - prev).abs() > f32::EPSILON {
            self.params.dirty.release = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- DirtyFlags ---

    #[test]
    fn dirty_flags_default_all_false() {
        let flags = DirtyFlags::default();
        assert!(!flags.osc_type);
        assert!(!flags.gain);
        assert!(!flags.attack);
        assert!(!flags.decay);
        assert!(!flags.sustain);
        assert!(!flags.release);
    }

    #[test]
    fn dirty_flags_any_false_when_all_clear() {
        let flags = DirtyFlags::default();
        assert!(!flags.any());
    }

    #[test]
    fn dirty_flags_any_true_when_osc_type_set() {
        let mut flags = DirtyFlags::default();
        flags.osc_type = true;
        assert!(flags.any());
    }

    #[test]
    fn dirty_flags_any_true_when_gain_set() {
        let mut flags = DirtyFlags::default();
        flags.gain = true;
        assert!(flags.any());
    }

    #[test]
    fn dirty_flags_any_true_when_attack_set() {
        let mut flags = DirtyFlags::default();
        flags.attack = true;
        assert!(flags.any());
    }

    #[test]
    fn dirty_flags_any_true_when_decay_set() {
        let mut flags = DirtyFlags::default();
        flags.decay = true;
        assert!(flags.any());
    }

    #[test]
    fn dirty_flags_any_true_when_sustain_set() {
        let mut flags = DirtyFlags::default();
        flags.sustain = true;
        assert!(flags.any());
    }

    #[test]
    fn dirty_flags_any_true_when_release_set() {
        let mut flags = DirtyFlags::default();
        flags.release = true;
        assert!(flags.any());
    }

    #[test]
    fn dirty_flags_any_true_when_all_set() {
        let flags = DirtyFlags {
            osc_type: true,
            gain: true,
            attack: true,
            decay: true,
            sustain: true,
            release: true,
        };
        assert!(flags.any());
    }

    #[test]
    fn dirty_flags_clear_resets_all() {
        let mut flags = DirtyFlags {
            osc_type: true,
            gain: true,
            attack: true,
            decay: true,
            sustain: true,
            release: true,
        };
        flags.clear();
        assert!(!flags.osc_type);
        assert!(!flags.gain);
        assert!(!flags.attack);
        assert!(!flags.decay);
        assert!(!flags.sustain);
        assert!(!flags.release);
        assert!(!flags.any());
    }

    #[test]
    fn dirty_flags_clear_partial() {
        let mut flags = DirtyFlags::default();
        flags.gain = true;
        flags.release = true;
        assert!(flags.any());
        flags.clear();
        assert!(!flags.any());
    }

    // --- WebParams ---

    #[test]
    fn web_params_default_osc_type() {
        let p = WebParams::default();
        assert_eq!(p.osc_type, 0, "default osc type should be Sine (0)");
    }

    #[test]
    fn web_params_default_gain() {
        let p = WebParams::default();
        assert!((p.gain - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn web_params_default_attack() {
        let p = WebParams::default();
        assert!((p.attack - 0.01).abs() < f32::EPSILON);
    }

    #[test]
    fn web_params_default_decay() {
        let p = WebParams::default();
        assert!((p.decay - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn web_params_default_sustain() {
        let p = WebParams::default();
        assert!((p.sustain - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn web_params_default_release() {
        let p = WebParams::default();
        assert!((p.release - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn web_params_default_dirty_flags_clear() {
        let p = WebParams::default();
        assert!(!p.dirty.any());
    }

    #[test]
    fn web_params_osc_type_maps_to_valid_oscillator() {
        let p = WebParams::default();
        let osc = OscillatorType::from_index(p.osc_type as usize);
        assert_eq!(osc, OscillatorType::Sine);
    }

    #[test]
    fn web_params_all_osc_indices_valid() {
        for i in 0..4 {
            let mut p = WebParams::default();
            p.osc_type = i;
            let osc = OscillatorType::from_index(p.osc_type as usize);
            assert_eq!(osc, OscillatorType::VARIANTS[i as usize]);
        }
    }
}
