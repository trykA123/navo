//! Marks an active journal follow in the volume's shared status.

use std::sync::PoisonError;

use super::answer::Shared;

pub(super) struct Updating<'a>(&'a Shared);

impl<'a> Updating<'a> {
    /// Marks the volume as updating and returns the guard for this follow attempt.
    pub(super) fn begin(shared: &'a Shared) -> Self {
        shared
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .status
            .progress = Some(100);
        Self(shared)
    }
}

impl Drop for Updating<'_> {
    /// Clears the updating marker if the volume still has it.
    fn drop(&mut self) {
        let mut state = self.0.write().unwrap_or_else(PoisonError::into_inner);
        if state.status.progress == Some(100) {
            state.status.progress = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use navo_index::ntfs::NtfsError;

    use super::*;

    /// Checks that a journal read error clears the updating status.
    #[test]
    fn follow_error_clears_updating_progress() {
        let shared = Shared::default();
        let follow = || -> Result<(), NtfsError> {
            let _updating = Updating::begin(&shared);
            Err(NtfsError::Reenumerate("journal"))
        };

        assert!(matches!(follow(), Err(NtfsError::Reenumerate("journal"))));
        assert_eq!(shared.read().unwrap().status.progress, None);
    }
}
