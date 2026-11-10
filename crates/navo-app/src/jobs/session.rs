use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[cfg(test)]
use navo_core::{FsErrorKind, Location};
use navo_core::{OpKind, OperationPlan};
use navo_fs::ProviderOps;
use navo_store::{JobId, JobState, StoreDirs};
use navo_ui::browser::LogFactory;

use super::{JournalLog, SharedJournal, held, lock, profile};

#[cfg(test)]
mod restore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kept {
    Held { experimental: bool },
    Retained,
}

/// Where "Restore previous version" put the old file back.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviousVersion {
    /// Retention off: the job's undo put the old bytes back at `at`.
    Undone { at: Location },
    /// D65 retention: each old version was restored beside the current file.
    Retained { at: Vec<Location> },
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    NotKept,
    Changed,
    Fs(FsErrorKind),
    Failed(String),
}

/// Compare jobs of this session; their held old files stay until `release` at exit.
#[derive(Clone)]
pub struct SessionHolds {
    journal: SharedJournal,
    dirs: StoreDirs,
    jobs: Arc<Mutex<BTreeMap<JobId, Kept>>>,
    compare_copies: bool,
}

impl SessionHolds {
    #[must_use]
    pub fn new(journal: SharedJournal, dirs: StoreDirs) -> Self {
        Self {
            journal,
            dirs,
            jobs: Arc::default(),
            compare_copies: false,
        }
    }

    #[must_use]
    pub fn journal(&self) -> SharedJournal {
        Arc::clone(&self.journal)
    }

    /// The job factory; saves made through it keep their held files for the session.
    pub fn factory(&self, drive: Option<Arc<dyn ProviderOps>>) -> LogFactory {
        profile::factory(
            Arc::clone(&self.journal),
            self.dirs.clone(),
            drive.map(profile::Drive::Probed),
            Some(self.clone()),
        )
    }

    /// The job factory for one job the user agreed to run without keeping old versions.
    pub fn factory_without_retention(&self) -> LogFactory {
        profile::factory(
            Arc::clone(&self.journal),
            self.dirs.clone(),
            Some(profile::Drive::Declined),
            Some(self.clone()),
        )
    }

    /// Compare saves and whole-file copies keep their old files for the session.
    pub fn compare_factory(&self, drive: Option<Arc<dyn ProviderOps>>) -> LogFactory {
        let holds = Self {
            compare_copies: true,
            ..self.clone()
        };
        holds.factory(drive)
    }

    pub(super) fn marks(&self, plan: &OperationPlan) -> bool {
        plan.kind == OpKind::Save || self.compare_copies && plan.kind == OpKind::Copy
    }

    /// Releases every session hold; held files whose target is gone are put back.
    pub fn release(&self, ops: &dyn ProviderOps) {
        let mut jobs = self.jobs();
        for (job, kept) in jobs.iter() {
            if matches!(kept, Kept::Held { .. })
                && let Err(error) = held::release_job(&self.journal, *job, ops)
            {
                tracing::warn!(job = job.0, %error, "session hold not released at exit");
            }
        }
        jobs.retain(|_, kept| *kept == Kept::Retained);
    }

    #[cfg(test)]
    pub fn restore_previous(
        &self,
        job: JobId,
        ops: &Arc<dyn ProviderOps>,
    ) -> Result<PreviousVersion, RestoreError> {
        self.restore_waiting(job, ops, super::mutation_lock::Wait::bounded())
    }

    #[cfg(test)]
    pub(super) fn restore_waiting(
        &self,
        job: JobId,
        ops: &Arc<dyn ProviderOps>,
        wait: super::mutation_lock::Wait<'_>,
    ) -> Result<PreviousVersion, RestoreError> {
        let kept = self.jobs().get(&job).copied();
        match kept {
            Some(held @ Kept::Held { experimental }) => {
                let _hold = super::default_lock::acquire(
                    &self.dirs.data,
                    super::mutation_lock::Mode::Exclusive,
                    wait,
                )
                .map_err(|error| RestoreError::Failed(error.into()))?;
                let _lease = super::mutation_lock::claim_recovery(&self.dirs.data, experimental)
                    .map_err(RestoreError::Failed)?;
                let jobs = self.jobs();
                if jobs.get(&job) != Some(&held) {
                    return Err(RestoreError::NotKept);
                }
                restore::claim(&self.journal, job)?;
                drop(jobs);
                restore::undo_held(&self.journal, job, ops.as_ref())
            }
            Some(Kept::Retained) => restore::retained(&self.dirs, job, ops),
            None => Err(RestoreError::NotKept),
        }
    }

    fn jobs(&self) -> MutexGuard<'_, BTreeMap<JobId, Kept>> {
        self.jobs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(super) fn keep(log: &JournalLog, kept: Kept) {
    if let Some(session) = &log.session {
        session.jobs().insert(log.job, kept);
    }
}

pub(super) fn finish(
    log: &JournalLog,
    state: JobState,
    ops: &dyn ProviderOps,
) -> Result<(), String> {
    if state == JobState::Done && log.kind == "copy" {
        held::hash_placed(&log.journal, log.job, ops)?;
    }
    lock(&log.journal)
        .finish(log.job, state)
        .map_err(|error| error.to_string())?;
    if !log.stranded {
        keep(
            log,
            Kept::Held {
                experimental: log.experimental_retention,
            },
        );
    }
    Ok(())
}
