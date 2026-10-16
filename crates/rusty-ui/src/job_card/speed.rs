use std::collections::HashMap;
use std::time::{Duration, Instant};

const MIN_GAP: Duration = Duration::from_millis(250);
const SMOOTHING: f64 = 0.3;
const STALLED_BELOW: f64 = 0.05;

#[derive(Clone, Copy)]
struct Sample {
    at: Instant,
    bytes: u64,
    speed: Option<f64>,
    peak: f64,
}

#[derive(Default)]
pub struct SpeedMeter {
    samples: HashMap<u64, Sample>,
}

impl SpeedMeter {
    pub fn observe(&mut self, id: u64, bytes: u64, now: Instant) {
        let Some(last) = self.samples.get(&id).copied() else {
            let first = Sample {
                at: now,
                bytes,
                speed: None,
                peak: 0.,
            };
            self.samples.insert(id, first);
            return;
        };
        let elapsed = now.saturating_duration_since(last.at);
        if elapsed < MIN_GAP {
            return;
        }
        let rate = bytes.saturating_sub(last.bytes) as f64 / elapsed.as_secs_f64();
        let speed = last
            .speed
            .map_or(rate, |speed| speed + SMOOTHING * (rate - speed));
        let next = Sample {
            at: now,
            bytes,
            speed: Some(speed),
            peak: last.peak.max(speed),
        };
        self.samples.insert(id, next);
    }

    pub fn speed(&self, id: u64) -> Option<u64> {
        let sample = self.samples.get(&id)?;
        let speed = sample.speed?;
        if speed <= sample.peak * STALLED_BELOW {
            return None;
        }
        Some(speed.round() as u64)
    }

    pub fn retain(&mut self, ids: &[u64]) {
        self.samples.retain(|id, _| ids.contains(id));
    }
}
