use rusty_core::{FsErrorKind, Location, ProviderPath};

use super::patch::{Look, Subtree, relative_of};
use crate::measure::FolderSize;
use crate::measure::profile::relative;
use crate::measure::walk::{Visit, Walk};
use crate::{CancelToken, EntryKind, FsEntry, Provider, WatchChange};

pub(super) struct Looker<'a> {
    pub provider: &'a dyn Provider,
    pub root: &'a Location,
    pub separator: char,
    pub cancel: &'a CancelToken,
}

impl Looker<'_> {
    pub(super) fn look(&self, change: &WatchChange, room: usize) -> Look {
        match change {
            WatchChange::Created(name) => self.at(name, Some(room)),
            WatchChange::Modified(name) => self.at(name, None),
            WatchChange::Renamed { to, .. } => self.at(to, None),
            WatchChange::Removed(_) => Look::Missing,
        }
    }

    fn at(&self, name: &ProviderPath, room: Option<usize>) -> Look {
        let Some(location) = self.resolve(&relative_of(name, self.separator)) else {
            return Look::Unreadable;
        };
        match self.provider.metadata(&location) {
            Ok(entry) if entry.kind == EntryKind::Folder => match room {
                Some(room) => self.subtree(&location, room),
                None => Look::Folder(None),
            },
            Ok(entry) => Look::File(entry),
            Err(error) if error.kind == FsErrorKind::NotFound => Look::Missing,
            Err(_) => Look::Unreadable,
        }
    }

    fn resolve(&self, path: &str) -> Option<Location> {
        path.split(self.separator)
            .filter(|segment| !segment.is_empty())
            .try_fold(self.root.clone(), |at, segment| {
                at.join(&ProviderPath::from_text(segment)).ok()
            })
            .filter(|at| at != self.root)
    }

    fn subtree(&self, folder: &Location, room: usize) -> Look {
        let mut gather = Gather {
            root: self.root.as_str(),
            room,
            subtree: Subtree {
                files: Vec::new(),
                folders: Vec::new(),
                is_complete: true,
            },
        };
        let walk = Walk {
            provider: self.provider,
            root: folder,
            cancel: self.cancel,
        };
        let mut totals = FolderSize::default();
        match walk.run(&mut totals, &mut gather) {
            Err(error) if error.kind == FsErrorKind::NotFound => Look::Missing,
            Err(_) => Look::Folder(Some(Subtree {
                is_complete: false,
                ..gather.subtree
            })),
            Ok(()) => Look::Folder(Some(gather.subtree)),
        }
    }
}

struct Gather<'a> {
    root: &'a str,
    room: usize,
    subtree: Subtree,
}

impl Gather<'_> {
    fn has_room(&mut self) -> bool {
        let used = self.subtree.files.len() + self.subtree.folders.len();
        if used >= self.room {
            self.subtree.is_complete = false;
        }
        self.subtree.is_complete
    }
}

impl Visit for Gather<'_> {
    fn file(&mut self, folder: &Location, entry: &FsEntry) {
        if self.has_room() {
            let path = relative(self.root, folder, &entry.name.to_text());
            self.subtree.files.push((path, entry.clone()));
        }
    }

    fn batch_end(&mut self, _totals: &FolderSize) {}

    fn folder(&mut self, parent: &Location, entry: &FsEntry) {
        if self.has_room() {
            let path = relative(self.root, parent, &entry.name.to_text());
            self.subtree.folders.push(path);
        }
    }
}
