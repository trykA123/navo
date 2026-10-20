use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusty_store::{Db, DbError, OpenReport};

use super::Pending;

static PREVIOUS_EXIT_UNCLEAN: AtomicBool = AtomicBool::new(false);

pub(crate) fn note_previous_exit(left_behind: Option<&Pending>) {
    PREVIOUS_EXIT_UNCLEAN.store(left_behind.is_some(), Ordering::Relaxed);
}

pub(crate) fn open_db(path: &Path) -> Result<(Db, OpenReport), DbError> {
    open_checked(path, PREVIOUS_EXIT_UNCLEAN.load(Ordering::Relaxed))
}

fn open_checked(path: &Path, previous_exit_unclean: bool) -> Result<(Db, OpenReport), DbError> {
    let sentinel = sentinel_path(path);
    let unclean = previous_exit_unclean || sentinel.exists();
    match Db::open(path, unclean) {
        Ok((db, report)) => {
            if report.checked {
                let _ = fs::remove_file(&sentinel);
            }
            Ok((db, report))
        }
        Err(error) => {
            if error.is_corruption()
                && let Err(write_error) = fs::write(&sentinel, b"")
            {
                tracing::error!(%write_error, "could not record the failed integrity check");
            }
            Err(error)
        }
    }
}

pub(crate) fn sentinel_path(db: &Path) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push(".check-failed");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::crash::{Running, pending};

    static GLOBAL: Mutex<()> = Mutex::new(());

    fn checked_after(finish: bool) -> bool {
        let _lock = GLOBAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let data = tempfile::tempdir().unwrap();
        let running = Running::start(data.path()).unwrap();
        if finish {
            running.finish().unwrap();
        }
        note_previous_exit(pending(data.path()).as_ref());
        let (_, report) = open_db(&data.path().join("ferrite.db")).unwrap();
        note_previous_exit(None);
        report.checked
    }

    #[test]
    fn the_open_after_an_unclean_exit_runs_the_integrity_check() {
        assert!(checked_after(false));
    }

    #[test]
    fn the_open_after_a_clean_exit_skips_the_integrity_check() {
        assert!(!checked_after(true));
    }

    fn broken_db(dir: &tempfile::TempDir) -> PathBuf {
        let path = dir.path().join("ferrite.db");
        std::fs::write(&path, vec![0x5A; 4096]).unwrap();
        path
    }

    #[test]
    fn a_failed_check_is_still_made_at_a_later_clean_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = broken_db(&dir);
        assert!(open_checked(&path, true).is_err());
        assert!(sentinel_path(&path).exists());
        assert!(open_checked(&path, false).is_err());
        assert!(sentinel_path(&path).exists());
    }

    #[test]
    fn a_passing_check_removes_the_sentinel() {
        let dir = tempfile::tempdir().unwrap();
        let path = broken_db(&dir);
        assert!(open_checked(&path, true).is_err());
        std::fs::remove_file(&path).unwrap();
        let (_, report) = open_checked(&path, false).unwrap();
        assert!(report.checked);
        assert!(!sentinel_path(&path).exists());
    }

    #[test]
    fn a_healthy_database_leaves_no_sentinel() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ferrite.db");
        open_checked(&path, true).unwrap();
        assert!(!sentinel_path(&path).exists());
    }
}
