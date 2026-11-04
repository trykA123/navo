//! Deletes list files that earlier runs left behind.

use std::path::Path;
use std::time::{Duration, SystemTime};

use super::{PENDING, list_dir, remove_list, remove_one, sidecar};

const LIST_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// How sure the sweep is that a command process is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::user_commands) enum Liveness {
    Running,
    Gone,
    Unknown,
}

enum Sidecar {
    Missing,
    /// The start never published the pid; a crash can leave it behind.
    Pending,
    Uncertain,
    Pid(u32),
}

/// Deletes list files left behind by earlier runs.
pub(in crate::user_commands) fn sweep(now: SystemTime, is_running: impl Fn(u32) -> Liveness) {
    sweep_with(now, is_running, |metadata| metadata.created().ok());
}

pub(super) fn sweep_with(
    now: SystemTime,
    is_running: impl Fn(u32) -> Liveness,
    creation_time: impl Fn(&std::fs::Metadata) -> Option<SystemTime>,
) {
    let dir = match list_dir() {
        Ok(dir) => dir,
        Err(error) => {
            tracing::warn!(%error, "could not find the user command list folder");
            return;
        }
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(%error, path = %dir.display(), "could not read the user command list folder");
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if is_pid_temp(&path) {
            if file_type.is_file()
                && list_age(&path, now, &creation_time).is_some_and(|age| age.modified_old)
            {
                remove_one(&path);
            }
            continue;
        }
        if !is_list(&path) || !file_type.is_file() {
            continue;
        }
        sweep_list(&path, now, &is_running, &creation_time);
    }
}

fn sweep_list(
    path: &Path,
    now: SystemTime,
    is_running: &impl Fn(u32) -> Liveness,
    creation_time: &impl Fn(&std::fs::Metadata) -> Option<SystemTime>,
) {
    let Some(age) = list_age(path, now, creation_time) else {
        return;
    };
    if !age.modified_old {
        return;
    }
    match sidecar_state(path) {
        Sidecar::Missing if age.created_old => remove_list(path),
        Sidecar::Missing | Sidecar::Uncertain => {}
        Sidecar::Pending => {
            if sidecar_old(path, now, creation_time) {
                remove_list(path);
            }
        }
        Sidecar::Pid(pid) => match is_running(pid) {
            Liveness::Gone => remove_list(path),
            Liveness::Running | Liveness::Unknown => {}
        },
    }
}

fn is_list(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("list-") && name.ends_with(".txt"))
}

fn is_pid_temp(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(".pid-") && name.ends_with(".tmp"))
}

struct ListAge {
    modified_old: bool,
    created_old: bool,
}

fn list_age(
    path: &Path,
    now: SystemTime,
    creation_time: impl Fn(&std::fs::Metadata) -> Option<SystemTime>,
) -> Option<ListAge> {
    let metadata = std::fs::metadata(path).ok()?;
    let old = |time: Option<SystemTime>| {
        time.is_some_and(|time| now.duration_since(time).is_ok_and(|age| age > LIST_AGE))
    };
    Some(ListAge {
        modified_old: old(metadata.modified().ok()),
        created_old: old(creation_time(&metadata)),
    })
}

/// Whether the sidecar's modified and creation times are both over a day old.
fn sidecar_old(
    list: &Path,
    now: SystemTime,
    creation_time: impl Fn(&std::fs::Metadata) -> Option<SystemTime>,
) -> bool {
    list_age(&sidecar(list), now, creation_time)
        .is_some_and(|age| age.modified_old && age.created_old)
}

fn sidecar_state(path: &Path) -> Sidecar {
    let pid = sidecar(path);
    match std::fs::symlink_metadata(&pid) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Sidecar::Missing,
        Err(_) => Sidecar::Uncertain,
        Ok(metadata) if !metadata.is_file() => Sidecar::Uncertain,
        Ok(_) => match std::fs::read_to_string(&pid) {
            Ok(text) if text.trim() == PENDING => Sidecar::Pending,
            Ok(text) => match text.trim().parse() {
                Ok(pid) => Sidecar::Pid(pid),
                Err(_) => Sidecar::Uncertain,
            },
            Err(_) => Sidecar::Uncertain,
        },
    }
}
