use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};

use super::copy::{failure, temp_of};
use super::{HeldFile, JobContext, StepOutcome};
use crate::{EntryKind, FsEntry, ProviderOps};

pub const HELD_PREFIX: &str = ".ferrite-held-";

static RUNS: AtomicU64 = AtomicU64::new(0);

pub(super) struct Aside {
    folder: ProviderPath,
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
        if presence(ops, target)?.is_none() {
            return ops.replace(&temp, target);
        }
        let held = self.held_file(target)?;
        if !ctx.log.journal_held(&held)? {
            return ops.replace(&temp, target);
        }
        make_room(ops, &held.held)?;
        ops.rename(target, &held.held)?;
        ops.replace(&temp, target).inspect_err(|_| {
            let _ = ops.rename(&held.held, target);
        })
    }
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

pub(super) fn restore(ops: &dyn ProviderOps, held: &HeldFile) -> Result<StepOutcome, FsError> {
    if presence(ops, &held.held)?.is_none() {
        return Ok(StepOutcome::Skipped);
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
    for folder in &folders {
        if let Err(error) = remove_folder(ops, folder) {
            first.get_or_insert(error);
        }
    }
    first.map_or(Ok(()), Err)
}

fn release_one(ops: &dyn ProviderOps, file: &HeldFile) -> Result<(), FsError> {
    if presence(ops, &file.held)?.is_none() {
        return Ok(());
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
