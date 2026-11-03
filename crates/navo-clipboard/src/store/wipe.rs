//! Delete all: a fresh database that keeps the old id high-water marks replaces every
//! clipboard file.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use crate::limits::Limits;

use super::ids::IdMarks;
use super::safety::{
    SUFFIXES, find_asides, move_no_replace, remove_if_present, suffixed, sync_parent,
};
use super::{ClipError, ClipStore};

const STAGED: &str = ".new";
const HEADER: &[u8; 16] = b"SQLite format 3\0";

impl ClipStore {
    /// Closes the store, removes the database with its `-wal` and `-shm` and every
    /// `<name>.corrupt-<stamp>` set-aside copy of a damaged file beside it, and opens it empty.
    /// The empty database is made first at `<name>.new`, with the old id high-water marks, so
    /// ids keep rising; when that fails nothing is deleted. Aside copies go before the live
    /// files, so a failure to remove one keeps the live database; the first failure is
    /// returned as [`ClipError::Io`] and the remaining files stay.
    /// `path` must name the open database; any other path is refused with
    /// [`ClipError::WrongPath`] before anything is touched.
    /// This consumes the store. On error the files stay and the caller must reopen the store.
    pub fn wipe_all(self, path: &Path) -> Result<ClipStore, ClipError> {
        let open_path = self.open_path(path)?;
        let limits = self.limits;
        let marks = self.id_marks()?;
        let staged = suffixed(&open_path, STAGED);
        stage(&staged, limits, marks)?;
        if let Err(error) = self.close_and_remove(&open_path) {
            let _ = remove_staged(&staged);
            return Err(error);
        }
        move_no_replace(&staged, &open_path)?;
        sync_parent(&open_path);
        ClipStore::open(&open_path, limits)
    }

    fn close_and_remove(self, open_path: &Path) -> Result<(), ClipError> {
        self.checkpoint()?;
        self.conn.close().map_err(|(_, error)| error)?;
        for aside in find_asides(open_path)? {
            remove_if_present(&aside)?;
        }
        for suffix in SUFFIXES {
            remove_if_present(&suffixed(open_path, suffix))?;
        }
        Ok(())
    }
}

/// A crash inside Delete all, after the old files went and before the rename, leaves only
/// `<name>.new`; open puts it in place when it is a regular file with a SQLite header.
pub(super) fn adopt_staged(path: &Path) {
    let staged = suffixed(path, STAGED);
    let regular = fs::symlink_metadata(&staged).is_ok_and(|meta| meta.is_file());
    if fs::symlink_metadata(path).is_err() && regular && has_header(&staged) {
        let _ = move_no_replace(&staged, path);
    }
}

fn has_header(path: &Path) -> bool {
    let mut head = [0; 16];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut head))
        .is_ok_and(|()| &head == HEADER)
}

/// Makes the empty, seeded database at `staged`; on failure no staged file stays.
fn stage(staged: &Path, limits: Limits, marks: IdMarks) -> Result<(), ClipError> {
    remove_staged(staged)?;
    let made = make_seeded(staged, limits, marks);
    if made.is_err() {
        let _ = remove_staged(staged);
    }
    made
}

fn make_seeded(staged: &Path, limits: Limits, marks: IdMarks) -> Result<(), ClipError> {
    let fresh = ClipStore::open_once(staged, limits)?;
    #[cfg(test)]
    super::tests::fail_seed()?;
    fresh.seed_ids(marks)?;
    fresh.checkpoint()?;
    fresh.conn.close().map_err(|(_, error)| error)?;
    // A clean close removes the empty log files; any left would only hold the marks.
    remove_if_present(&suffixed(staged, "-wal"))?;
    remove_if_present(&suffixed(staged, "-shm"))?;
    Ok(())
}

fn remove_staged(staged: &Path) -> io::Result<()> {
    for suffix in SUFFIXES {
        remove_if_present(&suffixed(staged, suffix))?;
    }
    Ok(())
}
