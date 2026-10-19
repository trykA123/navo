use std::io;
use std::os::windows::fs::OpenOptionsExt;

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, plan};

use super::ops::{Log, child};
use super::windows::attrib;
use super::*;
use crate::local::win_copy::{self, CopyProgress};
use crate::{
    AutoTuning, JobContext, JobOutcome, Journaled, Progress, ProviderOps, TEMP_SUFFIX, clean_up,
    existing_at, roll_back, run_job, scan,
};

const LARGE_SIZE: usize = 32 * 1024 * 1024 + 73;

fn copy_plan(ops: &LocalFs, root: &Location) -> OperationPlan {
    let items = scan(ops, &[child(root, "source.bin")], &CancelToken::new()).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(child(root, "dst")),
        existing: Vec::new(),
        policy: ConflictPolicy::Skip,
        same_volume: true,
        protected: Vec::new(),
    })
}

#[test]
fn cancelling_mid_file_returns_cancelled() {
    let (dir, root) = temp();
    write_file(dir.path(), "source.bin", LARGE_SIZE);
    let cancel = CancelToken::new();
    let mut cancelled_at = None;
    let result = LocalFs::new().copy_file(
        &child(&root, "source.bin"),
        &child(&root, "target.bin"),
        &mut |bytes| {
            if bytes > 0 && bytes < u64::try_from(LARGE_SIZE).unwrap() {
                cancelled_at = Some(bytes);
                cancel.cancel();
            }
        },
        &cancel,
    );
    assert!(cancelled_at.is_some());
    assert_eq!(result.unwrap_err().kind, FsErrorKind::Cancelled);
}

#[test]
fn a_job_cancelled_mid_file_leaves_no_part_file() {
    let (dir, root) = temp();
    write_file(dir.path(), "source.bin", LARGE_SIZE);
    fs::create_dir(dir.path().join("dst")).unwrap();
    let ops = LocalFs::new();
    let plan = copy_plan(&ops, &root);
    let cancel = CancelToken::new();
    let mut log = Log::default();
    let mut on_progress = |progress: &Progress| {
        if progress.bytes_done > 0 && progress.bytes_done < u64::try_from(LARGE_SIZE).unwrap() {
            cancel.cancel();
        }
    };
    let outcome = run_job(
        &plan,
        &[],
        &mut JobContext {
            ops: &ops,
            log: &mut log,
            cancel: &cancel,
            progress: &mut on_progress,
        },
    );
    assert_eq!(outcome, JobOutcome::Cancelled);
    assert!(!dir.path().join("dst/source.bin").exists());
    assert!(
        !dir.path()
            .join(format!("dst/source.bin{TEMP_SUFFIX}"))
            .exists()
    );
    assert!(dir.path().join("source.bin").exists());
}

#[test]
fn a_job_keeps_alternate_streams_and_reports_the_main_stream_size() {
    let (dir, root) = temp();
    write_file(dir.path(), "source.bin", 3_000_000);
    fs::write(dir.path().join("source.bin:details"), b"stream data").unwrap();
    fs::create_dir(dir.path().join("dst")).unwrap();
    let ops = LocalFs::new();
    let plan = copy_plan(&ops, &root);
    let cancel = CancelToken::new();
    let mut log = Log::default();
    let mut seen = Vec::new();
    let outcome = run_job(
        &plan,
        &[],
        &mut JobContext {
            ops: &ops,
            log: &mut log,
            cancel: &cancel,
            progress: &mut |progress| seen.push(progress.bytes_done),
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(
        fs::read(dir.path().join("dst/source.bin:details")).unwrap(),
        b"stream data"
    );
    assert_eq!(seen.last(), Some(&3_000_000));
    assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn copy_keeps_readonly_and_hidden_attributes() {
    let (dir, root) = temp();
    write_file(dir.path(), "source.bin", 3);
    let source = dir.path().join("source.bin");
    let target = dir.path().join("target.bin");
    attrib("+r", &source);
    attrib("+h", &source);
    LocalFs::new()
        .copy_file(
            &child(&root, "source.bin"),
            &child(&root, "target.bin"),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    let attributes = LocalFs::new()
        .metadata(&child(&root, "target.bin"))
        .unwrap()
        .attributes;
    attrib("-h", &source);
    attrib("-r", &source);
    attrib("-h", &target);
    attrib("-r", &target);
    assert!(attributes.readonly && attributes.hidden);
}

#[test]
fn rolling_back_a_job_removes_a_copy_of_a_readonly_source() {
    let (dir, root) = temp();
    write_file(dir.path(), "source.bin", 3);
    let source = dir.path().join("source.bin");
    attrib("+r", &source);
    fs::create_dir(dir.path().join("dst")).unwrap();
    let ops = LocalFs::new();
    let plan = copy_plan(&ops, &root);
    let cancel = CancelToken::new();
    let mut log = Log::default();
    let mut ctx = JobContext {
        ops: &ops,
        log: &mut log,
        cancel: &cancel,
        progress: &mut |_| {},
    };
    assert_eq!(run_job(&plan, &[], &mut ctx), JobOutcome::Done);
    assert!(
        ops.metadata(&child(&child(&root, "dst"), "source.bin"))
            .unwrap()
            .attributes
            .readonly
    );
    let job = Journaled {
        plan: &plan,
        done: &[0],
        held: &[],
    };
    let outcome = roll_back(&job, &mut ctx);
    attrib("-r", &source);
    assert_eq!(outcome, JobOutcome::Done);
    assert!(!dir.path().join("dst/source.bin").exists());
    assert!(source.exists());
}

#[test]
fn crash_cleanup_removes_a_readonly_part_file() {
    let (dir, root) = temp();
    write_file(dir.path(), "source.bin", 3);
    fs::create_dir(dir.path().join("dst")).unwrap();
    let ops = LocalFs::new();
    let plan = copy_plan(&ops, &root);
    let part_name = format!("dst/source.bin{TEMP_SUFFIX}");
    let part = dir.path().join(&part_name);
    write_file(dir.path(), &part_name, 1);
    attrib("+r", &part);
    assert_eq!(clean_up(&plan, &[], &ops).unwrap(), 1);
    assert!(!part.exists());
    assert!(!dir.path().join("dst/source.bin").exists());
    assert!(dir.path().join("source.bin").exists());
}

#[test]
fn a_job_overwrites_an_existing_readonly_target() {
    let (dir, root) = temp();
    fs::write(dir.path().join("source.bin"), b"replacement").unwrap();
    fs::create_dir(dir.path().join("dst")).unwrap();
    let target = dir.path().join("dst/source.bin");
    fs::write(&target, b"old").unwrap();
    attrib("+r", &target);
    let ops = LocalFs::new();
    let cancel = CancelToken::new();
    let items = scan(&ops, &[child(&root, "source.bin")], &cancel).unwrap();
    let destination = child(&root, "dst");
    let existing = existing_at(&ops, &items, &destination, &cancel).unwrap();
    let plan = plan(&PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(destination),
        existing,
        policy: ConflictPolicy::OverwriteAll,
        same_volume: true,
        protected: Vec::new(),
    });
    let mut log = Log::default();
    let outcome = run_job(
        &plan,
        &[],
        &mut JobContext {
            ops: &ops,
            log: &mut log,
            cancel: &cancel,
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(fs::read(&target).unwrap(), b"replacement");
    assert!(
        !dir.path()
            .join(format!("dst/source.bin{TEMP_SUFFIX}"))
            .exists()
    );
}

#[test]
fn unbuffered_copy_handles_the_threshold_and_an_unaligned_end() {
    let (dir, root) = temp();
    let ops = LocalFs::with_auto_tuning(AutoTuning {
        unbuffered_from: 4096,
        ..AutoTuning::default()
    });
    for size in [4096_u32, 8329] {
        let data: Vec<u8> = (0..size).map(|i| u8::try_from(i % 251).unwrap()).collect();
        fs::write(dir.path().join("source.bin"), &data).unwrap();
        let mut seen = Vec::new();
        let written = ops
            .copy_file(
                &child(&root, "source.bin"),
                &child(&root, "target.bin"),
                &mut |bytes| seen.push(bytes),
                &CancelToken::new(),
            )
            .unwrap();
        assert_eq!(written, u64::from(size));
        assert_eq!(fs::read(dir.path().join("target.bin")).unwrap(), data);
        assert_eq!(seen.last(), Some(&u64::from(size)));
        assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]));
        fs::remove_file(dir.path().join("target.bin")).unwrap();
    }
}

#[test]
fn a_failed_flush_fails_the_copy_at_the_target() {
    let (dir, root) = temp();
    write_file(dir.path(), "source.bin", 3_000);
    let target = child(&root, "target.bin");
    let cancel = CancelToken::new();
    let mut report = |_: u64| {};
    let failure = win_copy::copy(
        &child(&root, "source.bin"),
        &target,
        &AutoTuning::default(),
        CopyProgress::new(&mut report, &cancel)
            .with_flush(|_| Err(io::Error::from_raw_os_error(112))),
    )
    .unwrap_err();
    assert_eq!(failure.kind, FsErrorKind::DiskFull);
    assert!(failure.location.unwrap().same_place(&target));
}

#[test]
fn an_empty_file_copies() {
    let (dir, root) = temp();
    write_file(dir.path(), "source.bin", 0);
    let written = LocalFs::new()
        .copy_file(
            &child(&root, "source.bin"),
            &child(&root, "target.bin"),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(written, 0);
    assert_eq!(
        fs::metadata(dir.path().join("target.bin")).unwrap().len(),
        0
    );
}

#[test]
fn a_locked_source_is_reported_at_the_source() {
    let (dir, root) = temp();
    write_file(dir.path(), "source.bin", 3);
    let _lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(dir.path().join("source.bin"))
        .unwrap();
    let source = child(&root, "source.bin");
    let failure = LocalFs::new()
        .copy_file(
            &source,
            &child(&root, "target.bin"),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap_err();
    assert_eq!(failure.kind, FsErrorKind::SharingViolation);
    assert!(failure.location.unwrap().same_place(&source));
}
