use std::sync::{Arc, PoisonError, mpsc};
use std::time::{Duration, Instant};

use navo_core::{FsError, Location, ProviderPath};
use navo_fs::{CancelToken, EntryKind, FsEntry, WatchChange, WatchEvent};

use super::Worker;
use crate::ntfs::COMPACT_MIN_SLOTS;
use crate::table::{Flags, NO_PARENT, NameTable};
use crate::walker::{self, Outcome, Progress, WalkResult};

/// One watch change with its disk metadata already read, so applying it holds
/// the table write lock for table work only.
enum Prepared {
    Removed(Option<Location>),
    Refreshed {
        location: Location,
        entry: Result<FsEntry, FsError>,
    },
    Renamed {
        from: Option<Location>,
        to: Option<Location>,
    },
}

impl Worker {
    pub(super) fn scan(&mut self) {
        let root = self.options.root.clone();
        self.scan_root(&root);
    }

    fn scan_root(&mut self, root: &Location) {
        if self.cancel.is_cancelled() {
            return;
        }
        let (sender, result) = mpsc::channel();
        let options = self.options.clone();
        let cancel = self.cancel.clone();
        let updates = Arc::clone(&self.statuses);
        self.status.progress = Some(Progress::default());
        self.publish();
        let mut status = self.status.clone();
        let on_progress = move |_: &NameTable, progress: Progress| {
            if cancel.is_cancelled() {
                return;
            }
            status.entries = progress.entries;
            status.progress = Some(progress);
            updates.publish(status.clone());
        };
        let cancel = self.cancel.clone();
        let scan_root = root.clone();
        let job = move |_: &CancelToken| {
            let result = walker::walk(
                options.provider.as_ref(),
                &scan_root,
                options.limits.clone(),
                &cancel,
                on_progress,
            );
            drop((options, cancel));
            let _ = sender.send(result);
        };
        self.options.queue.submit(self.slot.clone(), Box::new(job));
        match result.recv() {
            Ok(Ok(result)) => self.finish_scan(root, result),
            Ok(Err(error)) => self.error(format!("scanning index: {error}")),
            _ if !self.cancel.is_cancelled() => self.error("scanning index: worker stopped".into()),
            _ => {}
        }
        self.status.progress = None;
        self.publish();
    }

    fn finish_scan(&mut self, root: &Location, result: WalkResult) {
        let incomplete = result.progress.skipped + result.progress.depth_limited > 0;
        if incomplete {
            self.error(format!("incomplete index scan: {:?}", result.progress));
        }
        if result.outcome == Outcome::EntryLimit {
            self.error("index entry limit reached".into());
        }
        if self.cancel.is_cancelled() || result.outcome != Outcome::Complete || incomplete {
            return;
        }
        let walked = result.table;
        let shared = Arc::clone(&self.table);
        let mut table = shared.write().unwrap_or_else(PoisonError::into_inner);
        if root.same_place(&self.options.root) {
            *table = walked;
        } else {
            match self.merge_subtree(&mut table, root, &walked) {
                Ok(true) => {
                    drop(walked);
                    compact_if_sparse(&mut table);
                }
                Ok(false) => return,
                Err(error) => {
                    self.error(format!("updating index: {error}"));
                    return;
                }
            }
        }
        self.dirty = Some((self.clock)());
    }

    /// Replaces the subtree at `root` with `walked`, freeing the old subtree before the append.
    fn merge_subtree(
        &mut self,
        table: &mut NameTable,
        root: &Location,
        walked: &NameTable,
    ) -> Result<bool, String> {
        let old = folder(table, root).ok_or("unknown index subtree")?;
        if table.len() - subtree_len(table, old) + walked.len() > self.options.limits.max_entries {
            return Err("index entry limit reached".into());
        }
        let parent = table.parent(old).unwrap_or(NO_PARENT);
        let name = table.name(old).unwrap_or_default().to_owned();
        let flags = table.flags(old).unwrap_or(Flags::FOLDER);
        table.remove(old);
        let parent = table.compact().get(parent).unwrap_or(NO_PARENT);
        if let Err(error) = reserve_append(table, walked, &name) {
            self.queue_rescan(root);
            return Err(error);
        }
        let new = match table.push(parent, &name, flags) {
            Ok(new) => new,
            Err(error) => {
                self.queue_rescan(root);
                return Err(error.to_string());
            }
        };
        let appended = append_children(table, new, walked);
        if appended.is_err() || self.cancel.is_cancelled() {
            table.remove(new);
            self.queue_rescan(root);
            return appended.map(|()| false);
        }
        Ok(true)
    }

    /// Leaves `root` for one more walk, unless this scan is already that walk.
    fn queue_rescan(&mut self, root: &Location) {
        if !self.retrying && !self.rescan.iter().any(|queued| queued.same_place(root)) {
            self.rescan.push(root.clone());
        }
    }

    /// Walks each queued root once; a second failure leaves it to the watcher.
    pub(super) fn scan_queued(&mut self) {
        if self.rescan.is_empty() {
            return;
        }
        self.retrying = true;
        for root in std::mem::take(&mut self.rescan) {
            self.scan_root(&root);
        }
        self.retrying = false;
    }

    pub(super) fn update(&mut self, event: WatchEvent) {
        let changes = match event {
            WatchEvent::Changed(changes) => changes,
            WatchEvent::Overflow => return self.scan(),
            WatchEvent::Failed(error) => {
                self.error(format!("watching root: {}", error.message()));
                return self.publish();
            }
        };
        let prepared = self.prepare(changes);
        let shared = Arc::clone(&self.table);
        let mut table = shared.write().unwrap_or_else(PoisonError::into_inner);
        let before = (table.len(), table.slots());
        let mut roots = Vec::new();
        for change in prepared {
            if let Err(error) = self.apply(&mut table, change, &mut roots) {
                self.error(format!("updating index: {error}"));
            }
        }
        if before != (table.len(), table.slots()) {
            self.dirty = Some((self.clock)());
        }
        let roots = self.rescan_roots(&table, roots);
        drop(table);
        self.publish();
        for root in roots {
            self.scan_root(&root);
        }
    }

    /// Reads the disk metadata of every created or modified path before the
    /// table lock is taken, so the lock is held for table work only.
    fn prepare(&self, changes: Vec<WatchChange>) -> Vec<Prepared> {
        let mut prepared = Vec::with_capacity(changes.len());
        for change in changes {
            match change {
                WatchChange::Removed(path) => {
                    prepared.push(Prepared::Removed(self.resolve(&path)));
                }
                WatchChange::Created(path) | WatchChange::Modified(path) => {
                    prepared.extend(self.prepared_refresh(&path));
                }
                WatchChange::Renamed { from, to } => prepared.push(Prepared::Renamed {
                    from: self.resolve(&from),
                    to: self.resolve(&to),
                }),
            }
        }
        prepared
    }

    /// Resolves one created or modified path and reads its metadata.
    fn prepared_refresh(&self, path: &ProviderPath) -> Option<Prepared> {
        let location = self.resolve(path)?;
        let entry = self.options.provider.metadata(&location);
        Some(Prepared::Refreshed { location, entry })
    }

    fn apply(
        &self,
        table: &mut NameTable,
        change: Prepared,
        roots: &mut Vec<Location>,
    ) -> Result<(), String> {
        match change {
            Prepared::Removed(location) => {
                let id = location.and_then(|location| self.entry(table, &location));
                table.remove(id.unwrap_or(NO_PARENT));
            }
            Prepared::Refreshed { location, entry } => {
                if let Some(root) = self.refresh(table, &location, entry)? {
                    roots.push(root);
                }
            }
            Prepared::Renamed { from, to } => {
                roots.extend([from, to].into_iter().flatten().map(|location| {
                    location
                        .parent()
                        .unwrap_or_else(|| self.options.root.clone())
                }));
            }
        }
        Ok(())
    }

    fn refresh(
        &self,
        table: &mut NameTable,
        location: &Location,
        entry: Result<FsEntry, FsError>,
    ) -> Result<Option<Location>, String> {
        let old = self.entry(table, location);
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind == navo_core::FsErrorKind::NotFound => {
                table.remove(old.unwrap_or(NO_PARENT));
                return Ok(None);
            }
            Err(error) => return Err(error.message()),
        };
        let parent_location = location
            .parent()
            .unwrap_or_else(|| self.options.root.clone());
        if entry.kind == EntryKind::Folder {
            return Ok(Some(parent_location));
        }
        let Some(parent) = folder(table, &parent_location) else {
            return Ok(Some(parent_location));
        };
        if table
            .flags(parent)
            .is_some_and(|flags| flags.contains(Flags::REPARSE))
        {
            return Ok(None);
        }
        let flags = flags(&entry);
        if old.is_some_and(|id| table.flags(id) == Some(flags)) {
            return Ok(None);
        }
        table.remove(old.unwrap_or(NO_PARENT));
        if table.len() >= self.options.limits.max_entries {
            return Err("index entry limit reached".into());
        }
        table
            .push(parent, &entry.name.to_text(), flags)
            .map_err(|error| error.to_string())?;
        Ok(None)
    }

    /// Finds a live entry by path: its parent folder through `FolderLookup`,
    /// then the matching child of that folder.
    fn entry(&self, table: &NameTable, location: &Location) -> Option<u32> {
        if location.same_place(&self.options.root) {
            return Some(0);
        }
        let parent = folder(table, &location.parent()?)?;
        table.children(parent).find(|&child| {
            self.location(table, child)
                .is_some_and(|child| child.same_place(location))
        })
    }

    fn rescan_roots(&self, table: &NameTable, roots: Vec<Location>) -> Vec<Location> {
        let mut distinct = Vec::new();
        for root in roots {
            if distinct.iter().any(|parent| contains(parent, &root)) {
                continue;
            }
            distinct.retain(|child| !contains(&root, child));
            distinct.push(root);
        }
        let mut roots = Vec::new();
        for root in distinct {
            let Some(id) = folder(table, &root) else {
                return vec![self.options.root.clone()];
            };
            if table
                .flags(id)
                .is_some_and(|flags| flags.contains(Flags::REPARSE))
            {
                continue;
            }
            roots.push(root);
        }
        roots
    }
}

/// Finds a folder by path through `FolderLookup`, in O(depth) instead of a
/// full-table scan.
fn folder(table: &NameTable, location: &Location) -> Option<u32> {
    table
        .folder_lookup()
        .find_folder(
            &location.path().to_text(),
            Instant::now() + Duration::from_millis(20),
        )
        .ok()
        .flatten()
}

fn contains(parent: &Location, location: &Location) -> bool {
    std::iter::successors(Some(location.clone()), Location::parent)
        .any(|location| parent.same_place(&location))
}

/// Compacts once dead slots pass a third of all slots.
fn compact_if_sparse(table: &mut NameTable) {
    if table.is_sparse(COMPACT_MIN_SLOTS) {
        drop(table.compact());
    }
}

/// Reserves the whole append while the table is still small, so no column is copied beside
/// `walked`. The root name counts from both tables, so a differing one still fits.
fn reserve_append(table: &mut NameTable, walked: &NameTable, name: &str) -> Result<(), String> {
    table
        .try_reserve_total(
            table.len() + walked.len(),
            table.folder_count() + walked.folder_count(),
            table.live_name_bytes() + walked.live_name_bytes() + name.len() + 1,
        )
        .map_err(|error| format!("reserving index subtree: {error}"))
}

/// Live entries at or below `old`, for the limit check before the subtree goes.
fn subtree_len(table: &NameTable, old: u32) -> usize {
    let mut count = 0;
    let mut pending = vec![old];
    while let Some(id) = pending.pop() {
        count += 1;
        pending.extend(table.children(id));
    }
    count
}

fn append_children(table: &mut NameTable, root: u32, walked: &NameTable) -> Result<(), String> {
    let mut ids = vec![NO_PARENT; walked.slots() as usize];
    let Some(first) = ids.first_mut() else {
        return Err("empty index subtree".into());
    };
    *first = root;
    for (id, name) in walked.iter().skip(1) {
        let parent = walked.parent(id).ok_or("unknown index parent")?;
        ids[id as usize] = table
            .push(
                ids[parent as usize],
                name,
                walked.flags(id).unwrap_or(Flags::NONE),
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn flags(entry: &FsEntry) -> Flags {
    [
        (entry.attributes.link_to_folder, Flags::FOLDER),
        (entry.kind == EntryKind::Link, Flags::REPARSE),
        (entry.attributes.hidden, Flags::HIDDEN),
        (entry.attributes.system, Flags::SYSTEM),
    ]
    .into_iter()
    .filter(|(set, _)| *set)
    .fold(Flags::NONE, |flags, (_, flag)| flags | flag)
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod merge_tests;
