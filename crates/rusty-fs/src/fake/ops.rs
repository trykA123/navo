use rusty_core::{FsError, FsErrorKind, Location};

use super::tree::{Tree, fold_text, split_last};
use super::{FakeFs, FakeOp, Fault, error, lock};
use crate::{
    ALREADY_EXISTS_CODES, Attributes, CancelToken, EntryKind, FsEntry, ProviderOps, WatchChange,
};

const CHUNK: u64 = 4096;
const DEFAULT_VOLUME: u64 = 1;

fn fail_now(fault: Option<Fault>, location: &Location) -> Result<(), FsError> {
    match fault {
        Some(
            Fault::Fail(kind)
            | Fault::FailAfterEntries { kind, .. }
            | Fault::FailAfterBytes { kind, .. },
        ) => Err(error(kind, location)),
        Some(Fault::Collide | Fault::LinkThenFail(_)) | None => Ok(()),
    }
}

fn vacant(tree: &Tree, path: &str) -> Result<(), FsErrorKind> {
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
        fail_now(self.record_call(&path, op), location)?;
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

    fn start_copy(&self, source: &Location, target: &Location) -> Result<(String, u64), FsError> {
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
        drop(tree);
        self.changed(&to, WatchChange::Created);
        Ok((to, size))
    }
}

impl ProviderOps for FakeFs {
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
        let (to, size) = self.start_copy(source, target)?;
        let end = limit.map_or(size, |(_, bytes)| bytes.min(size));
        let mut written = 0;
        while written < end {
            if cancel.is_cancelled() {
                return Err(error(FsErrorKind::Cancelled, target));
            }
            written = end.min(written.saturating_add(CHUNK));
            self.tree().resize(&to, written);
            progress(written);
        }
        self.changed(&to, WatchChange::Modified);
        match limit {
            Some((kind, _)) => Err(error(kind, target)),
            None => Ok(written),
        }
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        let from = self.own_path(source)?;
        let fault = self.record_call(&from, FakeOp::Rename);
        let to = self.own_path(target)?;
        let collide = fault == Some(Fault::Collide);
        let linked = match fault {
            Some(Fault::LinkThenFail(kind)) => Some(kind),
            _ => None,
        };
        if !collide && linked.is_none() {
            fail_now(fault, source)?;
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
        self.moved(&from, &to);
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
