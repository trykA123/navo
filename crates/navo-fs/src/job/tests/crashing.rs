use std::sync::atomic::{AtomicBool, Ordering};

use navo_core::{Location, ProviderId};

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
    takes: Option<String>,
    cancels: Option<CancelToken>,
    meddles: Option<Meddle>,
}

type Meddle = Box<dyn Fn(&FakeFs) + Send + Sync>;

impl<'a> Crashing<'a> {
    pub(super) fn new(fs: &'a FakeFs, path: &str, op: FakeOp) -> Self {
        Self {
            fs,
            path: path.to_owned(),
            ops: vec![op],
            dead: AtomicBool::new(false),
            inside: false,
            fatal: true,
            takes: None,
            cancels: None,
            meddles: None,
        }
    }

    pub(super) fn inside_holding(fs: &'a FakeFs, op: FakeOp) -> Self {
        Self {
            inside: true,
            ..Self::new(fs, HELD_PREFIX, op)
        }
    }

    pub(super) fn cancelling(fs: &'a FakeFs, path: &str, op: FakeOp, token: &CancelToken) -> Self {
        Self {
            cancels: Some(token.clone()),
            ..Self::new(fs, path, op)
        }
    }

    pub(super) fn meddling(
        fs: &'a FakeFs,
        (path, op): (&str, FakeOp),
        meddle: impl Fn(&FakeFs) + Send + Sync + 'static,
    ) -> Self {
        Self {
            meddles: Some(Box::new(meddle)),
            ..Self::new(fs, path, op)
        }
    }

    pub(super) fn refusing(self) -> Self {
        Self {
            fatal: false,
            ..self
        }
    }

    pub(super) fn taking(fs: &'a FakeFs, temp: &str, taken: &str) -> Self {
        Self {
            ops: vec![FakeOp::Replace],
            fatal: false,
            takes: Some(taken.to_owned()),
            ..Self::new(fs, temp, FakeOp::Replace)
        }
    }

    pub(super) fn placing(fs: &'a FakeFs, temp: &str) -> Self {
        Self {
            ops: vec![FakeOp::Rename, FakeOp::Replace],
            ..Self::new(fs, temp, FakeOp::Rename)
        }
    }

    pub(super) fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
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
            if let Some(meddle) = &self.meddles {
                meddle(self.fs);
                return Ok(());
            }
            if let Some(token) = &self.cancels {
                token.cancel();
                return Ok(());
            }
            if let Some(taken) = &self.takes {
                self.fs.add_file(taken, 1);
            }
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

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.enter(location, FakeOp::Create)?;
        self.fs.create_file(location)
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

    fn make_link(&self, target_of: &Location, at: &Location) -> Result<(), FsError> {
        self.enter(at, FakeOp::Create)?;
        self.fs.make_link(target_of, at)
    }

    fn volume_serial(&self, location: &Location) -> Result<Option<u64>, FsError> {
        self.fs.volume_serial(location)
    }

    fn content_hash(&self, location: &Location, cancel: &CancelToken) -> Result<[u8; 32], FsError> {
        self.enter(location, FakeOp::Read)?;
        self.fs.content_hash(location, cancel)
    }

    fn file_identity(&self, location: &Location) -> Result<Option<crate::FileIdentity>, FsError> {
        self.alive()?;
        self.fs.file_identity(location)
    }

    fn flush(&self, location: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.fs.flush(location)
    }

    fn supports_atomic_rename(&self) -> bool {
        self.fs.supports_atomic_rename()
    }

    fn rename_atomic(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.enter(source, FakeOp::Rename)?;
        self.fs.rename_atomic(source, target)
    }

    fn mount_identity(&self, location: &Location) -> Result<Option<u64>, FsError> {
        self.alive()?;
        self.fs.mount_identity(location)
    }

    fn hide_retained(
        &self,
        location: &Location,
        identity: crate::FileIdentity,
    ) -> Result<(), FsError> {
        self.enter(location, FakeOp::Secure)?;
        self.fs.hide_retained(location, identity)
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
