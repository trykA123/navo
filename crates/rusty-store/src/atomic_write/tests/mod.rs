use std::fs;
use std::io;
use std::path::Path;

use tempfile::TempDir;

use super::*;

mod faults;
mod props;
mod unit;
#[cfg(unix)]
mod unix;

const ALL_STAGES: [Stage; 4] = [
    Stage::TempWritten,
    Stage::TempSynced,
    Stage::BackupMade,
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
