use std::sync::Arc;

use rusty_core::Location;
use rusty_core::retention::RetentionId;

use super::Browser;
use super::rename_batch::RenameSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetainedKind {
    Delete,
    Overwrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetainedState {
    Ready,
    Busy,
    NeedsAttention,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedRow {
    pub id: RetentionId,
    pub original: Location,
    pub kind: RetainedKind,
    pub bytes: u64,
    pub deleted_at_ms: i64,
    pub expires_at_ms: i64,
    pub state: RetainedState,
}

pub trait RetainedHooks: Send + Sync {
    fn list(&self, after: Option<RetentionId>, limit: u16) -> Result<Vec<RetainedRow>, String>;
    fn restore(&self, id: RetentionId) -> Result<Location, String>;
    fn purge(&self, id: RetentionId) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoRetained;

impl RetainedHooks for NoRetained {
    fn list(&self, _after: Option<RetentionId>, _limit: u16) -> Result<Vec<RetainedRow>, String> {
        Ok(Vec::new())
    }

    fn restore(&self, _id: RetentionId) -> Result<Location, String> {
        Err(rusty_l10n::tr("retained-unavailable"))
    }

    fn purge(&self, _id: RetentionId) -> Result<(), String> {
        Err(rusty_l10n::tr("retained-unavailable"))
    }
}

pub(super) struct HookSlots {
    pub rename: RenameSettings,
    pub retained: Arc<dyn RetainedHooks>,
}

impl Default for HookSlots {
    fn default() -> Self {
        Self {
            rename: RenameSettings::default(),
            retained: Arc::new(NoRetained),
        }
    }
}

impl Browser {
    pub fn set_retained_hooks(&mut self, hooks: Arc<dyn RetainedHooks>) {
        self.hooks.retained = hooks;
    }

    #[must_use]
    pub fn retained_hooks(&self) -> Arc<dyn RetainedHooks> {
        Arc::clone(&self.hooks.retained)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_retained_lists_nothing() {
        for after in [None, Some(RetentionId(0)), Some(RetentionId(i64::MAX))] {
            for limit in [0, 1, u16::MAX] {
                assert_eq!(NoRetained.list(after, limit), Ok(Vec::new()));
            }
        }
    }

    #[test]
    fn no_retained_refuses_restore_and_purge() {
        let message = rusty_l10n::tr("retained-unavailable");
        assert_eq!(NoRetained.restore(RetentionId(1)), Err(message.clone()));
        assert_eq!(NoRetained.purge(RetentionId(1)), Err(message));
    }
}
