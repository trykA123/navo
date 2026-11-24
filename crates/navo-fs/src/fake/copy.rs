use super::ops::{fail_now, vacant};
use super::tree::split_last;
use super::{FakeFs, FakeOp, Fault, error, lock};
use crate::{CopiedSource, CopyControl, EntryKind, FsEntry, MoveCopy, ProviderOps, WatchChange};
use navo_core::{FsError, FsErrorKind, Location};
const CHUNK: u64 = 4096;

/// A source file as a copy started reading it, under the tree lock.
#[derive(Clone, Copy)]
struct Read {
    node: Option<u64>,
    size: u64,
    modified_ms: Option<i64>,
}

impl FakeFs {
    /// Copies `request` and returns the bytes written with the source as the copy read it.
    pub(super) fn transfer(
        &self,
        request: MoveCopy<'_>,
        control: CopyControl<'_>,
    ) -> Result<(u64, Option<CopiedSource>), FsError> {
        let (source, target) = (request.source, request.target);
        let CopyControl { progress, cancel } = control;
        let from = self.own_path(source)?;
        let fault = self.record_call(&from, FakeOp::Copy);
        if cancel.is_cancelled() {
            return Err(error(FsErrorKind::Cancelled, target));
        }
        let limit = match fault {
            Some(Fault::FailAfterBytes { kind, bytes }) => Some((kind, bytes)),
            other => {
                fail_now(other.as_ref(), source)?;
                None
            }
        };
        let (to, identity, read) = self.start_copy(source, target)?;
        let size = read.size;
        let owned = request.created.is_some();
        if let Some(created) = request.created {
            created(self.created_identity(target, identity)?)?;
        }
        let end = limit.map_or(size, |(_, bytes)| bytes.min(size));
        let mut written = 0;
        while written < end {
            if cancel.is_cancelled() {
                return Err(error(FsErrorKind::Cancelled, target));
            }
            written = end.min(written.saturating_add(CHUNK));
            let mut tree = self.tree();
            if owned && tree.identity(&to) != Some(identity) {
                return Err(error(FsErrorKind::IdentityUnavailable, target));
            }
            tree.resize(&to, written);
            drop(tree);
            progress(written);
        }
        self.changed(&to, WatchChange::Modified);
        match limit {
            Some((kind, _)) => Err(error(kind, target)),
            None => {
                self.copy_hash(&from, identity);
                Ok((written, self.proof(source, read)))
            }
        }
    }

    /// Gives the finished copy (tree identity `to`) the hash set on `from`, if it has one,
    /// so a copied fake file hashes like its source.
    fn copy_hash(&self, from: &str, to: u64) {
        let Some(source) = self.tree().identity(from) else {
            return;
        };
        let mut sizes = lock(&self.sizes);
        if let Some(hash) = sizes.get(&source).and_then(|found| found.hash) {
            sizes.entry(to).or_default().hash = Some(hash);
        }
    }

    /// Creates the empty copy; returns its path and tree identity, and the source as read.
    fn start_copy(
        &self,
        source: &Location,
        target: &Location,
    ) -> Result<(String, u64, Read), FsError> {
        let from = self.own_path(source)?;
        let to = self.own_path(target)?;
        let mut tree = self.tree();
        let entry = tree
            .metadata(&from)
            .ok_or_else(|| error(FsErrorKind::NotFound, source))?;
        if entry.kind != EntryKind::File {
            return Err(error(FsErrorKind::NameInvalid, source));
        }
        let read = Read {
            node: tree.identity(&from),
            size: entry.size,
            modified_ms: entry.modified_ms,
        };
        vacant(&tree, &to).map_err(|k| error(k, target))?;
        let (parent, name) =
            split_last(&to).ok_or_else(|| error(FsErrorKind::NameInvalid, target))?;
        tree.insert(
            parent,
            FsEntry {
                name,
                size: 0,
                ..entry
            },
        );
        let identity = tree
            .identity(&to)
            .ok_or_else(|| error(FsErrorKind::Unknown, target))?;
        drop(tree);
        self.changed(&to, WatchChange::Created);
        Ok((to, identity, read))
    }

    /// The source as `read` saw it; it proves nothing on a volume without file IDs.
    fn proof(&self, source: &Location, read: Read) -> Option<CopiedSource> {
        let volume = self.volume_serial(source).ok().flatten();
        Some(CopiedSource {
            identity: volume
                .zip(read.node)
                .map(|(volume, node)| crate::FileIdentity {
                    volume,
                    kind: crate::FileIdentityKind::Native,
                    file: u128::from(node).to_le_bytes(),
                }),
            size: read.size,
            modified_ms: read.modified_ms,
            created_ms: None,
            attributes: None,
        })
    }
    fn created_identity(
        &self,
        target: &Location,
        identity: u64,
    ) -> Result<crate::FileIdentity, FsError> {
        let path = self.own_path(target)?;
        fail_now(self.record_call(&path, FakeOp::Identity).as_ref(), target)?;
        let volume = self
            .volume_serial(target)?
            .ok_or_else(|| error(FsErrorKind::IdentityUnavailable, target))?;
        Ok(crate::FileIdentity {
            volume,
            file: u128::from(identity).to_le_bytes(),
            kind: crate::FileIdentityKind::Native,
        })
    }
}
