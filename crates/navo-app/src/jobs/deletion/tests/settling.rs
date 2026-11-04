use navo_store::{JobId, JobState};

use super::*;

fn captured(journal: &SharedJournal, dirs: &StoreDirs, fs: &FakeFs) -> JobId {
    fs.add_file("home/a", 8);
    captured_roots(journal, dirs, fs, &["home/a"])
}

fn captured_roots(journal: &SharedJournal, dirs: &StoreDirs, fs: &FakeFs, roots: &[&str]) -> JobId {
    let roots: Vec<_> = roots.iter().map(|root| fs.location(root)).collect();
    let plan = delete(fs, &roots);
    let mut log = crate::jobs::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, fs), JobOutcome::Done);
    drop(log);
    let job = super::super::lock(journal)
        .unfinished()
        .unwrap()
        .remove(0)
        .id;
    super::super::lock(journal)
        .finish(job, JobState::RestorePending)
        .unwrap();
    job
}

fn recovered(
    journal: &SharedJournal,
    dirs: &StoreDirs,
    fs: &FakeFs,
) -> crate::jobs::RecoveryReport {
    crate::jobs::recover_profile(journal, fs, (&dirs.data, &[]), true).unwrap()
}

fn assert_not_retried(journal: &SharedJournal, dirs: &StoreDirs, fs: &FakeFs, job: JobId) {
    let first = recovered(journal, dirs, fs);
    let (after_first, _) = super::super::lock(journal).read_job(job).unwrap();
    assert!(
        first
            .pending
            .iter()
            .any(|pending| pending.job == job && pending.attention),
        "{job:?} was not reported for attention on the first start: {:?} / {after_first:?}",
        first.pending
    );
    let second = recovered(journal, dirs, fs);
    let (state, _) = super::super::lock(journal).read_job(job).unwrap();
    assert!(
        second.pending.is_empty(),
        "{job:?} was retried on the next start with {:?}",
        second.pending
    );
    assert!(
        !matches!(state, JobState::Running | JobState::RestorePending),
        "{job:?} never reached a final state: {state:?}"
    );
}

#[test]
fn a_retained_delete_whose_source_was_replaced_stops_being_retried() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    let job = captured(&journal, &dirs, &fs);
    let held = entries(&dirs).remove(0).record.held().unwrap();
    fs.add_file("home/a", 99);
    assert_not_retried(&journal, &dirs, &fs, job);
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 99);
    assert_eq!(fs.metadata(&held).unwrap().size, 8);
}

#[test]
fn a_retained_delete_whose_retained_copy_vanished_stops_being_retried() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    let job = captured(&journal, &dirs, &fs);
    let held = entries(&dirs).remove(0).record.held().unwrap();
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection()
        .execute("DELETE FROM retained_entry", [])
        .unwrap();
    assert_not_retried(&journal, &dirs, &fs, job);
    assert_eq!(fs.metadata(&held).unwrap().size, 8);
}

#[test]
fn a_settled_delete_never_runs_again_and_keeps_its_retained_copy() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    let job = captured(&journal, &dirs, &fs);
    let held = entries(&dirs).remove(0).record.held().unwrap();
    fs.add_file("home/a", 99);
    assert_not_retried(&journal, &dirs, &fs, job);
    let third = recovered(&journal, &dirs, &fs);
    assert!(third.pending.is_empty() && third.restored.is_empty());
    assert_eq!(
        super::super::lock(&journal).read_job(job).unwrap().0,
        JobState::Failed
    );
    let entry = entries(&dirs).remove(0);
    assert_eq!(entry.state, RetainedState::Conflict);
    assert!(
        entry.pinned && entry.kept,
        "a failed job keeps its copy until the user acts"
    );
    assert_eq!(fs.calls(&held.path().to_text(), navo_fs::FakeOp::Rename), 0);
    assert_eq!(fs.metadata(&held).unwrap().size, 8);
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 99);
}

#[test]
fn a_transient_io_error_is_retried_on_the_next_start() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    let job = captured(&journal, &dirs, &fs);
    let held = entries(&dirs).remove(0).record.held().unwrap();
    fs.fault(
        &held.path().to_text(),
        navo_fs::FakeOp::Rename,
        1,
        navo_fs::Fault::Fail(navo_core::FsErrorKind::Locked),
    );
    let first = recovered(&journal, &dirs, &fs);
    assert!(first.pending.iter().any(|pending| pending.job == job));
    assert_eq!(
        super::super::lock(&journal).read_job(job).unwrap().0,
        JobState::RestorePending
    );
    let second = recovered(&journal, &dirs, &fs);
    assert_eq!(second.restored, vec![job]);
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 8);
}

#[test]
fn an_unreadable_source_is_retried_instead_of_settling() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    let job = captured(&journal, &dirs, &fs);
    let held = entries(&dirs).remove(0).record.held().unwrap();
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection()
        .execute("DELETE FROM retained_entry", [])
        .unwrap();
    fs.fault(
        "home/a",
        navo_fs::FakeOp::Identity,
        fs.calls("home/a", navo_fs::FakeOp::Identity) + 1,
        navo_fs::Fault::Fail(navo_core::FsErrorKind::Locked),
    );
    let first = recovered(&journal, &dirs, &fs);
    assert!(
        first
            .pending
            .iter()
            .any(|pending| pending.job == job && !pending.attention)
    );
    assert_eq!(
        super::super::lock(&journal).read_job(job).unwrap().0,
        JobState::RestorePending
    );
    fs.rename_entry(&held.path().to_text(), "home/a");
    let second = recovered(&journal, &dirs, &fs);
    assert_eq!(second.restored, vec![job]);
    assert!(second.pending.is_empty());
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 8);
}

/// Leaves the entry as a runtime rollback does when the original path is occupied.
fn runtime_conflict(dirs: &StoreDirs, fs: &FakeFs, job: JobId) {
    let entry = entries(dirs).remove(0);
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    let mut vault = Vault::new(db, &dirs.data, fs).unwrap();
    let cancel = CancelToken::new();
    let mut progress = |_| {};
    let mut run = RetentionRun::new(&cancel, &mut progress);
    assert!(vault.rollback(entry.id, job, &mut run).is_err());
    assert_eq!(entries(dirs).remove(0).state, RetainedState::Conflict);
}

#[test]
fn a_runtime_rollback_left_in_conflict_is_reported_once_as_needing_attention() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    let job = captured(&journal, &dirs, &fs);
    let held = entries(&dirs).remove(0).record.held().unwrap();
    fs.add_file("home/a", 99);
    runtime_conflict(&dirs, &fs, job);
    assert_not_retried(&journal, &dirs, &fs, job);
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 99);
    assert_eq!(fs.metadata(&held).unwrap().size, 8);
    assert_eq!(entries(&dirs).remove(0).state, RetainedState::Conflict);
}

#[test]
fn a_crash_in_preparing_that_settles_to_conflict_stops_being_retried() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    fs.fault(
        "home/a",
        navo_fs::FakeOp::Rename,
        1,
        navo_fs::Fault::Fail(navo_core::FsErrorKind::Locked),
    );
    let plan = delete(&fs, &[fs.location("home/a")]);
    let log = crate::jobs::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    let entry = super::interrupted_preparation((&dirs, &journal), &fs);
    drop(log);
    let job = JobId(entry.job_id.unwrap());
    fs.remove("home/a");
    fs.add_file("home/a", 99);
    assert_not_retried(&journal, &dirs, &fs, job);
    let entry = entries(&dirs).remove(0);
    assert_eq!(entry.state, RetainedState::Conflict);
    assert!(entry.target.is_none());
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 99);
}

#[test]
fn a_held_copy_removed_from_disk_stops_being_retried() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    let job = captured(&journal, &dirs, &fs);
    let held = entries(&dirs).remove(0).record.held().unwrap();
    fs.remove(&held.path().to_text());
    assert_not_retried(&journal, &dirs, &fs, job);
    assert!(fs.metadata(&fs.location("home/a")).is_err());
    assert_eq!(entries(&dirs).len(), 1);
}

#[test]
fn a_missing_original_folder_stops_being_retried() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 8);
    let plan = delete(&fs, &[fs.location("home/docs/a")]);
    let mut log = crate::jobs::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    drop(log);
    let job = super::super::lock(&journal)
        .unfinished()
        .unwrap()
        .remove(0)
        .id;
    let held = entries(&dirs).remove(0).record.held().unwrap();
    assert!(
        !held.as_str().contains("docs"),
        "the vault sits outside the removed folder: {}",
        held.as_str()
    );
    fs.remove("home/docs");
    assert_not_retried(&journal, &dirs, &fs, job);
    assert_eq!(fs.metadata(&held).unwrap().size, 8);
    let entry = entries(&dirs).remove(0);
    assert_eq!(entry.state, RetainedState::Ready);
    assert!(entry.pinned && entry.kept, "{entry:?}");
}

#[test]
fn a_missing_file_on_a_vanished_volume_is_retried_until_the_volume_returns() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/vol/docs/a", 8);
    fs.set_volume("home/vol", 5);
    let job = captured_roots(&journal, &dirs, &fs, &["home/vol/docs/a"]);
    let held = entries(&dirs).remove(0).record.held().unwrap();
    assert!(held.as_str().contains("home/vol/"), "{}", held.as_str());
    fs.rename_entry("home/vol", "home/unplugged");
    let first = recovered(&journal, &dirs, &fs);
    assert!(
        first
            .pending
            .iter()
            .any(|pending| pending.job == job && !pending.attention),
        "{:?}",
        first.pending
    );
    assert_eq!(
        super::super::lock(&journal).read_job(job).unwrap().0,
        JobState::RestorePending
    );
    let entry = entries(&dirs).remove(0);
    assert!(entry.pinned);
    assert_eq!(entry.state, RetainedState::Ready);
    fs.rename_entry("home/unplugged", "home/vol");
    let second = recovered(&journal, &dirs, &fs);
    assert_eq!(second.restored, vec![job]);
    assert_eq!(
        super::super::lock(&journal).read_job(job).unwrap().0,
        JobState::RolledBack
    );
    assert_eq!(
        fs.metadata(&fs.location("home/vol/docs/a")).unwrap().size,
        8
    );
}

#[test]
fn one_entry_needing_attention_does_not_stop_the_rest_of_the_job() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    fs.add_file("home/b", 9);
    let job = captured_roots(&journal, &dirs, &fs, &["home/a", "home/b"]);
    let saved = entries(&dirs);
    let (occupied, other) = (&saved[0].record, &saved[1].record);
    let held = occupied.held().unwrap();
    fs.add_file(&occupied.source.path().to_text(), 99);
    let first = recovered(&journal, &dirs, &fs);
    assert!(
        first
            .pending
            .iter()
            .any(|pending| pending.job == job && pending.attention),
        "{:?}",
        first.pending
    );
    assert_eq!(
        fs.metadata(&other.source).unwrap().size,
        other.nodes[0].size
    );
    assert_eq!(fs.metadata(&occupied.source).unwrap().size, 99);
    assert_eq!(fs.metadata(&held).unwrap().size, occupied.nodes[0].size);
    assert_eq!(entries(&dirs)[0].state, RetainedState::Conflict);
    assert_eq!(entries(&dirs)[1].state, RetainedState::Restored);
}

#[test]
fn a_same_named_vault_on_another_volume_is_retried_and_stays_pinned() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/vol/docs/a", 8);
    fs.set_volume("home/vol", 5);
    let job = captured_roots(&journal, &dirs, &fs, &["home/vol/docs/a"]);
    let record = entries(&dirs).remove(0).record;
    let held = record.held().unwrap().path().to_text();
    let anchor = record.anchor().unwrap().path().to_text();
    assert!(held.contains("home/vol/"), "{held}");
    fs.rename_entry("home/vol", "home/unplugged");
    fs.add_folder(&anchor);
    fs.set_volume("home/vol", 6);
    let first = recovered(&journal, &dirs, &fs);
    assert!(
        first
            .pending
            .iter()
            .any(|pending| pending.job == job && !pending.attention),
        "{:?}",
        first.pending
    );
    assert_eq!(
        super::super::lock(&journal).read_job(job).unwrap().0,
        JobState::RestorePending
    );
    let entry = entries(&dirs).remove(0);
    assert!(entry.pinned);
    assert_eq!(entry.state, RetainedState::Ready);
    let moved = held.replacen("home/vol/", "home/unplugged/", 1);
    assert_eq!(fs.metadata(&fs.location(&moved)).unwrap().size, 8);
    assert!(fs.metadata(&fs.location(&anchor)).is_ok());
}

#[test]
fn a_removed_folder_on_a_folder_mounted_volume_needs_attention() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/vol/docs/a", 8);
    fs.set_volume("home/vol", 5);
    let job = captured_roots(&journal, &dirs, &fs, &["home/vol/docs/a"]);
    let held = entries(&dirs).remove(0).record.held().unwrap();
    fs.remove("home/vol/docs");
    assert!(fs.metadata(&fs.location("home/a").root()).is_ok());
    assert_not_retried(&journal, &dirs, &fs, job);
    let entry = entries(&dirs).remove(0);
    assert!(entry.pinned && entry.kept);
    assert_eq!(entry.state, RetainedState::Ready);
    assert_eq!(fs.metadata(&held).unwrap().size, 8);
}

#[test]
fn a_usb_disk_mounted_at_a_folder_is_retried_while_unplugged_then_needs_attention() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("mnt/usb/docs/a", 8);
    fs.set_volume("mnt/usb", 7);
    let job = captured_roots(&journal, &dirs, &fs, &["mnt/usb/docs/a"]);
    let held = entries(&dirs).remove(0).record.held().unwrap();
    assert!(held.as_str().contains("mnt/usb/"), "{}", held.as_str());
    fs.rename_entry("mnt/usb", "mnt/unplugged");
    fs.add_folder("mnt/usb");
    fs.set_volume("mnt/usb", 1);
    assert_retried(&journal, &dirs, &fs, job);
    fs.remove("mnt/usb");
    fs.rename_entry("mnt/unplugged", "mnt/usb");
    fs.set_volume("mnt/usb", 7);
    fs.remove("mnt/usb/docs");
    assert_not_retried(&journal, &dirs, &fs, job);
    let entry = entries(&dirs).remove(0);
    assert!(entry.pinned && entry.kept);
    assert_eq!(fs.metadata(&held).unwrap().size, 8);
}

/// Captures `home/vol/docs/a` on volume 5, then mounts a different volume 6 at the same path.
fn reused_letter(journal: &SharedJournal, dirs: &StoreDirs, fs: &FakeFs) -> (JobId, String) {
    fs.add_file("home/vol/docs/a", 8);
    fs.set_volume("home/vol", 5);
    let job = captured_roots(journal, dirs, fs, &["home/vol/docs/a"]);
    let held = entries(dirs)
        .remove(0)
        .record
        .held()
        .unwrap()
        .path()
        .to_text();
    fs.rename_entry("home/vol", "home/unplugged");
    fs.set_volume("home/vol", 6);
    (job, held.replacen("home/vol/", "home/unplugged/", 1))
}

fn assert_retried(journal: &SharedJournal, dirs: &StoreDirs, fs: &FakeFs, job: JobId) {
    let report = recovered(journal, dirs, fs);
    assert!(
        report
            .pending
            .iter()
            .any(|pending| pending.job == job && !pending.attention),
        "{:?}",
        report.pending
    );
    assert_eq!(
        super::super::lock(journal).read_job(job).unwrap().0,
        JobState::RestorePending
    );
}

#[test]
fn a_reused_letter_with_a_restore_in_flight_and_a_foreign_vault_is_retried_untouched() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/vol/docs/a", 8);
    fs.set_volume("home/vol", 5);
    let job = captured_roots(&journal, &dirs, &fs, &["home/vol/docs/a"]);
    let record = entries(&dirs).remove(0).record;
    let held = record.held().unwrap().path().to_text();
    let anchor = record.anchor().unwrap().path().to_text();
    fs.fault(
        &held,
        navo_fs::FakeOp::Rename,
        fs.calls(&held, navo_fs::FakeOp::Rename) + 1,
        navo_fs::Fault::Fail(navo_core::FsErrorKind::Locked),
    );
    assert_retried(&journal, &dirs, &fs, job);
    let before = entries(&dirs).remove(0);
    assert_eq!(before.state, RetainedState::Restoring);
    fs.rename_entry("home/vol", "home/unplugged");
    fs.add_folder(&anchor);
    fs.set_volume("home/vol", 6);
    assert_retried(&journal, &dirs, &fs, job);
    let after = entries(&dirs).remove(0);
    assert!(after.pinned);
    assert_eq!(after.state, RetainedState::Restoring);
    assert_eq!(after.target, before.target);
    let moved = held.replacen("home/vol/", "home/unplugged/", 1);
    assert_eq!(fs.metadata(&fs.location(&moved)).unwrap().size, 8);
    assert!(fs.metadata(&fs.location(&anchor)).is_ok());
    assert!(fs.metadata(&fs.location("home/vol/docs/a")).is_err());
}

#[test]
fn a_reused_letter_with_a_foreign_file_at_the_source_is_retried_without_a_conflict() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    let (job, moved) = reused_letter(&journal, &dirs, &fs);
    fs.add_file("home/vol/docs/a", 99);
    fs.set_volume("home/vol", 6);
    assert_retried(&journal, &dirs, &fs, job);
    let entry = entries(&dirs).remove(0);
    assert!(entry.pinned);
    assert_eq!(entry.state, RetainedState::Ready);
    assert_eq!(
        fs.metadata(&fs.location("home/vol/docs/a")).unwrap().size,
        99
    );
    assert_eq!(fs.metadata(&fs.location(&moved)).unwrap().size, 8);
}

#[test]
fn a_source_on_a_foreign_volume_with_no_retained_entry_is_retried() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    let (job, moved) = reused_letter(&journal, &dirs, &fs);
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection()
        .execute("DELETE FROM retained_entry", [])
        .unwrap();
    fs.add_file("home/vol/docs/a", 99);
    fs.set_volume("home/vol", 6);
    assert_retried(&journal, &dirs, &fs, job);
    assert_eq!(
        fs.metadata(&fs.location("home/vol/docs/a")).unwrap().size,
        99
    );
    assert_eq!(fs.metadata(&fs.location(&moved)).unwrap().size, 8);
}
