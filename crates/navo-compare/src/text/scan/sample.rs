/// One line in `rate` becomes a candidate, at most `cap` kept; a full sample
/// doubles its rate and keeps only the lines that still match, so every key
/// keeps all or none of its occurrences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub rate: u32,
    pub cap: usize,
}

impl From<u32> for Sample {
    fn from(rate: u32) -> Self {
        Self {
            rate,
            cap: usize::MAX,
        }
    }
}
