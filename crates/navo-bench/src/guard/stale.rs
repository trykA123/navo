//! Start-of-run cleanup of runs a crash or Ctrl+C left behind.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use super::act::{self, Expected, Recycler};
use super::ledger::{self, Token};
use super::{GuardError, MARKER_FILE, PathRole, act_platform, platform, resolve};

const ROLE: PathRole = PathRole::StaleRun;
const TOKEN_LEN: usize = 32;

/// Counts only: folders recycled and marked-looking runs that were refused and left in place.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StaleCleanup {
    pub recycled: usize,
    pub refused: usize,
}

pub(super) fn clean(
    root: &Path,
    parent: &Path,
    recycler: &dyn Recycler,
) -> Result<StaleCleanup, GuardError> {
    let parent = resolve::resolve(parent, ROLE)?;
    let own_parent = root.parent().ok_or(GuardError::InvalidPath(ROLE))?;
    if !resolve::equal(&parent.path, own_parent) {
        return Err(GuardError::OutsideRoot(ROLE));
    }
    let current = root
        .file_name()
        .and_then(run_stamp)
        .ok_or(GuardError::InvalidPath(ROLE))?;
    // A missing or broken ledger trusts no run, so nothing is recycled.
    let listed = ledger::read(&parent.path, ROLE).unwrap_or_default();
    let mut cleanup = StaleCleanup::default();
    for entry in fs::read_dir(&parent.path).map_err(|_| GuardError::Io(ROLE))? {
        let Ok(entry) = entry else {
            cleanup.refused += 1;
            continue;
        };
        let Some(stamp) = run_stamp(&entry.file_name()) else {
            continue;
        };
        if stamp >= current {
            continue;
        }
        match recycle_run(&parent.path, &entry.path(), &listed, recycler) {
            Ok(token) => {
                cleanup.recycled += 1;
                let _ = ledger::remove(&parent.path, &token, ROLE);
            }
            Err(_) => cleanup.refused += 1,
        }
    }
    Ok(cleanup)
}

/// Recycles one run whose marker token is listed and returns that token.
fn recycle_run(
    parent: &Path,
    run: &Path,
    listed: &[Token],
    recycler: &dyn Recycler,
) -> Result<Token, GuardError> {
    let metadata = fs::symlink_metadata(run).map_err(|_| GuardError::Io(ROLE))?;
    if platform::is_reparse(&metadata) || !metadata.is_dir() {
        return Err(GuardError::ReparsePoint(ROLE));
    }
    let handle = act_platform::open_act(run).map_err(|_| GuardError::Io(ROLE))?;
    let identity = act::confirm_handle(&handle, run, ROLE)?;
    drop(handle);
    let token = read_marker(run, ROLE)?;
    if !listed.contains(&token) {
        return Err(GuardError::Ledger(ROLE));
    }
    let expected = Expected {
        container: parent,
        path: run,
        identity,
        role: ROLE,
    };
    act::recycle(&expected, recycler).map(|()| token)
}

/// Reads a run's marker; it must be a plain file of exactly one token, reached without links.
pub(super) fn read_marker(run: &Path, role: PathRole) -> Result<[u8; TOKEN_LEN], GuardError> {
    let path = run.join(MARKER_FILE);
    let metadata = fs::symlink_metadata(&path).map_err(|_| GuardError::Marker(role))?;
    if platform::is_reparse(&metadata) || !metadata.is_file() {
        return Err(GuardError::Marker(role));
    }
    let file = File::open(&path).map_err(|_| GuardError::Marker(role))?;
    act::confirm_handle(&file, &path, role).map_err(|_| GuardError::Marker(role))?;
    let mut bytes = Vec::with_capacity(TOKEN_LEN + 1);
    let limit = u64::try_from(TOKEN_LEN + 1).map_err(|_| GuardError::Marker(role))?;
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_| GuardError::Marker(role))?;
    <[u8; TOKEN_LEN]>::try_from(bytes.as_slice()).map_err(|_| GuardError::Marker(role))
}

/// The creation time in a `run-<nanos>-<32 hex>` name; any other name is not a run.
fn run_stamp(name: &OsStr) -> Option<u128> {
    let (nanos, nonce) = name.to_str()?.strip_prefix("run-")?.split_once('-')?;
    let digits = !nanos.is_empty() && nanos.bytes().all(|byte| byte.is_ascii_digit());
    let hex = nonce.len() == 32
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !digits || !hex {
        return None;
    }
    nanos.parse().ok()
}
