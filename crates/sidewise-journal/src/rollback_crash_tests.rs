//! A fault, then a power loss inside the rollback it starts (NC145): for every disk call that
//! fails once and every later op that crashes, the targets stay recoverable.
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sidewise_fs::durable::{Disk, DiskFile, FaultDisk, Meta, Plan, RealDisk, ReplaceError};

use crate::job_crash_tests::{PATTERNS, assert_recoverable};
use crate::job_tests::{Fixture, run};

/// A `FaultDisk` whose `at`-th disk call (file writes not counted) fails once; it notes the
/// `FaultDisk` op count at that moment, so a crash can be planned after the fault.
struct FailOnce {
    inner: Arc<FaultDisk>,
    at: u64,
    calls: AtomicU64,
    fault_op: AtomicU64,
}

impl FailOnce {
    /// Counts one call; fails when it is the `at`-th.
    fn tick(&self) -> io::Result<()> {
        if self.calls.fetch_add(1, Ordering::SeqCst) + 1 != self.at {
            return Ok(());
        }
        self.fault_op.store(self.inner.ops(), Ordering::SeqCst);
        Err(io::Error::other("injected fault before the crash"))
    }
}

impl Disk for FailOnce {
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        self.tick()?;
        self.inner.create_new(path)
    }
    fn open_append(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        self.tick()?;
        self.inner.open_append(path)
    }
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        self.tick()?;
        self.inner.open_read(path)
    }
    fn meta(&self, path: &Path) -> io::Result<Option<Meta>> {
        self.tick()?;
        self.inner.meta(path)
    }
    fn set_modified(&self, path: &Path, ms: i64) -> io::Result<()> {
        self.tick()?;
        self.inner.set_modified(path, ms)
    }
    fn copy_mode(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.tick()?;
        self.inner.copy_mode(from, to)
    }
    fn replace(&self, staged: &Path, target: &Path, old: &Path) -> Result<(), ReplaceError> {
        self.tick().map_err(ReplaceError::Unchanged)?;
        self.inner.replace(staged, target, old)
    }
    fn rename_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.tick()?;
        self.inner.rename_new(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.tick()?;
        self.inner.remove_file(path)
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.tick()?;
        self.inner.create_dir(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.tick()?;
        self.inner.remove_dir_all(path)
    }
    fn list(&self, dir: &Path) -> io::Result<Vec<String>> {
        self.tick()?;
        self.inner.list(dir)
    }
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        self.tick()?;
        self.inner.sync_dir(dir)
    }
}

/// Runs an aborted two-item job on a `FaultDisk` with `plan`, failing disk call `at` once.
/// Returns the layer, the op count at the fault (0 when it was not reached) and the calls.
fn faulted_run(fixture: &Fixture, plan: Plan, at: u64) -> (Arc<FaultDisk>, u64, u64) {
    let inner = Arc::new(FaultDisk::new(RealDisk, plan));
    let disk = Arc::new(FailOnce {
        inner: Arc::clone(&inner),
        at,
        calls: AtomicU64::new(0),
        fault_op: AtomicU64::new(0),
    });
    let _ = run(disk.clone(), fixture, false);
    let fault_op = disk.fault_op.load(Ordering::SeqCst);
    (inner, fault_op, disk.calls.load(Ordering::SeqCst))
}

#[test]
fn rollback_fault_then_crash_loop_is_recoverable() {
    let (_, _, calls) = faulted_run(&Fixture::new(2), Plan::Pass, 0);
    assert!(
        calls > 30,
        "the loop covers both items and the rollback ({calls} calls)"
    );
    for at in 1..=calls {
        let (layer, fault_op, _) = faulted_run(&Fixture::new(2), Plan::Pass, at);
        let ops = layer.ops();
        for crash in fault_op + 1..=ops + 1 {
            for (name, keep, tail) in PATTERNS {
                let fixture = Fixture::new(2);
                let (layer, _, _) = faulted_run(&fixture, Plan::CrashAt(crash), at);
                layer.crash_with(keep, tail).unwrap();
                let case = format!("fault at call {at}, crash at op {crash} of {ops}, {name}");
                assert_recoverable(&fixture, &case);
            }
        }
    }
}
