use navo_core::paging::ByteSource;

use super::ApplyError;

pub(super) struct Stable<'a> {
    target: &'a dyn ByteSource,
    source: &'a dyn ByteSource,
    target_len: u64,
    source_len: u64,
}

impl<'a> Stable<'a> {
    pub(super) fn start(
        target: &'a dyn ByteSource,
        source: &'a dyn ByteSource,
    ) -> Result<Self, ApplyError> {
        let stable = Self {
            target,
            source,
            target_len: target.len(),
            source_len: source.len(),
        };
        stable.check()?;
        Ok(stable)
    }

    pub(super) fn finish(&self, result: Result<(), ApplyError>) -> Result<(), ApplyError> {
        self.check()?;
        result
    }

    fn check(&self) -> Result<(), ApplyError> {
        let target = flagged(self.target) || self.target.len() != self.target_len;
        let source = flagged(self.source) || self.source.len() != self.source_len;
        if target || source {
            return Err(ApplyError::Changed);
        }
        Ok(())
    }
}

fn flagged(side: &dyn ByteSource) -> bool {
    let changed = side.take_changed();
    let rewritten = side.take_rewritten();
    changed || rewritten
}
