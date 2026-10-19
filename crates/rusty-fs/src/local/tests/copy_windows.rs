use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, plan};

use super::ops::{Log, child};
use super::windows::attrib;
use super::*;
use crate::{
    AutoTuning, JobContext, JobOutcome, Progress, ProviderOps, TEMP_SUFFIX, run_job, scan,
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
            if bytes > 0 && bytes < LARGE_SIZE as u64 {
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
        if progress.bytes_done > 0 && progress.bytes_done < LARGE_SIZE as u64 {
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
fn unbuffered_copy_handles_the_threshold_and_an_unaligned_end() {
    let (dir, root) = temp();
    let ops = LocalFs::with_auto_tuning(AutoTuning {
        unbuffered_from: 4096,
        ..AutoTuning::default()
    });
    for size in [4096, 8329] {
        let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
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
        assert_eq!(written, size as u64);
        assert_eq!(fs::read(dir.path().join("target.bin")).unwrap(), data);
        assert_eq!(seen.last(), Some(&(size as u64)));
        assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]));
        fs::remove_file(dir.path().join("target.bin")).unwrap();
    }
}
