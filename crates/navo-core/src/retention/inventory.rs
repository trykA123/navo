use super::{RetainedState, RetentionEntry, RetentionError, RetentionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionSummary {
    pub(super) id: RetentionId,
    pub(super) volume: u64,
    pub(super) state: RetainedState,
    pub(super) pinned: bool,
    resume_blocked: bool,
    pub(super) bytes: u64,
    pub(super) created_at: i64,
    pub(super) expires_at: i64,
    pub(super) attempts: u32,
    /// A Conflict left by an interrupted restore; maintenance checks it again.
    pub(super) restoring: bool,
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
            resume_blocked: entry.pinned && !entry.kept,
            bytes: entry.record.bytes()?,
            created_at: entry.record.created_at,
            expires_at: entry.record.expires_at,
            attempts: 0,
            restoring: entry.state == RetainedState::Conflict && entry.target.is_some(),
        })
    }
}

impl RetentionSummary {
    pub fn protect(&mut self) {
        self.pinned = true;
        self.resume_blocked = true;
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

    /// Maintenance attempts started on this entry; more attempts sort it later.
    #[must_use]
    pub fn with_attempts(self, attempts: u32) -> Self {
        Self { attempts, ..self }
    }

    pub fn attempts(self) -> u32 {
        self.attempts
    }

    /// An interrupted purge or restore that maintenance settles before choosing new purges.
    pub fn resumes(self, volume: u64) -> bool {
        self.volume == volume
            && (self.restoring
                || matches!(
                    self.state,
                    RetainedState::Purging | RetainedState::Restoring
                ))
            && !self.resume_blocked
    }
}
