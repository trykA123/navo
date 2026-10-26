use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusty_core::Location;
use rusty_core::retention::{RetainedDeletePlan, RetainedState, RetentionEntry};
use rusty_fs::{CancelToken, ProviderOps};
use rusty_retention::{RetentionRun, Vault};
use rusty_store::{Db, JobState, StepState, UnfinishedJob};

use super::super::{JournalLog, SharedJournal, identity, lock};

struct Restore<'a> {
    job: &'a UnfinishedJob,
    journal: &'a SharedJournal,
    ops: &'a dyn ProviderOps,
    data: &'a Path,
}

pub(super) fn runtime_rollback(log: &JournalLog, data: &Path, ops: &dyn ProviderOps) -> bool {
    let result = (|| {
        let (_, job) = lock(&log.journal)
            .read_job(log.job)
            .map_err(|error| error.to_string())?;
        let (db, _) = crate::crash::open_db(&rusty_store::StoreDirs::journal_in(data))
            .map_err(|error| error.to_string())?;
        restore_job(
            db,
            Restore {
                job: &job,
                journal: &log.journal,
                ops,
                data,
            },
        )
    })();
    if let Err(error) = &result {
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
    let result = Db::open(&rusty_store::StoreDirs::journal_in(data), false)
        .map_err(|error| error.to_string())
        .and_then(|(db, _)| {
            restore_job(
                db,
                Restore {
                    job,
                    journal,
                    ops,
                    data,
                },
            )
        });
    match result {
        Ok(()) => JobState::RolledBack,
        Err(error) => {
            tracing::warn!(job = job.id.0, %error, "retained delete recovery remains pending");
            JobState::RestorePending
        }
    }
}

fn restore_job(db: Db, scope: Restore<'_>) -> Result<(), String> {
    let plan: RetainedDeletePlan =
        serde_json::from_slice(&scope.job.plan).map_err(|error| error.to_string())?;
    let roots: HashMap<_, _> = plan
        .root_steps()
        .into_iter()
        .map(|(seq, path)| (path, seq))
        .collect();
    let mut vault = Vault::new(db, scope.data, scope.ops).map_err(|error| error.to_string())?;
    let mut restored = HashSet::new();
    let mut after = 0;
    loop {
        let entries = vault
            .list_after(after, 16)
            .map_err(|error| error.to_string())?;
        if entries.is_empty() {
            break;
        }
        for entry in entries {
            after = entry.id.0;
            if entry.job_id != Some(scope.job.id.0) {
                continue;
            }
            scope.restore_entry(&mut vault, (&entry, &roots))?;
            restored.insert(entry.record.source);
        }
    }
    scope.check_missing(&roots, &restored)
}

impl Restore<'_> {
    fn restore_entry(
        &self,
        vault: &mut Vault<'_>,
        (entry, roots): (&RetentionEntry, &HashMap<Location, u32>),
    ) -> Result<(), String> {
        let seq = roots
            .get(&entry.record.source)
            .ok_or("retained source is outside its owner plan")?;
        let intent = identity::read(self.journal, self.job.id, *seq)
            .map_err(|error| format!("{error:?}"))?;
        if intent != Some(entry.record.nodes[0].identity) {
            return Err("retained source does not match its durable delete intent".to_owned());
        }
        let restored = vault
            .rollback(
                entry.id,
                self.job.id,
                &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}),
            )
            .map_err(|error| error.to_string())?;
        if !matches!(
            restored.state,
            RetainedState::Restored | RetainedState::Aborted
        ) {
            return Err("retained delete is not fully restored".to_owned());
        }
        Ok(())
    }

    fn check_missing(
        &self,
        roots: &HashMap<Location, u32>,
        restored: &HashSet<Location>,
    ) -> Result<(), String> {
        for (root, seq) in roots {
            if restored.contains(root) {
                continue;
            }
            let intent = identity::read(self.journal, self.job.id, *seq)
                .map_err(|error| format!("{error:?}"))?;
            match intent {
                Some(identity) if self.ops.file_identity(root).ok().flatten() == Some(identity) => (),
                None if self.job.steps.iter().any(|step| step.seq == *seq && step.state == StepState::Pending) => (),
                _ => return Err("delete intent has no matching source or retained payload; manual recovery is required".to_owned()),
            }
        }
        Ok(())
    }
}
