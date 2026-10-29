use std::sync::{Arc, PoisonError, mpsc};

use rusty_core::{Location, ProviderPath};
use rusty_fs::{CancelToken, EntryKind, FsEntry, WatchChange, WatchEvent};

use super::Worker;
use crate::table::{Flags, NO_PARENT, NameTable};
use crate::walker::{self, Outcome, Progress, WalkResult};

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
        let updates = self.updates.clone();
        self.status.progress = Some(Progress::default());
        self.publish();
        let mut status = self.status.clone();
        let on_progress = move |_: &NameTable, progress: Progress| {
            if cancel.is_cancelled() {
                return;
            }
            status.entries = progress.entries;
            status.progress = Some(progress);
            let _ = updates.send(status.clone());
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
        let shared = Arc::clone(&self.table);
        let mut table = shared.write().unwrap_or_else(PoisonError::into_inner);
        if root.same_place(&self.options.root) {
            *table = result.table;
        } else {
            match self.merge_subtree(&mut table, root, &result.table) {
                Ok(true) => {}
                Ok(false) => return,
                Err(error) => {
                    self.error(format!("updating index: {error}"));
                    return;
                }
            }
        }
        self.dirty = Some((self.clock)());
    }

    fn merge_subtree(
        &self,
        table: &mut NameTable,
        root: &Location,
        walked: &NameTable,
    ) -> Result<bool, String> {
        let old = self.find(table, root).ok_or("unknown index subtree")?;
        let removed = table
            .iter()
            .filter(|(id, _)| {
                std::iter::successors(Some(*id), |&id| table.parent(id))
                    .take(table.slots() as usize)
                    .any(|id| id == old)
            })
            .count();
        if table.len() - removed + walked.len() > self.options.limits.max_entries {
            return Err("index entry limit reached".into());
        }
        let parent = table.parent(old).unwrap_or(NO_PARENT);
        let name = table.name(old).unwrap_or_default().to_owned();
        let flags = table.flags(old).unwrap_or(Flags::FOLDER);
        let new = table
            .push(parent, &name, flags)
            .map_err(|error| error.to_string())?;
        let appended = append_children(table, new, walked);
        if appended.is_err() || self.cancel.is_cancelled() {
            table.remove(new);
            return appended.map(|()| false);
        }
        table.remove(old);
        Ok(true)
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
        let shared = Arc::clone(&self.table);
        let mut table = shared.write().unwrap_or_else(PoisonError::into_inner);
        let before = (table.len(), table.slots());
        let mut roots = Vec::new();
        for change in changes {
            if let Err(error) = self.patch(&mut table, change, &mut roots) {
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

    fn patch(
        &self,
        table: &mut NameTable,
        change: WatchChange,
        roots: &mut Vec<Location>,
    ) -> Result<(), String> {
        match change {
            WatchChange::Removed(path) => {
                let location = self.resolve(&path);
                let id = location.and_then(|location| self.find(table, &location));
                table.remove(id.unwrap_or(NO_PARENT));
            }
            WatchChange::Created(path) | WatchChange::Modified(path) => {
                if let Some(root) = self.refresh(table, &path)? {
                    roots.push(root);
                }
            }
            WatchChange::Renamed { from, to } => {
                roots.extend([from, to].iter().filter_map(|path| self.resolve(path)).map(
                    |location| {
                        location
                            .parent()
                            .unwrap_or_else(|| self.options.root.clone())
                    },
                ));
            }
        }
        Ok(())
    }

    fn refresh(
        &self,
        table: &mut NameTable,
        path: &ProviderPath,
    ) -> Result<Option<Location>, String> {
        let Some(location) = self.resolve(path) else {
            return Ok(None);
        };
        let old = self.find(table, &location);
        let entry = match self.options.provider.metadata(&location) {
            Ok(entry) => entry,
            Err(error) if error.kind == rusty_core::FsErrorKind::NotFound => {
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
        let Some(parent) = self.find(table, &parent_location) else {
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
            let Some(id) = self.find(table, &root) else {
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

fn contains(parent: &Location, location: &Location) -> bool {
    std::iter::successors(Some(location.clone()), Location::parent)
        .any(|location| parent.same_place(&location))
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
