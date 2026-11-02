use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use rusty_core::{FileStamp, OperationPlan, write_plan};

use super::ops::{Log, child};
use super::*;
use crate::ProviderOps;
use crate::job::{JobContext, JobOutcome, StepOutcome, release_held, run_job, undo_write};

const OLD: &[u8] = b"old line\n";
const NEW: &[u8] = b"edited line\n";

fn old_time() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000)
}

fn set_time(path: &Path, time: SystemTime) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

struct Save {
    dir: TempDir,
    ops: LocalFs,
    plan: OperationPlan,
    log: Log,
}

impl Save {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn context<T>(&mut self, job: impl FnOnce(&mut JobContext<'_>) -> T) -> T {
        job(&mut JobContext {
            ops: &self.ops,
            log: &mut self.log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        })
    }

    fn run(&mut self) -> JobOutcome {
        let plan = self.plan.clone();
        self.context(|ctx| run_job(&plan, &[], ctx))
    }

    fn undo(&mut self) -> Result<StepOutcome, FsError> {
        let held = self.log.1[0].clone();
        self.context(|ctx| undo_write(&held, ctx))
    }
}

fn saving(edit: impl FnOnce(&Path)) -> Save {
    let (dir, root) = temp();
    let target = dir.path().join("a.txt");
    fs::write(&target, OLD).unwrap();
    set_time(&target, old_time());
    fs::write(dir.path().join("a.txt.part"), NEW).unwrap();
    let ops = LocalFs::new();
    let at = child(&root, "a.txt");
    let entry = ops.metadata(&at).unwrap();
    let seen = FileStamp {
        size: entry.size,
        modified_ms: entry.modified_ms,
        hash: *blake3::hash(OLD).as_bytes(),
    };
    edit(&target);
    let staged = child(&root, "a.txt.part");
    let plan = write_plan(&at, &staged, seen, u64::try_from(NEW.len()).unwrap());
    Save {
        dir,
        ops,
        plan,
        log: Log::default(),
    }
}

#[test]
fn a_save_on_disk_places_the_new_bytes_and_holds_the_old_ones() {
    let mut save = saving(|_| {});
    assert_eq!(save.run(), JobOutcome::Done);
    assert_eq!(fs::read(save.path("a.txt")).unwrap(), NEW);
    assert!(!save.path("a.txt.part").exists());
    let [held] = save.log.1.as_slice() else {
        panic!("one held file");
    };
    assert_eq!(fs::read(os_path(&held.held).unwrap()).unwrap(), OLD);
    release_held(&save.log.1, &save.ops).unwrap();
    assert_eq!(fs::read_dir(save.dir.path()).unwrap().count(), 1);
}

#[test]
fn undo_of_a_save_on_disk_restores_the_old_bytes_and_time() {
    let mut save = saving(|_| {});
    assert_eq!(save.run(), JobOutcome::Done);
    assert_eq!(save.undo(), Ok(StepOutcome::Done));
    let target = save.path("a.txt");
    assert_eq!(fs::read(&target).unwrap(), OLD);
    assert_eq!(
        fs::metadata(&target).unwrap().modified().unwrap(),
        old_time()
    );
}

#[test]
fn undo_of_a_save_on_disk_is_refused_after_an_outside_edit() {
    let mut save = saving(|_| {});
    assert_eq!(save.run(), JobOutcome::Done);
    fs::write(save.path("a.txt"), b"typed elsewhere\n").unwrap();
    assert_eq!(
        save.undo().map_err(|e| e.kind),
        Err(FsErrorKind::ChangedOnDisk)
    );
    assert_eq!(fs::read(save.path("a.txt")).unwrap(), b"typed elsewhere\n");
}

#[test]
fn undo_of_a_save_on_disk_is_refused_after_an_edit_keeping_size_and_time() {
    let mut save = saving(|_| {});
    assert_eq!(save.run(), JobOutcome::Done);
    let target = save.path("a.txt");
    let placed = fs::metadata(&target).unwrap().modified().unwrap();
    fs::write(&target, b"edited LINE\n").unwrap();
    set_time(&target, placed);
    assert_eq!(
        save.undo().map_err(|e| e.kind),
        Err(FsErrorKind::ChangedOnDisk)
    );
    assert_eq!(fs::read(&target).unwrap(), b"edited LINE\n");
    assert_eq!(
        fs::read(os_path(&save.log.1[0].held).unwrap()).unwrap(),
        OLD
    );
}

#[test]
fn a_save_on_disk_refuses_bytes_changed_behind_the_same_size_and_time() {
    let mut save = saving(|target| {
        fs::write(target, b"new line\n").unwrap();
        set_time(target, old_time());
    });
    let JobOutcome::Failed { error, .. } = save.run() else {
        panic!("a changed file was overwritten");
    };
    assert_eq!(error.kind, FsErrorKind::ChangedOnDisk);
    assert_eq!(fs::read(save.path("a.txt")).unwrap(), b"new line\n");
    assert_eq!(fs::read(save.path("a.txt.part")).unwrap(), NEW);
    assert_eq!(fs::read_dir(save.dir.path()).unwrap().count(), 2);
}

#[test]
fn hashing_on_disk_stops_when_cancelled() {
    let (dir, root) = temp();
    fs::write(dir.path().join("a.txt"), OLD).unwrap();
    let at = child(&root, "a.txt");
    let ops = LocalFs::new();
    let hash = ops.content_hash(&at, &CancelToken::new()).unwrap();
    assert_eq!(hash, *blake3::hash(OLD).as_bytes());
    let cancel = CancelToken::new();
    cancel.cancel();
    let error = ops.content_hash(&at, &cancel).unwrap_err();
    assert_eq!(error.kind, FsErrorKind::Cancelled);
}
