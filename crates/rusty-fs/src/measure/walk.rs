use std::ops::ControlFlow;

use rusty_core::{FsError, FsErrorKind, Location};

use super::FolderSize;
use crate::{CancelToken, EntryBatch, EntryKind, FsEntry, Provider};

pub(super) trait Visit {
    fn file(&mut self, folder: &Location, entry: &FsEntry);
    fn batch_end(&mut self, totals: &FolderSize);
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
        let mut pending = vec![self.root.clone()];
        let mut top = true;
        while let Some(location) = pending.pop() {
            if self.cancel.is_cancelled() {
                return Err(cancelled(&location));
            }
            match self.list_one(&location, totals, visit) {
                Ok(children) => pending.extend(children),
                Err(error) if top || error.kind == FsErrorKind::Cancelled => return Err(error),
                Err(_) => totals.skipped += 1,
            }
            top = false;
        }
        Ok(())
    }

    fn list_one(
        &self,
        location: &Location,
        totals: &mut FolderSize,
        visit: &mut dyn Visit,
    ) -> Result<Vec<Location>, FsError> {
        let mut folder = Folder {
            location,
            totals,
            visit,
            children: Vec::new(),
        };
        let mut sink = |batch: EntryBatch| {
            folder.take(&batch);
            if self.cancel.is_cancelled() {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        self.provider.list(location, &mut sink, self.cancel)?;
        if self.cancel.is_cancelled() {
            return Err(cancelled(location));
        }
        Ok(folder.children)
    }
}

struct Folder<'a> {
    location: &'a Location,
    totals: &'a mut FolderSize,
    visit: &'a mut dyn Visit,
    children: Vec<Location>,
}

impl Folder<'_> {
    fn take(&mut self, batch: &EntryBatch) {
        for entry in &batch.entries {
            if entry.kind == EntryKind::Folder {
                self.totals.folders += 1;
                self.children.extend(self.location.join(&entry.name).ok());
            } else {
                self.totals.files += 1;
                self.totals.bytes = self.totals.bytes.saturating_add(entry.size);
                self.visit.file(self.location, entry);
            }
        }
        self.visit.batch_end(self.totals);
    }
}

fn cancelled(location: &Location) -> FsError {
    FsError {
        kind: FsErrorKind::Cancelled,
        location: Some(location.clone()),
        os_code: None,
    }
}
