use std::collections::{HashMap, HashSet};
use std::path::Path;

use navo_core::retention::{RetainedDeletePlan, RetentionEntry};
use navo_core::{FsErrorKind, Location};
use navo_fs::{CancelToken, ProviderOps};
use navo_retention::{RetentionRun, Vault};
use navo_store::{Db, JobState, StepState, UnfinishedJob};

use super::super::{JournalLog, SharedJournal, identity, lock};

struct Restore<'a> {
    job: &'a UnfinishedJob,
    journal: &'a SharedJournal,
    ops: &'a dyn ProviderOps,
    data: &'a Path,
}

/// Why a rollback stopped: `Retry` may pass on a later start, `Attention` never will.
enum Failure {
    Retry(String),
    Attention(String),
}

impl Failure {
    fn retry(error: impl std::fmt::Display) -> Self {
        Self::Retry(error.to_string())
    }

    fn attention(error: impl std::fmt::Display) -> Self {
        Self::Attention(error.to_string())
    }

    fn message(&self) -> &str {
        match self {
            Self::Retry(error) | Self::Attention(error) => error,
        }
    }
}

/// Without an entry nothing shows the volume is present, so a missing file is retried.
impl From<navo_retention::Error> for Failure {
    fn from(error: navo_retention::Error) -> Self {
        if error.needs_attention(|| false) {
            Self::attention(error)
        } else {
            Self::retry(error)
        }
    }
}

pub(super) fn runtime_rollback(log: &JournalLog, data: &Path, ops: &dyn ProviderOps) -> bool {
    let result = (|| {
        let (_, job) = lock(&log.journal)
            .read_job(log.job)
            .map_err(Failure::retry)?;
        let (db, _) = crate::crash::open_db(&navo_store::StoreDirs::journal_in(data))
            .map_err(Failure::retry)?;
        restore_job(
            db,
            &Restore {
                job: &job,
                journal: &log.journal,
                ops,
                data,
            },
        )
    })();
    if let Err(error) = &result {
        let error = error.message();
        tracing::warn!(job = log.job.0, %error, "retained delete rollback remains pending");
    }
    result.is_ok()
}

pub(in crate::jobs) fn recover(
    journal: &SharedJournal,
    job: &UnfinishedJob,
    scope: (&Path, &dyn ProviderOps),
) -> JobState {
    let (data, ops) = scope;
    let result = Db::open(&navo_store::StoreDirs::journal_in(data), false)
        .map_err(Failure::retry)
        .and_then(|(db, _)| {
            db.defer_checkpoint().map_err(Failure::retry)?;
            restore_job(
                db,
                &Restore {
                    job,
                    journal,
                    ops,
                    data,
                },
            )
        });
    match result {
        Ok(()) => JobState::RolledBack,
        Err(Failure::Retry(error)) => {
            tracing::warn!(job = job.id.0, %error, "retained delete recovery remains pending");
            JobState::RestorePending
        }
        Err(Failure::Attention(error)) => {
            tracing::warn!(job = job.id.0, %error, "retained delete recovery needs attention");
            JobState::Failed
        }
    }
}

fn restore_job(db: Db, scope: &Restore<'_>) -> Result<(), Failure> {
    let plan: RetainedDeletePlan =
        serde_json::from_slice(&scope.job.plan).map_err(Failure::attention)?;
    let roots: HashMap<_, _> = plan
        .root_steps()
        .into_iter()
        .map(|(seq, path)| (path, seq))
        .collect();
    let mut vault = Vault::new(db, scope.data, scope.ops)?;
    let mut outcome = Outcome::default();
    let mut after = 0;
    loop {
        let entries = vault.list_after(after, 16)?;
        let Some(last) = entries.last() else {
            break;
        };
        after = last.id.0;
        for entry in entries
            .iter()
            .filter(|entry| entry.job_id == Some(scope.job.id.0))
        {
            let result = scope.restore_entry(&mut vault, (entry, &roots));
            outcome.record(scope.job, entry, result)?;
        }
    }
    match (scope.check_missing(&roots, &outcome), outcome.attention) {
        (Err(Failure::Retry(error)), _) => Err(Failure::Retry(error)),
        (_, Some(attention)) => Err(attention),
        (missing, None) => missing,
    }
}

#[derive(Default)]
struct Outcome {
    restored: HashSet<Location>,
    /// Sources whose entry passed the volume gate: their volume is shown present.
    shown: HashSet<Location>,
    attention: Option<Failure>,
}

impl Outcome {
    /// Goes on past an entry that needs attention; a failure that may pass stops the job.
    fn record(
        &mut self,
        job: &UnfinishedJob,
        entry: &RetentionEntry,
        result: Result<(), Failure>,
    ) -> Result<(), Failure> {
        if !matches!(result, Err(Failure::Retry(_))) {
            self.shown.insert(entry.record.source.clone());
        }
        match result {
            Ok(()) => {
                self.restored.insert(entry.record.source.clone());
            }
            Err(Failure::Attention(error)) => {
                tracing::warn!(
                    job = job.id.0,
                    entry = entry.id.0,
                    %error,
                    "retained entry needs attention"
                );
                self.attention.get_or_insert(Failure::Attention(error));
            }
            Err(retry) => return Err(retry),
        }
        Ok(())
    }
}

impl Restore<'_> {
    fn restore_entry(
        &self,
        vault: &mut Vault<'_>,
        (entry, roots): (&RetentionEntry, &HashMap<Location, u32>),
    ) -> Result<(), Failure> {
        // Before any check reads the path, so a foreign volume on the same letter is never judged.
        if !vault.on_recorded_volume(entry) {
            return Err(Failure::retry(
                "the volume holding the retained copy is not shown present",
            ));
        }
        let seq = roots
            .get(&entry.record.source)
            .ok_or_else(|| Failure::attention("retained source is outside its owner plan"))?;
        let intent = identity::read(self.journal, self.job.id, *seq)
            .map_err(|error| Failure::retry(format!("{error:?}")))?;
        if intent != Some(entry.record.nodes[0].identity) {
            return Err(Failure::attention(
                "retained source does not match its durable delete intent",
            ));
        }
        match vault.rollback(
            entry.id,
            self.job.id,
            &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}),
        ) {
            Ok(_) => Ok(()),
            Err(error) if vault.needs_attention(&error, entry) => Err(Failure::attention(error)),
            Err(error) => Err(Failure::retry(error)),
        }
    }

    fn check_missing(
        &self,
        roots: &HashMap<Location, u32>,
        outcome: &Outcome,
    ) -> Result<(), Failure> {
        for (root, seq) in roots {
            if outcome.restored.contains(root) {
                continue;
            }
            let intent = identity::read(self.journal, self.job.id, *seq)
                .map_err(|error| Failure::retry(format!("{error:?}")))?;
            let found = match self.ops.file_identity(root) {
                Err(error) if error.kind == FsErrorKind::NotFound => None,
                found => found.map_err(|error| Failure::retry(format!("{error:?}")))?,
            };
            let settled = match intent {
                Some(identity) => found == Some(identity),
                None => self
                    .job
                    .steps
                    .iter()
                    .any(|step| step.seq == *seq && step.state == StepState::Pending),
            };
            if settled {
                continue;
            }
            let foreign = match (intent, found) {
                (Some(recorded), Some(found)) => found.volume != recorded.volume,
                (_, None) => !outcome.shown.contains(root) && !self.volume_present(root, intent),
                (None, Some(_)) => false,
            };
            if foreign {
                return Err(Failure::retry(
                    "delete source is not on the volume its intent recorded",
                ));
            }
            return Err(Failure::attention(
                "delete intent has no matching source or retained payload; manual recovery is required",
            ));
        }
        Ok(())
    }

    /// The nearest existing folder above `at` is on the volume the intent recorded, so a folder
    /// mount counts; with no recorded volume, unknown.
    fn volume_present(&self, at: &Location, intent: Option<navo_fs::FileIdentity>) -> bool {
        let Some(recorded) = intent else {
            return false;
        };
        let mut folder = at.parent();
        while let Some(current) = folder {
            match self.ops.file_identity(&current) {
                Ok(Some(found)) => return found.volume == recorded.volume,
                Err(error) if error.kind == FsErrorKind::NotFound => folder = current.parent(),
                _ => return false,
            }
        }
        false
    }
}
