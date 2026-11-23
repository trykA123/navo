//! Strict (retained-overwrite) copies with verify-after-copy: a temporary file whose hash
//! differs after a full write fails the step, and `finish` leaves no partial file behind.

use super::*;

/// Marks every partial copy under `dest` with a hash its source does not have (the meddle).
fn flip_partials(fs: &FakeFs) {
    let files = scan(fs, &[fs.location("dest")], &CancelToken::new()).unwrap();
    for item in files {
        let path = item.location.path().to_text();
        if path.contains(navo_fs::TEMP_SUFFIX) {
            fs.set_hash(path.trim_start_matches('/'), [9; 32]);
        }
    }
}

/// Runs `plan` through `log` on one worker with verify on; each progress report flips the
/// partial copy's hash, so the check after the write sees a mismatch.
fn execute_verified(log: &mut dyn OpLog, plan: &OperationPlan, fs: &FakeFs) -> JobOutcome {
    log.check(LoggedOperation {
        plan,
        ops: fs,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: None,
    })
    .unwrap();
    let count = navo_fs::VerifyCount::default();
    let options = navo_fs::JobOptions {
        workers: 1,
        sources: None,
        verify: Some(&count),
        speed_cap: None,
    };
    let mut context = JobContext {
        ops: fs,
        log,
        cancel: &CancelToken::new(),
        progress: &mut |_| flip_partials(fs),
    };
    navo_fs::run_job_with(plan, &[], &mut context, options)
}

#[test]
fn a_strict_copy_that_fails_verify_after_a_full_write_leaves_no_partial() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = single(&fs);
    let mut log = profile_factory(journal, dirs)(&plan).unwrap();
    let outcome = execute_verified(log.as_mut(), &plan, &fs);
    let JobOutcome::Failed { error, .. } = &outcome else {
        panic!("a mismatched copy must fail the job, got {outcome:?}");
    };
    assert_eq!(error.kind, navo_core::FsErrorKind::VerifyFailed);
    log.finish(&outcome, &fs).unwrap();
    drop(log);
    no_partial(&fs);
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 2);
}
