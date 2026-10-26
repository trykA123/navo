use super::ops::{fail_now, vacant};
use super::tree::split_last;
use super::{FakeFs, FakeOp, Fault, error};
use crate::{CopyControl, EntryKind, FsEntry, ProviderOps, WatchChange};
use rusty_core::{FsError, FsErrorKind, Location};
const CHUNK: u64 = 4096;
pub(super) struct CopyRequest<'a> {
    pub(super) source: &'a Location,
    pub(super) target: &'a Location,
    pub(super) created: Option<&'a mut dyn FnMut(crate::FileIdentity) -> Result<(), FsError>>,
}

impl FakeFs {
    pub(super) fn transfer(
        &self,
        request: CopyRequest<'_>,
        control: CopyControl<'_>,
    ) -> Result<u64, FsError> {
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
                fail_now(other, source)?;
                None
            }
        };
        let (to, size, identity) = self.start_copy(source, target)?;
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
            None => Ok(written),
        }
    }

    fn start_copy(
        &self,
        source: &Location,
        target: &Location,
    ) -> Result<(String, u64, u64), FsError> {
        let from = self.own_path(source)?;
        let to = self.own_path(target)?;
        let mut tree = self.tree();
        let entry = tree
            .metadata(&from)
            .ok_or_else(|| error(FsErrorKind::NotFound, source))?;
        if entry.kind != EntryKind::File {
            return Err(error(FsErrorKind::NameInvalid, source));
        }
        vacant(&tree, &to).map_err(|k| error(k, target))?;
        let (parent, name) =
            split_last(&to).ok_or_else(|| error(FsErrorKind::NameInvalid, target))?;
        let size = entry.size;
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
        Ok((to, size, identity))
    }
    fn created_identity(
        &self,
        target: &Location,
        identity: u64,
    ) -> Result<crate::FileIdentity, FsError> {
        let path = self.own_path(target)?;
        fail_now(self.record_call(&path, FakeOp::Identity), target)?;
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
