use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusty_store::{Db, DbError, OpenReport};

use super::Pending;

static PREVIOUS_EXIT_UNCLEAN: AtomicBool = AtomicBool::new(false);

pub(crate) fn note_previous_exit(left_behind: Option<&Pending>) {
    PREVIOUS_EXIT_UNCLEAN.store(left_behind.is_some(), Ordering::Relaxed);
}

pub(crate) fn open_db(path: &Path) -> Result<(Db, OpenReport), DbError> {
    Db::open(path, PREVIOUS_EXIT_UNCLEAN.load(Ordering::Relaxed))
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
}
