use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use navo_core::retention::{CopyTemporary, OverwriteProof};
use navo_core::{FsError, FsErrorKind, Location, ProviderId};
use navo_fs::{
    CancelToken, Capabilities, FakeFs, FakeOp, FileIdentity, FsEntry, HeldFile, JobLog, ListSink,
    Provider, ProviderOps, StepOutcome,
};
use navo_ui::browser::OpLog;

type Meddle = Box<dyn Fn(&FakeFs) + Send + Sync>;

pub(super) struct Dying<'a> {
    pub fs: &'a FakeFs,
    at: (String, FakeOp),
    meddle: Option<Meddle>,
    pub dead: Arc<AtomicBool>,
}

impl<'a> Dying<'a> {
    pub(super) fn after(fs: &'a FakeFs, path: &str, op: FakeOp) -> Self {
        Self {
            fs,
            at: (path.to_owned(), op),
            meddle: None,
            dead: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn never(fs: &'a FakeFs) -> Self {
        Self::after(fs, "\0", FakeOp::Create)
    }

    pub(super) fn meddling(
        fs: &'a FakeFs,
        (path, op): (&str, FakeOp),
        meddle: impl Fn(&FakeFs) + Send + Sync + 'static,
    ) -> Self {
        Self {
            meddle: Some(Box::new(meddle)),
            ..Self::after(fs, path, op)
        }
    }

    fn alive(&self) -> Result<(), FsError> {
        if self.dead.load(Ordering::SeqCst) {
            return Err(FsError::new(FsErrorKind::Unknown));
        }
        Ok(())
    }

    fn done<T>(&self, at: &Location, op: FakeOp, result: Result<T, FsError>) -> Result<T, FsError> {
        let path = at.path().to_text();
        let path = path.trim_start_matches('/');
        let (want, want_op) = &self.at;
        let hit = *want_op == op
            && (path == want || (want.ends_with('*') && path.contains(want.trim_end_matches('*'))));
        if hit && result.is_ok() {
            match &self.meddle {
                Some(meddle) => meddle(self.fs),
                None => self.dead.store(true, Ordering::SeqCst),
            }
        }
        result
    }
}

impl Provider for Dying<'_> {
    fn id(&self) -> ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        at: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.alive()?;
        self.fs.list(at, sink, cancel)
    }

    fn metadata(&self, at: &Location) -> Result<FsEntry, FsError> {
        self.alive()?;
        self.fs.metadata(at)
    }
}

impl ProviderOps for Dying<'_> {
    fn create_folder(&self, at: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.done(at, FakeOp::Create, self.fs.create_folder(at))
    }

    fn create_file(&self, at: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.fs.create_file(at)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.alive()?;
        self.fs.copy_file(source, target, progress, cancel)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.done(source, FakeOp::Rename, self.fs.rename(source, target))
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.fs.replace(source, target)
    }

    fn remove_file(&self, at: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.fs.remove_file(at)
    }

    fn remove_link(&self, at: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.fs.remove_link(at)
    }

    fn remove_empty_folder(&self, at: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.fs.remove_empty_folder(at)
    }

    fn recycle(&self, at: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.fs.recycle(at)
    }

    fn volume_serial(&self, at: &Location) -> Result<Option<u64>, FsError> {
        self.alive()?;
        self.fs.volume_serial(at)
    }

    fn content_hash(&self, at: &Location, cancel: &CancelToken) -> Result<[u8; 32], FsError> {
        self.alive()?;
        self.fs.content_hash(at, cancel)
    }

    fn file_identity(&self, at: &Location) -> Result<Option<FileIdentity>, FsError> {
        self.alive()?;
        self.fs.file_identity(at)
    }

    fn flush(&self, at: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.fs.flush(at)
    }

    fn supports_atomic_rename(&self) -> bool {
        self.fs.supports_atomic_rename()
    }

    fn rename_atomic(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.alive()?;
        self.done(
            source,
            FakeOp::Rename,
            self.fs.rename_atomic(source, target),
        )
    }

    fn mount_identity(&self, at: &Location) -> Result<Option<u64>, FsError> {
        self.alive()?;
        self.fs.mount_identity(at)
    }

    fn hide_retained(&self, at: &Location, identity: FileIdentity) -> Result<(), FsError> {
        self.alive()?;
        self.done(at, FakeOp::Secure, self.fs.hide_retained(at, identity))
    }
}

pub(super) struct Halting {
    pub inner: Box<dyn OpLog>,
    pub dead: Arc<AtomicBool>,
    pub dies_when_marked: bool,
}

impl Halting {
    fn alive(&self) -> Result<(), FsError> {
        if self.dead.load(Ordering::SeqCst) {
            return Err(FsError::new(FsErrorKind::Unknown));
        }
        Ok(())
    }
}

impl JobLog for Halting {
    fn preserve_overwrites(&self) -> bool {
        self.inner.preserve_overwrites()
    }

    fn temporary_namespace(&self) -> Option<String> {
        self.inner.temporary_namespace()
    }

    fn prepare_temporary(&mut self, proof: &CopyTemporary) -> Result<(), FsError> {
        self.alive()?;
        self.inner.prepare_temporary(proof)
    }

    fn journal_proved_held(
        &mut self,
        held: &HeldFile,
        proof: &OverwriteProof,
    ) -> Result<bool, FsError> {
        self.alive()?;
        self.inner.journal_proved_held(held, proof)
    }

    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.alive()?;
        self.inner.step(seq, outcome)?;
        if self.dies_when_marked && outcome == StepOutcome::Done {
            self.dead.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.alive()?;
        self.inner.undone(seq, outcome)
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.alive()?;
        self.inner.journal_held(held)
    }

    fn kept_beside(&mut self, held: &HeldFile, beside: &Location) {
        if self.alive().is_ok() {
            self.inner.kept_beside(held, beside);
        }
    }

    fn held_stranded(&mut self, held: &HeldFile) {
        if self.alive().is_ok() {
            self.inner.held_stranded(held);
        }
    }
}
