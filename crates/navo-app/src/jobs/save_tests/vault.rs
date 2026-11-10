use std::sync::Arc;

use navo_core::retention::RetainedState;
use navo_fs::{JobOutcome, Provider, ProviderOps};
use navo_store::JobState;
use navo_ui::browser::UndoKind;

use super::super::lock;
use super::retained::{NEW_HASH, OLD_HASH, STAGED, Saving, TARGET, hash_at, put};

fn saved() -> Saving {
    let saving = Saving::new("");
    assert_eq!(saving.save(STAGED, (3, 1_000)), JobOutcome::Done);
    saving
}

fn restore(saving: &Saving) -> navo_core::Location {
    let ops: Arc<dyn ProviderOps> = saving.fs.clone();
    let hooks = crate::retained_hooks::hooks(Some(&saving.dirs), true, ops);
    hooks.restore(saving.entries()[0].id).unwrap()
}

#[test]
fn a_finished_save_keeps_the_exact_old_bytes_in_the_vault() {
    let saving = saved();
    assert_eq!(saving.hash(TARGET), Some(NEW_HASH));
    let [entry] = saving.entries().try_into().unwrap();
    assert_eq!(entry.state, RetainedState::Ready);
    let held = entry.record.held().unwrap();
    assert_eq!(hash_at(&saving.fs, &held), Some(OLD_HASH));
    let old = saving.fs.metadata(&held).unwrap();
    assert_eq!((old.size, old.modified_ms), (3, Some(1_000)));
    let job = navo_store::JobId(entry.job_id.unwrap());
    assert_eq!(
        lock(&saving.journal).read_job(job).unwrap().0,
        JobState::Done
    );
    assert_eq!(lock(&saving.journal).held(job).unwrap().len(), 1);
}

#[test]
fn undo_of_an_unchanged_save_is_offered_as_a_retained_restore_in_place() {
    let saving = saved();
    let hooks = crate::rename_hooks::hooks(Some(saving.journal.clone()), None);
    let offer = (hooks.last)().unwrap().unwrap();
    assert_eq!(offer.kind, UndoKind::Retained(vec![saving.entries()[0].id]));
    let place = restore(&saving);
    assert_eq!(place, saving.fs.location(TARGET));
    assert_eq!(hash_at(&saving.fs, &place), Some(OLD_HASH));
    assert_eq!(saving.fs.metadata(&place).unwrap().size, 3);
    assert!(
        saving
            .fs
            .metadata(&saving.fs.location("dest/a (restored).txt"))
            .is_err()
    );
}

#[test]
fn an_outside_edit_after_a_save_is_kept_and_the_old_version_restored_beside_it() {
    let saving = saved();
    put(&saving.fs, TARGET, (5, 9_000), [9; 32]);
    let place = restore(&saving);
    assert_eq!(place, saving.fs.location("dest/a (restored).txt"));
    assert_eq!(hash_at(&saving.fs, &place), Some(OLD_HASH));
    assert_eq!(saving.hash(TARGET), Some([9; 32]));
}

#[test]
fn the_next_save_enforces_the_volume_cap_on_an_earlier_saved_version() {
    let saving = Saving::new("[retention]\nmax-volume-mib = 1\nmin-free-mib = 0\n");
    put(&saving.fs, TARGET, (2 * 1024 * 1024, 1_000), OLD_HASH);
    assert_eq!(
        saving.save(STAGED, (2 * 1024 * 1024, 1_000)),
        JobOutcome::Done
    );
    assert_eq!(saving.entries()[0].state, RetainedState::Ready);
    put(&saving.fs, "dest/.a.txt.next", (13, 3_000), [3; 32]);
    assert_eq!(
        saving.save("dest/.a.txt.next", (11, 2_000)),
        JobOutcome::Done
    );
    let saved = saving.entries();
    assert_eq!(saved[0].state, RetainedState::Purged);
    assert_eq!(saved[1].state, RetainedState::Ready);
    assert!(saving.fs.metadata(&saved[0].record.container).is_err());
    assert_eq!(saving.hash(TARGET), Some([3; 32]));
}
