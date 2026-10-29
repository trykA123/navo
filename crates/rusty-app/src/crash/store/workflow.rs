use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusty_fs::ProviderOps;
use rusty_store::{Db, JobId, JobJournal, acknowledge_quarantine, quarantine_database};

use super::state::{Progress, State};
use super::{Error, RecoveryNotice, build, details, files, promote, refresh};

pub(super) struct Mode {
    pub unclean: bool,
    pub experimental: bool,
    pub sole: bool,
}

pub(super) fn run(
    data: &Path,
    mode: &Mode,
    ops: &dyn ProviderOps,
) -> Result<(Option<RecoveryNotice>, Option<crate::jobs::SharedJournal>), Error> {
    let Mode {
        unclean,
        experimental,
        sole,
    } = *mode;
    let mut state = State::load(data)?;
    let path = rusty_store::StoreDirs::journal_in(data);
    if !sole {
        if state.pending.is_some()
            || files::regular(&super::super::unclean::sentinel_path(&path))?
            || !files::regular(&path)?
        {
            return Err(Error::Invalid(
                "another database session prevents recovery; close Navo and restart".into(),
            ));
        }
        return Ok((RecoveryNotice::loaded(data, &state), None));
    }
    let _mutations = crate::jobs::claim_recovery(data, experimental).map_err(Error::Invalid)?;
    if state.pending.is_none() {
        clear_completed_receipt(data, &state)?;
        if artifacts(&path)? {
            state.begin(data)?;
        }
    }
    if state.pending.is_none() {
        let check = unclean || files::regular(&super::super::unclean::sentinel_path(&path))?;
        files::regular(&path)?;
        match Db::open(&path, check) {
            Ok((db, _)) => {
                let (journal, report) = recover(db, ops, (data, &[]))?;
                refresh::update(data, &mut state, &report, &journal)?;
                crate::jobs::lock(&journal).prune(crate::jobs::KEEP_FINISHED)?;
                clear_sentinel(&path)?;
                return Ok((RecoveryNotice::loaded(data, &state), Some(journal)));
            }
            Err(error) if error.is_database_corruption() => state.begin(data)?,
            Err(error) => return Err(error.into()),
        }
    }
    resume(data, &mut state, ops)?;
    clear_sentinel(&path)?;
    Ok((RecoveryNotice::loaded(data, &state), None))
}

fn artifacts(path: &Path) -> Result<bool, Error> {
    for suffix in [".quarantine-pending", ".quarantine-receipt"] {
        if files::regular(&files::suffix(path, suffix))? {
            return Ok(true);
        }
    }
    Ok(!files::regular(path)?
        && (files::regular(&files::suffix(path, "-wal"))?
            || files::regular(&files::suffix(path, "-shm"))?))
}

fn clear_completed_receipt(data: &Path, state: &State) -> Result<(), Error> {
    let path = rusty_store::StoreDirs::journal_in(data);
    if !files::regular(&files::suffix(&path, ".quarantine-receipt"))? {
        return Ok(());
    }
    let report = quarantine_database(&path)?
        .ok_or_else(|| Error::Invalid("quarantine receipt has no files".into()))?;
    if state
        .notices
        .iter()
        .any(|notice| notice.slot == report.slot)
    {
        acknowledge_quarantine(&path, report.slot)?;
    }
    Ok(())
}

fn resume(data: &Path, state: &mut State, ops: &dyn ProviderOps) -> Result<(), Error> {
    while let Some(progress) = state.pending.clone() {
        match progress {
            Progress::Preserve { case } => {
                let report = quarantine_database(&rusty_store::StoreDirs::journal_in(data))?
                    .ok_or_else(|| {
                        Error::Invalid("database files disappeared before quarantine".into())
                    })?;
                state.pending = Some(Progress::Build {
                    case,
                    slot: report.slot,
                    database: report.database.is_some(),
                });
                state.save(data)?;
            }
            Progress::Build {
                case,
                slot,
                database,
            } => build::rebuild(data, state, (case, slot, database))?,
            Progress::Promote { notice, identity } => {
                build::preserved(data, notice.slot)?;
                promote::install(data, &notice, &identity)?;
                let (db, _) = Db::open(&rusty_store::StoreDirs::journal_in(data), true)?;
                db.defer_checkpoint()?;
                let journal = JobJournal::new(db)?;
                let attempted = journal
                    .unfinished()?
                    .into_iter()
                    .chain(journal.restore_pending()?)
                    .map(|job| job.id.0)
                    .collect();
                state.pending = Some(Progress::Recover {
                    notice,
                    identity,
                    attempted,
                });
                state.save(data)?;
            }
            Progress::Recover {
                notice,
                identity,
                attempted,
            } => finish(data, state, (notice, identity, attempted), ops)?,
        }
    }
    Ok(())
}

fn finish(
    data: &Path,
    state: &mut State,
    (mut notice, identity, attempted): (super::state::Notice, super::state::Identity, Vec<i64>),
    ops: &dyn ProviderOps,
) -> Result<(), Error> {
    build::preserved(data, notice.slot)?;
    let path = rusty_store::StoreDirs::journal_in(data);
    if !promote::matches(&path, &identity)? {
        return Err(Error::Invalid(
            "the database was replaced before recovery; existing files were kept".into(),
        ));
    }
    let (db, _) = Db::open(&path, true)?;
    let expected = attempted.into_iter().map(JobId).collect::<Vec<_>>();
    let (journal, report) = recover(db, ops, (data, &expected))?;
    notice.recovered_jobs = report.restored.len();
    notice.retry_jobs = refresh::retry_jobs(&report.pending, &journal)?;
    notice.unreadable_jobs += report.pending.len();
    details::recovered(data, &notice, &report.pending)?;
    let slot = notice.slot;
    state.notices.push(notice);
    state.pending = None;
    state.save(data)?;
    acknowledge_quarantine(&path, slot)?;
    crate::jobs::lock(&journal).prune(crate::jobs::KEEP_FINISHED)?;
    Ok(())
}

fn recover(
    db: Db,
    ops: &dyn ProviderOps,
    scope: (&Path, &[JobId]),
) -> Result<(crate::jobs::SharedJournal, crate::jobs::RecoveryReport), Error> {
    db.defer_checkpoint()?;
    let journal = Arc::new(Mutex::new(JobJournal::new(db)?));
    let report = crate::jobs::recover_profile(&journal, ops, scope, true)?;
    tracing::info!(
        unfinished = report.unfinished,
        recovered = report.restored.len(),
        manual = report.pending.len(),
        "journal recovery checked"
    );
    for job in &report.pending {
        tracing::warn!(
            job = job.job.0,
            files = job.left.len(),
            "files not yet put back after recovery"
        );
    }
    Ok((journal, report))
}

fn clear_sentinel(path: &Path) -> Result<(), Error> {
    let sentinel = super::super::unclean::sentinel_path(path);
    if files::regular(&sentinel)? {
        fs::remove_file(sentinel)?;
    }
    Ok(())
}
