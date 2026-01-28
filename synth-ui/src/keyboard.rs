use egui;

pub enum KeyboardEvent {
    NoteOn(u8),
    NoteOff(u8),
}

/// MIDI note numbers for 2 octaves starting at C3 (MIDI 48).
const FIRST_NOTE: u8 = 48; // C3
const NUM_WHITE_KEYS: usize = 15; // C3 to D5 (two octaves + 1)

/// Map computer keyboard keys to semitone offsets from C3.
/// Bottom row: A=C, W=C#, S=D, E=D#, D=E, F=F, T=F#, G=G, Y=G#, H=A, U=A#, J=B
/// Continues: K=C4, O=C#4, L=D4
const KEY_MAP: &[(egui::Key, u8)] = &[
    (egui::Key::A, 0),  // C3
    (egui::Key::W, 1),  // C#3
    (egui::Key::S, 2),  // D3
    (egui::Key::E, 3),  // D#3
    (egui::Key::D, 4),  // E3
    (egui::Key::F, 5),  // F3
    (egui::Key::T, 6),  // F#3
    (egui::Key::G, 7),  // G3
    (egui::Key::Y, 8),  // G#3
    (egui::Key::H, 9),  // A3
    (egui::Key::U, 10), // A#3
    (egui::Key::J, 11), // B3
    (egui::Key::K, 12), // C4
    (egui::Key::O, 13), // C#4
    (egui::Key::L, 14), // D4
];

pub struct PianoKeyboard<'a> {
    pub rect: egui::Rect,
    pub held_notes: &'a [u8],
}

struct KeyLayout {
    note: u8,
    rect: egui::Rect,
    is_black: bool,
}

impl<'a> PianoKeyboard<'a> {
    pub fn paint_and_interact(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
    ) -> Vec<KeyboardEvent> {
        let mut events = Vec::new();
        let keys = self.compute_layout();
        let painter = ui.painter_at(self.rect);

        // Draw white keys first (they're behind black keys)
        for key in &keys {
            if !key.is_black {
                let is_held = self.held_notes.contains(&key.note);
                let fill = if is_held {
                    egui::Color32::from_rgb(100, 180, 255)
                } else {
                    egui::Color32::from_rgb(240, 240, 240)
                };
                painter.rect_filled(key.rect, 2.0, fill);
                painter.rect_stroke(
                    key.rect,
                    2.0,
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 80, 80)),
                    egui::StrokeKind::Middle,
                );
            }
        }

        // Draw black keys on top
        for key in &keys {
            if key.is_black {
                let is_held = self.held_notes.contains(&key.note);
                let fill = if is_held {
                    egui::Color32::from_rgb(60, 120, 200)
                } else {
                    egui::Color32::from_rgb(30, 30, 30)
                };
                painter.rect_filled(key.rect, 2.0, fill);
                painter.rect_stroke(
                    key.rect,
                    2.0,
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(10, 10, 10)),
                    egui::StrokeKind::Middle,
                );
            }
        }

        // Handle mouse interaction
        if response.is_pointer_button_down_on() {
            if let Some(pos) = response.interact_pointer_pos() {
                // Check black keys first (they overlap white keys visually)
                let mut clicked_note = None;
                for key in keys.iter().rev() {
                    if key.rect.contains(pos) {
                        clicked_note = Some(key.note);
                        break;
                    }
                }
                if let Some(note) = clicked_note {
                    if !self.held_notes.contains(&note) {
                        events.push(KeyboardEvent::NoteOn(note));
                    }
                }
            }
        } else {
            // Mouse released — release all mouse-held notes
            // (Computer keyboard notes are handled separately)
            // We release all held notes here; the editor state tracks them
            for &note in self.held_notes {
                events.push(KeyboardEvent::NoteOff(note));
            }
        }

        // Handle computer keyboard input
        let input = ui.input(|i| {
            let mut pressed = Vec::new();
            let mut released = Vec::new();
            for &(key, offset) in KEY_MAP {
                if i.key_pressed(key) {
                    pressed.push(FIRST_NOTE + offset);
                }
                if i.key_released(key) {
                    released.push(FIRST_NOTE + offset);
                }
            }
            (pressed, released)
        });

        for note in input.0 {
            events.push(KeyboardEvent::NoteOn(note));
        }
        for note in input.1 {
            events.push(KeyboardEvent::NoteOff(note));
        }

        events
    }

    fn compute_layout(&self) -> Vec<KeyLayout> {
        let mut keys = Vec::new();
        let white_key_width = self.rect.width() / NUM_WHITE_KEYS as f32;
        let black_key_width = white_key_width * 0.6;
        let black_key_height = self.rect.height() * 0.6;

        let mut white_idx = 0u8;
        // Walk through 2 octaves + 1 note = 25 semitones
        for semitone in 0u8..25 {
            let note = FIRST_NOTE + semitone;
            let is_black = matches!(semitone % 12, 1 | 3 | 6 | 8 | 10);

            if is_black {
                // Black key sits between the previous and next white keys
                let x = self.rect.left() + white_idx as f32 * white_key_width
                    - black_key_width / 2.0;
                keys.push(KeyLayout {
                    note,
                    rect: egui::Rect::from_min_size(
                        egui::pos2(x, self.rect.top()),
                        egui::vec2(black_key_width, black_key_height),
                    ),
                    is_black: true,
                });
            } else {
                let x = self.rect.left() + white_idx as f32 * white_key_width;
                keys.push(KeyLayout {
                    note,
                    rect: egui::Rect::from_min_size(
                        egui::pos2(x, self.rect.top()),
                        egui::vec2(white_key_width, self.rect.height()),
                    ),
                    is_black: false,
                });
                white_idx += 1;
            }
        }

        keys
    }
}
