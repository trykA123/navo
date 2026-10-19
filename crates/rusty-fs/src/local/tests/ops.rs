use std::path::PathBuf;

use rusty_core::{ConflictPolicy, OpKind, PlanRequest, plan};

use super::*;
use crate::job::{HeldFile, JobContext, JobLog, JobOutcome, Progress, StepOutcome, run_job};
use crate::{ProviderOps, existing_at, scan};

pub(super) fn child(folder: &Location, name: &str) -> Location {
    folder.join(&ProviderPath::from_text(name)).unwrap()
}

fn path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(name)
}

#[test]
fn auto_tuning_defaults_match_the_copy_contract() {
    let tuning = crate::AutoTuning::default();
    assert_eq!(tuning.unbuffered_from, 256 * 1024 * 1024);
    assert_eq!(tuning.ssd_workers, 4);
    assert_eq!(tuning.network_workers, 8);
    assert_eq!(tuning.slow_workers, 1);
}

#[test]
fn copy_file_writes_every_byte_and_reports_progress() {
    let (dir, root) = temp();
    let data: Vec<u8> = (0..3_000_000_u32).map(|i| (i % 251) as u8).collect();
    fs::write(path(&dir, "a.bin"), &data).unwrap();
    let mut seen = Vec::new();
    let written = LocalFs::new()
        .copy_file(
            &child(&root, "a.bin"),
            &child(&root, "b.bin"),
            &mut |bytes| seen.push(bytes),
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(written, 3_000_000);
    assert_eq!(fs::read(path(&dir, "b.bin")).unwrap(), data);
    assert_eq!(seen.last(), Some(&3_000_000));
    assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn copy_file_refuses_an_existing_target() {
    let (dir, root) = temp();
    write_file(dir.path(), "a.txt", 3);
    write_file(dir.path(), "b.txt", 5);
    let result = LocalFs::new().copy_file(
        &child(&root, "a.txt"),
        &child(&root, "b.txt"),
        &mut |_| {},
        &CancelToken::new(),
    );
    assert!(result.is_err());
    assert_eq!(fs::metadata(path(&dir, "b.txt")).unwrap().len(), 5);
}

#[test]
fn copy_file_keeps_the_modified_time() {
    let (dir, root) = temp();
    write_file(dir.path(), "a.txt", 3);
    let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    fs::File::options()
        .write(true)
        .open(path(&dir, "a.txt"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    LocalFs::new()
        .copy_file(
            &child(&root, "a.txt"),
            &child(&root, "b.txt"),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(
        fs::metadata(path(&dir, "b.txt"))
            .unwrap()
            .modified()
            .unwrap(),
        old
    );
}

#[test]
fn a_cancelled_copy_stops() {
    let (dir, root) = temp();
    fs::write(path(&dir, "a.bin"), vec![1_u8; 4_000_000]).unwrap();
    let cancel = CancelToken::new();
    let inner = cancel.clone();
    let result = LocalFs::new().copy_file(
        &child(&root, "a.bin"),
        &child(&root, "b.bin"),
        &mut |_| inner.cancel(),
        &cancel,
    );
    assert_eq!(
        result.map_err(|e| e.kind).err(),
        Some(FsErrorKind::Cancelled)
    );
}

#[test]
fn create_folder_refuses_an_existing_entry() {
    let (dir, root) = temp();
    let ops = LocalFs::new();
    ops.create_folder(&child(&root, "new")).unwrap();
    assert!(path(&dir, "new").is_dir());
    assert!(ops.create_folder(&child(&root, "new")).is_err());
}

#[test]
fn rename_refuses_an_existing_target() {
    let (dir, root) = temp();
    write_file(dir.path(), "a.txt", 3);
    write_file(dir.path(), "b.txt", 5);
    assert!(
        LocalFs::new()
            .rename(&child(&root, "a.txt"), &child(&root, "b.txt"))
            .is_err()
    );
    assert_eq!(fs::metadata(path(&dir, "b.txt")).unwrap().len(), 5);
}

#[test]
fn rename_moves_a_file() {
    let (dir, root) = temp();
    write_file(dir.path(), "a.txt", 3);
    LocalFs::new()
        .rename(&child(&root, "a.txt"), &child(&root, "c.txt"))
        .unwrap();
    assert!(!path(&dir, "a.txt").exists() && path(&dir, "c.txt").exists());
}

#[test]
fn remove_empty_folder_refuses_a_full_folder() {
    let (dir, root) = temp();
    fs::create_dir(path(&dir, "full")).unwrap();
    write_file(&path(&dir, "full"), "x", 1);
    let ops = LocalFs::new();
    assert!(ops.remove_empty_folder(&child(&root, "full")).is_err());
    fs::create_dir(path(&dir, "empty")).unwrap();
    ops.remove_empty_folder(&child(&root, "empty")).unwrap();
    assert!(!path(&dir, "empty").exists());
}

#[cfg(unix)]
#[test]
fn remove_link_keeps_the_link_target() {
    let (dir, root) = temp();
    fs::create_dir(path(&dir, "real")).unwrap();
    write_file(&path(&dir, "real"), "keep.txt", 1);
    std::os::unix::fs::symlink(path(&dir, "real"), path(&dir, "link")).unwrap();
    LocalFs::new().remove_link(&child(&root, "link")).unwrap();
    assert!(!path(&dir, "link").exists() && path(&dir, "real/keep.txt").exists());
}

#[test]
fn remove_file_removes_a_file() {
    let (dir, root) = temp();
    write_file(dir.path(), "a.txt", 3);
    LocalFs::new().remove_file(&child(&root, "a.txt")).unwrap();
    assert!(!path(&dir, "a.txt").exists());
}

#[derive(Default)]
pub(super) struct Log(Vec<(u32, StepOutcome)>, pub(super) Vec<HeldFile>);

impl JobLog for Log {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.0.push((seq, outcome));
        Ok(())
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.1.push(held.clone());
        Ok(true)
    }
}

#[test]
fn a_folder_tree_copies_end_to_end_on_disk() {
    let (dir, root) = temp();
    fs::create_dir_all(path(&dir, "src/sub")).unwrap();
    write_file(&path(&dir, "src"), "a.txt", 10);
    write_file(&path(&dir, "src/sub"), "b.txt", 2_000_000);
    fs::create_dir(path(&dir, "dst")).unwrap();
    let ops = LocalFs::new();
    let cancel = CancelToken::new();
    let items = scan(&ops, &[child(&root, "src")], &cancel).unwrap();
    let target = child(&root, "dst");
    let existing = existing_at(&ops, &items, &target, &cancel).unwrap();
    let request = PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(target),
        existing,
        policy: ConflictPolicy::Skip,
        same_volume: true,
        protected: Vec::new(),
    };
    let plan = plan(&request);
    let mut log = Log::default();
    let mut on_progress = |_: &Progress| {};
    let mut ctx = JobContext {
        ops: &ops,
        log: &mut log,
        cancel: &cancel,
        progress: &mut on_progress,
    };
    assert_eq!(run_job(&plan, &[], &mut ctx), JobOutcome::Done);
    assert_eq!(
        fs::metadata(path(&dir, "dst/src/sub/b.txt")).unwrap().len(),
        2_000_000
    );
    assert_eq!(fs::metadata(path(&dir, "dst/src/a.txt")).unwrap().len(), 10);
    let leftovers: Vec<_> = walkdir(&path(&dir, "dst"))
        .into_iter()
        .filter(|p| p.to_string_lossy().ends_with(".ferrite-part"))
        .collect();
    assert_eq!(leftovers, Vec::<PathBuf>::new());
}

fn walkdir(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            }
            out.push(entry.path());
        }
    }
    out
}
