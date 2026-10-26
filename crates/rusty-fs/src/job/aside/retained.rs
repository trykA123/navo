use super::{Aside, presence};
use crate::job::copy::failure;
use crate::{EntryKind, FileIdentity, FileStamp, HeldFile, JobContext, ProviderOps};
use rusty_core::retention::{OverwriteProof, RetainedKind, RetainedNode};
use rusty_core::{FsError, FsErrorKind, Location};
use std::sync::PoisonError;

pub(in crate::job) struct Replacement<'a> {
    pub source: &'a Location,
    pub target: &'a Location,
    pub allow: bool,
    pub expected_identity: Option<FileIdentity>,
}

impl Aside {
    pub(super) fn place_retained(
        &self,
        ctx: &mut JobContext<'_>,
        request: Replacement<'_>,
    ) -> Result<(), FsError> {
        let Replacement {
            source,
            target,
            allow,
            expected_identity,
        } = request;
        let ops = ctx.ops;
        if !ops.supports_atomic_rename() {
            return Err(failure(FsErrorKind::NotSupportedYet, target));
        }
        let old = presence(ops, target)?;
        if let Some(expected) = expected_identity
            && identity(ops, source)? != expected
        {
            return Err(failure(FsErrorKind::IdentityUnavailable, source));
        }
        let Some(old) = old else {
            return ops.rename_atomic(source, target);
        };
        if !allow || !matches!(old.kind, EntryKind::File | EntryKind::Link) {
            return Err(failure(FsErrorKind::AccessDenied, target));
        }
        let new = ops.metadata(source)?;
        if !matches!(new.kind, EntryKind::File | EntryKind::Link) {
            return Err(failure(FsErrorKind::NotSupportedYet, source));
        }
        let held = self.held_file(
            ops,
            &self.on_disk(ops, target),
            Some(FileStamp {
                size: new.size,
                modified_ms: new.modified_ms,
            }),
        )?;
        let (proof, created) = self.prepare_proof(ops, (&held, source, expected_identity))?;
        journal(ctx, (&held, &proof), created)?;
        let container = held
            .held
            .parent()
            .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.held))?;
        ops.hide_retained(&container, proof.container_identity)?;
        place_proved(ctx, (source, target), (&held, &proof))
    }

    fn prepare_proof(
        &self,
        ops: &dyn ProviderOps,
        (held, source, expected): (&HeldFile, &Location, Option<FileIdentity>),
    ) -> Result<(OverwriteProof, bool), FsError> {
        let parent = held
            .original
            .parent()
            .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.original))?;
        let parent_identity = identity(ops, &parent)?;
        check_folder(ops, &parent, parent_identity)?;
        let mount_identity = ops
            .mount_identity(&parent)?
            .ok_or_else(|| failure(FsErrorKind::NotSupportedYet, &parent))?;
        let node = retained_node(ops, &held.original)?;
        let replacement_identity = identity(ops, source)?;
        if expected.is_some_and(|expected| expected != replacement_identity) {
            return Err(failure(FsErrorKind::IdentityUnavailable, source));
        }
        for path in [&held.original, source] {
            if identity(ops, path)?.volume != parent_identity.volume
                || ops.mount_identity(path)? != Some(mount_identity)
            {
                return Err(failure(FsErrorKind::NotSupportedYet, path));
            }
        }
        let container = held
            .held
            .parent()
            .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.held))?;
        let (container_identity, created) = self.room(ops, &container)?;
        let scope = ops.mount_identity(&container).and_then(|mount| {
            if container_identity.volume == parent_identity.volume && mount == Some(mount_identity)
            {
                Ok(())
            } else {
                Err(failure(FsErrorKind::NotSupportedYet, &container))
            }
        });
        if let Err(cause) = scope {
            if created {
                remove_room(ops, &container, container_identity)?;
            }
            return Err(cause);
        }
        Ok((
            OverwriteProof {
                parent_identity,
                container_identity,
                mount_identity,
                node,
                replacement_identity,
            },
            created,
        ))
    }

    fn room(
        &self,
        ops: &dyn ProviderOps,
        container: &Location,
    ) -> Result<(FileIdentity, bool), FsError> {
        let mut containers = self
            .containers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let created = !containers.contains_key(container);
        if created {
            ops.create_folder(container)?;
            containers.insert(container.clone(), identity(ops, container)?);
        }
        let expected = *containers
            .get(container)
            .ok_or_else(|| failure(FsErrorKind::Unknown, container))?;
        check_folder(ops, container, expected)?;
        Ok((expected, created))
    }
}

fn retained_node(ops: &dyn ProviderOps, at: &Location) -> Result<RetainedNode, FsError> {
    let old = ops.metadata(at)?;
    Ok(RetainedNode {
        path: vec![],
        kind: match old.kind {
            EntryKind::File => RetainedKind::File,
            EntryKind::Link => RetainedKind::Link,
            _ => return Err(failure(FsErrorKind::NameInvalid, at)),
        },
        identity: identity(ops, at)?,
        size: old.size,
        modified_ms: old.modified_ms,
    })
}

fn place_proved(
    ctx: &mut JobContext<'_>,
    (source, target): (&Location, &Location),
    (held, proof): (&HeldFile, &OverwriteProof),
) -> Result<(), FsError> {
    let ops = ctx.ops;
    verify(ops, held, proof)?;
    if identity(ops, source)? != proof.replacement_identity {
        return Err(failure(FsErrorKind::Unknown, source));
    }
    ops.rename_atomic(&held.original, &held.held)?;
    match ops.rename_atomic(source, target) {
        Ok(()) => Ok(()),
        Err(cause) => {
            if put_back(ops, held, proof).is_err() {
                ctx.log.held_stranded(held);
            }
            Err(cause)
        }
    }
}

fn journal(
    ctx: &mut JobContext<'_>,
    (held, proof): (&HeldFile, &OverwriteProof),
    created: bool,
) -> Result<(), FsError> {
    let result = ctx
        .log
        .journal_proved_held(held, proof)
        .and_then(|recorded| {
            if recorded {
                Ok(())
            } else {
                Err(failure(FsErrorKind::Unknown, &held.original))
            }
        });
    if let Err(cause) = result {
        if created {
            let container = held
                .held
                .parent()
                .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.held))?;
            remove_room(ctx.ops, &container, proof.container_identity)?;
        }
        return Err(cause);
    }
    Ok(())
}

fn remove_room(
    ops: &dyn ProviderOps,
    at: &Location,
    expected: FileIdentity,
) -> Result<(), FsError> {
    check_folder(ops, at, expected)?;
    ops.remove_empty_folder(at)
}

fn identity(ops: &dyn ProviderOps, at: &Location) -> Result<FileIdentity, FsError> {
    ops.file_identity(at)?
        .ok_or_else(|| failure(FsErrorKind::NotSupportedYet, at))
}

fn check_folder(
    ops: &dyn ProviderOps,
    at: &Location,
    expected: FileIdentity,
) -> Result<(), FsError> {
    if ops.metadata(at)?.kind != EntryKind::Folder || identity(ops, at)? != expected {
        return Err(failure(FsErrorKind::Unknown, at));
    }
    Ok(())
}

fn verify(ops: &dyn ProviderOps, held: &HeldFile, proof: &OverwriteProof) -> Result<(), FsError> {
    let parent = held
        .original
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.original))?;
    let container = held
        .held
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.held))?;
    check_folder(ops, &parent, proof.parent_identity)?;
    check_folder(ops, &container, proof.container_identity)?;
    let old = ops.metadata(&held.original)?;
    if identity(ops, &held.original)? != proof.node.identity
        || old.size != proof.node.size
        || old.modified_ms != proof.node.modified_ms
    {
        return Err(failure(FsErrorKind::Unknown, &held.original));
    }
    Ok(())
}

fn put_back(ops: &dyn ProviderOps, held: &HeldFile, proof: &OverwriteProof) -> Result<(), FsError> {
    let parent = held
        .original
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.original))?;
    let container = held
        .held
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.held))?;
    check_folder(ops, &parent, proof.parent_identity)?;
    check_folder(ops, &container, proof.container_identity)?;
    if identity(ops, &held.held)? != proof.node.identity
        || ops.mount_identity(&held.held)? != Some(proof.mount_identity)
    {
        return Err(failure(FsErrorKind::Unknown, &held.held));
    }
    ops.rename_atomic(&held.held, &held.original)
}

#[cfg(test)]
mod tests;
