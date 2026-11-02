use super::{RetainedState, RetentionEntry, RetentionError, RetentionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionSummary {
    pub(super) id: RetentionId,
    pub(super) volume: u64,
    pub(super) state: RetainedState,
    pub(super) pinned: bool,
    pub(super) bytes: u64,
    pub(super) created_at: i64,
    pub(super) expires_at: i64,
}

impl TryFrom<&RetentionEntry> for RetentionSummary {
    type Error = RetentionError;

    fn try_from(entry: &RetentionEntry) -> Result<Self, Self::Error> {
        entry.validate()?;
        Ok(Self {
            id: entry.id,
            volume: entry.record.parent_identity.volume,
            state: entry.state,
            pinned: entry.pinned,
            bytes: entry.record.bytes()?,
            created_at: entry.record.created_at,
            expires_at: entry.record.expires_at,
        })
    }
}

impl RetentionSummary {
    pub fn protect(&mut self) {
        self.pinned = true;
    }

    pub fn id(self) -> RetentionId {
        self.id
    }

    pub fn bytes_on(self, volume: u64) -> u64 {
        if self.volume == volume && !self.state.is_terminal() {
            self.bytes
        } else {
            0
        }
    }

    pub fn resumes_purge(self, volume: u64) -> bool {
        self.volume == volume && self.state == RetainedState::Purging && !self.pinned
    }
}
