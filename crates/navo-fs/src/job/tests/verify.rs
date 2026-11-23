//! Tests for verify-after-copy in `write()`: a copy is hashed against its source before it is
//! placed, and a mismatch fails the step so a move keeps its source.

use navo_core::Totals;

use super::crashing::Crashing;
use super::*;
use crate::LocalFs;

const SOURCE_HASH: [u8; 32] = [7; 32];

/// Runs `plan` on `ops` with `workers` and the counter `count` switched on (`None` = verify off).
fn run_verified(
    harness: &mut Harness,
    ops: &dyn ProviderOps,
    (plan, workers): (&OperationPlan, usize),
    count: Option<&VerifyCount>,
) -> JobOutcome {
    harness.with(ops, |ctx| {
        let options = JobOptions {
            workers,
            sources: None,
            verify: count,
            speed_cap: None,
        };
        run_job_with(plan, &[], ctx, options)
    })
}

/// A fake tree with two hashed source files under `src/b`.
fn hashed() -> FakeFs {
    let fs = fixture(&[("src/b/c.txt", BIG), ("src/b/d.txt", 6_000)]);
    fs.set_hash("src/b/c.txt", SOURCE_HASH);
    fs.set_hash("src/b/d.txt", SOURCE_HASH);
    fs
}

/// The non-strict temporary path of `target` as the fake tree spells it (no leading slash).
fn temp_path(fs: &FakeFs, target: &str) -> String {
    let temp = navo_core::retention::copy_temporary(&fs.location(target), 0, None).unwrap();
    temp.path().to_text().trim_start_matches('/').to_owned()
}

/// True when any `.navo-part` file is left under `root`.
fn has_part(fs: &FakeFs, root: &str) -> bool {
    tree_of(fs, root)
        .iter()
        .any(|(path, _, _)| path.contains(navo_core::brand::TEMP_SUFFIX))
}

#[test]
fn a_verified_copy_through_the_cache_ends_done_and_counts_unverified() {
    for workers in MODES {
        let fs = hashed();
        let plan = copy_plan(&fs, &["src/b"], "dst");
        let count = VerifyCount::default();
        let outcome = run_verified(&mut Harness::default(), &fs, (&plan, workers), Some(&count));
        assert_eq!(outcome, JobOutcome::Done);
        assert_eq!((count.verified(), count.unverified()), (0, 2));
        assert_eq!(size(&fs, "dst/b/d.txt"), Some(6_000));
        assert!(!count.verifying());
    }
}

#[test]
fn a_copy_with_verify_off_reads_no_source() {
    for workers in MODES {
        let fs = hashed();
        let plan = copy_plan(&fs, &["src/b"], "dst");
        let outcome = run_verified(&mut Harness::default(), &fs, (&plan, workers), None);
        assert_eq!(outcome, JobOutcome::Done);
        assert_eq!(fs.calls("src/b/d.txt", FakeOp::Read), 0);
    }
}

#[test]
fn a_move_whose_copy_differs_keeps_its_source() {
    for workers in MODES {
        let fs = hashed();
        let plan = plan(&request(&fs, OpKind::Move, &["src/b"], "dst"));
        let before = tree_of(&fs, "src");
        let temp = temp_path(&fs, "dst/b/d.txt");
        let flipped = temp.clone();
        let ops = Crashing::meddling(&fs, (&temp, FakeOp::Read), move |fs| {
            fs.set_hash(&flipped, [9; 32]);
        });
        let count = VerifyCount::default();
        let outcome = run_verified(
            &mut Harness::default(),
            &ops,
            (&plan, workers),
            Some(&count),
        );
        assert_eq!(failure(&outcome), Some((2, FsErrorKind::VerifyFailed)));
        assert_eq!(tree_of(&fs, "src"), before);
        assert!(!has_part(&fs, "dst"), "{:?}", tree_of(&fs, "dst"));
    }
}

#[test]
fn a_source_changed_before_the_target_read_fails_the_step() {
    for workers in MODES {
        let fs = hashed();
        let plan = copy_plan(&fs, &["src/b"], "dst");
        let ops = Crashing::meddling(&fs, ("src/b/d.txt", FakeOp::Read), |fs| {
            fs.set_hash("src/b/d.txt", [8; 32]);
        });
        let count = VerifyCount::default();
        let outcome = run_verified(
            &mut Harness::default(),
            &ops,
            (&plan, workers),
            Some(&count),
        );
        assert_eq!(failure(&outcome), Some((2, FsErrorKind::VerifyFailed)));
        assert_eq!(size(&fs, "dst/b/d.txt"), None);
        assert!(!has_part(&fs, "dst"), "{:?}", tree_of(&fs, "dst"));
    }
}

#[test]
fn a_cancel_during_the_target_hash_cancels_and_leaves_no_temporary() {
    for workers in MODES {
        let fs = hashed();
        let plan = copy_plan(&fs, &["src/b"], "dst");
        let temp = temp_path(&fs, "dst/b/d.txt");
        let mut harness = Harness::default();
        let ops = Crashing::cancelling(&fs, &temp, FakeOp::Read, &harness.cancel);
        let count = VerifyCount::default();
        let outcome = run_verified(&mut harness, &ops, (&plan, workers), Some(&count));
        assert_eq!(outcome, JobOutcome::Cancelled);
        assert!(!has_part(&fs, "dst"), "{:?}", tree_of(&fs, "dst"));
        assert!(!count.verifying());
    }
}

/// A one-file local-to-local copy plan from `source` to `target`.
fn local_plan(source: &std::path::Path, target: &std::path::Path, size: u64) -> OperationPlan {
    OperationPlan {
        kind: OpKind::Copy,
        steps: vec![Step::CopyFile {
            source: super::cross_provider::local(source),
            target: super::cross_provider::local(target),
            size,
        }],
        totals: Totals {
            files: 1,
            folders: 0,
            bytes: size,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    }
}

#[test]
fn a_local_copy_reads_its_target_from_the_device_and_counts_verified() {
    // Under target/, not /tmp: tmpfs may refuse the Linux cache-drop hint.
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    let bytes: Vec<u8> = (0..70_000_u32).map(|at| (at % 251) as u8).collect();
    let (source, target) = (dir.path().join("a.bin"), dir.path().join("b.bin"));
    std::fs::write(&source, &bytes).unwrap();
    let plan = local_plan(&source, &target, bytes.len() as u64);
    let count = VerifyCount::default();
    let outcome = run_verified(
        &mut Harness::default(),
        &LocalFs::new(),
        (&plan, 0),
        Some(&count),
    );
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    assert_eq!(
        (count.verified(), count.unverified()),
        (1, 0),
        "the uncached target read fell back to the cache: the volume under {} refused the \
         cache-drop hint (Linux) or the unbuffered open (Windows)",
        dir.path().display()
    );
}

#[test]
fn a_verified_copy_of_a_read_only_file_ends_done_and_stays_read_only() {
    // Windows: the copy keeps the read-only attribute, so the flush before the hash must cope.
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    let (source, target) = (dir.path().join("a.bin"), dir.path().join("b.bin"));
    std::fs::write(&source, b"read-only source").unwrap();
    let mut read_only = std::fs::metadata(&source).unwrap().permissions();
    read_only.set_readonly(true);
    std::fs::set_permissions(&source, read_only).unwrap();
    let plan = local_plan(&source, &target, 16);
    let count = VerifyCount::default();
    let outcome = run_verified(
        &mut Harness::default(),
        &LocalFs::new(),
        (&plan, 0),
        Some(&count),
    );
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(std::fs::read(&target).unwrap(), b"read-only source");
    let placed = std::fs::metadata(&target).unwrap().permissions();
    assert!(
        placed.readonly(),
        "the copy must keep the source's read-only flag"
    );
    for path in [&source, &target] {
        let mut writable = std::fs::metadata(path).unwrap().permissions();
        #[expect(
            clippy::permissions_set_readonly_false,
            reason = "test cleanup: lets the temp folder be removed on Windows"
        )]
        writable.set_readonly(false);
        std::fs::set_permissions(path, writable).unwrap();
    }
}
