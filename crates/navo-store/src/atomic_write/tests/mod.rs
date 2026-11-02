use std::fs;
use std::io;
use std::path::Path;

use tempfile::TempDir;

use super::*;

type StageHook = Box<dyn FnMut(&Path, Stage) -> io::Result<()>>;

thread_local! {
    static HOOK: std::cell::RefCell<Option<StageHook>> = const { std::cell::RefCell::new(None) };
    static FAULTS: std::cell::RefCell<Vec<(Op, io::ErrorKind)>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn inject(op: Op, kind: io::ErrorKind) {
    let mut faults = vec![(op, kind)];
    // Copy steps run only when the hard link fails.
    if matches!(op, Op::BackupCopy | Op::BackupSync) {
        faults.push((Op::BackupLink, io::ErrorKind::Unsupported));
    }
    FAULTS.with(|slot| *slot.borrow_mut() = faults);
}

fn clear_fault() {
    FAULTS.with(|slot| slot.borrow_mut().clear());
}

pub(super) fn before(op: Op) -> io::Result<()> {
    let found = FAULTS.with(|slot| slot.borrow().iter().find(|(at, _)| *at == op).copied());
    match found {
        Some((_, kind)) => Err(io::Error::new(kind, format!("injected at {op:?}"))),
        None => Ok(()),
    }
}

pub(crate) fn on_stage(hook: impl FnMut(&Path, Stage) -> io::Result<()> + 'static) {
    HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

pub(super) fn at_stage(path: &Path, stage: Stage) -> io::Result<()> {
    HOOK.with(|slot| match slot.borrow_mut().as_mut() {
        Some(hook) => hook(path, stage),
        None => Ok(()),
    })
}

mod backup;
mod faults;
mod io_faults;
mod orphans;
mod props;
mod unit;
#[cfg(unix)]
mod unix;

const ALL_STAGES: [Stage; 5] = [
    Stage::TempWritten,
    Stage::TempSynced,
    Stage::BackupMade,
    Stage::BeforeRename,
    Stage::Replaced,
];

const ALL_OPS: [Op; 8] = [
    Op::Write,
    Op::Sync,
    Op::BackupCopy,
    Op::BackupSync,
    Op::BackupRename,
    Op::BackupFolderSync,
    Op::Rename,
    Op::FolderSync,
];

const BACKUP_OPS: [Op; 4] = [
    Op::BackupCopy,
    Op::BackupSync,
    Op::BackupRename,
    Op::BackupFolderSync,
];

const INJECTED: io::ErrorKind = io::ErrorKind::ConnectionAborted;

fn fail_at(stage: Stage) -> impl FnMut(Stage) -> io::Result<()> {
    move |reached| {
        if reached == stage {
            Err(io::Error::new(INJECTED, format!("injected at {reached:?}")))
        } else {
            Ok(())
        }
    }
}

fn temp_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp-") || name.contains(".bak-tmp-"))
        .collect()
}

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}
