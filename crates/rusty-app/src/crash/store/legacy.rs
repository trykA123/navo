use std::path::Path;
use std::sync::{Arc, Mutex};

use rusty_fs::ProviderOps;
use rusty_store::{JobJournal, JournalError};

use super::StoreStart;

pub(super) fn start(data: Option<&Path>, unclean: bool, ops: &dyn ProviderOps) -> StoreStart {
    let Some(data) = data else {
        return StoreStart::default();
    };
    if let Err(error) = std::fs::create_dir_all(data) {
        tracing::warn!(%error, "profile directory is unavailable");
        return StoreStart::default();
    }
    let Some(instance) = crate::jobs::claim(&rusty_store::StoreDirs::lock_in(data)) else {
        return StoreStart::default();
    };
    let opened = if instance.sole {
        recover(data, unclean, ops)
    } else {
        crate::jobs::open(&rusty_store::StoreDirs::journal_in(data))
    };
    let journal = opened
        .inspect_err(|error| {
            if error.is_corruption() {
                super::super::record_failed_check(&rusty_store::StoreDirs::journal_in(data));
            }
            tracing::error!(%error, "recovering unfinished jobs failed");
        })
        .ok();
    StoreStart {
        _legacy: instance.share(),
        journal,
        ..StoreStart::default()
    }
}

fn recover(
    data: &Path,
    unclean: bool,
    ops: &dyn ProviderOps,
) -> Result<crate::jobs::SharedJournal, JournalError> {
    let (db, _) =
        super::super::unclean::open_legacy(&rusty_store::StoreDirs::journal_in(data), unclean)?;
    let journal = Arc::new(Mutex::new(JobJournal::new(db)?));
    let report = crate::jobs::recover_profile(&journal, ops, (data, &[]), false)?;
    crate::jobs::lock(&journal).prune(crate::jobs::KEEP_FINISHED)?;
    for pending in report.pending {
        tracing::warn!(
            job = pending.job.0,
            files = pending.left.len(),
            "files not yet put back after recovery"
        );
    }
    Ok(journal)
}
