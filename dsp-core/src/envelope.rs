/// ADSR envelope generator.
///
/// Produces a gain multiplier in [0, 1] that shapes the amplitude of a note
/// over time. The envelope transitions through stages:
/// Idle → Attack → Decay → Sustain → Release → Idle
#[derive(Debug)]
pub struct Envelope {
    stage: Stage,
    level: f32,
    sample_rate: f32,

    // Times in seconds
    attack: f32,
    decay: f32,
    sustain: f32, // level, not time
    release: f32,

    // Per-sample increments (computed from times + sample rate)
    attack_rate: f32,
    decay_rate: f32,
    release_rate: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl Envelope {
    pub fn new() -> Self {
        let mut env = Self {
            stage: Stage::Idle,
            level: 0.0,
            sample_rate: 44100.0,
            attack: 0.01,
            decay: 0.1,
            sustain: 0.7,
            release: 0.3,
            attack_rate: 0.0,
            decay_rate: 0.0,
            release_rate: 0.0,
        };
        env.recalculate_rates();
        env
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.recalculate_rates();
    }

    pub fn set_attack(&mut self, seconds: f32) {
        self.attack = seconds.max(0.001);
        self.attack_rate = 1.0 / (self.attack * self.sample_rate);
    }

    pub fn set_decay(&mut self, seconds: f32) {
        self.decay = seconds.max(0.001);
        self.decay_rate = (1.0 - self.sustain) / (self.decay * self.sample_rate);
    }

    pub fn set_sustain(&mut self, level: f32) {
        self.sustain = level.clamp(0.0, 1.0);
        // Decay rate depends on sustain level
        self.decay_rate = (1.0 - self.sustain) / (self.decay * self.sample_rate);
    }

    pub fn set_release(&mut self, seconds: f32) {
        self.release = seconds.max(0.001);
        self.release_rate = self.sustain / (self.release * self.sample_rate);
    }

    pub fn note_on(&mut self) {
        self.stage = Stage::Attack;
        // Don't reset level to 0 — allows retriggering without clicks
    }

    pub fn note_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
            // Recalculate release rate from current level so it reaches 0
            // in the configured release time
            let release_samples = self.release * self.sample_rate;
            self.release_rate = self.level / release_samples;
        }
    }

    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// Produce the next envelope value and advance state.
    pub fn tick(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => 0.0,
            Stage::Attack => {
                self.level += self.attack_rate;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
                self.level
            }
            Stage::Decay => {
                self.level -= self.decay_rate;
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.stage = Stage::Sustain;
                }
                self.level
            }
            Stage::Sustain => self.level,
            Stage::Release => {
                self.level -= self.release_rate;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
                self.level
            }
        }
    }

    fn recalculate_rates(&mut self) {
        self.attack_rate = 1.0 / (self.attack * self.sample_rate);
        self.decay_rate = (1.0 - self.sustain) / (self.decay * self.sample_rate);
        self.release_rate = self.sustain / (self.release * self.sample_rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_n(env: &mut Envelope, n: usize) -> f32 {
        let mut last = 0.0;
        for _ in 0..n {
            last = env.tick();
        }
        last
    }

    fn collect_ticks(env: &mut Envelope, n: usize) -> Vec<f32> {
        (0..n).map(|_| env.tick()).collect()
    }

    #[test]
    fn test_idle_returns_zero() {
        let mut env = Envelope::new();
        let samples = collect_ticks(&mut env, 10);
        assert!(samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn test_idle_is_not_active() {
        let env = Envelope::new();
        assert!(!env.is_active());
    }

    #[test]
    fn test_note_on_activates() {
        let mut env = Envelope::new();
        env.note_on();
        assert!(env.is_active());
    }

    #[test]
    fn test_attack_reaches_one() {
        let mut env = Envelope::new();
        // attack = 0.01s, SR = 44100 → 441 samples
        env.note_on();
        let val = tick_n(&mut env, 441);
        assert!(
            (val - 1.0).abs() < 1e-3,
            "attack should reach 1.0, got {}",
            val
        );
    }

    #[test]
    fn test_attack_is_monotonically_increasing() {
        let mut env = Envelope::new();
        env.note_on();
        let samples = collect_ticks(&mut env, 441);
        for window in samples.windows(2) {
            assert!(
                window[1] >= window[0],
                "attack decreased: {} -> {}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn test_decay_reaches_sustain() {
        let mut env = Envelope::new();
        // defaults: attack=0.01s (441 samples), decay=0.1s (4410 samples), sustain=0.7
        env.note_on();
        tick_n(&mut env, 441); // skip attack
        let val = tick_n(&mut env, 4410); // decay
        assert!(
            (val - 0.7).abs() < 1e-3,
            "decay should reach sustain 0.7, got {}",
            val
        );
    }

    #[test]
    fn test_sustain_holds_steady() {
        let mut env = Envelope::new();
        env.note_on();
        tick_n(&mut env, 441 + 4410); // skip attack + decay
        let samples = collect_ticks(&mut env, 10000);
        for (i, &s) in samples.iter().enumerate() {
            assert!(
                (s - 0.7).abs() < 2e-3,
                "sustain not steady at sample {}: {}",
                i, s
            );
        }
    }

    #[test]
    fn test_release_reaches_zero() {
        let mut env = Envelope::new();
        // defaults: release=0.3s, sustain=0.7
        env.note_on();
        tick_n(&mut env, 441 + 4410); // into sustain
        env.note_off();
        // release from 0.7 over 0.3s = 13230 samples
        let val = tick_n(&mut env, 13230 + 100); // extra margin
        assert!(val.abs() < 1e-3, "release should reach 0.0, got {}", val);
    }

    #[test]
    fn test_release_reaches_idle() {
        let mut env = Envelope::new();
        env.note_on();
        tick_n(&mut env, 441 + 4410); // into sustain
        env.note_off();
        tick_n(&mut env, 13230 + 100);
        assert!(!env.is_active(), "should be idle after release");
    }

    #[test]
    fn test_note_off_during_attack() {
        let mut env = Envelope::new();
        env.note_on();
        // Tick 200 samples into attack (less than 441)
        let level = tick_n(&mut env, 200);
        assert!(level > 0.0 && level < 1.0, "should be mid-attack: {}", level);
        env.note_off();
        assert!(env.is_active(), "should still be active during release");
        // Release takes 0.3s = 13230 samples (note_off recalculates rate from current level)
        let mut count = 0;
        while env.is_active() && count < 20000 {
            env.tick();
            count += 1;
        }
        assert!(!env.is_active(), "should reach idle");
        assert!(
            (count as i32 - 13230).abs() < 10,
            "release should take ~13230 samples, took {}",
            count
        );
    }

    #[test]
    fn test_retrigger_does_not_reset_to_zero() {
        let mut env = Envelope::new();
        env.note_on();
        let level_before = tick_n(&mut env, 200);
        assert!(level_before > 0.0);
        // Retrigger
        env.note_on();
        let level_after = env.tick();
        assert!(
            level_after >= level_before,
            "retrigger dropped level: {} -> {}",
            level_before,
            level_after
        );
    }

    #[test]
    fn test_minimum_time_clamping_attack() {
        let mut env = Envelope::new();
        env.set_attack(0.0); // should clamp to 0.001
        env.note_on();
        // 0.001s * 44100 ≈ 44 samples
        let val = tick_n(&mut env, 50);
        assert!(
            val >= 0.95,
            "attack with min time should reach ~1.0, got {}",
            val
        );
    }

    #[test]
    fn test_minimum_time_clamping_decay() {
        let mut env = Envelope::new();
        env.set_decay(0.0); // should clamp to 0.001
        env.note_on();
        tick_n(&mut env, 441); // complete attack
        // Decay should be very fast: 0.001s * 44100 ≈ 44 samples
        let val = tick_n(&mut env, 50);
        assert!(
            (val - 0.7).abs() < 0.05,
            "fast decay should reach sustain, got {}",
            val
        );
    }

    #[test]
    fn test_minimum_time_clamping_release() {
        let mut env = Envelope::new();
        env.set_release(0.0); // should clamp to 0.001
        env.note_on();
        tick_n(&mut env, 441 + 4410); // into sustain
        env.note_off();
        // 0.001s * 44100 ≈ 44 samples for release
        tick_n(&mut env, 50);
        assert!(!env.is_active(), "fast release should reach idle");
    }

    #[test]
    fn test_zero_sustain_envelope() {
        let mut env = Envelope::new();
        env.set_sustain(0.0);
        env.note_on();
        // After attack (441) + decay, level should reach 0
        let val = tick_n(&mut env, 441 + 4410 + 100);
        assert!(val.abs() < 1e-3, "zero sustain should reach 0, got {}", val);
        // note_off with level ≈ 0 should reach idle quickly
        env.note_off();
        env.tick();
        assert!(
            !env.is_active(),
            "release from zero should immediately go idle"
        );
    }

    #[test]
    fn test_full_sustain_envelope() {
        let mut env = Envelope::new();
        env.set_sustain(1.0);
        env.note_on();
        tick_n(&mut env, 441); // attack reaches 1.0
        // With sustain=1.0, decay_rate = 0, so level stays at 1.0
        let samples = collect_ticks(&mut env, 100);
        for (i, &s) in samples.iter().enumerate() {
            assert!(
                (s - 1.0).abs() < 1e-6,
                "full sustain not holding at sample {}: {}",
                i, s
            );
        }
    }
}
