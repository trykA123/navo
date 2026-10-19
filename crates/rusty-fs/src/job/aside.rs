use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};

use super::copy::{failure, temp_of};
use super::{HeldFile, JobContext, StepOutcome};
use crate::{CancelToken, EntryBatch, EntryKind, FsEntry, ProviderOps};

pub const HELD_PREFIX: &str = ".ferrite-held-";

static RUNS: AtomicU64 = AtomicU64::new(0);

pub(super) struct Aside {
    folder: ProviderPath,
    spellings: Mutex<HashMap<String, Vec<ProviderPath>>>,
}

impl Aside {
    pub(super) fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let run = RUNS.fetch_add(1, Ordering::Relaxed);
        let name = format!("{HELD_PREFIX}{:x}-{nanos:x}-{run:x}", std::process::id());
        Self {
            folder: ProviderPath::from_text(&name),
            spellings: Mutex::new(HashMap::new()),
        }
    }

    fn held_file(&self, target: &Location) -> Result<HeldFile, FsError> {
        let invalid = || failure(FsErrorKind::NameInvalid, target);
        let name = target.name().ok_or_else(invalid)?;
        let folder = target.parent().ok_or_else(invalid)?;
        let held = folder
            .join(&self.folder)
            .and_then(|folder| folder.join(&name))
            .map_err(|_| invalid())?;
        Ok(HeldFile {
            original: target.clone(),
            held,
        })
    }

    pub(super) fn place_over(
        &self,
        ctx: &mut JobContext<'_>,
        target: &Location,
    ) -> Result<(), FsError> {
        let ops = ctx.ops;
        let temp = temp_of(target)?;
        match presence(ops, target)? {
            None => return ops.replace(&temp, target),
            Some(entry) if matches!(entry.kind, EntryKind::File | EntryKind::Link) => {}
            Some(_) => return Err(failure(FsErrorKind::NameInvalid, target)),
        }
        let held = self.held_file(&self.on_disk(ops, target))?;
        if !ctx.log.journal_held(&held)? {
            return ops.replace(&temp, target);
        }
        make_room(ops, &held.held)?;
        ops.rename(&held.original, &held.held)?;
        ops.replace(&temp, target).inspect_err(|_| {
            let _ = ops.rename(&held.held, &held.original);
        })
    }

    fn on_disk(&self, ops: &dyn ProviderOps, target: &Location) -> Location {
        let Some(folder) = target.parent() else {
            return target.clone();
        };
        let mut spellings = self
            .spellings
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let names = spellings
            .entry(folder.as_str().to_owned())
            .or_insert_with(|| names_in(ops, &folder));
        let found = names
            .iter()
            .filter_map(|name| folder.join(name).ok())
            .filter(|candidate| candidate.same_place(target))
            .min_by_key(|candidate| candidate != target);
        found.unwrap_or_else(|| target.clone())
    }
}

fn names_in(ops: &dyn ProviderOps, folder: &Location) -> Vec<ProviderPath> {
    let mut names = Vec::new();
    let mut sink = |batch: EntryBatch| {
        names.extend(batch.entries.into_iter().map(|entry| entry.name));
        ControlFlow::Continue(())
    };
    let _ = ops.list(folder, &mut sink, &CancelToken::new());
    names
}

fn presence(ops: &dyn ProviderOps, location: &Location) -> Result<Option<FsEntry>, FsError> {
    match ops.metadata(location) {
        Ok(entry) => Ok(Some(entry)),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn make_room(ops: &dyn ProviderOps, held: &Location) -> Result<(), FsError> {
    let folder = held
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, held))?;
    match presence(ops, &folder)? {
        Some(entry) if entry.kind == EntryKind::Folder => {}
        Some(_) => return Err(failure(FsErrorKind::Unknown, &folder)),
        None => ops.create_folder(&folder)?,
    }
    match presence(ops, held)? {
        Some(_) => Err(failure(FsErrorKind::Unknown, held)),
        None => Ok(()),
    }
}

fn surely_gone(ops: &dyn ProviderOps, held: &HeldFile) -> Result<(), FsError> {
    let folder = held
        .original
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.original))?;
    match presence(ops, &folder)? {
        Some(_) => Ok(()),
        None => Err(failure(FsErrorKind::NotFound, &folder)),
    }
}

pub(super) fn restore(ops: &dyn ProviderOps, held: &HeldFile) -> Result<StepOutcome, FsError> {
    if presence(ops, &held.held)?.is_none() {
        return surely_gone(ops, held).map(|()| StepOutcome::Skipped);
    }
    ops.replace(&held.held, &held.original)
        .map(|()| StepOutcome::Done)
}

pub(super) fn release(held: &[HeldFile], ops: &dyn ProviderOps) -> Result<(), FsError> {
    let mut first = None;
    let mut folders: Vec<Location> = Vec::new();
    for file in held {
        if let Err(error) = release_one(ops, file) {
            first.get_or_insert(error);
        }
        if let Some(folder) = file.held.parent()
            && !folders.iter().any(|known| known.same_place(&folder))
        {
            folders.push(folder);
        }
    }
    if first.is_none() {
        for folder in &folders {
            let _ = remove_folder(ops, folder);
        }
    }
    first.map_or(Ok(()), Err)
}

fn release_one(ops: &dyn ProviderOps, file: &HeldFile) -> Result<(), FsError> {
    if presence(ops, &file.held)?.is_none() {
        return surely_gone(ops, file);
    }
    match presence(ops, &file.original)? {
        Some(_) => ops.remove_file(&file.held),
        None => ops.rename(&file.held, &file.original),
    }
}

fn remove_folder(ops: &dyn ProviderOps, folder: &Location) -> Result<(), FsError> {
    match presence(ops, folder)? {
        Some(_) => ops.remove_empty_folder(folder),
        None => Ok(()),
    }
}
