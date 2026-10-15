use std::sync::atomic::{AtomicBool, Ordering};

use rusty_core::{Location, ProviderId};

use super::*;
use crate::{Capabilities, ListSink};

const DIED: i32 = -9;
const PARTIAL: u64 = 4096;

fn died() -> FsError {
    FsError {
        kind: FsErrorKind::Unknown,
        location: None,
        os_code: Some(DIED),
    }
}

pub(super) struct Crashing<'a> {
    fs: &'a FakeFs,
    path: String,
    op: FakeOp,
    dead: AtomicBool,
}

impl<'a> Crashing<'a> {
    pub(super) fn new(fs: &'a FakeFs, path: &str, op: FakeOp) -> Self {
        Self {
            fs,
            path: path.to_owned(),
            op,
            dead: AtomicBool::new(false),
        }
    }

    fn alive(&self) -> Result<(), FsError> {
        if self.dead.load(Ordering::SeqCst) {
            return Err(died());
        }
        Ok(())
    }

    fn hits(&self, location: &Location, op: FakeOp) -> bool {
        op == self.op && location.path().to_text().trim_start_matches('/') == self.path
    }

    fn die(&self) -> FsError {
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
    let ops = Crashing::new(fs, copy_of, FakeOp::Copy);
    let mut harness = Harness::default();
    let _ = harness.run(&ops, plan, &[]);
    harness.log.done()
}
