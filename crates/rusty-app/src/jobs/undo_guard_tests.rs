use rusty_core::ProviderPath;
use rusty_fs::{Attributes, EntryKind, FakeFs, FsEntry};
use rusty_store::JobId;

use super::held_tests::{
    beside, fake, held_rows, holding, journal, old_report, placed_rows, run, target,
};
use super::{SharedJournal, lock, recover};

const TARGET: &str = "dest/docs/report.txt";

fn pending_jobs(journal: &SharedJournal) -> usize {
    lock(journal).restore_pending().unwrap().len()
}

fn undone_steps(journal: &SharedJournal) -> usize {
    lock(journal)
        .restore_pending()
        .unwrap()
        .iter()
        .flat_map(|job| &job.steps)
        .filter(|step| step.is_undone())
        .count()
}

fn plain_copy() -> (FakeFs, SharedJournal) {
    let fs = fake();
    fs.remove(TARGET);
    let journal = journal();
    run(&fs, &journal, false);
    (fs, journal)
}

fn user_file(fs: &FakeFs, size: u64, modified_ms: Option<i64>) {
    fs.remove(TARGET);
    fs.add_entry(
        "dest/docs",
        FsEntry {
            name: ProviderPath::from_text("report.txt"),
            kind: EntryKind::File,
            size,
            modified_ms,
            attributes: Attributes::default(),
        },
    );
}

#[test]
fn an_unreadable_held_row_keeps_a_copy_that_has_no_held_record() {
    let (fs, journal) = plain_copy();
    lock(&journal)
        .record_held(JobId(1), b"not a record", 0)
        .unwrap();
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (
            target(&fs).map(|entry| entry.size),
            pending_jobs(&journal),
            undone_steps(&journal)
        ),
        (Some(7), 1, 0)
    );
}

#[test]
fn a_copy_target_that_changed_within_two_seconds_is_kept() {
    let (fs, journal) = plain_copy();
    user_file(&fs, 7, Some(9_001));
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        target(&fs).map(|entry| (entry.size, entry.modified_ms)),
        Some((7, Some(9_001)))
    );
}

#[test]
fn a_copy_target_without_modified_times_is_kept() {
    let fs = FakeFs::new();
    fs.add_file("home/docs/report.txt", 7);
    fs.add_folder("dest/docs");
    let journal = journal();
    run(&fs, &journal, false);
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(target(&fs).map(|entry| entry.size), Some(7));
}

#[test]
fn a_deleted_target_folder_ends_the_pending_restore() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    fs.remove("dest/docs");
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!((pending_jobs(&journal), held_rows(&journal)), (0, 0));
}

#[test]
fn an_absent_target_volume_keeps_the_restore_pending() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    fs.remove("dest");
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!((pending_jobs(&journal), held_rows(&journal)), (1, 1));
}

#[test]
fn a_deleted_source_folder_puts_the_old_file_beside_the_copy() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    fs.remove("home/docs");
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (
            target(&fs).map(|entry| entry.size),
            beside(&fs),
            holding(&fs),
            pending_jobs(&journal),
            held_rows(&journal)
        ),
        (Some(7), Some(old_report()), Vec::new(), 0, 0)
    );
}

#[test]
fn a_new_copy_is_journaled_with_its_stamp_until_the_job_finishes() {
    let (fs, journal) = plain_copy();
    assert_eq!(placed_rows(&journal), 1);
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!((target(&fs), placed_rows(&journal)), (None, 0));
}

#[test]
fn a_finished_copy_forgets_its_stamp() {
    let fs = fake();
    fs.remove(TARGET);
    let journal = journal();
    run(&fs, &journal, true);
    assert_eq!(
        (target(&fs).map(|entry| entry.size), placed_rows(&journal)),
        (Some(7), 0)
    );
}

#[test]
fn a_copy_without_a_journaled_stamp_is_kept() {
    let (fs, journal) = plain_copy();
    lock(&journal).forget_held(JobId(1)).unwrap();
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(target(&fs).map(|entry| entry.size), Some(7));
}

fn forget_volumes(journal: &SharedJournal) {
    let rows = lock(journal).held(JobId(1)).unwrap();
    lock(journal).forget_held(JobId(1)).unwrap();
    for row in rows {
        let mut value: serde_json::Value = serde_json::from_slice(&row).unwrap();
        if let Some(record) = value.as_object_mut() {
            record.remove("volume");
        }
        let bytes = serde_json::to_vec(&value).unwrap();
        lock(journal).record_held(JobId(1), &bytes, 0).unwrap();
    }
}

#[test]
fn a_missing_mount_point_keeps_the_restore_pending() {
    let fs = fake();
    fs.set_volume("dest/docs", 2);
    let journal = journal();
    run(&fs, &journal, false);
    fs.remove("dest/docs");
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!((pending_jobs(&journal), held_rows(&journal)), (1, 1));
}

#[test]
fn a_held_row_from_before_volumes_keeps_the_restore_pending() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    forget_volumes(&journal);
    fs.remove("dest/docs");
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!((pending_jobs(&journal), held_rows(&journal)), (1, 1));
}
