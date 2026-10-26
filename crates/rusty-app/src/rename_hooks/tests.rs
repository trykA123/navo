use std::sync::{Arc, Mutex};

use rusty_core::{OpKind, OperationPlan, Step, Totals};
use rusty_fs::{FakeFs, JobOutcome, ProviderOps, StepOutcome};
use rusty_store::{Db, JobJournal, JobState};
use rusty_ui::browser::{RenameHooks, UndoOffer};

use super::hooks;
use crate::files::SharedDb;
use crate::jobs::{self, SharedJournal, lock, log_factory};

mod single;

fn memory_journal() -> SharedJournal {
    Arc::new(Mutex::new(
        JobJournal::new(Db::open_in_memory().unwrap()).unwrap(),
    ))
}

fn memory_db() -> SharedDb {
    Arc::new(Mutex::new(Db::open_in_memory().unwrap()))
}

fn plan(fs: &FakeFs, moves: &[(&str, &str)]) -> OperationPlan {
    let at = |name: &str| fs.location(&format!("d/{name}"));
    OperationPlan {
        kind: OpKind::Rename,
        steps: moves
            .iter()
            .map(|(from, to)| Step::Rename {
                source: at(from),
                target: at(to),
            })
            .collect(),
        totals: Totals {
            files: u64::try_from(moves.len()).unwrap(),
            folders: 0,
            bytes: 0,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    }
}

fn journal_batch(journal: &SharedJournal, plan: &OperationPlan, fs: &FakeFs) {
    let mut log = log_factory(Arc::clone(journal))(plan).unwrap();
    for seq in 0..plan.steps.len() {
        log.step(u32::try_from(seq).unwrap(), StepOutcome::Done)
            .unwrap();
    }
    log.finish(&JobOutcome::Done, fs as &dyn ProviderOps);
}

fn offer(hooks: &RenameHooks) -> Option<UndoOffer> {
    (hooks.last)().unwrap()
}

#[test]
fn a_journaled_rename_gives_an_offer() {
    let fs = FakeFs::new();
    let journal = memory_journal();
    let batch = plan(&fs, &[("a", "x"), ("b", "y")]);
    journal_batch(&journal, &batch, &fs);
    let offer = offer(&hooks(Some(journal), None)).expect("an offer");
    assert_eq!(offer.plan, batch);
    assert_eq!(offer.done, [0, 1]);
    assert_eq!(offer.items(), 2);
    assert!(!offer.restore_pending);
}

#[test]
fn there_is_no_offer_without_a_journal_or_a_rename() {
    let journal = memory_journal();
    assert!(offer(&hooks(Some(journal), None)).is_none());
    assert!(offer(&hooks(None, None)).is_none());
}

#[test]
fn an_undone_batch_marks_undone_and_the_older_batch_is_offered() {
    let fs = FakeFs::new();
    let journal = memory_journal();
    let older = plan(&fs, &[("c", "z")]);
    journal_batch(&journal, &older, &fs);
    journal_batch(&journal, &plan(&fs, &[("a", "x"), ("b", "y")]), &fs);
    let hooks = hooks(Some(Arc::clone(&journal)), None);
    let newest = offer(&hooks).unwrap();
    let mut log = (hooks.undo_log)(&newest).unwrap();
    log.undone(0, StepOutcome::Done).unwrap();
    log.undone(1, StepOutcome::Done).unwrap();
    log.finish(&JobOutcome::Done, &fs as &dyn ProviderOps);
    let next = offer(&hooks).unwrap();
    assert_ne!(next.job, newest.job);
    assert_eq!(next.plan, older);
}

#[test]
fn the_only_batch_is_gone_once_it_is_undone() {
    let fs = FakeFs::new();
    let journal = memory_journal();
    journal_batch(&journal, &plan(&fs, &[("a", "x")]), &fs);
    let hooks = hooks(Some(journal), None);
    let mut log = (hooks.undo_log)(&offer(&hooks).unwrap()).unwrap();
    log.undone(0, StepOutcome::Done).unwrap();
    log.finish(&JobOutcome::Done, &fs as &dyn ProviderOps);
    assert!(offer(&hooks).is_none());
}

#[test]
fn the_offer_is_still_there_after_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("navo.db");
    let fs = FakeFs::new();
    let batch = plan(&fs, &[("a", "x"), ("b", "y")]);
    journal_batch(&jobs::open(&file).unwrap(), &batch, &fs);
    let reopened = hooks(Some(jobs::open(&file).unwrap()), None);
    let offer = offer(&reopened).expect("an offer");
    assert_eq!((offer.plan, offer.done), (batch, vec![0, 1]));
}

#[test]
fn an_undone_batch_stays_gone_after_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("navo.db");
    let fs = FakeFs::new();
    let journal = jobs::open(&file).unwrap();
    journal_batch(&journal, &plan(&fs, &[("a", "x")]), &fs);
    let first = hooks(Some(journal), None);
    let mut log = (first.undo_log)(&offer(&first).unwrap()).unwrap();
    log.undone(0, StepOutcome::Done).unwrap();
    log.finish(&JobOutcome::Done, &fs as &dyn ProviderOps);
    drop((log, first));
    assert!(offer(&hooks(Some(jobs::open(&file).unwrap()), None)).is_none());
}

#[test]
fn an_undo_stopped_halfway_is_offered_again_as_restore_pending() {
    let fs = FakeFs::new();
    let journal = memory_journal();
    journal_batch(&journal, &plan(&fs, &[("a", "x"), ("b", "y")]), &fs);
    let hooks = hooks(Some(journal), None);
    let mut log = (hooks.undo_log)(&offer(&hooks).unwrap()).unwrap();
    log.undone(0, StepOutcome::Done).unwrap();
    log.undone(1, StepOutcome::Failed).unwrap();
    log.finish(&JobOutcome::Done, &fs as &dyn ProviderOps);
    let again = offer(&hooks).expect("still offered");
    assert!(again.restore_pending);
    assert_eq!(again.done, [1]);
}

#[test]
fn opening_an_undo_marks_the_batch_restore_pending_at_once() {
    let fs = FakeFs::new();
    let journal = memory_journal();
    journal_batch(&journal, &plan(&fs, &[("a", "x")]), &fs);
    let hooks = hooks(Some(Arc::clone(&journal)), None);
    let log = (hooks.undo_log)(&offer(&hooks).unwrap()).unwrap();
    drop(log);
    assert_eq!(lock(&journal).restore_pending().unwrap().len(), 1);
    assert!(offer(&hooks).unwrap().restore_pending);
}

#[test]
fn a_journal_that_cannot_be_read_is_an_error_not_a_missing_offer() {
    let journal = memory_journal();
    let job = lock(&journal)
        .create_job("rename", b"not json", 1, 1)
        .unwrap();
    lock(&journal).finish(job, JobState::Done).unwrap();
    assert!((hooks(Some(journal), None).last)().is_err());
    assert!(
        (hooks(None, None).undo_log)(&UndoOffer {
            job: 1,
            plan: plan(&FakeFs::new(), &[]),
            done: Vec::new(),
            created_at: 0,
            restore_pending: false,
        })
        .is_err()
    );
}

const RULES: &str = r#"{"v":1,"mask":"[N]"}"#;

#[test]
fn the_empty_name_is_the_last_used_slot_and_never_a_named_preset() {
    let hooks = hooks(None, Some(memory_db()));
    (hooks.save)("", RULES).unwrap();
    (hooks.save)("mine", r#"{"v":1,"mask":"[C]"}"#).unwrap();
    let list = (hooks.presets)().unwrap();
    assert_eq!(list.last_used.as_deref(), Some(RULES));
    let names: Vec<_> = list
        .named
        .iter()
        .map(|preset| preset.name.as_str())
        .collect();
    assert_eq!(names, ["mine"]);
}

#[test]
fn saving_last_used_again_replaces_it() {
    let hooks = hooks(None, Some(memory_db()));
    (hooks.save)("", RULES).unwrap();
    (hooks.save)("", r#"{"v":1,"mask":"[N]_[C]"}"#).unwrap();
    let list = (hooks.presets)().unwrap();
    assert_eq!(
        list.last_used.as_deref(),
        Some(r#"{"v":1,"mask":"[N]_[C]"}"#)
    );
    assert!(list.named.is_empty());
}

#[test]
fn a_blank_name_cannot_become_a_named_preset() {
    let hooks = hooks(None, Some(memory_db()));
    assert!((hooks.save)("  ", RULES).is_err());
    let list = (hooks.presets)().unwrap();
    assert!(list.named.is_empty() && list.last_used.is_none());
}

#[test]
fn deleting_the_empty_name_keeps_last_used_and_deleting_a_preset_removes_it() {
    let hooks = hooks(None, Some(memory_db()));
    (hooks.save)("", RULES).unwrap();
    (hooks.save)("mine", RULES).unwrap();
    (hooks.delete)("").unwrap();
    (hooks.delete)("mine").unwrap();
    let list = (hooks.presets)().unwrap();
    assert_eq!(list.last_used.as_deref(), Some(RULES));
    assert!(list.named.is_empty());
}

#[test]
fn presets_without_a_database_are_errors() {
    let hooks = hooks(None, None);
    assert!((hooks.presets)().is_err());
    assert!((hooks.save)("mine", RULES).is_err());
    assert!((hooks.delete)("mine").is_err());
}
