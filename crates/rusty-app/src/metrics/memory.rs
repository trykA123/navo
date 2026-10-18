#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Memory {
    pub(super) private_bytes: Option<u64>,
    pub(super) working_set_bytes: Option<u64>,
    pub(super) peak_working_set_bytes: Option<u64>,
}

impl Memory {
    pub(super) fn sample() -> Self {
        let sampled = crate::diag::probe().sample().ok();
        Self {
            private_bytes: sampled.map(|memory| memory.private_bytes),
            working_set_bytes: sampled.map(|memory| memory.resident_bytes),
            peak_working_set_bytes: super::platform::peak_working_set(),
        }
    }
}
