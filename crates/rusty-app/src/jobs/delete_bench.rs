use std::path::{Path, PathBuf};
use std::time::Instant;

use rusty_core::{ConflictPolicy, Location, OpKind, PlanRequest, plan};
use rusty_fs::{CancelToken, JobContext, JobOutcome, LocalFs, run_job_parallel, scan};

use super::{log_factory, open};

const FILES_PER_FOLDER: usize = 200;
const FANOUT: usize = 10;

fn setting(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(default)
}

fn leaf(root: &Path, index: usize) -> PathBuf {
    root.join(format!("d{}", index / (FANOUT * FANOUT)))
        .join(format!("e{}", (index / FANOUT) % FANOUT))
        .join(format!("f{}", index % FANOUT))
}

fn build_tree(root: &Path, files: usize) {
    for index in 0..files.div_ceil(FILES_PER_FOLDER) {
        let folder = leaf(root, index);
        std::fs::create_dir_all(&folder).unwrap();
        let count = FILES_PER_FOLDER.min(files - index * FILES_PER_FOLDER);
        for file in 0..count {
            std::fs::write(folder.join(format!("file{file}.dat")), b"x").unwrap();
        }
    }
}

#[test]
#[ignore = "benchmark: NAVO_BENCH_FILES, NAVO_BENCH_WORKERS, NAVO_BENCH_DIR"]
fn deleting_many_real_files_is_timed() {
    let files = setting("NAVO_BENCH_FILES", 50_000);
    let workers = setting("NAVO_BENCH_WORKERS", 4);
    let base = std::env::var_os("NAVO_BENCH_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    std::fs::create_dir_all(&base).unwrap();
    let work = tempfile::tempdir_in(&base).unwrap();
    let tree = work.path().join("tree");
    build_tree(&tree, files);
    let journal = open(&work.path().join("navo.db")).unwrap();
    let fs = LocalFs::new();
    let cancel = CancelToken::new();
    let source = Location::new(tree.to_string_lossy().into_owned()).unwrap();
    let started = Instant::now();
    let items = scan(&fs, std::slice::from_ref(&source), &cancel).unwrap();
    let plan = plan(&PlanRequest {
        kind: OpKind::Delete,
        items,
        target: None,
        existing: Vec::new(),
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: Vec::new(),
    });
    let planned = started.elapsed();
    let mut log = log_factory(journal)(&plan).unwrap();
    let outcome = run_job_parallel(
        &plan,
        &[],
        &mut JobContext {
            ops: &fs,
            log: log.as_mut(),
            cancel: &cancel,
            progress: &mut |_| {},
        },
        workers,
    );
    log.finish(&outcome, &fs).unwrap();
    let total = started.elapsed();
    assert_eq!(outcome, JobOutcome::Done);
    assert!(!tree.exists());
    let seconds = total.as_secs_f64();
    #[expect(clippy::cast_precision_loss, reason = "a rough rate for the report")]
    let rate = files as f64 / seconds;
    println!(
        "delete bench: files={files} workers={workers} scan+plan={:.2}s total={seconds:.2}s rate={rate:.0} files/s",
        planned.as_secs_f64()
    );
}
