use std::time::Duration;

use super::combo::Combo;

const SHORTEST_MS: i64 = 150;
const LONGEST_MS: i64 = 2000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Delay(Duration);

impl Delay {
    #[must_use]
    pub fn from_ms(ms: i64) -> Self {
        let ms = ms.clamp(SHORTEST_MS, LONGEST_MS).unsigned_abs();
        Self(Duration::from_millis(ms))
    }

    #[must_use]
    pub fn get(self) -> Duration {
        self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Arm {
    pub token: u64,
    pub delay: Duration,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Phase {
    #[default]
    Idle,
    Waiting {
        combo: Combo,
        token: u64,
    },
    Shown(Combo),
    Cancelled,
}

#[derive(Clone, Debug, Default)]
pub struct Hold {
    phase: Phase,
    tokens: u64,
}

impl Hold {
    pub fn modifiers(&mut self, held: Option<Combo>, delay: Delay) -> Option<Arm> {
        let Some(combo) = held else {
            self.phase = Phase::Idle;
            return None;
        };
        if combo.system {
            self.phase = Phase::Cancelled;
            return None;
        }
        match self.phase {
            Phase::Cancelled => None,
            Phase::Shown(_) => {
                self.phase = Phase::Shown(combo);
                None
            }
            Phase::Waiting { combo: waiting, .. } if waiting == combo => None,
            Phase::Idle | Phase::Waiting { .. } => {
                self.tokens += 1;
                let token = self.tokens;
                self.phase = Phase::Waiting { combo, token };
                Some(Arm {
                    token,
                    delay: delay.get(),
                })
            }
        }
    }

    pub fn key_pressed(&mut self) {
        if matches!(self.phase, Phase::Waiting { .. }) {
            self.phase = Phase::Cancelled;
        }
    }

    pub fn elapsed(&mut self, token: u64) -> bool {
        match self.phase {
            Phase::Waiting {
                combo,
                token: armed,
            } if armed == token => {
                self.phase = Phase::Shown(combo);
                true
            }
            _ => false,
        }
    }

    pub fn cancel(&mut self) {
        if self.phase != Phase::Idle {
            self.phase = Phase::Cancelled;
        }
    }

    #[must_use]
    pub fn shown(&self) -> Option<Combo> {
        match self.phase {
            Phase::Shown(combo) => Some(combo),
            _ => None,
        }
    }
}
