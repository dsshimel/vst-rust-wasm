/// Oscillator waveform types available in the synth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscillatorType {
    Sine,
    Triangle,
    Square,
    Saw,
}

impl OscillatorType {
    pub const VARIANTS: &'static [OscillatorType] = &[
        OscillatorType::Sine,
        OscillatorType::Triangle,
        OscillatorType::Square,
        OscillatorType::Saw,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            OscillatorType::Sine => "Sine",
            OscillatorType::Triangle => "Triangle",
            OscillatorType::Square => "Square",
            OscillatorType::Saw => "Saw",
        }
    }

    pub fn from_index(index: usize) -> Self {
        Self::VARIANTS[index.min(Self::VARIANTS.len() - 1)]
    }
}
