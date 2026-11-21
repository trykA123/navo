//! What a `FaultDisk` crash may still undo: folder-entry changes not yet durable, unsynced
//! times and modes, and how a sync (or a crash that keeps a change) settles them.

use std::fs::{self, File, Permissions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::State;
use super::image::{Change, cut, undo};

/// How much of a file's unsynced tail a crash keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tail {
    /// Every unsynced byte is lost.
    Lost,
    /// The first half of the unsynced bytes survives (rounded down): a torn write.
    Torn,
    /// Every unsynced byte survives.
    Kept,
}

/// One change and the folders still unsynced since it.
pub(super) struct Pending {
    pub(super) change: Change,
    pub(super) dirs: Vec<PathBuf>,
}

/// A time or mode set on `path` since its last file sync, with the value a crash puts back.
pub(super) struct Attr {
    path: PathBuf,
    old: Old,
}

/// The value an attribute change replaced.
enum Old {
    Modified(SystemTime),
    Mode(Permissions),
}

impl Attr {
    /// Reads the modified time of `path`, before `set_modified` changes it.
    pub(super) fn modified(path: &Path) -> io::Result<Self> {
        let old = Old::Modified(fs::symlink_metadata(path)?.modified()?);
        Ok(Self {
            path: path.to_path_buf(),
            old,
        })
    }

    /// Reads the permissions of `path`, before `copy_mode` changes them.
    pub(super) fn mode(path: &Path) -> io::Result<Self> {
        let old = Old::Mode(fs::symlink_metadata(path)?.permissions());
        Ok(Self {
            path: path.to_path_buf(),
            old,
        })
    }

    /// Puts the old value back; fails (also on NotFound) when the disk refuses.
    fn undo(self) -> io::Result<()> {
        match self.old {
            Old::Modified(time) => File::open(&self.path)?.set_modified(time),
            Old::Mode(permissions) => fs::set_permissions(&self.path, permissions),
        }
    }
}

/// The folder that holds `path` (empty for a bare name).
pub(super) fn parent(path: &Path) -> PathBuf {
    path.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// `path` with its prefix `from` swapped for `to`; `None` when `path` is not at or under `from`.
pub(super) fn rebase(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    let rest = path.strip_prefix(from).ok()?;
    // `join("")` would add a trailing slash, which a file path must not have.
    Some(if rest.as_os_str().is_empty() {
        to.to_path_buf()
    } else {
        to.join(rest)
    })
}

/// Swaps the prefix `from` for `to` in `path`, when `path` is at or under `from`.
fn rebase_in(path: &mut PathBuf, from: &Path, to: &Path) {
    if let Some(moved) = rebase(path, from, to) {
        *path = moved;
    }
}

/// The paths a change names: where its undo looks.
fn paths(change: &mut Change) -> Vec<&mut PathBuf> {
    match change {
        Change::Created(path) | Change::Removed { path, .. } => vec![path],
        Change::Renamed { from, to } => vec![from, to],
        Change::Replaced { staged, target, .. } => vec![staged, target],
    }
}

/// The amount of the unsynced `extra` bytes `tail` keeps.
fn kept(tail: Tail, extra: u64) -> u64 {
    match tail {
        Tail::Lost => 0,
        Tail::Torn => extra / 2,
        Tail::Kept => extra,
    }
}

impl State {
    /// Records a change that stays undoable until the folders of `paths` are synced.
    pub(super) fn push(&mut self, change: Change, paths: &[&Path]) {
        let mut dirs: Vec<PathBuf> = paths.iter().map(|path| parent(path)).collect();
        dirs.dedup();
        self.pending.push(Pending { change, dirs });
    }

    /// Marks `dir` synced: changes with no unsynced folder left become durable.
    pub(super) fn synced_dir(&mut self, dir: &Path) {
        for pending in &mut self.pending {
            pending.dirs.retain(|unsynced| unsynced != dir);
        }
        self.settle_durable(false);
    }

    /// Settles, oldest first, every pending change with no unsynced folder left; `kept` says
    /// they were kept by a crash rather than made durable by a sync.
    fn settle_durable(&mut self, kept: bool) {
        while let Some(at) = self.pending.iter().position(|p| p.dirs.is_empty()) {
            self.settle(at, kept);
        }
    }

    /// Drops the settled change at `at` and makes the older pending ones match it: a rename
    /// moves their paths; a create, unlink or rename onto a path drops the older changes at
    /// or under it, as the object they name is gone (so a later NotFound in undo is a real
    /// inconsistency). So a kept change keeps the older changes on its paths. A durable one
    /// instead turns an older rename out of that path into a create at its far end, which a
    /// crash still undoes until that folder is synced: the moved object may be lost.
    fn settle(&mut self, at: usize, kept: bool) {
        let done = self.pending.remove(at);
        let (gone, from, to) = match &done.change {
            Change::Created(path) | Change::Removed { path, .. } => (path, None, None),
            Change::Renamed { from, to } => (to, Some(from), Some(to)),
            Change::Replaced { staged, target, .. } => (target, Some(staged), Some(target)),
        };
        let newer = self.pending.split_off(at);
        self.pending.retain_mut(|pending| {
            if let Change::Renamed { from, to } = &pending.change
                && !kept
                && from.starts_with(gone)
                && !to.starts_with(gone)
            {
                pending.change = Change::Created(to.clone());
                return true;
            }
            !paths(&mut pending.change)
                .iter()
                .any(|p| p.starts_with(gone))
        });
        if let (Some(from), Some(to)) = (from, to) {
            for pending in &mut self.pending {
                paths(&mut pending.change)
                    .into_iter()
                    .chain(pending.dirs.iter_mut())
                    .for_each(|path| rebase_in(path, from, to));
            }
        }
        self.pending.extend(newer);
    }

    /// Moves tracked files and unsynced attributes at or under `from` to the same place
    /// under `to`.
    pub(super) fn moved(&mut self, from: &Path, to: &Path) {
        for file in &mut self.files {
            if let Some(moved) = file.path.as_deref().and_then(|p| rebase(p, from, to)) {
                file.path = Some(moved);
            }
        }
        for attr in &mut self.attrs {
            rebase_in(&mut attr.path, from, to);
        }
    }

    /// Forgets tracked files and unsynced attributes at or under `path`: they were unlinked.
    pub(super) fn unlinked(&mut self, path: &Path) {
        for file in &mut self.files {
            if file.path.as_deref().is_some_and(|p| p.starts_with(path)) {
                file.path = None;
            }
        }
        self.attrs.retain(|attr| !attr.path.starts_with(path));
    }

    /// Marks the times and mode of the tracked file `id` durable after its `sync`.
    pub(super) fn synced_file(&mut self, id: usize) {
        let Some(file) = self.files.get_mut(id) else {
            return;
        };
        file.synced = file.len;
        if let Some(path) = file.path.clone() {
            self.attrs.retain(|attr| attr.path != path);
        }
    }

    /// Builds the crash image: cuts each tracked file to its synced length plus the part of
    /// its tail that `tail(file index)` keeps; puts unsynced times and modes back; keeps each
    /// pending change `i` with `keep(i)` (oldest is 0) and undoes the rest, newest first.
    /// Fails when the disk refuses a step or a path to undo is missing.
    pub(super) fn revert(
        &mut self,
        keep: &dyn Fn(usize) -> bool,
        tail: &dyn Fn(usize) -> Tail,
    ) -> io::Result<()> {
        for (id, file) in self.files.iter().enumerate() {
            if let Some(path) = file.path.as_deref().filter(|_| file.len > file.synced) {
                cut(path, file.synced + kept(tail(id), file.len - file.synced))?;
            }
        }
        while let Some(attr) = self.attrs.pop() {
            attr.undo()?;
        }
        for (index, pending) in self.pending.iter_mut().enumerate() {
            if keep(index) {
                pending.dirs.clear();
            }
        }
        self.settle_durable(true);
        while let Some(pending) = self.pending.pop() {
            undo(pending.change)?;
        }
        self.files.clear();
        Ok(())
    }
}
