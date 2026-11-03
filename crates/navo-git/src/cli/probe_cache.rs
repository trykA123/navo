use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime};

use crate::GitError;

use super::process::{Version, resolve_git, version_of};

type Stamp = Option<SystemTime>;
type Key = (PathBuf, Vec<(OsString, OsString)>);

#[derive(Clone)]
struct Probed {
    git: PathBuf,
    version: Version,
    text: String,
    requested_stamp: Stamp,
    resolved_stamp: Stamp,
}

static PROBED: LazyLock<Mutex<HashMap<Key, Probed>>> = LazyLock::new(Mutex::default);

fn stamp(path: &Path) -> Stamp {
    fs::metadata(path).ok()?.modified().ok()
}

/// The resolved Git path and its version. `--exec-path` and `--version` run once per executable
/// and environment; a changed path or modification time reads them again. Failures are not cached.
pub(super) fn probe(
    requested: &Path,
    resolve: bool,
    env: &[(OsString, OsString)],
    timeout: Duration,
) -> Result<(PathBuf, Version, String), GitError> {
    let key = (requested.to_path_buf(), env.to_vec());
    let requested_stamp = stamp(requested);
    if requested_stamp.is_some() {
        let cached = PROBED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned();
        if let Some(hit) = cached
            && hit.requested_stamp == requested_stamp
            && hit.resolved_stamp == stamp(&hit.git)
        {
            return Ok((hit.git, hit.version, hit.text));
        }
    }
    let git = if resolve {
        resolve_git(requested, env, timeout)
    } else {
        requested.to_path_buf()
    };
    let (version, text) = version_of(&git, env, timeout)?;
    let resolved_stamp = stamp(&git);
    if requested_stamp.is_some() && resolved_stamp.is_some() {
        let entry = Probed {
            git: git.clone(),
            version,
            text: text.clone(),
            requested_stamp,
            resolved_stamp,
        };
        PROBED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, entry);
    }
    Ok((git, version, text))
}
