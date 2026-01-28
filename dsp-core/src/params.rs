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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variants_count() {
        assert_eq!(OscillatorType::VARIANTS.len(), 4);
    }

    #[test]
    fn test_variants_contains_all_types() {
        let v = OscillatorType::VARIANTS;
        assert!(v.contains(&OscillatorType::Sine));
        assert!(v.contains(&OscillatorType::Triangle));
        assert!(v.contains(&OscillatorType::Square));
        assert!(v.contains(&OscillatorType::Saw));
    }

    #[test]
    fn test_from_index_valid() {
        assert_eq!(OscillatorType::from_index(0), OscillatorType::Sine);
        assert_eq!(OscillatorType::from_index(1), OscillatorType::Triangle);
        assert_eq!(OscillatorType::from_index(2), OscillatorType::Square);
        assert_eq!(OscillatorType::from_index(3), OscillatorType::Saw);
    }

    #[test]
    fn test_from_index_out_of_range_clamps() {
        assert_eq!(OscillatorType::from_index(4), OscillatorType::Saw);
        assert_eq!(OscillatorType::from_index(100), OscillatorType::Saw);
        assert_eq!(OscillatorType::from_index(usize::MAX), OscillatorType::Saw);
    }

    #[test]
    fn test_name_returns_expected_strings() {
        assert_eq!(OscillatorType::Sine.name(), "Sine");
        assert_eq!(OscillatorType::Triangle.name(), "Triangle");
        assert_eq!(OscillatorType::Square.name(), "Square");
        assert_eq!(OscillatorType::Saw.name(), "Saw");
    }

    #[test]
    fn test_name_matches_variant_debug() {
        for variant in OscillatorType::VARIANTS {
            assert_eq!(variant.name(), format!("{:?}", variant));
        }
    }
}
