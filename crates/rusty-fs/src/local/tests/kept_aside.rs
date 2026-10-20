use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, plan};

use super::ops::{Log, child};
use super::*;
use crate::job::{JobContext, JobOutcome, Journaled, release_held, roll_back, run_job};
use crate::{HELD_PREFIX, existing_at, scan};

const NEW: &[u8] = b"replacement bytes";
const OLD: &[u8] = b"old";

fn old_time() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000)
}

struct Overwrite {
    dir: TempDir,
    ops: LocalFs,
    plan: OperationPlan,
    log: Log,
}

impl Overwrite {
    fn target(&self) -> PathBuf {
        self.dir.path().join("dst/a.bin")
    }

    fn holding(&self) -> Vec<String> {
        fs::read_dir(self.dir.path().join("dst"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(HELD_PREFIX))
            .collect()
    }

    fn undo(&mut self) -> JobOutcome {
        let done: Vec<u32> = (0..u32::try_from(self.plan.steps.len()).unwrap()).collect();
        let held = self.log.1.clone();
        let job = Journaled {
            plan: &self.plan,
            done: &done,
            held: &held,
            placed: &[],
            held_unreadable: false,
            unsure: None,
        };
        roll_back(
            &job,
            &mut JobContext {
                ops: &self.ops,
                log: &mut self.log,
                cancel: &CancelToken::new(),
                progress: &mut |_| {},
            },
        )
    }

    fn release(&self) -> Result<(), FsError> {
        release_held(&self.log.1, &self.ops)
    }
}

fn with_old_target(prepare: impl FnOnce(&Path)) -> Overwrite {
    let (dir, root) = temp();
    fs::create_dir_all(dir.path().join("src/far")).unwrap();
    fs::create_dir(dir.path().join("dst")).unwrap();
    fs::write(dir.path().join("src/far/a.bin"), NEW).unwrap();
    let target = dir.path().join("dst/a.bin");
    fs::write(&target, OLD).unwrap();
    fs::File::options()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(old_time())
        .unwrap();
    prepare(&target);
    let ops = LocalFs::new();
    let cancel = CancelToken::new();
    let source = child(&child(&child(&root, "src"), "far"), "a.bin");
    let items = scan(&ops, &[source], &cancel).unwrap();
    let destination = child(&root, "dst");
    let existing = existing_at(&ops, &items, &destination, &cancel).unwrap();
    let plan = plan(&PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(destination),
        existing,
        policy: ConflictPolicy::OverwriteAll,
        same_volume: false,
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
    Overwrite {
        dir,
        ops,
        plan,
        log,
    }
}

#[test]
fn an_overwrite_on_disk_holds_the_old_file_next_to_the_target() {
    let job = with_old_target(|_| {});
    let [held] = job.log.1.as_slice() else {
        panic!("one held file");
    };
    let held_path = os_path(&held.held).unwrap();
    assert_eq!(fs::read(job.target()).unwrap(), NEW);
    assert_eq!(fs::read(&held_path).unwrap(), OLD);
    let holder = held_path.parent().and_then(Path::parent).unwrap();
    assert_eq!(
        fs::canonicalize(holder).unwrap(),
        fs::canonicalize(job.dir.path().join("dst")).unwrap()
    );
}

#[test]
fn undo_on_disk_restores_the_old_bytes_and_time() {
    let mut job = with_old_target(|_| {});
    assert_eq!(job.undo(), JobOutcome::Done);
    let metadata = fs::metadata(job.target()).unwrap();
    assert_eq!(
        (
            fs::read(job.target()).unwrap(),
            metadata.modified().unwrap()
        ),
        (OLD.to_vec(), old_time())
    );
}

#[test]
fn release_on_disk_leaves_only_the_new_file() {
    let job = with_old_target(|_| {});
    job.release().unwrap();
    assert_eq!(
        (fs::read(job.target()).unwrap(), job.holding()),
        (NEW.to_vec(), Vec::<String>::new())
    );
}

#[cfg(unix)]
#[test]
fn undo_on_disk_restores_a_read_only_target() {
    let read_only = |path: &Path| {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions).unwrap();
    };
    let mut job = with_old_target(read_only);
    assert_eq!(job.undo(), JobOutcome::Done);
    job.release().unwrap();
    let metadata = fs::metadata(job.target()).unwrap();
    assert_eq!(
        (
            fs::read(job.target()).unwrap(),
            metadata.permissions().readonly(),
            job.holding()
        ),
        (OLD.to_vec(), true, Vec::<String>::new())
    );
}

#[cfg(windows)]
mod on_windows {
    use std::os::windows::fs::MetadataExt;

    use super::super::windows::attrib;
    use super::*;

    const READONLY: u32 = 0x1;
    const HIDDEN: u32 = 0x2;

    fn read_only_hidden_with_a_stream(target: &Path) {
        let mut stream = target.as_os_str().to_owned();
        stream.push(":tag");
        fs::write(PathBuf::from(stream), b"old stream").unwrap();
        fs::File::options()
            .write(true)
            .open(target)
            .unwrap()
            .set_modified(old_time())
            .unwrap();
        attrib("+r", target);
        attrib("+h", target);
    }

    #[test]
    fn undo_restores_attributes_streams_and_time_of_a_read_only_target() {
        let mut job = with_old_target(read_only_hidden_with_a_stream);
        assert_eq!(job.undo(), JobOutcome::Done);
        job.release().unwrap();
        let target = job.target();
        let metadata = fs::metadata(&target).unwrap();
        let mut stream = target.as_os_str().to_owned();
        stream.push(":tag");
        assert_eq!(
            (
                fs::read(&target).unwrap(),
                metadata.file_attributes() & (READONLY | HIDDEN),
                metadata.modified().unwrap(),
                fs::read(PathBuf::from(stream)).unwrap(),
                job.holding(),
            ),
            (
                OLD.to_vec(),
                READONLY | HIDDEN,
                old_time(),
                b"old stream".to_vec(),
                Vec::<String>::new()
            )
        );
        attrib("-r", &target);
    }

    #[test]
    fn release_removes_a_read_only_held_file() {
        let job = with_old_target(read_only_hidden_with_a_stream);
        job.release().unwrap();
        assert_eq!(
            (fs::read(job.target()).unwrap(), job.holding()),
            (NEW.to_vec(), Vec::<String>::new())
        );
    }

    #[test]
    fn undo_brings_the_old_file_back_under_its_own_spelling() {
        let mut job = with_old_target(|target| {
            fs::rename(target, target.with_file_name("A.bin")).unwrap();
        });
        assert_eq!(job.undo(), JobOutcome::Done);
        let names: Vec<String> = fs::read_dir(job.dir.path().join("dst"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| !name.starts_with(HELD_PREFIX))
            .collect();
        assert_eq!(
            (names, fs::read(job.target()).unwrap()),
            (vec!["A.bin".to_owned()], OLD.to_vec())
        );
    }
}
