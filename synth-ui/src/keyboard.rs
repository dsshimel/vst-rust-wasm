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

/// Labels for each semitone offset (indexed by offset from KEY_MAP).
const KEY_LABELS: &[&str] = &[
    "A", "W", "S", "E", "D", "F", "T", "G", "Y", "H", "U", "J", "K", "O", "L",
];

fn key_label_for_note(note: u8, first_note: u8) -> Option<&'static str> {
    let offset = note.checked_sub(first_note)?;
    KEY_LABELS.get(offset as usize).copied()
}

pub struct PianoKeyboard<'a> {
    pub rect: egui::Rect,
    pub held_notes: &'a [u8],
    pub octave_offset: i8,
    pub mouse_note: &'a mut Option<u8>,
}

struct KeyLayout {
    note: u8,
    rect: egui::Rect,
    is_black: bool,
}

impl<'a> PianoKeyboard<'a> {
    pub fn paint_and_interact(
        &mut self,
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

        // Draw keyboard letter labels
        let white_key_width = self.rect.width() / NUM_WHITE_KEYS as f32;
        let label_font = egui::FontId::proportional((white_key_width * 0.28).min(14.0));
        for key in &keys {
            if let Some(label) = key_label_for_note(key.note, self.effective_first_note()) {
                let (pos, color) = if key.is_black {
                    (
                        egui::pos2(key.rect.center().x, key.rect.bottom() - 4.0),
                        egui::Color32::from_rgb(200, 200, 200),
                    )
                } else {
                    (
                        egui::pos2(key.rect.center().x, key.rect.bottom() - 4.0),
                        egui::Color32::from_rgb(100, 100, 100),
                    )
                };
                painter.text(pos, egui::Align2::CENTER_BOTTOM, label, label_font.clone(), color);
            }
        }

        // Handle mouse interaction — track mouse-held note separately
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
                    if *self.mouse_note != Some(note) {
                        // Release old mouse note if switching keys
                        if let Some(old) = self.mouse_note.take() {
                            events.push(KeyboardEvent::NoteOff(old));
                        }
                        *self.mouse_note = Some(note);
                        events.push(KeyboardEvent::NoteOn(note));
                    }
                }
            }
        } else if let Some(note) = self.mouse_note.take() {
            // Mouse released — only release the mouse-held note
            events.push(KeyboardEvent::NoteOff(note));
        }

        // Handle computer keyboard input using raw events to avoid OS key-repeat
        let first_note = self.effective_first_note();
        ui.input(|i| {
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    ..
                } = event
                {
                    if *repeat {
                        continue;
                    }
                    for &(map_key, offset) in KEY_MAP {
                        if *key == map_key {
                            if *pressed {
                                events.push(KeyboardEvent::NoteOn(first_note + offset));
                            } else {
                                events.push(KeyboardEvent::NoteOff(first_note + offset));
                            }
                        }
                    }
                }
            }
        });

        events
    }

    fn effective_first_note(&self) -> u8 {
        (FIRST_NOTE as i16 + self.octave_offset as i16 * 12).clamp(0, 103) as u8
    }

    fn compute_layout(&self) -> Vec<KeyLayout> {
        let mut keys = Vec::new();
        let white_key_width = self.rect.width() / NUM_WHITE_KEYS as f32;
        let black_key_width = white_key_width * 0.6;
        let black_key_height = self.rect.height() * 0.6;

        let first_note = self.effective_first_note();
        let mut white_idx = 0u8;
        // Walk through 2 octaves + 1 note = 25 semitones
        for semitone in 0u8..25 {
            let note = first_note + semitone;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_keyboard<'a>(held: &'a [u8], mouse_note: &'a mut Option<u8>) -> PianoKeyboard<'a> {
        PianoKeyboard {
            rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(750.0, 120.0)),
            held_notes: held,
            octave_offset: 0,
            mouse_note,
        }
    }

    // --- Constants ---

    #[test]
    fn first_note_is_c3() {
        // MIDI note 48 = C3
        assert_eq!(FIRST_NOTE, 48);
    }

    #[test]
    fn num_white_keys_is_15() {
        // Two octaves of white keys (7 + 7) plus one extra (D5) = 15
        assert_eq!(NUM_WHITE_KEYS, 15);
    }

    // --- KEY_MAP validation ---

    #[test]
    fn key_map_has_15_entries() {
        assert_eq!(KEY_MAP.len(), 15);
    }

    #[test]
    fn key_map_offsets_are_sequential() {
        // Offsets should go 0..=14 (one per semitone, covering two octaves + 2 semitones)
        for (i, &(_, offset)) in KEY_MAP.iter().enumerate() {
            assert_eq!(
                offset, i as u8,
                "KEY_MAP entry {} has offset {}, expected {}",
                i, offset, i
            );
        }
    }

    #[test]
    fn key_map_has_no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for &(key, _) in KEY_MAP {
            assert!(
                seen.insert(key),
                "Duplicate keyboard key in KEY_MAP: {:?}",
                key
            );
        }
    }

    #[test]
    fn key_map_has_no_duplicate_offsets() {
        let mut seen = std::collections::HashSet::new();
        for &(_, offset) in KEY_MAP {
            assert!(
                seen.insert(offset),
                "Duplicate offset in KEY_MAP: {}",
                offset
            );
        }
    }

    #[test]
    fn key_map_midi_notes_are_valid() {
        for &(_, offset) in KEY_MAP {
            let note = FIRST_NOTE + offset;
            assert!(note <= 127, "MIDI note {} exceeds 127", note);
        }
    }

    // --- compute_layout ---

    #[test]
    fn layout_has_25_keys() {
        // 2 octaves + 1 = 25 semitones (C3 to C5)
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        assert_eq!(keys.len(), 25);
    }

    #[test]
    fn layout_has_15_white_and_10_black_keys() {
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        let white_count = keys.iter().filter(|k| !k.is_black).count();
        let black_count = keys.iter().filter(|k| k.is_black).count();
        assert_eq!(white_count, 15, "expected 15 white keys");
        assert_eq!(black_count, 10, "expected 10 black keys");
    }

    #[test]
    fn layout_notes_start_at_c3_end_at_c5() {
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        assert_eq!(keys.first().unwrap().note, 48, "first note should be C3 (MIDI 48)");
        assert_eq!(keys.last().unwrap().note, 72, "last note should be C5 (MIDI 72)");
    }

    #[test]
    fn layout_notes_are_sequential() {
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(key.note, FIRST_NOTE + i as u8);
        }
    }

    #[test]
    fn layout_black_key_detection_matches_music_theory() {
        // In a chromatic scale, black keys are: C#(1), D#(3), F#(6), G#(8), A#(10)
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        for key in &keys {
            let semitone = (key.note - FIRST_NOTE) % 12;
            let expected_black = matches!(semitone, 1 | 3 | 6 | 8 | 10);
            assert_eq!(
                key.is_black, expected_black,
                "note {} (semitone {}): is_black={}, expected={}",
                key.note, semitone, key.is_black, expected_black
            );
        }
    }

    #[test]
    fn layout_white_keys_span_full_width() {
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        let white_keys: Vec<_> = keys.iter().filter(|k| !k.is_black).collect();

        // First white key starts at left edge
        let first = white_keys.first().unwrap();
        assert!((first.rect.left() - 0.0).abs() < 0.01);

        // Last white key ends at right edge
        let last = white_keys.last().unwrap();
        assert!(
            (last.rect.right() - 750.0).abs() < 0.01,
            "last white key right edge: {}, expected 750.0",
            last.rect.right()
        );
    }

    #[test]
    fn layout_white_keys_have_equal_width() {
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        let white_keys: Vec<_> = keys.iter().filter(|k| !k.is_black).collect();
        let expected_width = 750.0 / NUM_WHITE_KEYS as f32;
        for (i, wk) in white_keys.iter().enumerate() {
            assert!(
                (wk.rect.width() - expected_width).abs() < 0.01,
                "white key {} width: {}, expected {}",
                i,
                wk.rect.width(),
                expected_width
            );
        }
    }

    #[test]
    fn layout_white_keys_have_full_height() {
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        for key in keys.iter().filter(|k| !k.is_black) {
            assert!(
                (key.rect.height() - 120.0).abs() < 0.01,
                "white key height: {}, expected 120.0",
                key.rect.height()
            );
        }
    }

    #[test]
    fn layout_black_keys_are_shorter_than_white() {
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        let black_height = 120.0 * 0.6;
        for key in keys.iter().filter(|k| k.is_black) {
            assert!(
                (key.rect.height() - black_height).abs() < 0.01,
                "black key height: {}, expected {}",
                key.rect.height(),
                black_height
            );
        }
    }

    #[test]
    fn layout_black_keys_are_narrower_than_white() {
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        let white_width = 750.0 / NUM_WHITE_KEYS as f32;
        let black_width = white_width * 0.6;
        for key in keys.iter().filter(|k| k.is_black) {
            assert!(
                (key.rect.width() - black_width).abs() < 0.01,
                "black key width: {}, expected {}",
                key.rect.width(),
                black_width
            );
        }
    }

    #[test]
    fn layout_no_keys_exceed_keyboard_bounds() {
        let mut mn = None;
        let kb = make_keyboard(&[], &mut mn);
        let keys = kb.compute_layout();
        for key in &keys {
            assert!(
                key.rect.top() >= -0.01,
                "key {} top {} is above keyboard",
                key.note,
                key.rect.top()
            );
            assert!(
                key.rect.bottom() <= 120.01,
                "key {} bottom {} exceeds keyboard height",
                key.note,
                key.rect.bottom()
            );
        }
    }

    #[test]
    fn layout_handles_zero_size_rect() {
        let mut mn = None;
        let kb = PianoKeyboard {
            rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(0.0, 0.0)),
            held_notes: &[],
            octave_offset: 0,
            mouse_note: &mut mn,
        };
        let keys = kb.compute_layout();
        // Should still produce 25 keys, just with zero-size rects
        assert_eq!(keys.len(), 25);
    }

    #[test]
    fn layout_handles_offset_rect() {
        let mut mn = None;
        let kb = PianoKeyboard {
            rect: egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(750.0, 120.0)),
            held_notes: &[],
            octave_offset: 0,
            mouse_note: &mut mn,
        };
        let keys = kb.compute_layout();
        // First white key should start at x=100
        let first_white = keys.iter().find(|k| !k.is_black).unwrap();
        assert!((first_white.rect.left() - 100.0).abs() < 0.01);
        // All keys should start at y=50
        for key in &keys {
            assert!((key.rect.top() - 50.0).abs() < 0.01);
        }
    }
}
