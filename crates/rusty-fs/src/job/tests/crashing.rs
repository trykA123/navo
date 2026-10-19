use std::sync::atomic::{AtomicBool, Ordering};

use rusty_core::{Location, ProviderId};

use super::*;
use crate::{Capabilities, ListSink};

const DIED: i32 = -9;
const PARTIAL: u64 = 4096;

fn died() -> FsError {
    FsError {
        os_code: Some(DIED),
        ..FsError::new(FsErrorKind::Unknown)
    }
}

pub(super) struct Crashing<'a> {
    fs: &'a FakeFs,
    path: String,
    ops: Vec<FakeOp>,
    dead: AtomicBool,
    inside: bool,
    fatal: bool,
}

impl<'a> Crashing<'a> {
    pub(super) fn new(fs: &'a FakeFs, path: &str, op: FakeOp) -> Self {
        Self {
            fs,
            path: path.to_owned(),
            ops: vec![op],
            dead: AtomicBool::new(false),
            inside: false,
            fatal: true,
        }
    }

    pub(super) fn inside_holding(fs: &'a FakeFs, op: FakeOp) -> Self {
        Self {
            inside: true,
            ..Self::new(fs, HELD_PREFIX, op)
        }
    }

    pub(super) fn refusing(self) -> Self {
        Self {
            fatal: false,
            ..self
        }
    }

    pub(super) fn placing(fs: &'a FakeFs, temp: &str) -> Self {
        Self {
            ops: vec![FakeOp::Rename, FakeOp::Replace],
            ..Self::new(fs, temp, FakeOp::Rename)
        }
    }

    fn alive(&self) -> Result<(), FsError> {
        if self.dead.load(Ordering::SeqCst) {
            return Err(died());
        }
        Ok(())
    }

    fn hits(&self, location: &Location, op: FakeOp) -> bool {
        let text = location.path().to_text();
        let path = text.trim_start_matches('/');
        let matches = if self.inside {
            path.split('/').any(|part| part.starts_with(&self.path))
        } else {
            path == self.path
        };
        self.ops.contains(&op) && matches
    }

    fn die(&self) -> FsError {
        if !self.fatal {
            return FsError::new(FsErrorKind::AccessDenied);
        }
        self.dead.store(true, Ordering::SeqCst);
        died()
    }

    fn enter(&self, location: &Location, op: FakeOp) -> Result<(), FsError> {
        self.alive()?;
        if self.hits(location, op) {
            return Err(self.die());
        }
        Ok(())
    }
}

impl Provider for Crashing<'_> {
    fn id(&self) -> ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.alive()?;
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.alive()?;
        self.fs.metadata(location)
    }
}

impl ProviderOps for Crashing<'_> {
    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.enter(location, FakeOp::Create)?;
        self.fs.create_folder(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.alive()?;
        if !self.hits(source, FakeOp::Copy) {
            return self.fs.copy_file(source, target, progress, cancel);
        }
        let next = self.fs.calls(&self.path, FakeOp::Copy) + 1;
        let partial = Fault::FailAfterBytes {
            kind: FsErrorKind::Unknown,
            bytes: PARTIAL,
        };
        self.fs.fault(&self.path, FakeOp::Copy, next, partial);
        let _ = self.fs.copy_file(source, target, progress, cancel);
        Err(self.die())
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.enter(source, FakeOp::Rename)?;
        self.fs.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.enter(source, FakeOp::Replace)?;
        self.fs.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.enter(location, FakeOp::Remove)?;
        self.fs.remove_file(location)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.enter(location, FakeOp::Remove)?;
        self.fs.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.enter(location, FakeOp::Remove)?;
        self.fs.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.enter(location, FakeOp::Recycle)?;
        self.fs.recycle(location)
    }
}

pub(super) fn crashed(fs: &FakeFs, plan: &OperationPlan, copy_of: &str) -> Vec<u32> {
    crashed_with(fs, plan, copy_of, 0)
}

pub(super) fn crashed_with(
    fs: &FakeFs,
    plan: &OperationPlan,
    copy_of: &str,
    workers: usize,
) -> Vec<u32> {
    crashed_on(&Crashing::new(fs, copy_of, FakeOp::Copy), plan, workers)
}

pub(super) fn crashed_on(ops: &Crashing<'_>, plan: &OperationPlan, workers: usize) -> Vec<u32> {
    crashed_keeping(ops, plan, workers).0
}

pub(super) fn crashed_keeping(
    ops: &Crashing<'_>,
    plan: &OperationPlan,
    workers: usize,
) -> (Vec<u32>, Vec<HeldFile>) {
    let mut harness = Harness {
        workers,
        keeps_held: true,
        ..Harness::default()
    };
    let _ = harness.run(ops, plan, &[]);
    (harness.log.done(), harness.log.held)
}
