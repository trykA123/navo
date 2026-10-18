use std::fs;
use std::io;
use std::path::Path;

use tempfile::TempDir;

use super::*;

type StageHook = Box<dyn FnMut(&Path, Stage) -> io::Result<()>>;

thread_local! {
    static HOOK: std::cell::RefCell<Option<StageHook>> = const { std::cell::RefCell::new(None) };
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

mod faults;
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
        .filter(|name| name.contains(".tmp-"))
        .collect()
}

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}
