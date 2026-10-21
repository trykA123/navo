use std::sync::Arc;

use rusty_core::OperationPlan;
use rusty_store::{FinishedJob, JobId, StepState};
use rusty_ui::browser::{NamedPreset, RenameHooks, RenamePresetList, UndoOffer};

use crate::files::{SharedDb, lock as lock_db, now_ms};
use crate::jobs::{self, SharedJournal};

const KIND: &str = "rename";

pub fn hooks(journal: Option<SharedJournal>, db: Option<SharedDb>) -> RenameHooks {
    let (last, undo) = (journal.clone(), journal);
    let (list, save, delete) = (db.clone(), db.clone(), db);
    RenameHooks {
        last: Arc::new(move || match &last {
            Some(journal) => offer(journal),
            None => Ok(None),
        }),
        undo_log: Arc::new(move |offer: &UndoOffer| {
            let journal = undo.as_ref().ok_or_else(no_journal)?;
            let job = i64::try_from(offer.job).map_err(|error| error.to_string())?;
            jobs::undo_log(journal, JobId(job))
        }),
        presets: Arc::new(move || presets(list.as_ref().ok_or_else(no_database)?)),
        save: Arc::new(move |name, rules| {
            save_preset(save.as_ref().ok_or_else(no_database)?, name, rules)
        }),
        delete: Arc::new(move |name| delete_preset(delete.as_ref().ok_or_else(no_database)?, name)),
    }
}

fn no_journal() -> String {
    "the job journal is not available".to_owned()
}

fn no_database() -> String {
    "the database is not available".to_owned()
}

fn offer(journal: &SharedJournal) -> Result<Option<UndoOffer>, String> {
    let job = jobs::lock(journal)
        .last_undoable(KIND)
        .map_err(|error| error.to_string())?;
    job.map(decode).transpose()
}

fn decode(job: FinishedJob) -> Result<UndoOffer, String> {
    let plan: OperationPlan =
        serde_json::from_slice(&job.plan).map_err(|error| error.to_string())?;
    let done = job
        .steps
        .iter()
        .filter(|step| step.state == StepState::Done && !step.is_undone())
        .map(|step| step.seq)
        .collect();
    Ok(UndoOffer {
        job: u64::try_from(job.id.0).map_err(|error| error.to_string())?,
        plan,
        done,
        created_at: job.created_at,
        restore_pending: job.restore_pending,
    })
}

fn presets(db: &SharedDb) -> Result<RenamePresetList, String> {
    let stored = lock_db(db).presets().map_err(|error| error.to_string())?;
    Ok(RenamePresetList {
        named: stored
            .named
            .into_iter()
            .map(|preset| NamedPreset {
                name: preset.name,
                rules: preset.rules,
            })
            .collect(),
        last_used: stored.last_used.map(|preset| preset.rules),
    })
}

fn save_preset(db: &SharedDb, name: &str, rules: &str) -> Result<(), String> {
    let db = lock_db(db);
    let saved = if name.is_empty() {
        db.save_last_used(rules, now_ms())
    } else {
        db.save_preset(name, rules, now_ms())
    };
    saved.map_err(|error| error.to_string())
}

fn delete_preset(db: &SharedDb, name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Ok(());
    }
    lock_db(db)
        .delete_preset(name)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
