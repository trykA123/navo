use rusty_core::{FsError, FsErrorKind, Location};

use super::copy::CopyRequest;
use super::tree::{Tree, fold_text, split_last};
use super::{FakeFs, FakeOp, Fault, error, lock};
use crate::{
    ALREADY_EXISTS_CODES, Attributes, CancelToken, CopyControl, EntryKind, FsEntry, OwnedCopy,
    ProviderOps, WatchChange,
};

const DEFAULT_VOLUME: u64 = 1;

pub(super) fn fail_now(fault: Option<&Fault>, location: &Location) -> Result<(), FsError> {
    match fault {
        Some(
            Fault::Fail(kind)
            | Fault::FailAfterEntries { kind, .. }
            | Fault::FailAfterBytes { kind, .. },
        ) => Err(error(*kind, location)),
        Some(Fault::FailOs { kind, code }) => Err(FsError {
            os_code: Some(*code),
            ..error(*kind, location)
        }),
        Some(Fault::Collide | Fault::LinkThenFail(_) | Fault::RenameThenFail(_)) | None => Ok(()),
    }
}

pub(super) fn vacant(tree: &Tree, path: &str) -> Result<(), FsErrorKind> {
    let (parent, _) = split_last(path).ok_or(FsErrorKind::NameInvalid)?;
    if tree.metadata(path).is_some() {
        return Err(FsErrorKind::Unknown);
    }
    if !tree.is_folder(parent) {
        return Err(FsErrorKind::NotFound);
    }
    Ok(())
}

fn already_exists_error(location: &Location) -> FsError {
    FsError {
        os_code: Some(ALREADY_EXISTS_CODES[0]),
        ..error(FsErrorKind::Unknown, location)
    }
}

fn existing_kind(tree: &Tree, path: &str) -> Result<EntryKind, FsErrorKind> {
    split_last(path).ok_or(FsErrorKind::NameInvalid)?;
    tree.metadata(path)
        .map(|entry| entry.kind)
        .ok_or(FsErrorKind::NotFound)
}

impl FakeFs {
    fn write_call(&self, location: &Location, op: FakeOp) -> Result<String, FsError> {
        let path = self.own_path(location)?;
        fail_now(self.record_call(&path, op).as_ref(), location)?;
        Ok(path)
    }

    fn remove_kind(&self, location: &Location, kind: EntryKind) -> Result<(), FsError> {
        let path = self.write_call(location, FakeOp::Remove)?;
        let mut tree = self.tree();
        let found = existing_kind(&tree, &path).map_err(|k| error(k, location))?;
        if found != kind {
            return Err(error(FsErrorKind::NameInvalid, location));
        }
        if tree.has_children(&path) {
            return Err(error(FsErrorKind::Unknown, location));
        }
        tree.remove(&path);
        drop(tree);
        lock(&self.links).remove(&path);
        self.changed(&path, WatchChange::Removed);
        Ok(())
    }
}

impl ProviderOps for FakeFs {
    fn available_space(&self, location: &Location) -> Result<Option<u64>, FsError> {
        let path = self.own_path(location)?;
        fail_now(self.record_call(&path, FakeOp::Space).as_ref(), location)?;
        crate::Provider::metadata(self, location)?;
        Ok(lock(&self.space).get(&path).copied().flatten())
    }

    fn supports_atomic_rename(&self) -> bool {
        true
    }

    fn rename_atomic(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.rename(source, target)
    }

    fn mount_identity(&self, location: &Location) -> Result<Option<u64>, FsError> {
        let path = self.own_path(location)?;
        fail_now(self.record_call(&path, FakeOp::Mount).as_ref(), location)?;
        let mounts = lock(&self.mounts);
        let nearest = mounts
            .iter()
            .filter(|(mount, _)| path == **mount || path.starts_with(&format!("{mount}/")))
            .max_by_key(|(mount, _)| mount.len());
        match nearest {
            Some((_, mount)) => Ok(*mount),
            None => self.volume_serial(location),
        }
    }
    fn file_identity(&self, location: &Location) -> Result<Option<crate::FileIdentity>, FsError> {
        let path = self.own_path(location)?;
        fail_now(self.record_call(&path, FakeOp::Identity).as_ref(), location)?;
        let identity = self
            .tree()
            .identity(&path)
            .ok_or_else(|| error(FsErrorKind::NotFound, location))?;
        Ok(self
            .volume_serial(location)?
            .map(|volume| crate::FileIdentity {
                volume,
                kind: crate::FileIdentityKind::Native,
                file: u128::from(identity).to_le_bytes(),
            }))
    }

    fn hide_retained(
        &self,
        location: &Location,
        identity: crate::FileIdentity,
    ) -> Result<(), FsError> {
        self.record_secure(location)?;
        if self.matches_identity(location, Some(identity))? {
            return Ok(());
        }
        Err(error(FsErrorKind::IdentityUnavailable, location))
    }

    fn unix_access(&self, location: &Location) -> Result<Option<crate::UnixAccess>, FsError> {
        self.access_of(location)
    }

    fn restrict_to_owner(&self, location: &Location) -> Result<(), FsError> {
        self.restrict(location)
    }

    fn owned_by_current_user(&self, location: &Location) -> Result<Option<bool>, FsError> {
        self.owner_of(location)
    }

    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        let path = self.write_call(location, FakeOp::Create)?;
        let mut tree = self.tree();
        vacant(&tree, &path).map_err(|k| error(k, location))?;
        tree.add_folder(&path);
        drop(tree);
        self.changed(&path, WatchChange::Created);
        Ok(())
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        let path = self.write_call(location, FakeOp::Create)?;
        let mut tree = self.tree();
        vacant(&tree, &path).map_err(|k| error(k, location))?;
        tree.add_file(&path, 0);
        drop(tree);
        self.changed(&path, WatchChange::Created);
        Ok(())
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.transfer(
            CopyRequest {
                source,
                target,
                created: None,
            },
            CopyControl { progress, cancel },
        )
    }

    fn copy_file_owned(
        &self,
        request: OwnedCopy<'_>,
        control: CopyControl<'_>,
    ) -> Result<u64, FsError> {
        self.transfer(
            CopyRequest {
                source: request.source,
                target: request.target,
                created: Some(request.created),
            },
            control,
        )
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        let from = self.own_path(source)?;
        let fault = self.record_call(&from, FakeOp::Rename);
        let to = self.own_path(target)?;
        let after = match fault {
            Some(Fault::RenameThenFail(kind)) => Some(kind),
            _ => None,
        };
        let collide = fault == Some(Fault::Collide);
        let linked = match fault {
            Some(Fault::LinkThenFail(kind)) => Some(kind),
            _ => None,
        };
        if !collide && linked.is_none() {
            fail_now(fault.as_ref(), source)?;
        }
        let mut tree = self.tree();
        if collide || linked.is_some() {
            tree.add_file(&to, 1);
        }
        if let Some(kind) = linked {
            return Err(error(kind, source));
        }
        existing_kind(&tree, &from).map_err(|k| error(k, source))?;
        let case_only = tree.folds() && fold_text(&from) == fold_text(&to);
        if !case_only && tree.metadata(&to).is_some() {
            return Err(already_exists_error(target));
        }
        if !case_only {
            vacant(&tree, &to).map_err(|k| error(k, target))?;
        }
        if to.starts_with(&format!("{from}/")) {
            return Err(error(FsErrorKind::NameInvalid, target));
        }
        tree.move_node(&from, &to);
        drop(tree);
        self.move_links(&from, &to);
        self.moved(&from, &to);
        if let Some(kind) = after {
            return Err(error(kind, source));
        }
        Ok(())
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        let from = self.write_call(source, FakeOp::Replace)?;
        let to = self.own_path(target)?;
        let mut tree = self.tree();
        if existing_kind(&tree, &from).map_err(|k| error(k, source))? != EntryKind::File {
            return Err(error(FsErrorKind::NameInvalid, source));
        }
        let replaced = match existing_kind(&tree, &to) {
            Ok(EntryKind::File) => true,
            Ok(_) => return Err(error(FsErrorKind::NameInvalid, target)),
            Err(_) => {
                vacant(&tree, &to).map_err(|k| error(k, target))?;
                false
            }
        };
        if replaced {
            tree.remove(&to);
        }
        tree.move_node(&from, &to);
        drop(tree);
        if replaced {
            self.changed(&to, WatchChange::Removed);
        }
        self.moved(&from, &to);
        Ok(())
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.remove_kind(location, EntryKind::File)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.remove_kind(location, EntryKind::Link)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.remove_kind(location, EntryKind::Folder)
    }

    fn make_link(&self, target_of: &Location, at: &Location) -> Result<(), FsError> {
        let path = self.write_call(at, FakeOp::Create)?;
        let target = self.own_path(target_of)?;
        let mut tree = self.tree();
        if tree.metadata(&path).is_some() {
            return Err(already_exists_error(at));
        }
        vacant(&tree, &path).map_err(|k| error(k, at))?;
        let (parent, name) =
            split_last(&path).ok_or_else(|| error(FsErrorKind::NameInvalid, at))?;
        tree.insert(
            parent,
            FsEntry {
                name,
                kind: EntryKind::Link,
                size: 0,
                modified_ms: None,
                attributes: Attributes::default(),
            },
        );
        drop(tree);
        lock(&self.links).insert(path.clone(), target);
        self.changed(&path, WatchChange::Created);
        Ok(())
    }

    fn link_target(&self, at: &Location) -> Result<Option<Location>, FsError> {
        let path = self.own_path(at)?;
        Ok(FakeFs::link_target(self, &path).map(|target| self.location(&target)))
    }

    fn volume_serial(&self, location: &Location) -> Result<Option<u64>, FsError> {
        let path = self.own_path(location)?;
        if self.tree().metadata(&path).is_none() {
            return Err(error(FsErrorKind::NotFound, location));
        }
        let volumes = lock(&self.volumes);
        let nearest = volumes
            .iter()
            .filter(|(mount, _)| path == **mount || path.starts_with(&format!("{mount}/")))
            .max_by_key(|(mount, _)| mount.len());
        Ok(nearest.map_or(Some(DEFAULT_VOLUME), |(_, serial)| *serial))
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        let path = self.write_call(location, FakeOp::Recycle)?;
        let mut tree = self.tree();
        existing_kind(&tree, &path).map_err(|k| error(k, location))?;
        tree.remove(&path);
        drop(tree);
        self.changed(&path, WatchChange::Removed);
        lock(&self.recycled).push(path);
        Ok(())
    }
}
