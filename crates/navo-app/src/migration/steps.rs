use std::io;
use std::path::{Path, PathBuf};

use super::fs::MigrationFs;
use super::journal::{self, Phase, folder_of};
use super::layout::{
    CREATED, Plan, RestoreSet, Root, free_name, free_names, is_rebuildable, with_suffix,
};

pub(super) fn is_in_use(error: &io::Error, folder: bool) -> bool {
    cfg!(windows)
        && match error.raw_os_error() {
            Some(32 | 33) => true,
            Some(5) => folder,
            _ => false,
        }
}

pub(super) fn holds_navo_data(fs: &impl MigrationFs, plan: &Plan) -> bool {
    plan.roots.iter().any(|root| {
        if root.in_place() {
            return root.renamed().any(|entry| fs.exists(&root.target(entry)));
        }
        fs.exists(&root.new)
            && fs
                .entries(&root.new)
                .map_or(true, |names| names.iter().any(|name| !is_rebuildable(name)))
    })
}

pub(super) fn commit_started(fs: &impl MigrationFs, plan: &Plan) -> bool {
    plan.roots.iter().any(|root| {
        root.moving()
            .any(|entry| fs.exists(&root.source(entry)) && !fs.exists(&root.staged(entry)))
    })
}

pub(super) fn infer_phase(fs: &impl MigrationFs, plan: &Plan) -> Phase {
    if !holds_navo_data(fs, plan) {
        Phase::Detached
    } else if plan.roots.iter().any(|root| fs.exists(&root.staging())) {
        Phase::Staged
    } else {
        Phase::Committed
    }
}

pub(super) fn holds_legacy_data(fs: &impl MigrationFs, plan: &Plan) -> bool {
    plan.roots.iter().any(|root| {
        root.renamed()
            .any(|entry| fs.exists(&root.original(entry)) || fs.exists(&root.detached(entry)))
    })
}

pub(super) fn detach(fs: &impl MigrationFs, plan: &Plan) -> io::Result<()> {
    for root in &plan.roots {
        let mut renamed = false;
        for entry in root.entries.iter().filter(|entry| entry.detach) {
            let original = root.original(entry);
            if fs.exists(&original) {
                fs.rename(&original, &root.detached(entry))?;
                renamed = true;
            }
        }
        if renamed {
            fs.sync_dir(&root.old)?;
        }
    }
    Ok(())
}

pub(super) fn stage(fs: &impl MigrationFs, plan: &Plan) -> io::Result<()> {
    for root in &plan.roots {
        let staging = root.staging();
        if fs.exists(&staging) {
            fs.remove_dir_all(&staging)?;
        }
        let files: Vec<_> = root
            .moving()
            .filter(|entry| fs.exists(&root.source(entry)))
            .collect();
        if files.is_empty() {
            continue;
        }
        fs.create_dir(&staging)?;
        for entry in files {
            copy_verified(fs, &root.source(entry), &root.staged(entry))?;
        }
        fs.sync_dir(&staging)?;
        fs.sync_dir(folder_of(&staging))?;
    }
    Ok(())
}

fn copy_verified(fs: &impl MigrationFs, source: &Path, copy: &Path) -> io::Result<()> {
    fs.copy(source, copy)?;
    fs.sync_file(copy)?;
    if fs.same_bytes(source, copy)? {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("the copy of {} differs from the original", source.display()),
    ))
}

pub(super) fn commit(fs: &impl MigrationFs, plan: &Plan) -> io::Result<()> {
    for root in &plan.roots {
        if root.in_place() {
            commit_files(fs, root)?;
        } else {
            commit_folder(fs, root)?;
        }
    }
    Ok(())
}

fn commit_folder(fs: &impl MigrationFs, root: &Root) -> io::Result<()> {
    let staging = root.staging();
    if !fs.exists(&staging) {
        return check_committed(fs, root, fs.exists(&root.new));
    }
    if fs.exists(&root.new) {
        let names = fs.entries(&root.new)?;
        if !names.is_empty() {
            if names
                .iter()
                .any(|name| !is_rebuildable(name) && !root.is_target(name))
            {
                return Err(conflict(&root.new));
            }
            return commit_files(fs, root);
        }
        fs.remove_dir(&root.new)?;
    }
    fs.rename(&staging, &root.new)?;
    fs.sync_dir(folder_of(&root.new))
}

fn commit_files(fs: &impl MigrationFs, root: &Root) -> io::Result<()> {
    for entry in root.moving() {
        let (staged, target) = (root.staged(entry), root.target(entry));
        if !fs.exists(&staged) {
            check_committed(
                fs,
                root,
                fs.exists(&target) || !fs.exists(&root.source(entry)),
            )?;
            continue;
        }
        if fs.exists(&target) {
            return Err(conflict(&target));
        }
        fs.rename(&staged, &target)?;
    }
    let staging = root.staging();
    if fs.exists(&staging) {
        fs.remove_dir(&staging)?;
    }
    fs.sync_dir(&root.new)
}

fn check_committed(fs: &impl MigrationFs, root: &Root, done: bool) -> io::Result<()> {
    let had_files = root.moving().any(|entry| fs.exists(&root.source(entry)));
    if done || !had_files {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("the staged copy for {} is missing", root.new.display()),
    ))
}

fn conflict(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("{} already holds files", path.display()),
    )
}

pub(super) fn retire(fs: &impl MigrationFs, plan: &Plan) -> io::Result<()> {
    for root in &plan.roots {
        let mut renamed = false;
        for entry in root.entries.iter().filter(|entry| entry.detach) {
            let detached = root.detached(entry);
            if fs.exists(&detached) {
                fs.rename(&detached, &free_name(fs, &root.retired(entry))?)?;
                renamed = true;
            }
        }
        if renamed {
            fs.sync_dir(&root.old)?;
        }
        if !root.in_place() && fs.exists(&root.old) {
            fs.rename(&root.old, &free_name(fs, &root.retired_root())?)?;
            fs.sync_dir(folder_of(&root.old))?;
        }
    }
    Ok(())
}

pub(super) fn rollback(fs: &impl MigrationFs, plan: &Plan) -> io::Result<()> {
    for root in &plan.roots {
        let staging = root.staging();
        if fs.exists(&staging) {
            fs.remove_dir_all(&staging)?;
        }
        let mut renamed = false;
        for set in root.restore_sets() {
            renamed |= restore(fs, root, &set)?;
        }
        if renamed {
            fs.sync_dir(&root.old)?;
        }
    }
    journal::remove(fs, &plan.journal)
}

fn restore(fs: &impl MigrationFs, root: &Root, set: &RestoreSet<'_>) -> io::Result<bool> {
    let detached: Vec<_> = set
        .entries
        .iter()
        .filter(|entry| fs.exists(&root.detached(entry)))
        .collect();
    if detached.is_empty() {
        return Ok(false);
    }
    if detached
        .iter()
        .any(|entry| fs.exists(&root.original(entry)))
    {
        set_aside(fs, root, set)?;
    }
    for entry in detached {
        fs.rename(&root.detached(entry), &root.original(entry))?;
    }
    Ok(true)
}

fn set_aside(fs: &impl MigrationFs, root: &Root, set: &RestoreSet<'_>) -> io::Result<()> {
    let created: Vec<PathBuf> = set
        .entries
        .iter()
        .map(|entry| root.original(entry))
        .chain(set.companions.iter().cloned())
        .filter(|path| fs.exists(path))
        .collect();
    let wanted: Vec<PathBuf> = created
        .iter()
        .map(|path| with_suffix(path, CREATED))
        .collect();
    for (from, to) in created.iter().zip(free_names(fs, &wanted)?) {
        fs.rename(from, &to)?;
    }
    Ok(())
}
