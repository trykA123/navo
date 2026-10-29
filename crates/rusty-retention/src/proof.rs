use rusty_core::retention::{RetainedKind, RetainedNode, RetentionEntry, RetentionRecord};
use rusty_core::{FileIdentity, FsErrorKind, Location};
use rusty_fs::{EntryKind, FsEntry, ProviderOps};

use crate::Error;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Scope {
    pub volume: u64,
    pub mount: u64,
}

pub(super) fn scope(ops: &dyn ProviderOps, parent: &Location) -> Result<Scope, Error> {
    let volume = identity(ops, parent)?.volume;
    let mount = ops
        .mount_identity(parent)?
        .ok_or_else(|| Error::Unsupported(parent.clone()))?;
    Ok(Scope { volume, mount })
}

pub(super) fn on_scope(ops: &dyn ProviderOps, at: &Location, scope: Scope) -> Result<(), Error> {
    if identity(ops, at)?.volume != scope.volume || ops.mount_identity(at)? != Some(scope.mount) {
        return Err(Error::Unsupported(at.clone()));
    }
    Ok(())
}

pub(super) fn present(ops: &dyn ProviderOps, at: &Location) -> Result<Option<FsEntry>, Error> {
    match ops.metadata(at) {
        Ok(entry) => Ok(Some(entry)),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[must_use]
pub fn can_retain(ops: &dyn ProviderOps, at: &Location) -> bool {
    let has_identity = |at: &Location| matches!(ops.file_identity(at), Ok(Some(_)));
    ops.supports_atomic_rename()
        && has_identity(at)
        && at.parent().is_some_and(|parent| has_identity(&parent))
}

pub(super) fn identity(ops: &dyn ProviderOps, at: &Location) -> Result<FileIdentity, Error> {
    ops.file_identity(at)?
        .ok_or_else(|| Error::Unsupported(at.clone()))
}

pub(super) fn folder(
    ops: &dyn ProviderOps,
    at: &Location,
    expected: FileIdentity,
) -> Result<(), Error> {
    let entry = ops.metadata(at)?;
    if entry.kind != EntryKind::Folder || identity(ops, at)? != expected {
        return Err(Error::Changed(at.clone()));
    }
    Ok(())
}

pub(super) fn anchor(record: &RetentionRecord) -> Result<Location, Error> {
    record
        .anchor()
        .ok_or_else(|| Error::Changed(record.container.clone()))
}

pub(super) fn parent(ops: &dyn ProviderOps, record: &RetentionRecord) -> Result<(), Error> {
    folder(ops, &anchor(record)?, record.parent_identity)
}

pub(super) fn owned(ops: &dyn ProviderOps, record: &RetentionRecord) -> Result<Scope, Error> {
    parent(ops, record)?;
    let expected = record
        .container_identity
        .ok_or_else(|| Error::Changed(record.container.clone()))?;
    folder(ops, &record.container, expected)?;
    let scope = scope(ops, &anchor(record)?)?;
    on_scope(ops, &record.container, scope)?;
    Ok(scope)
}

pub(super) fn node(
    ops: &dyn ProviderOps,
    at: &Location,
    expected: &RetainedNode,
) -> Result<bool, Error> {
    let Some(found) = present(ops, at)? else {
        return Ok(false);
    };
    Ok(kind(found.kind) == expected.kind
        && identity(ops, at)? == expected.identity
        && (expected.kind == RetainedKind::Folder
            || (found.size == expected.size && found.modified_ms == expected.modified_ms)))
}

pub(super) fn kind(kind: EntryKind) -> RetainedKind {
    match kind {
        EntryKind::File => RetainedKind::File,
        EntryKind::Folder => RetainedKind::Folder,
        EntryKind::Link => RetainedKind::Link,
    }
}

pub(super) fn same_object(
    ops: &dyn ProviderOps,
    at: &Location,
    expected: &RetainedNode,
) -> Result<bool, Error> {
    let Some(found) = present(ops, at)? else {
        return Ok(false);
    };
    Ok(kind(found.kind) == expected.kind && identity(ops, at)? == expected.identity)
}

pub(super) fn cleanup(ops: &dyn ProviderOps, entry: &RetentionEntry) -> Result<bool, Error> {
    cleanup_record(ops, &entry.record)
}

pub(super) fn cleanup_record(
    ops: &dyn ProviderOps,
    record: &RetentionRecord,
) -> Result<bool, Error> {
    parent(ops, record)?;
    if record.container_identity.is_none() {
        return Ok(present(ops, &record.container)?.is_none());
    }
    if present(ops, &record.container)?.is_none() {
        return Ok(true);
    }
    owned(ops, record)?;
    if record.version == 2 {
        let mut occupied = false;
        ops.list(
            &record.container,
            &mut |batch: rusty_fs::EntryBatch| {
                occupied |= !batch.entries.is_empty();
                if occupied {
                    std::ops::ControlFlow::Break(())
                } else {
                    std::ops::ControlFlow::Continue(())
                }
            },
            &rusty_fs::CancelToken::new(),
        )?;
        if occupied {
            return Ok(false);
        }
        owned(ops, record)?;
    }
    ops.remove_empty_folder(&record.container)?;
    Ok(true)
}
