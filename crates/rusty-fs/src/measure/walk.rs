use std::collections::HashSet;
use std::ops::ControlFlow;

use rusty_core::{FsError, FsErrorKind, Location};

use super::FolderSize;
use crate::{CancelToken, EntryBatch, EntryKind, FsEntry, LinkKey, Provider, SizeEntry};

const LINK_LIMIT: usize = 1_000_000;

pub(super) trait Visit {
    fn needs_fs_entries(&self) -> bool {
        true
    }

    fn file(&mut self, folder: &Location, entry: &FsEntry);
    fn batch_end(&mut self, totals: &FolderSize);
    fn folder(&mut self, _parent: &Location, _entry: &FsEntry) {}
}

pub(super) struct Walk<'a> {
    pub(super) provider: &'a dyn Provider,
    pub(super) root: &'a Location,
    pub(super) cancel: &'a CancelToken,
}

impl Walk<'_> {
    pub(super) fn run(
        &self,
        totals: &mut FolderSize,
        visit: &mut dyn Visit,
    ) -> Result<(), FsError> {
        self.run_with_link_limit(totals, visit, LINK_LIMIT)
    }

    pub(super) fn run_with_link_limit(
        &self,
        totals: &mut FolderSize,
        visit: &mut dyn Visit,
        limit: usize,
    ) -> Result<(), FsError> {
        let mut pending = vec![self.root.clone()];
        let mut links = Links {
            keys: HashSet::new(),
            limit,
        };
        let mut top = true;
        while let Some(location) = pending.pop() {
            if self.cancel.is_cancelled() {
                return Err(cancelled(&location));
            }
            let mut folder = Folder {
                location: &location,
                totals,
                visit,
                links: &mut links,
                children: Vec::new(),
            };
            match self.list_one(&mut folder) {
                Ok(()) => pending.extend(folder.children),
                Err(error) if top || error.kind == FsErrorKind::Cancelled => return Err(error),
                Err(_) => totals.skipped += 1,
            }
            top = false;
        }
        Ok(())
    }

    fn list_one(&self, folder: &mut Folder<'_>) -> Result<(), FsError> {
        let location = folder.location;
        if folder.visit.needs_fs_entries() {
            self.provider.list(
                location,
                &mut |batch: EntryBatch| {
                    folder.take(&batch);
                    self.control()
                },
                self.cancel,
            )?;
        } else {
            self.provider.list_sizes(
                location,
                &mut |entry| self.take_size(folder, &entry),
                self.cancel,
            )?;
        }
        if self.cancel.is_cancelled() {
            return Err(cancelled(location));
        }
        Ok(())
    }

    fn take_size(&self, folder: &mut Folder<'_>, entry: &SizeEntry) -> ControlFlow<()> {
        if self.cancel.is_cancelled() {
            return ControlFlow::Break(());
        }
        folder.take_size(entry);
        self.control()
    }

    fn control(&self) -> ControlFlow<()> {
        if self.cancel.is_cancelled() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

struct Links {
    keys: HashSet<LinkKey>,
    limit: usize,
}

impl Links {
    fn merge(&mut self, link: Option<LinkKey>, totals: &mut FolderSize) -> bool {
        let Some(key) = link else {
            return false;
        };
        if self.keys.len() >= self.limit {
            totals.links_unchecked = true;
            return false;
        }
        if !self.keys.insert(key) {
            totals.links_merged += 1;
            return true;
        }
        totals.links_unchecked |= self.keys.len() == self.limit;
        false
    }
}

struct Folder<'a> {
    location: &'a Location,
    totals: &'a mut FolderSize,
    visit: &'a mut dyn Visit,
    links: &'a mut Links,
    children: Vec<Location>,
}

impl Folder<'_> {
    fn take(&mut self, batch: &EntryBatch) {
        for entry in &batch.entries {
            if entry.kind == EntryKind::Folder {
                self.totals.folders += 1;
                self.visit.folder(self.location, entry);
                self.children.extend(self.location.join(&entry.name).ok());
            } else {
                self.count_file(entry.size, None, None);
                self.visit.file(self.location, entry);
            }
        }
        self.visit.batch_end(self.totals);
    }

    fn take_size(&mut self, entry: &SizeEntry) {
        if entry.kind == EntryKind::Folder {
            self.totals.folders += 1;
            self.children.extend(self.location.join(&entry.name).ok());
        } else {
            self.count_file(entry.size, entry.on_disk, entry.link);
        }
        self.visit.batch_end(self.totals);
    }

    fn count_file(&mut self, size: u64, on_disk: Option<u64>, link: Option<LinkKey>) {
        if self.links.merge(link, self.totals) {
            return;
        }
        self.totals.files += 1;
        self.totals.bytes = self.totals.bytes.saturating_add(size);
        self.totals.on_disk = self.totals.on_disk.saturating_add(on_disk.unwrap_or(size));
        self.totals.on_disk_known &= on_disk.is_some();
    }
}

fn cancelled(location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(FsErrorKind::Cancelled)
    }
}
