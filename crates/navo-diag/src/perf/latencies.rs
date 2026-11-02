use super::{Latency, LatencySample, Ring};

#[derive(Debug, Clone)]
pub struct Latencies {
    rings: [Ring<LatencySample>; Latency::ALL.len()],
}

impl Latencies {
    pub(super) fn new() -> Self {
        Self {
            rings: Latency::ALL.map(|kind| Ring::new(kind.capacity())),
        }
    }

    pub fn push(&mut self, sample: LatencySample) {
        self.rings[sample.kind.index()].push(sample);
    }

    #[must_use]
    pub fn of(&self, kind: Latency) -> &Ring<LatencySample> {
        &self.rings[kind.index()]
    }

    pub fn iter(&self) -> impl Iterator<Item = &LatencySample> {
        self.rings.iter().flat_map(Ring::iter)
    }

    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.rings.iter().map(Ring::dropped).sum()
    }
}
