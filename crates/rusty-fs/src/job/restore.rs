use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};

use super::HeldFile;
use super::aside::presence;
use super::copy::failure;
use crate::{EntryKind, FsEntry, ProviderOps};

const BESIDE_LIMIT: u32 = 999;

pub(super) enum Restored {
    Back,
    Beside(Location),
    Gone,
}

pub(super) fn restore(ops: &dyn ProviderOps, held: &HeldFile) -> Result<Restored, FsError> {
    bring_back(ops, held, true)
}

pub(super) fn keep(ops: &dyn ProviderOps, held: &HeldFile) -> Result<Restored, FsError> {
    bring_back(ops, held, false)
}

fn bring_back(
    ops: &dyn ProviderOps,
    held: &HeldFile,
    replace_placed: bool,
) -> Result<Restored, FsError> {
    if presence(ops, &held.held)?.is_none() {
        return surely_gone(ops, held).map(|()| Restored::Gone);
    }
    match presence(ops, &held.original)? {
        None => rename_back(ops, held),
        Some(entry) if replace_placed && is_placed(&entry, held) => ops
            .replace(&held.held, &held.original)
            .map(|()| Restored::Back),
        Some(_) => keep_beside(ops, held).map(Restored::Beside),
    }
}

fn rename_back(ops: &dyn ProviderOps, held: &HeldFile) -> Result<Restored, FsError> {
    match ops.rename(&held.held, &held.original) {
        Ok(()) => Ok(Restored::Back),
        Err(error) => {
            if presence(ops, &held.original)?.is_none() {
                return Err(error);
            }
            keep_beside(ops, held).map(Restored::Beside)
        }
    }
}

fn is_placed(entry: &FsEntry, held: &HeldFile) -> bool {
    held.placed.is_some_and(|stamp| {
        entry.kind == EntryKind::File
            && entry.size == stamp.size
            && entry.modified_ms.is_some()
            && entry.modified_ms == stamp.modified_ms
    })
}

fn keep_beside(ops: &dyn ProviderOps, held: &HeldFile) -> Result<Location, FsError> {
    let invalid = || failure(FsErrorKind::NameInvalid, &held.original);
    let folder = held.original.parent().ok_or_else(invalid)?;
    let name = held.original.name().ok_or_else(invalid)?.to_text();
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name.as_str(), ""),
    };
    for number in 1..=BESIDE_LIMIT {
        let label = match number {
            1 => "before copy".to_owned(),
            n => format!("before copy {n}"),
        };
        let beside = ProviderPath::from_text(&format!("{stem} ({label}){extension}"));
        let candidate = folder.join(&beside).map_err(|_| invalid())?;
        match ops.rename(&held.held, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if presence(ops, &candidate)?.is_none() => return Err(error),
            Err(_) => {}
        }
    }
    Err(failure(FsErrorKind::Unknown, &held.original))
}

fn surely_gone(ops: &dyn ProviderOps, held: &HeldFile) -> Result<(), FsError> {
    let folder = held
        .held
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.held))?;
    if presence(ops, &folder)?.is_some() || presence(ops, &held.original)?.is_some() {
        return Ok(());
    }
    Err(failure(FsErrorKind::NotFound, &folder))
}

pub(super) fn surely_absent(ops: &dyn ProviderOps, location: &Location) -> Result<bool, FsError> {
    if presence(ops, location)?.is_some() {
        return Ok(false);
    }
    let folder = location
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, location))?;
    match presence(ops, &folder)? {
        Some(_) => Ok(true),
        None => Err(failure(FsErrorKind::NotFound, &folder)),
    }
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
        None => rename_back(ops, file).map(|_| ()),
    }
}

fn remove_folder(ops: &dyn ProviderOps, folder: &Location) -> Result<(), FsError> {
    match presence(ops, folder)? {
        Some(_) => ops.remove_empty_folder(folder),
        None => Ok(()),
    }
}
