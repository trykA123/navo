use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusty_core::{FsError, FsErrorKind, OpKind, OperationPlan};
use rusty_fs::{FileIdentity, HeldFile, JobLog, JobOutcome, PlacedFile, ProviderOps, StepOutcome};
use rusty_store::{JobId, JobJournal, JobState, JournalError};
use rusty_ui::browser::OpLog;

mod deletion;
mod guard_log;
mod held;
mod identity;
pub(crate) mod maintenance;
mod marks;
mod mutation_lock;
#[cfg(test)]
pub(crate) use mutation_lock::test_file as test_mutation_file;
pub(crate) use mutation_lock::{claim_exclusive, claim_recovery};
mod overwrite;
mod pending;
mod profile;
mod recovery;
mod temporary;

pub(crate) use guard_log::{guard_factory, guard_write};
pub use pending::StillPending;
#[cfg(test)]
pub use pending::still_pending;
pub(crate) use profile::{claim_write, read_retention, retains_deletes, write_factory};
#[cfg(test)]
pub use profile::{confined_factory, experimental_factory};
pub use profile::{profile_factory, retaining_factory};
pub use recovery::{RecoveryReport, recover_profile};
#[cfg(test)]
mod default_profile;
#[cfg(test)]
pub use default_profile::{log_factory, recover};

pub(crate) const KEEP_FINISHED: u32 = 50;

pub type SharedJournal = Arc<Mutex<JobJournal>>;

pub fn open(file: &Path) -> Result<SharedJournal, JournalError> {
    let (db, _) = crate::crash::open_db(file)?;
    Ok(Arc::new(Mutex::new(JobJournal::new(db)?)))
}

pub(crate) fn lock(journal: &SharedJournal) -> MutexGuard<'_, JobJournal> {
    journal
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

pub struct Instance {
    file: File,
    pub sole: bool,
}

impl Instance {
    pub fn into_file(self) -> File {
        self.file
    }

    pub fn share(self) -> Option<File> {
        if self.sole {
            self.file.unlock().ok()?;
            self.file.lock_shared().ok()?;
        }
        Some(self.file)
    }
}

pub fn claim(file: &Path) -> Option<Instance> {
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(file)
        .ok()?;
    if lock.try_lock().is_ok() {
        return Some(Instance {
            file: lock,
            sole: true,
        });
    }
    lock.lock_shared().ok()?;
    Some(Instance {
        file: lock,
        sole: false,
    })
}

fn create_log(
    journal: &SharedJournal,
    plan: &OperationPlan,
    retained: Option<deletion::RetainedJob>,
) -> Result<JournalLog, String> {
    let (kind, bytes) = match &retained {
        Some(job) => (
            rusty_core::retention::RETAINED_DELETE_KIND.to_owned(),
            serde_json::to_vec(&job.plan),
        ),
        None => (
            format!("{:?}", plan.kind).to_lowercase(),
            serde_json::to_vec(plan),
        ),
    };
    let bytes = bytes.map_err(|error| error.to_string())?;
    let steps = u32::try_from(plan.steps.len()).map_err(|error| error.to_string())?;
    let job = lock(journal)
        .create_job(&kind, &bytes, steps, now_ms())
        .map_err(|error| error.to_string())?;
    let mut log = JournalLog::new(Arc::clone(journal), job, kind);
    log.renames = plan.kind == OpKind::Rename;
    log.retained = retained;
    Ok(log)
}

mod undo;
pub(crate) use undo::claim_undo_log;
pub use undo::profile_undo;
#[cfg(test)]
pub use undo::undo_log;

struct JournalLog {
    journal: SharedJournal,
    job: JobId,
    kind: String,
    held: Vec<HeldFile>,
    stranded: bool,
    put_back_failed: bool,
    undoing: bool,
    renames: bool,
    retained: Option<deletion::RetainedJob>,
    delete_policy: Option<rusty_core::retention::RetentionPolicy>,
    overwrites: Option<overwrite::WriteJob>,
    global_lease: Option<File>,
    experimental_retention: bool,
    deferred_undo: bool,
    profile: Option<rusty_store::StoreDirs>,
}

impl JournalLog {
    fn new(journal: SharedJournal, job: JobId, kind: String) -> Self {
        Self {
            journal,
            job,
            kind,
            held: Vec::new(),
            stranded: false,
            put_back_failed: false,
            undoing: false,
            renames: false,
            retained: None,
            delete_policy: None,
            overwrites: None,
            global_lease: None,
            experimental_retention: false,
            deferred_undo: false,
            profile: None,
        }
    }
}

impl JournalLog {
    fn complete(&self, state: JobState, ops: &dyn ProviderOps) -> Result<(), String> {
        if self.overwrites.is_some() {
            return overwrite::finish(self, state, ops);
        }
        if !self.experimental_retention {
            self.complete_like_main(state, ops);
            return Ok(());
        }
        lock(&self.journal)
            .finish(self.job, state)
            .map_err(|error| error.to_string())?;
        if !self.stranded {
            held::release(&self.journal, self.job, &self.held, ops)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

impl JournalLog {
    fn complete_like_main(&self, state: JobState, ops: &dyn ProviderOps) {
        if let Err(error) = lock(&self.journal).finish(self.job, state) {
            tracing::error!(job = self.job.0, %error, "recording the finished job failed");
            return;
        }
        if !self.stranded
            && let Err(error) = held::release(&self.journal, self.job, &self.held, ops)
        {
            tracing::warn!(job = self.job.0, %error, "releasing held files failed");
        }
    }
}

impl JobLog for JournalLog {
    fn preserve_overwrites(&self) -> bool {
        self.overwrites.is_some()
    }

    fn temporary_namespace(&self) -> Option<String> {
        self.overwrites
            .as_ref()
            .and_then(|request| request.plan.temporary_namespace())
            .map(str::to_owned)
    }
    fn prepare_temporary(
        &mut self,
        proof: &rusty_core::retention::CopyTemporary,
    ) -> Result<(), FsError> {
        temporary::prepare(self, proof)
    }
    fn created_temporary(
        &mut self,
        created: (u32, FileIdentity),
        ops: &dyn ProviderOps,
    ) -> Result<(), FsError> {
        temporary::created(self, created, ops)
    }
    fn retire_temporary(&mut self, seq: u32, ops: &dyn ProviderOps) -> Result<(), FsError> {
        temporary::retire(self, seq, ops)
    }
    fn journal_proved_held(
        &mut self,
        held: &HeldFile,
        proof: &rusty_core::retention::OverwriteProof,
    ) -> Result<bool, FsError> {
        held::record_proved(&self.journal, self.job, (held, proof))?;
        self.held.push(held.clone());
        Ok(true)
    }
    fn prepare_rename(&mut self, seq: u32, identity: FileIdentity) -> Result<(), FsError> {
        identity::record(&self.journal, self.job, (seq, identity))
    }

    fn rename_identity(&self, seq: u32) -> Result<Option<FileIdentity>, FsError> {
        identity::read(&self.journal, self.job, seq)
    }

    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        marks::mark(self, seq, outcome)
    }

    fn steps(&mut self, marks: &[(u32, StepOutcome)]) -> Result<(), FsError> {
        marks::mark_all(self, marks)
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if outcome == StepOutcome::Failed {
            self.put_back_failed = true;
            return Ok(());
        }
        let marked = lock(&self.journal).mark_undone(self.job, seq);
        if marked.is_err() && self.renames {
            self.put_back_failed = true;
        }
        marked.map_err(|_| FsError::new(FsErrorKind::Unknown))
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        held::record(&self.journal, self.job, held)?;
        self.held.push(held.clone());
        Ok(true)
    }

    fn journal_placed(&mut self, placed: &PlacedFile) -> Result<(), FsError> {
        held::record_placed(&self.journal, self.job, placed)
    }

    fn held_stranded(&mut self, _: &HeldFile) {
        self.stranded = true;
    }
}

impl OpLog for JournalLog {
    fn check(
        &mut self,
        mut operation: rusty_ui::browser::LoggedOperation<'_>,
    ) -> Result<(), String> {
        profile::check_confirmed(self, &operation)?;
        self.claim_lease(&mut operation)?;
        profile::check_plan(self, &operation)?;
        if self.overwrites.is_some() && !operation.ops.supports_atomic_rename() {
            return Err(rusty_l10n::tr("job-retention-unsupported"));
        }
        if self.deferred_undo {
            crate::rename_hooks::claim_undo(&self.journal, self.job, JobState::RestorePending)?;
            self.deferred_undo = false;
        }
        Ok(())
    }

    fn execute(&mut self, operation: rusty_ui::browser::LoggedOperation<'_>) -> Option<JobOutcome> {
        self.retained.as_ref()?;
        Some(deletion::execute(self, operation))
    }

    fn finish(&mut self, outcome: &JobOutcome, ops: &dyn ProviderOps) -> Result<(), String> {
        let state = match outcome {
            _ if self.put_back_failed => JobState::RestorePending,
            JobOutcome::Done if self.undoing => JobState::Undone,
            _ if self.undoing => JobState::RestorePending,
            JobOutcome::Done => JobState::Done,
            JobOutcome::Cancelled => JobState::Cancelled,
            JobOutcome::Failed { .. } if self.stranded => JobState::RestorePending,
            JobOutcome::Failed { .. } => JobState::Failed,
        };
        if let JobOutcome::Failed { seq, error } = outcome {
            tracing::warn!(
                job = self.job.0,
                kind = self.kind,
                step = seq,
                error = ?error.kind,
                "job failed"
            );
        } else {
            tracing::info!(
                job = self.job.0,
                kind = self.kind,
                state = ?state,
                "job finished"
            );
        }
        self.complete(state, ops)?;
        maintenance::after(self, state, ops);
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod events_tests;

#[cfg(test)]
mod held_tests;

#[cfg(test)]
mod move_over_tests;

#[cfg(test)]
mod undo_guard_tests;

#[cfg(test)]
mod undo_retry_tests;

#[cfg(test)]
mod parallel_tests;

#[cfg(test)]
mod rename_tests;

#[cfg(test)]
mod link_tests;

#[cfg(test)]
mod delete_bench;

#[cfg(test)]
mod delete_tests;
