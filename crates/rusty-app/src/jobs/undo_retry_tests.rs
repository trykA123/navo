use rusty_fs::FakeFs;
use rusty_store::JobId;

use super::held_tests::{
    beside, crashed_with_a_failing_restore, fake, held_rows, holding, journal, old_report, run,
    target,
};
use super::{SharedJournal, lock, recover};

const NOTES: &str = "dest/docs/notes.txt";
const SUB: &str = "dest/docs/sub";

fn with_notes() -> FakeFs {
    let fs = fake();
    fs.add_file("home/docs/notes.txt", 5);
    fs
}

fn size_of(fs: &FakeFs, path: &str) -> Option<u64> {
    use rusty_fs::Provider;
    fs.metadata(&fs.location(path)).ok().map(|entry| entry.size)
}

fn pending_jobs(journal: &SharedJournal) -> usize {
    lock(journal).restore_pending().unwrap().len()
}

#[test]
fn a_retried_undo_keeps_a_file_the_user_made_after_the_first_undo() {
    let fs = with_notes();
    let journal = journal();
    crashed_with_a_failing_restore(&fs, &journal);
    assert_eq!(size_of(&fs, NOTES), None);
    fs.add_file(NOTES, 5);
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (target(&fs), size_of(&fs, NOTES), held_rows(&journal)),
        (Some(old_report()), Some(5), 0)
    );
}

#[test]
fn a_retried_undo_keeps_a_folder_the_user_made_after_the_first_undo() {
    let fs = fake();
    fs.add_file("home/docs/sub/x.txt", 3);
    let journal = journal();
    crashed_with_a_failing_restore(&fs, &journal);
    assert_eq!(size_of(&fs, SUB), None);
    fs.add_folder(SUB);
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!((size_of(&fs, SUB), held_rows(&journal)), (Some(0), 0));
}

#[test]
fn a_plain_copy_whose_source_vanished_keeps_the_old_file_beside_the_copy() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    fs.remove("home/docs/report.txt");
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (
            target(&fs).map(|entry| entry.size),
            beside(&fs),
            holding(&fs),
            held_rows(&journal)
        ),
        (Some(7), Some(old_report()), Vec::new(), 0)
    );
}

fn rewrite_rows(journal: &SharedJournal, change: impl Fn(&mut serde_json::Value)) {
    let rows = lock(journal).held(JobId(1)).unwrap();
    lock(journal).forget_held(JobId(1)).unwrap();
    for row in rows {
        let mut value: serde_json::Value = serde_json::from_slice(&row).unwrap();
        change(&mut value);
        let bytes = serde_json::to_vec(&value).unwrap();
        lock(journal).record_held(JobId(1), &bytes, 0).unwrap();
    }
}

#[test]
fn a_held_row_without_a_stamp_is_restored_beside_the_copy() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    rewrite_rows(&journal, |row| {
        let fields = row.as_object_mut().unwrap();
        fields.remove("placed_size");
        fields.remove("placed_modified_ms");
    });
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (
            target(&fs).map(|entry| entry.size),
            beside(&fs),
            holding(&fs),
            held_rows(&journal)
        ),
        (Some(7), Some(old_report()), Vec::new(), 0)
    );
}

#[test]
fn one_unreadable_held_row_does_not_stop_the_others_and_keeps_the_job_pending() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    lock(&journal)
        .record_held(JobId(1), b"not a record", 0)
        .unwrap();
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (target(&fs), pending_jobs(&journal), held_rows(&journal)),
        (Some(old_report()), 1, 2)
    );
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (target(&fs), pending_jobs(&journal), held_rows(&journal)),
        (Some(old_report()), 1, 2)
    );
}
