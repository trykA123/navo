use std::fs;
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

use super::Operation;
use crate::GitError;

/// Markers in priority order: a rebase that stopped on a pick also leaves `CHERRY_PICK_HEAD`.
pub(crate) const MARKERS: [(&str, Operation); 6] = [
    ("rebase-merge", Operation::Rebase),
    ("rebase-apply", Operation::Rebase),
    ("MERGE_HEAD", Operation::Merge),
    ("CHERRY_PICK_HEAD", Operation::CherryPick),
    ("REVERT_HEAD", Operation::Revert),
    ("BISECT_LOG", Operation::Bisect),
];

/// The operation under way in `git_dir`, read from Git's marker files.
pub(crate) fn operation(git_dir: &Path) -> Result<Option<Operation>, GitError> {
    for (name, operation) in MARKERS {
        let path = git_dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => return Ok(Some(operation)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(GitError::Io {
                    path,
                    kind: error.kind(),
                });
            }
        }
    }
    Ok(None)
}

/// Modified time of `FETCH_HEAD` in milliseconds since the Unix epoch; `None` when never fetched.
pub(crate) fn fetched_ms(git_dir: &Path) -> Option<i64> {
    let modified = fs::metadata(git_dir.join("FETCH_HEAD"))
        .ok()?
        .modified()
        .ok()?;
    let elapsed = modified.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(elapsed.as_millis()).ok()
}
