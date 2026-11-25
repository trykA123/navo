use std::path::Path;
use std::sync::{Arc, Mutex};

use navo_core::Location;
use navo_fs::ProviderOps;
use navo_store::{JobJournal, JournalError};

use super::StoreStart;

pub(super) fn start(data: Option<&Path>, unclean: bool, ops: &dyn ProviderOps) -> StoreStart {
    let Some(data) = data else {
        return StoreStart::default();
    };
    if let Err(error) = std::fs::create_dir_all(data) {
        tracing::warn!(%error, "profile directory is unavailable");
        return StoreStart::default();
    }
    let Some(instance) = crate::jobs::claim(&navo_store::StoreDirs::lock_in(data)) else {
        return StoreStart::default();
    };
    let mut kept = Vec::new();
    let opened = if instance.sole {
        recover(data, unclean, ops, &mut kept)
    } else {
        crate::jobs::open(&navo_store::StoreDirs::journal_in(data))
    };
    let journal = opened
        .inspect_err(|error| {
            if error.is_corruption() {
                super::super::record_failed_check(&navo_store::StoreDirs::journal_in(data));
            }
            tracing::error!(%error, "recovering unfinished jobs failed");
        })
        .ok();
    StoreStart {
        _legacy: instance.share(),
        journal,
        notice: super::RecoveryNotice::with_kept(None, data, kept),
        ..StoreStart::default()
    }
}

/// Recovers unfinished jobs; `kept` collects the move copies the recovery kept, even when a
/// later step fails.
fn recover(
    data: &Path,
    unclean: bool,
    ops: &dyn ProviderOps,
    kept: &mut Vec<Location>,
) -> Result<crate::jobs::SharedJournal, JournalError> {
    let (db, _) =
        super::super::unclean::open_legacy(&navo_store::StoreDirs::journal_in(data), unclean)?;
    let journal = Arc::new(Mutex::new(JobJournal::new(db)?));
    let report = crate::jobs::recover_keeping(&journal, ops, (data, &[]), (false, kept))?;
    crate::jobs::trim_at_startup(&journal, data, ops)?;
    crate::jobs::log_pending(&report.pending);
    Ok(journal)
}
