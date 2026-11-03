use navo_core::FsErrorKind;
use navo_fs::{FakeFs, FakeOp, Fault, JobOutcome};
use navo_store::{JobId, JobState};

use super::super::held::held_of;
use super::super::{RecoveryReport, lock, recover_profile};
use super::dying::Dying;
use super::retained::{NEW_HASH, OLD_HASH, STAGED, Saving, TARGET, put};

const OUTSIDE: [u8; 32] = [9; 32];

fn crashed_after_the_hold() -> Saving {
    let saving = Saving::new("");
    let plan = saving.plan(STAGED, (3, 1_000));
    let ops = Dying::after(&saving.fs, TARGET, FakeOp::Rename);
    drop(saving.run(&plan, &ops, false));
    assert!(ops.dead.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(saving.hash(TARGET), None);
    saving
}

fn recover(saving: &Saving) -> RecoveryReport {
    recover_profile(
        &saving.journal,
        saving.fs.as_ref(),
        (&saving.dirs.data, &[]),
        true,
    )
    .unwrap()
}

fn state(saving: &Saving) -> JobState {
    lock(&saving.journal).read_job(JobId(1)).unwrap().0
}

fn fail_next_flushes(fs: &FakeFs, folder: &str) {
    let next = fs.calls(folder, FakeOp::Flush) + 1;
    for call in [next, next + 1] {
        fs.fault(
            folder,
            FakeOp::Flush,
            call,
            Fault::Fail(FsErrorKind::Unknown),
        );
    }
}

fn an_unflushed_return_keeps_the_proof(saving: &Saving) {
    fail_next_flushes(&saving.fs, "dest");
    recover(saving);
    assert_eq!(saving.hash(TARGET), Some(OLD_HASH));
    assert_eq!(state(saving), JobState::RestorePending);
    assert_eq!(held_of(&saving.journal, JobId(1)).unwrap().proved.len(), 1);
    assert!(recover(saving).pending.is_empty());
    assert_eq!(state(saving), JobState::Failed);
    assert_eq!(saving.hash(TARGET), Some(OLD_HASH));
    assert!(saving.entries().is_empty());
}

#[test]
fn a_returned_save_whose_folder_flush_fails_keeps_its_proof_until_the_next_start() {
    let saving = crashed_after_the_hold();
    an_unflushed_return_keeps_the_proof(&saving);
    assert_eq!(saving.hash(STAGED), Some(NEW_HASH));
}

fn stranded_save() -> Saving {
    let saving = Saving::new("");
    let plan = saving.plan(STAGED, (3, 1_000));
    let ops = Dying::meddling(&saving.fs, (TARGET, FakeOp::Rename), |fs| {
        put(fs, TARGET, (5, 7_000), OUTSIDE);
    });
    let (mut log, outcome) = saving.run(&plan, &ops, false);
    assert!(matches!(outcome, JobOutcome::Failed { .. }), "{outcome:?}");
    log.inner.finish(&outcome, saving.fs.as_ref()).unwrap();
    assert_eq!(state(&saving), JobState::RestorePending);
    drop(log);
    saving
}

fn lists_the_stranded_save(saving: &Saving, report: &RecoveryReport) {
    assert_eq!(state(saving), JobState::Failed);
    let [pending] = report.pending.as_slice() else {
        panic!("the stranded save is still pending: {:?}", report.pending);
    };
    assert_eq!(pending.job, JobId(1));
    assert!(pending.left.contains(&saving.fs.location(STAGED)));
    assert!(pending.left.contains(&saving.fs.location(TARGET)));
}

#[test]
fn a_stranded_save_keeps_listing_its_staged_file_after_a_restart() {
    let saving = stranded_save();
    let report = recover(&saving);
    lists_the_stranded_save(&saving, &report);
    assert_eq!(saving.hash(STAGED), Some(NEW_HASH));
    assert_eq!(saving.hash(TARGET), Some(OUTSIDE));
    let [entry] = saving.entries().try_into().unwrap();
    assert_eq!(
        super::retained::hash_at(&saving.fs, &entry.record.held().unwrap()),
        Some(OLD_HASH)
    );
}

fn proofs(saving: &Saving) -> usize {
    held_of(&saving.journal, JobId(1)).unwrap().proved.len()
}

fn cleaned_up(saving: &Saving) {
    lock(&saving.journal)
        .prune(navo_store::PruneLimits::keep(0))
        .unwrap();
    super::super::held::release_finished(&saving.journal, saving.fs.as_ref()).unwrap();
}

fn recover_logged(saving: &Saving) -> (RecoveryReport, String) {
    use tracing_subscriber::layer::SubscriberExt;
    let redactor = std::sync::Arc::new(navo_diag::Redactor::new(
        navo_diag::Salt::from_bytes([3; 16]),
        navo_diag::Identity::default(),
    ));
    let ring = std::sync::Arc::new(navo_diag::EventRing::new(
        navo_diag::EventRing::DEFAULT_BUDGET,
    ));
    let opened = crate::logging::open(crate::logging::log_config(None), &redactor, &ring);
    let subscriber = tracing_subscriber::Registry::default().with(opened.layer);
    let report = tracing::subscriber::with_default(subscriber, || recover(saving));
    (report, ring.snapshot().join("\n"))
}

#[test]
fn a_stranded_save_keeps_its_proof_through_cleanup_and_is_listed_on_every_start() {
    let saving = stranded_save();
    for _ in 0..3 {
        lists_the_stranded_save(&saving, &recover(&saving));
        cleaned_up(&saving);
        assert_eq!(proofs(&saving), 1);
    }
    assert_eq!(saving.hash(STAGED), Some(NEW_HASH));
    assert_eq!(saving.entries().len(), 1);
}

#[test]
fn a_staged_file_removed_behind_our_back_is_logged_and_dropped_at_the_next_start() {
    let saving = stranded_save();
    recover(&saving);
    saving.fs.remove(STAGED);
    let (report, log) = recover_logged(&saving);
    assert!(report.pending.is_empty());
    assert!(log.contains("stranded save"), "{log}");
    assert!(!log.contains("a.txt"), "{log}");
    assert_eq!(proofs(&saving), 0);
    assert_eq!(state(&saving), JobState::Failed);
    assert_eq!(saving.entries().len(), 1);
    assert!(recover(&saving).pending.is_empty());
}

#[test]
fn a_stranded_save_stops_listing_once_its_staged_file_is_replaced() {
    let saving = stranded_save();
    recover(&saving);
    saving.fs.remove(STAGED);
    put(&saving.fs, STAGED, (11, 2_000), NEW_HASH);
    assert!(recover(&saving).pending.is_empty());
    assert_eq!(proofs(&saving), 0);
}

#[test]
fn a_resolved_stranded_save_releases_its_proof_and_is_not_listed_again() {
    let saving = stranded_save();
    recover(&saving);
    let resolve = super::super::recovery::stranded::resolve_stranded_save;
    assert!(resolve(&saving.journal, JobId(1)).unwrap());
    assert_eq!(proofs(&saving), 0);
    assert!(recover(&saving).pending.is_empty());
    assert_eq!(saving.hash(STAGED), Some(NEW_HASH));
    assert_eq!(saving.entries().len(), 1);
}

#[test]
fn resolving_a_save_that_is_still_pending_keeps_its_proof() {
    let saving = stranded_save();
    let resolve = super::super::recovery::stranded::resolve_stranded_save;
    assert!(!resolve(&saving.journal, JobId(1)).unwrap());
    assert_eq!(proofs(&saving), 1);
}

#[test]
fn placed_and_returned_saves_never_keep_their_proof_or_get_listed() {
    let placed = Saving::new("");
    assert!(matches!(placed.save(STAGED, (3, 1_000)), JobOutcome::Done));
    let returned = crashed_after_the_hold();
    for saving in [&placed, &returned] {
        for _ in 0..2 {
            assert!(recover(saving).pending.is_empty());
            assert_eq!(proofs(saving), 0);
        }
    }
    assert_eq!(state(&returned), JobState::Failed);
    assert_eq!(returned.hash(STAGED), Some(NEW_HASH));
    assert_eq!(returned.hash(TARGET), Some(OLD_HASH));
}

#[test]
fn a_failed_job_of_another_kind_is_never_listed() {
    let saving = stranded_save();
    recover(&saving);
    lock(&saving.journal)
        .database()
        .connection()
        .execute(
            "UPDATE job SET kind = ?1 WHERE id = 1",
            [navo_core::retention::COPY_RETAINED_KIND],
        )
        .unwrap();
    assert!(recover(&saving).pending.is_empty());
}

#[test]
fn only_the_newest_failed_saves_are_looked_at() {
    let saving = stranded_save();
    recover(&saving);
    let kind = navo_core::retention::SAVE_RETAINED_KIND;
    for at in 0..super::super::recovery::FAILED_SAVES_LISTED {
        let mut journal = lock(&saving.journal);
        let job = journal
            .create_job(kind, b"{}", 0, i64::MAX - 1_000 + i64::from(at))
            .unwrap();
        journal.record_held(job, b"{}", 0).unwrap();
        journal.finish(job, JobState::Failed).unwrap();
    }
    let (report, log) = recover_logged(&saving);
    assert!(report.pending.is_empty());
    assert_eq!(proofs(&saving), 1);
    assert!(log.contains("\"unseen\":1"), "{log}");
}

#[test]
fn a_stranded_save_whose_staged_file_cannot_be_checked_keeps_its_proof_and_is_listed() {
    let saving = stranded_save();
    let next = saving.fs.calls(STAGED, FakeOp::Identity) + 1;
    for call in next..next + 8 {
        saving.fs.fault(
            STAGED,
            FakeOp::Identity,
            call,
            Fault::Fail(FsErrorKind::AccessDenied),
        );
    }
    lists_the_stranded_save(&saving, &recover(&saving));
    assert_eq!(proofs(&saving), 1);
    assert!(saving.fs.calls(STAGED, FakeOp::Identity) < next + 8);
    lists_the_stranded_save(&saving, &recover(&saving));
    assert_eq!(proofs(&saving), 1);
}
