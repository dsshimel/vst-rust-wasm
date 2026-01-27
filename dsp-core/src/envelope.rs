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
